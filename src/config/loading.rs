use indexmap::IndexSet;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use super::flavor::ConfigLoaded;
use super::flavor::ConfigValidated;
use super::parsers;
use super::registry::RuleRegistry;
use super::source_tracking::{
    ConfigSource, ConfigValidationWarning, SourcedConfig, SourcedConfigFragment, SourcedGlobalConfig, SourcedValue,
};
use super::types::{Config, ConfigError, GlobalConfig, MARKDOWNLINT_CONFIG_FILES, RUMDL_CONFIG_FILES, RuleConfig};
use super::validation::validate_config_sourced_internal;
use crate::utils::upward_walk::UpwardWalk;

/// Maximum depth for extends chains to prevent runaway recursion
const MAX_EXTENDS_DEPTH: usize = 10;

/// Cheap pre-filter for whether a `pyproject.toml` declares rumdl config.
///
/// Matches the flat section header `[tool.rumdl]` as well as dotted sections
/// like `[tool.rumdl.MD013]` or `[tool.rumdl.rules.MD007]` (which are valid on
/// their own, without a flat header). Requiring the leading `[` avoids matching
/// a bare `tool.rumdl` in prose or dependency names; a literal `[tool.rumdl...`
/// inside a comment or string would still match, but the subsequent parse
/// handles that gracefully.
fn pyproject_declares_rumdl_config(content: &str) -> bool {
    content.contains("[tool.rumdl]") || content.contains("[tool.rumdl.")
}

/// True if `b` may start a `$VAR` identifier (`[A-Za-z_]`).
fn is_var_name_start(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphabetic()
}

/// True if `b` may continue a `$VAR` identifier (`[A-Za-z0-9_]`).
fn is_var_name_continue(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// True if `name` is a non-empty valid environment-variable identifier.
fn is_valid_var_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty() && is_var_name_start(bytes[0]) && bytes[1..].iter().all(|&b| is_var_name_continue(b))
}

/// Expand `$VAR` and `${VAR}` references in `input` using `lookup`.
///
/// Grammar (frozen; documented in `docs/global-settings.md`):
/// - `$NAME` / `${NAME}` with `NAME = [A-Za-z_][A-Za-z0-9_]*` expands to the variable's
///   value; the longest valid identifier is matched (`$FOO_BAR` is one name).
/// - `$$` is a literal `$` (escape), so `$$VAR` -> `$VAR` and `$${VAR}` -> `${VAR}` (no
///   expansion of the escaped form).
/// - Any other `$` is left literal: `$` before a non-identifier char (`$5`, trailing `$`),
///   an empty `${}`, an unterminated `${VAR`, or a `${...}` whose body is not a valid
///   identifier (e.g. nested `${A${B}}`) - the whole `${...}` span up to the first `}` is
///   emitted literally.
/// - Replacement values are inserted literally and are NOT re-scanned (single left-to-right
///   pass): if `A="$B"`, then `$A` expands to the literal string `$B`.
///
/// Returns `Err(name)` on the first well-formed reference to an undefined variable. All
/// special characters (`$`, `{`, `}`, identifier chars) are ASCII, so byte scanning never
/// splits a multibyte UTF-8 sequence; non-ASCII bytes are copied verbatim as literals.
fn expand_env_vars(input: &str, lookup: impl Fn(&str) -> Option<String>) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'$' {
            // Copy the maximal run of non-`$` bytes as a slice (preserves UTF-8).
            let start = i;
            while i < bytes.len() && bytes[i] != b'$' {
                i += 1;
            }
            out.push_str(&input[start..i]);
            continue;
        }

        match bytes.get(i + 1).copied() {
            // `$$` -> literal `$`.
            Some(b'$') => {
                out.push('$');
                i += 2;
            }
            // `${...}` braced form.
            Some(b'{') => {
                if let Some(rel) = input[i + 2..].find('}') {
                    let close = i + 2 + rel;
                    let name = &input[i + 2..close];
                    if is_valid_var_name(name) {
                        match lookup(name) {
                            Some(value) => out.push_str(&value),
                            None => return Err(name.to_string()),
                        }
                    } else {
                        // Empty / invalid / nested body -> whole `${...}` span is literal.
                        out.push_str(&input[i..=close]);
                    }
                    i = close + 1;
                } else {
                    // No closing `}` -> leave the `$` literal and resume at `{`.
                    out.push('$');
                    i += 1;
                }
            }
            // `$NAME` bare form.
            Some(b) if is_var_name_start(b) => {
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() && is_var_name_continue(bytes[j]) {
                    j += 1;
                }
                let name = &input[start..j];
                match lookup(name) {
                    Some(value) => out.push_str(&value),
                    None => return Err(name.to_string()),
                }
                i = j;
            }
            // `$` before a non-identifier char or at end of input -> literal `$`.
            _ => {
                out.push('$');
                i += 1;
            }
        }
    }

    Ok(out)
}

/// Resolve an `extends` path relative to the config file that contains it.
///
/// - `$VAR` / `${VAR}`: expanded from the environment first (see [`expand_env_vars`])
/// - `~/` prefix: expanded to home directory
/// - Relative paths: resolved against the config file's parent directory
/// - Absolute paths: used as-is
fn resolve_extends_path(extends_value: &str, config_file_path: &Path) -> Result<PathBuf, ConfigError> {
    let expanded = expand_env_vars(extends_value, |key| std::env::var(key).ok()).map_err(|var| {
        ConfigError::ExtendsUndefinedVar {
            var,
            from: config_file_path.display().to_string(),
        }
    })?;

    if let Some(suffix) = expanded.strip_prefix("~/") {
        // Expand tilde to home directory
        #[cfg(feature = "native")]
        {
            use etcetera::{BaseStrategy, choose_base_strategy};
            let home = choose_base_strategy().map_or_else(|_| PathBuf::from("~"), |s| s.home_dir().to_path_buf());
            Ok(home.join(suffix))
        }
        #[cfg(not(feature = "native"))]
        {
            let _ = suffix;
            Ok(PathBuf::from(expanded))
        }
    } else {
        let path = PathBuf::from(&expanded);
        if path.is_absolute() {
            Ok(path)
        } else {
            // Resolve relative to config file's directory
            let config_dir = config_file_path.parent().unwrap_or(Path::new("."));
            Ok(config_dir.join(&expanded))
        }
    }
}

/// Determine ConfigSource from a config filename.
fn source_from_filename(filename: &str) -> ConfigSource {
    if filename == "pyproject.toml" {
        ConfigSource::PyprojectToml
    } else {
        ConfigSource::ProjectConfig
    }
}

/// The rumdl-native config files that actually exist in `dir`, in precedence order.
///
/// Walks `RUMDL_CONFIG_FILES` (the single source of truth for discovery) joined onto
/// `dir`, so `.config/rumdl.toml` is recognised at the same level as `.rumdl.toml`.
/// `pyproject.toml` counts only when it declares `[tool.rumdl]`. markdownlint configs
/// are intentionally excluded: they are a separate fallback tier, not a same-tool
/// collision, and projects routinely keep one around while migrating.
pub(crate) fn rumdl_configs_in_dir(dir: &Path) -> Vec<PathBuf> {
    RUMDL_CONFIG_FILES
        .iter()
        .map(|name| dir.join(name))
        .filter(|path| {
            if !path.exists() {
                return false;
            }
            if path.file_name().and_then(|n| n.to_str()) == Some("pyproject.toml") {
                std::fs::read_to_string(path).is_ok_and(|content| pyproject_declares_rumdl_config(&content))
            } else {
                true
            }
        })
        .collect()
}

/// A directory holding more than one rumdl-native config file.
///
/// `winner` is the file discovery uses (highest precedence); `shadowed` are the
/// silently-ignored siblings. Having both `.rumdl.toml` and `rumdl.toml` (or either
/// plus a `[tool.rumdl]` in `pyproject.toml`) in one directory is redundant by
/// construction and a common footgun: editing the shadowed file appears to do
/// nothing. Resolution is unchanged (the dot file still wins, matching Ruff); this
/// type only lets callers surface the collision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShadowedConfigs {
    pub dir: PathBuf,
    pub winner: PathBuf,
    pub shadowed: Vec<PathBuf>,
}

/// Detect rumdl-native config files that shadow each other in `dir`.
///
/// Returns `None` unless two or more rumdl-native configs coexist at this directory
/// level (markdownlint files and configs in other directories never count). The
/// highest-precedence file is the `winner`; the rest are silently `shadowed`.
pub(crate) fn detect_shadowed_configs(dir: &Path) -> Option<ShadowedConfigs> {
    let mut configs = rumdl_configs_in_dir(dir);
    if configs.len() < 2 {
        return None;
    }
    let winner = configs.remove(0);
    Some(ShadowedConfigs {
        dir: dir.to_path_buf(),
        winner,
        shadowed: configs,
    })
}

/// Format a shadowed-config collision as a single user-facing warning line.
///
/// The directory is named once; the winner and shadowed files are shown relative
/// to it (e.g. `.rumdl.toml`, `.config/rumdl.toml`) rather than repeating the full
/// directory in every path. Paths are normalized to forward slashes on Windows for
/// stable, copy-pasteable output; non-UTF-8 components degrade lossily rather than
/// panicking.
pub(crate) fn format_shadow_warning(shadow: &ShadowedConfigs) -> String {
    let norm = |s: String| if cfg!(windows) { s.replace('\\', "/") } else { s };
    let rel = |path: &Path| {
        let relative = path.strip_prefix(&shadow.dir).unwrap_or(path);
        norm(relative.to_string_lossy().into_owned())
    };
    let shadowed = shadow.shadowed.iter().map(|p| rel(p)).collect::<Vec<_>>().join(", ");
    format!(
        "multiple rumdl config files in {}: using {}, ignoring {}",
        norm(shadow.dir.to_string_lossy().into_owned()),
        rel(&shadow.winner),
        shadowed,
    )
}

/// Load a config file (and any base configs it extends) into a SourcedConfig.
///
/// This function handles the recursive `extends` chain:
/// 1. Parse the config file into a fragment
/// 2. If the fragment has `extends`, recursively load the base config first
/// 3. Merge the base config, then merge this fragment on top
fn load_config_with_extends(
    sourced_config: &mut SourcedConfig<ConfigLoaded>,
    config_file_path: &Path,
    visited: &mut IndexSet<PathBuf>,
    chain_source: ConfigSource,
) -> Result<(), ConfigError> {
    // Canonicalize the path for circular reference detection
    let canonical = config_file_path
        .canonicalize()
        .unwrap_or_else(|_| config_file_path.to_path_buf());

    // Check for circular references
    if visited.contains(&canonical) {
        let chain: Vec<String> = visited.iter().map(|p| p.display().to_string()).collect();
        return Err(ConfigError::CircularExtends {
            path: config_file_path.display().to_string(),
            chain,
        });
    }

    // Check depth limit
    if visited.len() >= MAX_EXTENDS_DEPTH {
        return Err(ConfigError::ExtendsDepthExceeded {
            path: config_file_path.display().to_string(),
            max_depth: MAX_EXTENDS_DEPTH,
        });
    }

    // Mark as visited
    visited.insert(canonical);

    let path_str = config_file_path.display().to_string();
    let filename = config_file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Read and parse the config file
    let content = std::fs::read_to_string(config_file_path).map_err(|e| ConfigError::IoError {
        source: e,
        path: path_str.clone(),
    })?;

    let fragment = if filename == "pyproject.toml" {
        match parsers::parse_pyproject_toml(&content, &path_str, chain_source)? {
            Some(f) => f,
            None => return Ok(()), // No [tool.rumdl] section
        }
    } else {
        parsers::parse_rumdl_toml(&content, &path_str, chain_source)?
    };

    // If this fragment has `extends`, load the base config first
    if let Some(ref extends_value) = fragment.extends {
        let base_path = resolve_extends_path(extends_value, config_file_path)?;

        if !base_path.exists() {
            return Err(ConfigError::ExtendsNotFound {
                path: base_path.display().to_string(),
                from: path_str.clone(),
            });
        }

        log::debug!(
            "[rumdl-config] Config {} extends {}, loading base first",
            path_str,
            base_path.display()
        );

        // Recursively load the base config
        load_config_with_extends(sourced_config, &base_path, visited, chain_source)?;
    }

    // Merge this fragment on top (base config was already merged if present)
    // Strip the `extends` field since it's been consumed
    let mut fragment_for_merge = fragment;
    fragment_for_merge.extends = None;
    sourced_config.merge(fragment_for_merge);
    sourced_config.loaded_files.push(path_str);

    Ok(())
}

impl SourcedConfig<ConfigLoaded> {
    /// Merges another SourcedConfigFragment into this SourcedConfig.
    /// Uses source precedence to determine which values take effect.
    pub(super) fn merge(&mut self, fragment: SourcedConfigFragment) {
        // Merge global config. Enable/disable use replace semantics (child
        // config overrides parent, matching Ruff's `select`/`ignore`);
        // extend-enable/extend-disable use union semantics (additive across
        // config levels).
        self.global.enable.merge_from(fragment.global.enable);
        self.global.disable.merge_from(fragment.global.disable);
        self.global
            .extend_enable
            .merge_union_from(fragment.global.extend_enable);
        self.global
            .extend_disable
            .merge_union_from(fragment.global.extend_disable);

        // Conflict resolution: Enable overrides disable
        // Remove any rules from disable that appear in enable
        self.global
            .disable
            .value
            .retain(|rule| !self.global.enable.value.contains(rule));

        self.global.include.merge_from(fragment.global.include);
        self.global.exclude.merge_from(fragment.global.exclude);
        self.global
            .respect_gitignore
            .merge_from(fragment.global.respect_gitignore);
        self.global.line_length.merge_from(fragment.global.line_length);
        self.global.fixable.merge_from(fragment.global.fixable);
        self.global.unfixable.merge_from(fragment.global.unfixable);
        self.global.flavor.merge_from(fragment.global.flavor);
        self.global.force_exclude.merge_from(fragment.global.force_exclude);

        // Merge output_format if present
        if let Some(output_format_fragment) = fragment.global.output_format {
            if let Some(ref mut output_format) = self.global.output_format {
                output_format.merge_from(output_format_fragment);
            } else {
                self.global.output_format = Some(output_format_fragment);
            }
        }

        // Merge cache_dir if present
        if let Some(cache_dir_fragment) = fragment.global.cache_dir {
            if let Some(ref mut cache_dir) = self.global.cache_dir {
                cache_dir.merge_from(cache_dir_fragment);
            } else {
                self.global.cache_dir = Some(cache_dir_fragment);
            }
        }

        // Merge cache if not default (only override when explicitly set)
        if fragment.global.cache.source != ConfigSource::Default {
            self.global.cache.merge_from(fragment.global.cache);
        }

        self.per_file_ignores.merge_from(fragment.per_file_ignores);
        self.per_file_flavor.merge_from(fragment.per_file_flavor);
        self.code_block_tools.merge_from(fragment.code_block_tools);

        // Merge rule configs
        for (rule_name, rule_fragment) in fragment.rules {
            let norm_rule_name = rule_name.to_ascii_uppercase(); // Normalize to uppercase for case-insensitivity
            let rule_entry = self.rules.entry(norm_rule_name).or_default();

            // Merge severity if present in fragment
            if let Some(severity_fragment) = rule_fragment.severity {
                if let Some(ref mut existing_severity) = rule_entry.severity {
                    existing_severity.merge_from(severity_fragment);
                } else {
                    rule_entry.severity = Some(severity_fragment);
                }
            }

            // Merge values
            for (key, sourced_value_fragment) in rule_fragment.values {
                let sv_entry = rule_entry
                    .values
                    .entry(key.clone())
                    .or_insert_with(|| SourcedValue::new(sourced_value_fragment.value.clone(), ConfigSource::Default));
                sv_entry.merge_from(sourced_value_fragment);
            }
        }

        // Merge unknown_keys from fragment
        for (section, key, file_path) in fragment.unknown_keys {
            // Deduplicate: only add if not already present
            if !self.unknown_keys.iter().any(|(s, k, _)| s == &section && k == &key) {
                self.unknown_keys.push((section, key, file_path));
            }
        }
    }

    /// Load and merge configurations from files and CLI overrides.
    pub fn load(config_path: Option<&str>, cli_overrides: Option<&SourcedGlobalConfig>) -> Result<Self, ConfigError> {
        Self::load_with_discovery(config_path, cli_overrides, false)
    }

    /// Finds project root by walking up from start_dir looking for .git directory.
    /// Falls back to start_dir if no .git found.
    fn find_project_root_from(start_dir: &Path) -> std::path::PathBuf {
        UpwardWalk::new(start_dir)
            .find(|dir| dir.join(".git").exists())
            .unwrap_or_else(|| {
                log::debug!(
                    "[rumdl-config] No .git found, using config location as project root: {}",
                    start_dir.display()
                );
                start_dir.to_path_buf()
            })
    }

    /// Resolve the home-directory boundary used to stop project-config discovery.
    ///
    /// `home_override` wins (supplied by tests); otherwise the real home is resolved on
    /// native builds via `etcetera`. Wasm has no home/project walk to bound, so it
    /// returns `None` there.
    fn resolve_home_boundary(home_override: Option<&Path>) -> Option<std::path::PathBuf> {
        home_override.map(Path::to_path_buf).or_else(|| {
            #[cfg(feature = "native")]
            {
                use etcetera::{BaseStrategy, choose_base_strategy};
                choose_base_strategy().ok().map(|s| s.home_dir().to_path_buf())
            }
            #[cfg(not(feature = "native"))]
            {
                None
            }
        })
    }

    /// Discover configuration file by traversing up the directory tree.
    /// Returns the first configuration file found.
    /// Discovers config file and returns both the config path and project root.
    /// Returns: (config_file_path, project_root_path)
    /// Project root is the directory containing .git, or config parent as fallback.
    ///
    /// The walk stops at the home directory: a config file located in `$HOME`
    /// itself is user-level, not a project config, and must reach the loader only
    /// through the user-config fallback (`load_user_config`) so the platform
    /// user-config directory keeps precedence over `~/.rumdl.toml`. The cwd is
    /// exempt from that boundary: it is an explicitly chosen project context, so
    /// its configs apply even when the cwd *is* `$HOME` (pre-commit.ci sets `HOME`
    /// to the git checkout, and `pyproject.toml` has no user-config fallback).
    /// `home_override` supplies the boundary for tests; production resolves the
    /// real home directory.
    fn discover_config_upward(
        home_override: Option<&Path>,
    ) -> Option<(std::path::PathBuf, std::path::PathBuf, Option<ShadowedConfigs>)> {
        let start_dir = match std::env::current_dir() {
            Ok(dir) => dir,
            Err(e) => {
                log::debug!("[rumdl-config] Failed to get current directory: {e}");
                return None;
            }
        };

        // `rumdl_configs_in_dir` is the single source of truth for "which rumdl
        // configs live here", shared with the LSP and the shadow detector, so the
        // winner and the silently-shadowed siblings are computed identically.
        let (config_path, config_dir, shadow) = UpwardWalk::new(&start_dir)
            .stop_below(Self::resolve_home_boundary(home_override))
            .always_yield_start()
            .stop_at_git_root()
            .find_map(|dir| {
                rumdl_configs_in_dir(&dir).into_iter().next().map(|winner| {
                    log::debug!("[rumdl-config] Found config file: {}", winner.display());
                    let shadow = detect_shadowed_configs(&dir);
                    (winner, dir, shadow)
                })
            })?;

        // Determine project root by walking up from the config location.
        let project_root = Self::find_project_root_from(&config_dir);
        Some((config_path, project_root, shadow))
    }

    /// Discover markdownlint configuration file by traversing up the directory tree.
    /// Similar to discover_config_upward but for .markdownlint.yaml/json files, and
    /// bounded at the home directory for the same reason: a markdownlint config in
    /// `$HOME` is user-level, not a project config. The cwd is exempt from the
    /// boundary just like rumdl config discovery, and markdownlint files have no
    /// user-config fallback at all, so without the exemption a config in a
    /// `HOME == cwd` checkout would be ignored entirely.
    fn discover_markdownlint_config_upward(home_override: Option<&Path>) -> Option<std::path::PathBuf> {
        let start_dir = match std::env::current_dir() {
            Ok(dir) => dir,
            Err(e) => {
                log::debug!("[rumdl-config] Failed to get current directory for markdownlint discovery: {e}");
                return None;
            }
        };

        UpwardWalk::new(&start_dir)
            .stop_below(Self::resolve_home_boundary(home_override))
            .always_yield_start()
            .stop_at_git_root()
            .find_map(|dir| {
                MARKDOWNLINT_CONFIG_FILES
                    .iter()
                    .map(|name| dir.join(name))
                    .find(|path| path.exists())
            })
    }

    /// Internal implementation that accepts config directory for testing
    fn user_configuration_path_impl(config_dir: &Path) -> Option<std::path::PathBuf> {
        let config_dir = config_dir.join("rumdl");

        // Check for config files in precedence order (same as project discovery)
        const USER_CONFIG_FILES: &[&str] = &[".rumdl.toml", "rumdl.toml", "pyproject.toml"];

        log::debug!(
            "[rumdl-config] Checking for user configuration in: {}",
            config_dir.display()
        );

        for filename in USER_CONFIG_FILES {
            let config_path = config_dir.join(filename);

            if config_path.exists() {
                // For pyproject.toml, verify it contains [tool.rumdl] section
                if *filename == "pyproject.toml" {
                    if let Ok(content) = std::fs::read_to_string(&config_path) {
                        if pyproject_declares_rumdl_config(&content) {
                            log::debug!("[rumdl-config] Found user configuration at: {}", config_path.display());
                            return Some(config_path);
                        }
                        log::debug!("[rumdl-config] Found user pyproject.toml but no [tool.rumdl] section");
                        continue;
                    }
                } else {
                    log::debug!("[rumdl-config] Found user configuration at: {}", config_path.display());
                    return Some(config_path);
                }
            }
        }

        log::debug!(
            "[rumdl-config] No user configuration found in: {}",
            config_dir.display()
        );
        None
    }

    /// Discover user-level configuration file from platform-specific config directory.
    /// Returns the first configuration file found in the user config directory.
    #[cfg(feature = "native")]
    fn user_configuration_path() -> Option<std::path::PathBuf> {
        use etcetera::{BaseStrategy, choose_base_strategy};

        match choose_base_strategy() {
            Ok(strategy) => {
                let config_dir = strategy.config_dir();
                Self::user_configuration_path_impl(&config_dir)
            }
            Err(e) => {
                log::debug!("[rumdl-config] Failed to determine user config directory: {e}");
                None
            }
        }
    }

    /// Stub for WASM builds - user config not supported
    #[cfg(not(feature = "native"))]
    fn user_configuration_path() -> Option<std::path::PathBuf> {
        None
    }

    /// Internal implementation that accepts the home directory for testing.
    ///
    /// Probes `<home>/.rumdl.toml` then `<home>/rumdl.toml`, returning the first match.
    ///
    /// `pyproject.toml` is intentionally **not** searched in `$HOME`, even though
    /// `user_configuration_path_impl` does check it inside the platform config dir.
    /// The asymmetry is deliberate: a `pyproject.toml` directly in `$HOME` almost
    /// always belongs to unrelated python tooling (poetry/uv/pip's user-level config),
    /// and silently picking it up as a rumdl config would surprise users. The
    /// platform config dir (`~/.config/rumdl/`) is rumdl-scoped, so the same
    /// concern doesn't apply there.
    fn home_configuration_path_impl(home_dir: &Path) -> Option<std::path::PathBuf> {
        const HOME_CONFIG_FILES: &[&str] = &[".rumdl.toml", "rumdl.toml"];

        log::debug!(
            "[rumdl-config] Checking for home-directory configuration in: {}",
            home_dir.display()
        );

        for filename in HOME_CONFIG_FILES {
            let config_path = home_dir.join(filename);
            if config_path.exists() {
                log::debug!(
                    "[rumdl-config] Found home-directory configuration at: {}",
                    config_path.display()
                );
                return Some(config_path);
            }
        }

        log::debug!(
            "[rumdl-config] No home-directory configuration found in: {}",
            home_dir.display()
        );
        None
    }

    /// Discover a home-directory configuration file (`~/.rumdl.toml` or `~/rumdl.toml`).
    ///
    /// This is a final fallback after the platform user-config directory
    /// (`user_configuration_path`). It honors the classic Unix dotfile convention so
    /// users who keep tool config in `$HOME` rather than `$XDG_CONFIG_HOME` are picked up.
    #[cfg(feature = "native")]
    fn home_configuration_path() -> Option<std::path::PathBuf> {
        use etcetera::{BaseStrategy, choose_base_strategy};

        match choose_base_strategy() {
            Ok(strategy) => Self::home_configuration_path_impl(strategy.home_dir()),
            Err(e) => {
                log::debug!("[rumdl-config] Failed to determine home directory: {e}");
                None
            }
        }
    }

    /// Stub for WASM builds - home config not supported
    #[cfg(not(feature = "native"))]
    fn home_configuration_path() -> Option<std::path::PathBuf> {
        None
    }

    /// Load an explicit config file (standalone, no user config merging)
    fn load_explicit_config(sourced_config: &mut Self, path: &str) -> Result<(), ConfigError> {
        let path_obj = Path::new(path);
        let filename = path_obj.file_name().and_then(|name| name.to_str()).unwrap_or("");
        let path_str = path.to_string();

        log::debug!("[rumdl-config] Loading explicit config file: {filename}");

        // Find project root by walking up from config location looking for .git
        if let Some(config_parent) = path_obj.parent() {
            let project_root = Self::find_project_root_from(config_parent);
            log::debug!(
                "[rumdl-config] Project root (from explicit config): {}",
                project_root.display()
            );
            sourced_config.project_root = Some(project_root);
        }

        // Known markdownlint config files
        const MARKDOWNLINT_FILENAMES: &[&str] = &[
            ".markdownlint-cli2.jsonc",
            ".markdownlint-cli2.yaml",
            ".markdownlint-cli2.yml",
            ".markdownlint.json",
            ".markdownlint.yaml",
            ".markdownlint.yml",
        ];

        if filename == "pyproject.toml" || filename == ".rumdl.toml" || filename == "rumdl.toml" {
            // Use extends-aware loading for rumdl TOML configs
            let mut visited = IndexSet::new();
            let chain_source = source_from_filename(filename);
            load_config_with_extends(sourced_config, path_obj, &mut visited, chain_source)?;
        } else if MARKDOWNLINT_FILENAMES.contains(&filename)
            || path_str.ends_with(".json")
            || path_str.ends_with(".jsonc")
            || path_str.ends_with(".yaml")
            || path_str.ends_with(".yml")
        {
            // Parse as markdownlint config (JSON/YAML) - no extends support
            let fragment = parsers::load_from_markdownlint(&path_str)?;
            sourced_config.merge(fragment);
            sourced_config.loaded_files.push(path_str);
        } else {
            // Try TOML with extends support
            let mut visited = IndexSet::new();
            let chain_source = source_from_filename(filename);
            load_config_with_extends(sourced_config, path_obj, &mut visited, chain_source)?;
        }

        Ok(())
    }

    /// Load and merge user-level configuration into this `SourcedConfig`.
    ///
    /// Discovers the user config file in this order, taking the first match:
    /// 1. Platform user-config directory, resolved via `etcetera::choose_base_strategy`
    ///    (the CLI/XDG convention): `~/.config` on Linux and macOS, `%APPDATA%` on
    ///    Windows. Note macOS uses the XDG-style `~/.config`, not the GUI-app location
    ///    `~/Library/Application Support`. Override with `user_config_dir` for tests.
    /// 2. Home-directory dotfile (`~/.rumdl.toml`, then `~/rumdl.toml`). Override with
    ///    `home_dir` for tests. Honors the classic Unix dotfile convention.
    ///
    /// Resolves any `extends` chain and merges each fragment with
    /// `ConfigSource::UserConfig` precedence.
    ///
    /// Called in two contexts:
    /// - When no project config is found: provides user defaults as the sole base
    /// - When a markdownlint project config is found: provides rumdl-specific
    ///   defaults that the markdownlint format cannot express; the markdownlint
    ///   fragment is merged on top and wins on any overlapping key
    fn load_user_config(
        sourced_config: &mut Self,
        user_config_dir: Option<&Path>,
        home_dir: Option<&Path>,
    ) -> Result<(), ConfigError> {
        let user_config_path = if let Some(dir) = user_config_dir {
            Self::user_configuration_path_impl(dir)
        } else {
            Self::user_configuration_path()
        };

        let user_config_path = user_config_path.or_else(|| match home_dir {
            Some(home) => Self::home_configuration_path_impl(home),
            None => Self::home_configuration_path(),
        });

        if let Some(user_config_path) = user_config_path {
            let path_str = user_config_path.display().to_string();

            log::debug!("[rumdl-config] Loading user config: {path_str}");

            // User config fallback also supports extends chains.
            // Use a uniform source across the chain so child overrides are determined by chain order.
            let mut visited = IndexSet::new();
            load_config_with_extends(
                sourced_config,
                &user_config_path,
                &mut visited,
                ConfigSource::UserConfig,
            )?;
        } else {
            log::debug!("[rumdl-config] No user configuration file found");
        }

        Ok(())
    }

    /// Internal implementation that accepts user config directory and home directory for testing
    #[doc(hidden)]
    pub fn load_with_discovery_impl(
        config_path: Option<&str>,
        cli_overrides: Option<&SourcedGlobalConfig>,
        skip_auto_discovery: bool,
        user_config_dir: Option<&Path>,
        home_dir: Option<&Path>,
    ) -> Result<Self, ConfigError> {
        use std::env;
        log::debug!("[rumdl-config] Current working directory: {:?}", env::current_dir());

        let mut sourced_config = SourcedConfig::default();

        // Ruff model: Project config is standalone, user config is fallback only
        //
        // Priority order:
        // 1. If explicit config path provided → use ONLY that (standalone)
        // 2. Else if project config discovered → use ONLY that (standalone)
        // 3. Else if user config exists → use it as fallback
        // 4. CLI overrides always apply last
        //
        // This ensures project configs are reproducible across machines and
        // CI/local runs behave identically.

        // Explicit config path always takes precedence
        if let Some(path) = config_path {
            // Explicit config path provided - use ONLY this config (standalone)
            log::debug!("[rumdl-config] Explicit config_path provided: {path:?}");
            Self::load_explicit_config(&mut sourced_config, path)?;
        } else if skip_auto_discovery {
            log::debug!("[rumdl-config] Skipping config discovery due to --no-config/--isolated flag");
            // No config loading, just apply CLI overrides at the end
        } else {
            // No explicit path - try auto-discovery
            log::debug!("[rumdl-config] No explicit config_path, searching default locations");

            // Try to discover project config first
            if let Some((config_file, project_root, shadow)) = Self::discover_config_upward(home_dir) {
                // Project config found - use ONLY this (standalone, no user config).
                // Rumdl project configs can express all settings directly, so user config
                // is not needed and omitting it ensures CI and local runs are identical.
                log::debug!("[rumdl-config] Found project config: {}", config_file.display());
                log::debug!("[rumdl-config] Project root: {}", project_root.display());

                // Record any same-directory sibling configs that are silently shadowed,
                // so the CLI and LSP can warn the user. Resolution is unchanged.
                if let Some(shadow) = shadow {
                    sourced_config.discovery_warnings.push(format_shadow_warning(&shadow));
                }

                sourced_config.project_root = Some(project_root);

                // Use extends-aware loading for discovered configs
                let mut visited = IndexSet::new();
                let root_filename = config_file.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let chain_source = source_from_filename(root_filename);
                load_config_with_extends(&mut sourced_config, &config_file, &mut visited, chain_source)?;
            } else {
                // No rumdl project config - try markdownlint config
                log::debug!("[rumdl-config] No rumdl config found, checking markdownlint config");

                if let Some(markdownlint_path) = Self::discover_markdownlint_config_upward(home_dir) {
                    let path_str = markdownlint_path.display().to_string();
                    log::debug!("[rumdl-config] Found markdownlint config: {path_str}");
                    // Load user config first as a base so rumdl-specific settings (e.g. flavor,
                    // cache) take effect. Markdownlint configs cannot express these settings.
                    // The markdownlint fragment uses ConfigSource::ProjectConfig (precedence 3)
                    // vs UserConfig (precedence 1), so project settings always win on overlap.
                    Self::load_user_config(&mut sourced_config, user_config_dir, home_dir)?;
                    match parsers::load_from_markdownlint(&path_str) {
                        Ok(fragment) => {
                            sourced_config.merge(fragment);
                            sourced_config.loaded_files.push(path_str);
                        }
                        Err(_e) => {
                            log::debug!("[rumdl-config] Failed to load markdownlint config");
                        }
                    }
                } else {
                    // No project config at all - use user config as fallback
                    log::debug!("[rumdl-config] No project config found, using user config as fallback");
                    Self::load_user_config(&mut sourced_config, user_config_dir, home_dir)?;
                }
            }
        }

        // Apply CLI overrides (highest precedence)
        if let Some(cli) = cli_overrides {
            sourced_config
                .global
                .enable
                .merge_override(cli.enable.value.clone(), ConfigSource::Cli, None);
            sourced_config
                .global
                .disable
                .merge_override(cli.disable.value.clone(), ConfigSource::Cli, None);
            sourced_config
                .global
                .exclude
                .merge_override(cli.exclude.value.clone(), ConfigSource::Cli, None);
            sourced_config
                .global
                .include
                .merge_override(cli.include.value.clone(), ConfigSource::Cli, None);
            sourced_config.global.respect_gitignore.merge_override(
                cli.respect_gitignore.value,
                ConfigSource::Cli,
                None,
            );
            sourced_config
                .global
                .fixable
                .merge_override(cli.fixable.value.clone(), ConfigSource::Cli, None);
            sourced_config
                .global
                .unfixable
                .merge_override(cli.unfixable.value.clone(), ConfigSource::Cli, None);
            // No rule-specific CLI overrides implemented yet
        }

        // Unknown keys are now collected during parsing and validated via validate_config_sourced()

        Ok(sourced_config)
    }

    /// Load and merge configurations from files and CLI overrides.
    /// If skip_auto_discovery is true, only explicit config paths are loaded.
    pub fn load_with_discovery(
        config_path: Option<&str>,
        cli_overrides: Option<&SourcedGlobalConfig>,
        skip_auto_discovery: bool,
    ) -> Result<Self, ConfigError> {
        Self::load_with_discovery_impl(config_path, cli_overrides, skip_auto_discovery, None, None)
    }

    /// Validate the configuration against a rule registry.
    ///
    /// This method transitions the config from `ConfigLoaded` to `ConfigValidated` state,
    /// enabling conversion to `Config`. Validation warnings are stored in the config
    /// and can be displayed to the user.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let loaded = SourcedConfig::load_with_discovery(path, None, false)?;
    /// let validated = loaded.validate(&registry)?;
    /// let config: Config = validated.into();
    /// ```
    pub fn validate(self, registry: &RuleRegistry) -> Result<SourcedConfig<ConfigValidated>, ConfigError> {
        let warnings = validate_config_sourced_internal(&self, registry);

        Ok(SourcedConfig {
            global: self.global,
            per_file_ignores: self.per_file_ignores,
            per_file_flavor: self.per_file_flavor,
            code_block_tools: self.code_block_tools,
            rules: self.rules,
            loaded_files: self.loaded_files,
            unknown_keys: self.unknown_keys,
            project_root: self.project_root,
            discovery_warnings: self.discovery_warnings,
            validation_warnings: warnings,
            _state: PhantomData,
        })
    }

    /// Validate and convert to Config in one step (convenience method).
    ///
    /// This combines `validate()` and `into()` for callers who want the
    /// validation warnings separately.
    pub fn validate_into(self, registry: &RuleRegistry) -> Result<(Config, Vec<ConfigValidationWarning>), ConfigError> {
        let validated = self.validate(registry)?;
        let warnings = validated.validation_warnings.clone();
        Ok((validated.into(), warnings))
    }

    /// Skip validation and convert directly to ConfigValidated state.
    ///
    /// # Safety
    ///
    /// This method bypasses validation. Use only when:
    /// - You've already validated via `validate_config_sourced()`
    /// - You're in test code that doesn't need validation
    /// - You're migrating legacy code and will add proper validation later
    ///
    /// Prefer `validate()` for new code.
    pub fn into_validated_unchecked(self) -> SourcedConfig<ConfigValidated> {
        SourcedConfig {
            global: self.global,
            per_file_ignores: self.per_file_ignores,
            per_file_flavor: self.per_file_flavor,
            code_block_tools: self.code_block_tools,
            rules: self.rules,
            loaded_files: self.loaded_files,
            unknown_keys: self.unknown_keys,
            project_root: self.project_root,
            discovery_warnings: self.discovery_warnings,
            validation_warnings: Vec::new(),
            _state: PhantomData,
        }
    }

    /// Discover the nearest config file for a specific directory,
    /// walking upward to `project_root` (inclusive).
    ///
    /// Searches for rumdl config files (`.rumdl.toml`, `rumdl.toml`,
    /// `.config/rumdl.toml`, `pyproject.toml` with `[tool.rumdl]`) and
    /// markdownlint config files at each directory level.
    ///
    /// Returns the config file path if found. Does NOT use CWD.
    pub fn discover_config_for_dir(dir: &Path, project_root: &Path) -> Option<PathBuf> {
        // The walk never canonicalizes the directories it yields (symlinks and
        // Windows short names stay as the caller wrote them); only the stop
        // checks inside `UpwardWalk` compare canonically. A relative `dir` is
        // resolved against the current directory, so the returned config path
        // is always absolute.
        //
        // The home boundary keeps the walk from treating `~/.rumdl.toml` as a
        // project config, consistent with `discover_config_upward`. This only has
        // an effect when `project_root` is at or above the home directory (e.g. a
        // multi-path run whose grouping root spans the home boundary); for the
        // usual project root below home the walk stops there first.
        UpwardWalk::new(dir)
            .stop_below(Self::resolve_home_boundary(None))
            .stop_at(project_root)
            .find_map(|current| {
                // Check rumdl config files first (higher precedence)
                for config_name in RUMDL_CONFIG_FILES {
                    let config_path = current.join(config_name);
                    if config_path.exists() {
                        if *config_name == "pyproject.toml" {
                            if let Ok(content) = std::fs::read_to_string(&config_path)
                                && pyproject_declares_rumdl_config(&content)
                            {
                                return Some(config_path);
                            }
                            continue;
                        }
                        return Some(config_path);
                    }
                }

                // Check markdownlint config files (lower precedence)
                MARKDOWNLINT_CONFIG_FILES
                    .iter()
                    .map(|name| current.join(name))
                    .find(|path| path.exists())
            })
    }

    /// Load a config from a specific file path, with extends resolution, returning
    /// the still-`Loaded` `SourcedConfig` (before validation and conversion).
    ///
    /// Used by per-directory resolution so the caller can layer CLI-level overrides
    /// (e.g. inline `--config`) on top before converting to `Config`, matching the
    /// precedence applied to the global config.
    pub fn load_sourced_for_path(
        config_path: &Path,
        project_root: &Path,
    ) -> Result<SourcedConfig<ConfigLoaded>, ConfigError> {
        let mut sourced_config = SourcedConfig {
            project_root: Some(project_root.to_path_buf()),
            ..SourcedConfig::default()
        };

        let filename = config_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let path_str = config_path.display().to_string();

        // Determine if this is a markdownlint config or rumdl config
        let is_markdownlint = MARKDOWNLINT_CONFIG_FILES.contains(&filename)
            || (filename != "pyproject.toml"
                && filename != ".rumdl.toml"
                && filename != "rumdl.toml"
                && (path_str.ends_with(".json")
                    || path_str.ends_with(".jsonc")
                    || path_str.ends_with(".yaml")
                    || path_str.ends_with(".yml")));

        if is_markdownlint {
            let fragment = parsers::load_from_markdownlint(&path_str)?;
            sourced_config.merge(fragment);
            sourced_config.loaded_files.push(path_str);
        } else {
            let mut visited = IndexSet::new();
            let chain_source = source_from_filename(filename);
            load_config_with_extends(&mut sourced_config, config_path, &mut visited, chain_source)?;
        }

        Ok(sourced_config)
    }

    /// Load a config from a specific file path, with extends resolution, and convert
    /// to `Config`. Used for per-directory config loading where each subdirectory
    /// config is standalone.
    pub fn load_config_for_path(config_path: &Path, project_root: &Path) -> Result<Config, ConfigError> {
        Ok(Self::load_sourced_for_path(config_path, project_root)?
            .into_validated_unchecked()
            .into())
    }
}

/// Convert a validated configuration to the final Config type.
///
/// This implementation only exists for `SourcedConfig<ConfigValidated>`,
/// ensuring that validation must occur before conversion.
impl From<SourcedConfig<ConfigValidated>> for Config {
    fn from(sourced: SourcedConfig<ConfigValidated>) -> Self {
        let mut rules = BTreeMap::new();
        for (rule_name, sourced_rule_cfg) in sourced.rules {
            // Normalize rule name to uppercase for case-insensitive lookup
            let normalized_rule_name = rule_name.to_ascii_uppercase();
            let severity = sourced_rule_cfg.severity.map(|sv| sv.value);
            let mut values = BTreeMap::new();
            for (key, sourced_val) in sourced_rule_cfg.values {
                values.insert(key, sourced_val.value);
            }
            rules.insert(normalized_rule_name, RuleConfig { severity, values });
        }
        // Enable is "explicit" if it was set by something other than the Default source
        let enable_is_explicit = sourced.global.enable.source != ConfigSource::Default;

        #[allow(deprecated)]
        let global = GlobalConfig {
            enable: sourced.global.enable.value,
            disable: sourced.global.disable.value,
            exclude: sourced.global.exclude.value,
            include: sourced.global.include.value,
            respect_gitignore: sourced.global.respect_gitignore.value,
            line_length: sourced.global.line_length.value,
            output_format: sourced.global.output_format.as_ref().map(|v| v.value.clone()),
            fixable: sourced.global.fixable.value,
            unfixable: sourced.global.unfixable.value,
            flavor: sourced.global.flavor.value,
            force_exclude: sourced.global.force_exclude.value,
            cache_dir: sourced.global.cache_dir.as_ref().map(|v| v.value.clone()),
            cache: sourced.global.cache.value,
            extend_enable: sourced.global.extend_enable.value,
            extend_disable: sourced.global.extend_disable.value,
            enable_is_explicit,
        };

        let mut config = Config {
            extends: None,
            global,
            per_file_ignores: sourced.per_file_ignores.value,
            per_file_flavor: sourced.per_file_flavor.value,
            code_block_tools: sourced.code_block_tools.value,
            rules,
            project_root: sourced.project_root,
            per_file_ignores_cache: Arc::new(OnceLock::new()),
            per_file_flavor_cache: Arc::new(OnceLock::new()),
            canonical_project_root_cache: Arc::new(OnceLock::new()),
        };

        // Apply per-rule `enabled = true/false` to global enable/disable lists
        config.apply_per_rule_enabled();

        // Enforce the runtime invariant: every rule-name list is canonicalised.
        // After this point, downstream consumers (`rules::filter_rules`, the LSP,
        // WASM, fix coordinator, per-file-ignores) can match against
        // `Rule::name()` with simple string equality regardless of whether the
        // user's config used canonical IDs (`"MD033"`) or aliases
        // (`"no-inline-html"`).
        config.canonicalize_rule_lists();

        config
    }
}

#[cfg(test)]
mod tests {
    use super::pyproject_declares_rumdl_config;

    #[test]
    fn detects_flat_and_dotted_rumdl_sections() {
        assert!(pyproject_declares_rumdl_config("[tool.rumdl]\nline-length = 80\n"));
        // Dotted sections are valid on their own, without a flat header.
        assert!(pyproject_declares_rumdl_config(
            "[tool.rumdl.MD013]\nstyle = \"fixed\"\n"
        ));
        assert!(pyproject_declares_rumdl_config(
            "[tool.rumdl.rules.MD007]\nindent = 4\n"
        ));
    }

    #[test]
    fn ignores_incidental_mentions() {
        // A bare `tool.rumdl` in a comment or string value must not be treated
        // as a config section.
        assert!(!pyproject_declares_rumdl_config("# configure tool.rumdl later\n"));
        assert!(!pyproject_declares_rumdl_config(
            "[project]\ndependencies = [\"tool.rumdl-helper\"]\n"
        ));
        assert!(!pyproject_declares_rumdl_config("[tool.black]\nline-length = 88\n"));
    }

    /// Pure tests for the `$VAR` / `${VAR}` expander used by `extends` resolution.
    /// The injected `lookup` keeps these independent of the real process environment.
    mod expand_env_vars {
        use super::super::expand_env_vars;
        use std::collections::HashMap;

        /// Build a lookup closure from `(name, value)` pairs.
        fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
            let map: HashMap<String, String> = pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect();
            move |k: &str| map.get(k).cloned()
        }

        #[test]
        fn expands_bare_and_braced_forms() {
            let e = env(&[("VAR", "val"), ("FOO_BAR", "fb")]);
            assert_eq!(expand_env_vars("$VAR", &e).unwrap(), "val");
            assert_eq!(expand_env_vars("${VAR}", &e).unwrap(), "val");
            // Longest-match identifier: `$FOO_BAR` is one name, not `$FOO` + `_BAR`.
            assert_eq!(expand_env_vars("$FOO_BAR", &e).unwrap(), "fb");
        }

        #[test]
        fn expands_within_paths() {
            let e = env(&[("BASE", "/opt/cfg"), ("A", "x"), ("B", "y")]);
            assert_eq!(expand_env_vars("$BASE/x/y.toml", &e).unwrap(), "/opt/cfg/x/y.toml");
            assert_eq!(expand_env_vars("$A/$B", &e).unwrap(), "x/y");
            assert_eq!(expand_env_vars("${A}suffix", &e).unwrap(), "xsuffix");
        }

        #[test]
        fn dollar_dollar_is_a_literal_dollar() {
            let e = env(&[("VAR", "val")]);
            assert_eq!(expand_env_vars("$$", &e).unwrap(), "$");
            // The escaped `$` is consumed; what follows is literal (not expanded).
            assert_eq!(expand_env_vars("$$VAR", &e).unwrap(), "$VAR");
            assert_eq!(expand_env_vars("$${VAR}", &e).unwrap(), "${VAR}");
            // `$$` is how a literal `$` in a path is written once this feature exists.
            assert_eq!(expand_env_vars("file-$$name.toml", &e).unwrap(), "file-$name.toml");
        }

        #[test]
        fn bare_dollar_name_in_path_is_a_variable_reference() {
            // Documented behavior change: an unescaped `$name` in a path is a variable,
            // not a literal. `$$` writes a literal `$` (see dollar_dollar test above).
            let e = env(&[("name", "core")]);
            assert_eq!(expand_env_vars("file-$name.toml", &e).unwrap(), "file-core.toml");
        }

        #[test]
        fn incidental_dollar_stays_literal() {
            let e = env(&[]);
            // `$` before a non-identifier-start char (or end of input) is literal.
            assert_eq!(expand_env_vars("$5", &e).unwrap(), "$5");
            assert_eq!(expand_env_vars("cost$", &e).unwrap(), "cost$");
            assert_eq!(expand_env_vars("a$/b", &e).unwrap(), "a$/b");
        }

        #[test]
        fn malformed_braces_stay_literal() {
            let e = env(&[("B", "x")]);
            assert_eq!(expand_env_vars("${}", &e).unwrap(), "${}");
            assert_eq!(expand_env_vars("${VAR", &e).unwrap(), "${VAR");
            // Nested `${...}` is not supported: the whole span is literal, no partial expand.
            assert_eq!(expand_env_vars("${A${B}}", &e).unwrap(), "${A${B}}");
        }

        #[test]
        fn undefined_variable_is_an_error() {
            let e = env(&[]);
            assert_eq!(expand_env_vars("$NOPE", &e).unwrap_err(), "NOPE");
            assert_eq!(expand_env_vars("${NOPE}", &e).unwrap_err(), "NOPE");
            assert_eq!(expand_env_vars("prefix/$NOPE/x", &e).unwrap_err(), "NOPE");
        }

        #[test]
        fn replacement_is_not_rescanned() {
            // If `A` expands to "$B", the result is the literal "$B"; `B` is NOT expanded.
            let e = env(&[("A", "$B"), ("B", "should-not-appear")]);
            assert_eq!(expand_env_vars("$A", &e).unwrap(), "$B");
            assert_eq!(expand_env_vars("${A}", &e).unwrap(), "$B");
        }

        #[test]
        fn identifiers_are_ascii_only_unicode_stays_literal() {
            let e = env(&[("VAR", "v")]);
            // Non-ASCII inside braces is not a valid identifier -> whole span literal.
            assert_eq!(expand_env_vars("${föö}", &e).unwrap(), "${föö}");
            // A name ends at the first non-identifier byte; trailing unicode is preserved.
            assert_eq!(expand_env_vars("$VARö", &e).unwrap(), "vö");
            // Literal runs preserve multibyte content around an expansion.
            assert_eq!(expand_env_vars("café/$VAR", &e).unwrap(), "café/v");
        }

        #[test]
        fn passthrough_for_plain_input() {
            let e = env(&[]);
            assert_eq!(expand_env_vars("", &e).unwrap(), "");
            assert_eq!(expand_env_vars("/plain/path.toml", &e).unwrap(), "/plain/path.toml");
        }
    }

    /// Discovery must stop at the project root even when the root is supplied in
    /// a different path representation than the walked directory's ancestors.
    ///
    /// This reproduces the Windows 8.3-short-name / canonical mismatch using a
    /// Unix symlink: the project root is passed as a symlink to the real root, so
    /// it does not string-match the canonical ancestors of the starting
    /// directory. Without canonicalization the walk overshoots the project root
    /// and incorrectly picks up the config in the parent directory.
    #[cfg(unix)]
    #[test]
    fn discover_stops_at_project_root_across_path_representations() {
        use super::SourcedConfig;
        use std::os::unix::fs::symlink;
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        // A config ABOVE the project root that must never be discovered.
        std::fs::write(tmp.path().join(".rumdl.toml"), "").unwrap();

        let real_root = tmp.path().join("project");
        let subdir = real_root.join("docs");
        std::fs::create_dir_all(&subdir).unwrap();

        // Supply the project root via a symlink so it does not string-match the
        // canonical ancestors of `subdir`.
        let linked_root = tmp.path().join("project-link");
        symlink(&real_root, &linked_root).unwrap();

        let found = SourcedConfig::discover_config_for_dir(&subdir, &linked_root);
        assert_eq!(
            found, None,
            "discovery must stop at the project root, not overshoot to the parent config"
        );
    }

    mod shadowed_configs {
        use super::super::{ShadowedConfigs, detect_shadowed_configs, format_shadow_warning, rumdl_configs_in_dir};
        use tempfile::tempdir;

        fn names(paths: &[std::path::PathBuf]) -> Vec<String> {
            paths
                .iter()
                .map(|p| {
                    // Use the last two components so `.config/rumdl.toml` is distinguishable
                    // from a top-level `rumdl.toml` without depending on the temp dir prefix.
                    let file = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                    let parent = p.parent().and_then(|d| d.file_name()).and_then(|n| n.to_str());
                    match parent {
                        Some(".config") => format!(".config/{file}"),
                        _ => file.to_string(),
                    }
                })
                .collect()
        }

        #[test]
        fn empty_directory_has_no_configs_and_no_shadow() {
            let tmp = tempdir().unwrap();
            assert!(rumdl_configs_in_dir(tmp.path()).is_empty());
            assert!(detect_shadowed_configs(tmp.path()).is_none());
        }

        #[test]
        fn single_config_does_not_shadow() {
            let tmp = tempdir().unwrap();
            std::fs::write(tmp.path().join(".rumdl.toml"), "").unwrap();
            assert_eq!(names(&rumdl_configs_in_dir(tmp.path())), vec![".rumdl.toml"]);
            assert!(detect_shadowed_configs(tmp.path()).is_none());
        }

        #[test]
        fn dot_wins_over_non_dot_and_non_dot_is_shadowed() {
            let tmp = tempdir().unwrap();
            std::fs::write(tmp.path().join(".rumdl.toml"), "").unwrap();
            std::fs::write(tmp.path().join("rumdl.toml"), "").unwrap();

            let ShadowedConfigs { winner, shadowed, .. } = detect_shadowed_configs(tmp.path()).unwrap();
            assert_eq!(names(&[winner]), vec![".rumdl.toml"]);
            assert_eq!(names(&shadowed), vec!["rumdl.toml"]);
        }

        #[test]
        fn config_subdir_counts_as_same_level_shadow() {
            let tmp = tempdir().unwrap();
            std::fs::write(tmp.path().join(".rumdl.toml"), "").unwrap();
            std::fs::create_dir_all(tmp.path().join(".config")).unwrap();
            std::fs::write(tmp.path().join(".config/rumdl.toml"), "").unwrap();

            let ShadowedConfigs { winner, shadowed, .. } = detect_shadowed_configs(tmp.path()).unwrap();
            assert_eq!(names(&[winner]), vec![".rumdl.toml"]);
            assert_eq!(names(&shadowed), vec![".config/rumdl.toml"]);
        }

        #[test]
        fn pyproject_counts_only_when_it_declares_rumdl() {
            // pyproject WITHOUT [tool.rumdl] is not a rumdl config source -> no shadow.
            let bare = tempdir().unwrap();
            std::fs::write(bare.path().join(".rumdl.toml"), "").unwrap();
            std::fs::write(bare.path().join("pyproject.toml"), "[tool.black]\nline-length = 88\n").unwrap();
            assert_eq!(names(&rumdl_configs_in_dir(bare.path())), vec![".rumdl.toml"]);
            assert!(detect_shadowed_configs(bare.path()).is_none());

            // pyproject WITH [tool.rumdl] is a real shadowed source.
            let declared = tempdir().unwrap();
            std::fs::write(declared.path().join(".rumdl.toml"), "").unwrap();
            std::fs::write(
                declared.path().join("pyproject.toml"),
                "[tool.rumdl]\nline-length = 80\n",
            )
            .unwrap();
            let ShadowedConfigs { winner, shadowed, .. } = detect_shadowed_configs(declared.path()).unwrap();
            assert_eq!(names(&[winner]), vec![".rumdl.toml"]);
            assert_eq!(names(&shadowed), vec!["pyproject.toml"]);
        }

        #[test]
        fn markdownlint_configs_are_not_rumdl_native_and_never_shadow() {
            let tmp = tempdir().unwrap();
            std::fs::write(tmp.path().join(".rumdl.toml"), "").unwrap();
            std::fs::write(tmp.path().join(".markdownlint.json"), "{}").unwrap();
            assert_eq!(names(&rumdl_configs_in_dir(tmp.path())), vec![".rumdl.toml"]);
            assert!(detect_shadowed_configs(tmp.path()).is_none());
        }

        #[test]
        fn configs_returned_in_precedence_order() {
            let tmp = tempdir().unwrap();
            std::fs::write(tmp.path().join(".rumdl.toml"), "").unwrap();
            std::fs::write(tmp.path().join("rumdl.toml"), "").unwrap();
            std::fs::create_dir_all(tmp.path().join(".config")).unwrap();
            std::fs::write(tmp.path().join(".config/rumdl.toml"), "").unwrap();
            std::fs::write(tmp.path().join("pyproject.toml"), "[tool.rumdl]\n").unwrap();

            assert_eq!(
                names(&rumdl_configs_in_dir(tmp.path())),
                vec![".rumdl.toml", "rumdl.toml", ".config/rumdl.toml", "pyproject.toml"]
            );
        }

        #[test]
        fn warning_names_dir_once_with_relative_filenames() {
            let tmp = tempdir().unwrap();
            std::fs::write(tmp.path().join(".rumdl.toml"), "").unwrap();
            std::fs::write(tmp.path().join("rumdl.toml"), "").unwrap();
            std::fs::write(tmp.path().join("pyproject.toml"), "[tool.rumdl]\n").unwrap();

            let shadow = detect_shadowed_configs(tmp.path()).unwrap();
            let msg = format_shadow_warning(&shadow);

            let dir = {
                let s = tmp.path().to_string_lossy().into_owned();
                if cfg!(windows) { s.replace('\\', "/") } else { s }
            };
            assert!(msg.contains("multiple rumdl config files"), "got: {msg}");
            // The directory is named once; files are shown relative to it (no
            // repeated directory prefix on every path).
            assert_eq!(
                msg.matches(dir.as_str()).count(),
                1,
                "directory should appear exactly once, got: {msg}"
            );
            assert!(
                msg.contains("using .rumdl.toml, ignoring rumdl.toml, pyproject.toml"),
                "winner and shadowed files should be relative names in precedence order, got: {msg}"
            );
            // Paths are normalized to forward slashes on all platforms.
            assert!(!msg.contains('\\'), "paths must be normalized to '/': {msg}");
        }
    }
}
