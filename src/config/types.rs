use crate::types::LineLength;
use globset::{Glob, GlobBuilder, GlobMatcher, GlobSet, GlobSetBuilder};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use super::flavor::{MarkdownFlavor, normalize_key};

/// Represents a rule-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, schemars::JsonSchema)]
pub struct RuleConfig {
    /// Severity override for this rule (Error, Warning, or Info)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<crate::rule::Severity>,

    /// Configuration values for the rule
    #[serde(flatten)]
    #[schemars(schema_with = "arbitrary_value_schema")]
    pub values: BTreeMap<String, toml::Value>,
}

/// Generate a JSON schema for arbitrary configuration values
fn arbitrary_value_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": true
    })
}

/// Represents the complete configuration loaded from rumdl.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[schemars(
    description = "rumdl configuration for linting Markdown files. Rules can be configured individually using [MD###] sections with rule-specific options."
)]
pub struct Config {
    /// Path to a base config file to inherit settings from.
    /// Supports relative paths, absolute paths, and `~/` for home directory.
    /// Example: `extends = "../base.rumdl.toml"`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,

    /// Global configuration options
    #[serde(default)]
    pub global: GlobalConfig,

    /// Per-file rule ignores: maps file patterns to lists of rules to ignore
    /// Example: { "README.md": ["MD033"], "docs/**/*.md": ["MD013"] }
    #[serde(default, rename = "per-file-ignores")]
    pub per_file_ignores: BTreeMap<String, Vec<String>>,

    /// Per-file flavor overrides: maps file patterns to Markdown flavors
    /// Example: { "docs/**/*.md": MkDocs, "**/*.mdx": MDX }
    /// Uses IndexMap to preserve config file order for "first match wins" semantics
    #[serde(default, rename = "per-file-flavor")]
    #[schemars(with = "BTreeMap<String, MarkdownFlavor>")]
    pub per_file_flavor: IndexMap<String, MarkdownFlavor>,

    /// Code block tools configuration for per-language linting and formatting
    /// using external tools like ruff, prettier, shellcheck, etc.
    #[serde(default, rename = "code-block-tools")]
    pub code_block_tools: crate::code_block_tools::CodeBlockToolsConfig,

    /// Rule-specific configurations (e.g., MD013, MD007, MD044)
    /// Each rule section can contain options specific to that rule.
    ///
    /// Common examples:
    /// - MD013: line_length, code_blocks, tables, headings
    /// - MD007: indent
    /// - MD003: style ("atx", "atx-closed", "setext")
    /// - MD044: names (array of proper names to check)
    ///
    /// See <https://github.com/rvben/rumdl> for full rule documentation.
    #[serde(flatten)]
    pub rules: BTreeMap<String, RuleConfig>,

    /// Project root directory, used for resolving relative paths in per-file-ignores
    #[serde(skip)]
    pub project_root: Option<std::path::PathBuf>,

    #[serde(skip)]
    #[schemars(skip)]
    pub(super) per_file_ignores_cache: Arc<OnceLock<PerFileIgnoreCache>>,

    #[serde(skip)]
    #[schemars(skip)]
    pub(super) per_file_flavor_cache: Arc<OnceLock<PerFileFlavorCache>>,

    /// Lazily-computed canonical form of `project_root`.
    ///
    /// `normalize_match_path` needs the canonical project root to strip
    /// prefixes from absolute file paths. Without this cache, every per-file
    /// lookup would re-canonicalize the project root (one syscall per file).
    ///
    /// ## Invariants
    ///
    /// - **Single-shot**: computed once on first use of [`Config::canonical_project_root`].
    /// - **Never invalidated**: callers must not mutate `project_root` after
    ///   the first call. `Config` is treated as immutable post-construction
    ///   (the same assumption as `per_file_ignores_cache` and `per_file_flavor_cache`).
    /// - **Construction-time existence**: the cache stores `None` if
    ///   `project_root` is unset, missing on disk, or otherwise can't be
    ///   canonicalized. In practice `project_root` is set after walking up to
    ///   `.git`, so the directory always exists at the time the cache is first
    ///   read; if a caller sets `project_root` to a not-yet-existing path,
    ///   the cache will permanently store `None`.
    /// - **`Arc` wrapping**: `Config` derives `Clone`, and clones share the
    ///   same `OnceLock` so a value computed by one clone is observable to all.
    ///
    /// `cwd` deliberately is NOT cached symmetrically: callers read it fresh
    /// from `std::env::current_dir()` per call because tests (and embedding
    /// hosts like LSP servers) may legitimately mutate the process cwd
    /// between lookups.
    #[serde(skip)]
    #[schemars(skip)]
    pub(super) canonical_project_root_cache: Arc<OnceLock<Option<PathBuf>>>,
}

impl PartialEq for Config {
    fn eq(&self, other: &Self) -> bool {
        self.global == other.global
            && self.per_file_ignores == other.per_file_ignores
            && self.per_file_flavor == other.per_file_flavor
            && self.code_block_tools == other.code_block_tools
            && self.rules == other.rules
            && self.project_root == other.project_root
    }
}

#[derive(Debug)]
pub(super) struct PerFileIgnoreCache {
    globset: GlobSet,
    rules: Vec<Vec<String>>,
}

#[derive(Debug)]
pub(super) struct PerFileFlavorCache {
    matchers: Vec<(GlobMatcher, MarkdownFlavor)>,
}

impl Config {
    /// Check if the Markdown flavor is set to MkDocs
    pub fn is_mkdocs_flavor(&self) -> bool {
        self.global.flavor == MarkdownFlavor::MkDocs
    }

    // Future methods for when GFM and CommonMark are implemented:
    // pub fn is_gfm_flavor(&self) -> bool
    // pub fn is_commonmark_flavor(&self) -> bool

    /// Get the configured Markdown flavor
    pub fn markdown_flavor(&self) -> MarkdownFlavor {
        self.global.flavor
    }

    /// Legacy method for backwards compatibility - redirects to is_mkdocs_flavor
    pub fn is_mkdocs_project(&self) -> bool {
        self.is_mkdocs_flavor()
    }

    /// Apply per-rule `enabled` config to the global enable/disable lists.
    ///
    /// For `[MD060] enabled = true`: adds the rule to `extend_enable` and
    /// removes it from `disable` and `extend_disable`, ensuring the rule is active.
    ///
    /// For `[MD041] enabled = false`: adds the rule to `disable` and
    /// removes it from `extend_enable`, ensuring the rule is inactive.
    ///
    /// Per-rule `enabled` takes precedence over global lists when there
    /// is a conflict, since it represents a more specific intent.
    pub fn apply_per_rule_enabled(&mut self) {
        let mut to_enable: Vec<String> = Vec::new();
        let mut to_disable: Vec<String> = Vec::new();

        for (name, cfg) in &self.rules {
            match cfg.values.get("enabled") {
                Some(toml::Value::Boolean(true)) => {
                    to_enable.push(name.clone());
                }
                Some(toml::Value::Boolean(false)) => {
                    to_disable.push(name.clone());
                }
                _ => {}
            }
        }

        for name in to_enable {
            if !self.global.extend_enable.contains(&name) {
                self.global.extend_enable.push(name.clone());
            }
            self.global.disable.retain(|n| n != &name);
            self.global.extend_disable.retain(|n| n != &name);
        }

        for name in to_disable {
            if !self.global.disable.contains(&name) {
                self.global.disable.push(name.clone());
            }
            self.global.extend_enable.retain(|n| n != &name);
        }
    }

    /// Get the severity override for a specific rule, if configured
    pub fn get_rule_severity(&self, rule_name: &str) -> Option<crate::rule::Severity> {
        self.rules.get(rule_name).and_then(|r| r.severity)
    }

    /// Return the canonical form of `project_root`, computed once and cached.
    ///
    /// Returns `None` if `project_root` is unset, doesn't exist on disk, or
    /// otherwise cannot be canonicalized. Subsequent calls reuse the cached
    /// value, eliminating the per-file `canonicalize()` syscall that
    /// `normalize_match_path` would otherwise perform.
    pub(super) fn canonical_project_root(&self) -> Option<&Path> {
        self.canonical_project_root_cache
            .get_or_init(|| self.project_root.as_deref().and_then(|p| p.canonicalize().ok()))
            .as_deref()
    }

    /// Get the set of rules that should be ignored for a specific file based on per-file-ignores configuration
    /// Returns a HashSet of rule names (uppercase, e.g., "MD033") that match the given file path
    pub fn get_ignored_rules_for_file(&self, file_path: &Path) -> HashSet<String> {
        let mut ignored_rules = HashSet::new();

        if self.per_file_ignores.is_empty() {
            return ignored_rules;
        }

        let cwd = std::env::current_dir().ok();
        let path_for_matching = normalize_match_path(file_path, self.canonical_project_root(), cwd.as_deref());

        let cache = self
            .per_file_ignores_cache
            .get_or_init(|| PerFileIgnoreCache::new(&self.per_file_ignores));

        // Match the file path against all patterns
        for match_idx in cache.globset.matches(path_for_matching.as_ref()) {
            if let Some(rules) = cache.rules.get(match_idx) {
                for rule in rules {
                    // Normalize rule names to uppercase (MD033, md033 -> MD033)
                    ignored_rules.insert(rule.clone());
                }
            }
        }

        ignored_rules
    }

    /// Get the MarkdownFlavor for a specific file based on per-file-flavor configuration.
    /// Returns the first matching pattern's flavor, or falls back to global flavor,
    /// or auto-detects from extension, or defaults to Standard.
    pub fn get_flavor_for_file(&self, file_path: &Path) -> MarkdownFlavor {
        // If no per-file patterns, use fallback logic
        if self.per_file_flavor.is_empty() {
            return self.resolve_flavor_fallback(file_path);
        }

        let cwd = std::env::current_dir().ok();
        let path_for_matching = normalize_match_path(file_path, self.canonical_project_root(), cwd.as_deref());

        let cache = self
            .per_file_flavor_cache
            .get_or_init(|| PerFileFlavorCache::new(&self.per_file_flavor));

        // Iterate in config order and return first match (IndexMap preserves order)
        for (matcher, flavor) in &cache.matchers {
            if matcher.is_match(path_for_matching.as_ref()) {
                return *flavor;
            }
        }

        // No pattern matched, use fallback
        self.resolve_flavor_fallback(file_path)
    }

    /// Fallback flavor resolution: global flavor → auto-detect → Standard
    fn resolve_flavor_fallback(&self, file_path: &Path) -> MarkdownFlavor {
        // If global flavor is explicitly set to non-Standard, use it
        if self.global.flavor != MarkdownFlavor::Standard {
            return self.global.flavor;
        }
        // Auto-detect from extension
        MarkdownFlavor::from_path(file_path)
    }

    /// Canonicalize every rule-name list inside this `Config`.
    ///
    /// This is the single enforcement point for the runtime invariant:
    /// **after a `Config` is fully built, every rule-name list contains
    /// canonical rule IDs (`"MD033"`) — never aliases (`"no-inline-html"`).**
    ///
    /// The invariant lets every consumer (`rules::filter_rules`, the LSP,
    /// WASM, fix coordinator, per-file-ignore lookups) match against
    /// `Rule::name()` with simple string equality. Mutation boundaries
    /// (`From<SourcedConfig> for Config`, LSP `apply_lsp_settings_*`, WASM
    /// `to_config_with_warnings`) call this before handing the `Config` to
    /// the linting pipeline.
    ///
    /// Covers `global.{enable,disable,extend_enable,extend_disable,fixable,unfixable}`
    /// and the values of `per_file_ignores`. Idempotent.
    pub fn canonicalize_rule_lists(&mut self) {
        use super::registry::canonicalize_rule_list_in_place;
        self.global.canonicalize_rule_lists();
        for rules in self.per_file_ignores.values_mut() {
            canonicalize_rule_list_in_place(rules);
        }
    }

    /// Merge inline configuration overrides into a copy of this config
    ///
    /// This enables automatic inline config support - the engine can merge
    /// inline overrides and recreate rules without any per-rule changes.
    ///
    /// Returns a new Config with the inline overrides merged in.
    /// If there are no inline overrides, returns a clone of self.
    pub fn merge_with_inline_config(&self, inline_config: &crate::inline_config::InlineConfig) -> Self {
        let overrides = inline_config.get_all_rule_configs();
        if overrides.is_empty() {
            return self.clone();
        }

        let mut merged = self.clone();

        for (rule_name, json_override) in overrides {
            // Get or create the rule config entry
            let rule_config = merged.rules.entry(rule_name.clone()).or_default();

            // Merge JSON values into the rule's config
            if let Some(obj) = json_override.as_object() {
                for (key, value) in obj {
                    // Normalize key to kebab-case for consistency
                    let normalized_key = key.replace('_', "-");

                    // Convert JSON value to TOML value
                    if let Some(toml_value) = json_to_toml(value) {
                        rule_config.values.insert(normalized_key, toml_value);
                    }
                }
            }
        }

        merged
    }
}

/// Normalize a file path for matching against a glob pattern from configuration.
///
/// Glob patterns in `per-file-ignores` and `per-file-flavor` are written relative
/// to the project root (e.g. `docs/**/*.md`), and the underlying matcher uses
/// `literal_separator(true)` so an absolute path like `/home/user/proj/docs/x.md`
/// will not match `docs/**/*.md`. This helper produces the form the glob expects:
///
/// 1. **Relative path** → return as-is.
/// 2. **Absolute path under `project_root`** → return path relative to `project_root`.
/// 3. **Absolute path under `cwd`** → return path relative to `cwd`. This is the
///    safety net for invocations where `project_root` could not be discovered
///    (no `.git` upward, LSP/CLI calls outside a project) but the file still
///    lives somewhere under the working directory.
/// 4. **Anywhere else** → return the raw path. A relative glob simply won't
///    match it, which is the desired outcome for files outside any known root.
///
/// All canonicalization failures degrade gracefully to step 4 so editor buffers
/// and pre-creation paths still flow through without panicking.
///
/// `canonical_project_root` is expected to already be canonical (via
/// `Config::canonical_project_root`). `cwd` is canonicalized internally on each
/// call since it is read fresh from the environment per invocation.
pub(super) fn normalize_match_path<'a>(
    file_path: &'a Path,
    canonical_project_root: Option<&Path>,
    cwd: Option<&Path>,
) -> std::borrow::Cow<'a, Path> {
    use std::borrow::Cow;

    if file_path.is_relative() {
        return Cow::Borrowed(file_path);
    }

    let Ok(canonical_file) = file_path.canonicalize() else {
        log::debug!(
            "normalize_match_path: canonicalize failed for {}; returning raw path. \
             Per-file glob patterns may not match (file may not yet exist on disk).",
            file_path.display()
        );
        return Cow::Borrowed(file_path);
    };

    if let Some(root) = canonical_project_root
        && let Ok(rel) = canonical_file.strip_prefix(root)
    {
        return Cow::Owned(rel.to_path_buf());
    }

    if let Some(working_dir) = cwd
        && let Ok(canonical_cwd) = working_dir.canonicalize()
        && let Ok(rel) = canonical_file.strip_prefix(&canonical_cwd)
    {
        return Cow::Owned(rel.to_path_buf());
    }

    // Surface the silent fallback once per process at warn level so users with
    // per-file glob configs notice when their patterns can't match a file.
    // Subsequent occurrences stay at debug to avoid log spam.
    static SILENT_FALLBACK_WARNED: OnceLock<()> = OnceLock::new();
    log::log!(
        first_call_warn_else_debug(&SILENT_FALLBACK_WARNED),
        "{}",
        format_silent_fallback_message(file_path, canonical_project_root, cwd),
    );
    Cow::Borrowed(file_path)
}

/// Returns [`log::Level::Warn`] the first time it is called with a given
/// `latch`, and [`log::Level::Debug`] on every subsequent call. The latch
/// is consumed by the first caller via `OnceLock::set`; later callers
/// observe the latch as already set and downgrade.
///
/// Used to flag a fallback condition once per process without flooding
/// logs when the same condition recurs (e.g. once per linted file).
pub(super) fn first_call_warn_else_debug(latch: &OnceLock<()>) -> log::Level {
    if latch.set(()).is_ok() {
        log::Level::Warn
    } else {
        log::Level::Debug
    }
}

/// Format the diagnostic emitted when [`normalize_match_path`] cannot
/// relativise `file_path` against either the project root or the current
/// working directory. Extracted so the exact wording can be asserted in
/// tests without capturing log output.
pub(super) fn format_silent_fallback_message(
    file_path: &Path,
    canonical_project_root: Option<&Path>,
    cwd: Option<&Path>,
) -> String {
    format!(
        "Per-file glob patterns will not match {}: file is outside project_root ({}) and cwd ({})",
        file_path.display(),
        DisplayPathOrUnset(canonical_project_root),
        DisplayPathOrUnset(cwd),
    )
}

/// Display adapter for `Option<&Path>` that renders the path via
/// [`Path::display`] when present, or the literal `<unset>` when absent.
/// Angle brackets follow Rust's diagnostic convention (e.g. `<unknown>`)
/// and avoid double-paren rendering when the surrounding format string
/// already wraps the value in `(…)`.
struct DisplayPathOrUnset<'a>(Option<&'a Path>);

impl std::fmt::Display for DisplayPathOrUnset<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(path) => std::fmt::Display::fmt(&path.display(), f),
            None => f.write_str("<unset>"),
        }
    }
}

/// Convert a serde_json::Value to a toml::Value
pub(super) fn json_to_toml(json: &serde_json::Value) -> Option<toml::Value> {
    match json {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(b) => Some(toml::Value::Boolean(*b)),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(toml::Value::Integer)
            .or_else(|| n.as_f64().map(toml::Value::Float)),
        serde_json::Value::String(s) => Some(toml::Value::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let toml_arr: Vec<toml::Value> = arr.iter().filter_map(json_to_toml).collect();
            Some(toml::Value::Array(toml_arr))
        }
        serde_json::Value::Object(obj) => {
            let mut table = toml::map::Map::new();
            for (k, v) in obj {
                if let Some(tv) = json_to_toml(v) {
                    table.insert(k.clone(), tv);
                }
            }
            Some(toml::Value::Table(table))
        }
    }
}

impl PerFileIgnoreCache {
    fn new(per_file_ignores: &BTreeMap<String, Vec<String>>) -> Self {
        let mut builder = GlobSetBuilder::new();
        let mut rules = Vec::new();

        for (pattern, rules_list) in per_file_ignores {
            if let Ok(glob) = Glob::new(pattern) {
                builder.add(glob);
                // Canonicalize defensively: callers should have run
                // Config::canonicalize_rule_lists already, but per-file-ignores
                // has reached this cache directly from a few code paths
                // historically, so we re-canonicalize here to keep the cache
                // sound regardless of caller discipline.
                rules.push(
                    rules_list
                        .iter()
                        .map(|rule| super::registry::resolve_rule_name(rule))
                        .collect(),
                );
            } else {
                log::warn!("Invalid glob pattern in per-file-ignores: {pattern}");
            }
        }

        let globset = builder.build().unwrap_or_else(|e| {
            log::error!("Failed to build globset for per-file-ignores: {e}");
            GlobSetBuilder::new().build().unwrap()
        });

        Self { globset, rules }
    }
}

impl PerFileFlavorCache {
    fn new(per_file_flavor: &IndexMap<String, MarkdownFlavor>) -> Self {
        let mut matchers = Vec::new();

        for (pattern, flavor) in per_file_flavor {
            if let Ok(glob) = GlobBuilder::new(pattern).literal_separator(true).build() {
                matchers.push((glob.compile_matcher(), *flavor));
            } else {
                log::warn!("Invalid glob pattern in per-file-flavor: {pattern}");
            }
        }

        Self { matchers }
    }
}

/// Global configuration options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(default, rename_all = "kebab-case")]
pub struct GlobalConfig {
    /// Enabled rules
    #[serde(default)]
    pub enable: Vec<String>,

    /// Disabled rules
    #[serde(default)]
    pub disable: Vec<String>,

    /// Files to exclude
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Files to include
    #[serde(default)]
    pub include: Vec<String>,

    /// Respect .gitignore files when scanning directories
    #[serde(default = "default_respect_gitignore", alias = "respect_gitignore")]
    pub respect_gitignore: bool,

    /// Global line length setting (used by MD013 and other rules if not overridden)
    #[serde(default, alias = "line_length")]
    pub line_length: LineLength,

    /// Output format for linting results (e.g., "text", "json", "pylint", etc.)
    #[serde(skip_serializing_if = "Option::is_none", alias = "output_format")]
    pub output_format: Option<String>,

    /// Rules that are allowed to be fixed when --fix is used
    /// If specified, only these rules will be fixed
    #[serde(default)]
    pub fixable: Vec<String>,

    /// Rules that should never be fixed, even when --fix is used
    /// Takes precedence over fixable
    #[serde(default)]
    pub unfixable: Vec<String>,

    /// Markdown flavor/dialect to use (mkdocs, gfm, commonmark, etc.)
    /// When set, adjusts parsing and validation rules for that specific Markdown variant
    #[serde(default)]
    pub flavor: MarkdownFlavor,

    /// \[DEPRECATED\] Whether to enforce exclude patterns for explicitly passed paths.
    /// This option is deprecated as of v0.0.156 and has no effect.
    /// Exclude patterns are now always respected, even for explicitly provided files.
    /// This prevents duplication between rumdl config and tool configs like pre-commit.
    #[serde(default, alias = "force_exclude")]
    #[deprecated(since = "0.0.156", note = "Exclude patterns are now always respected")]
    pub force_exclude: bool,

    /// Directory to store cache files (default: .rumdl_cache)
    /// Can also be set via --cache-dir CLI flag or RUMDL_CACHE_DIR environment variable
    #[serde(default, alias = "cache_dir", skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<String>,

    /// Whether caching is enabled (default: true)
    /// Can also be disabled via --no-cache CLI flag
    #[serde(default = "default_true")]
    pub cache: bool,

    /// Additional rules to enable on top of the base set (additive)
    #[serde(default, alias = "extend_enable")]
    pub extend_enable: Vec<String>,

    /// Additional rules to disable on top of the base set (additive)
    #[serde(default, alias = "extend_disable")]
    pub extend_disable: Vec<String>,

    /// Whether the enable list was explicitly set (even if empty).
    /// Used to distinguish "no enable list configured" from "enable list is empty"
    /// (e.g., markdownlint `default: false` with no rules enabled).
    #[serde(skip)]
    pub enable_is_explicit: bool,
}

fn default_respect_gitignore() -> bool {
    true
}

fn default_true() -> bool {
    true
}

// Add the Default impl
impl Default for GlobalConfig {
    #[allow(deprecated)]
    fn default() -> Self {
        Self {
            enable: Vec::new(),
            disable: Vec::new(),
            exclude: Vec::new(),
            include: Vec::new(),
            respect_gitignore: true,
            line_length: LineLength::default(),
            output_format: None,
            fixable: Vec::new(),
            unfixable: Vec::new(),
            flavor: MarkdownFlavor::default(),
            force_exclude: false,
            cache_dir: None,
            cache: true,
            extend_enable: Vec::new(),
            extend_disable: Vec::new(),
            enable_is_explicit: false,
        }
    }
}

impl GlobalConfig {
    /// Canonicalize every rule-name list in this `GlobalConfig`.
    ///
    /// Rewrites `enable`, `disable`, `extend_enable`, `extend_disable`, `fixable`,
    /// and `unfixable` so that all entries are canonical rule IDs (`"MD033"`)
    /// rather than aliases (`"no-inline-html"`). Duplicates are removed,
    /// preserving first-occurrence order; the special `"all"` keyword is
    /// preserved.
    ///
    /// This must be called by every code path that mutates a runtime
    /// `Config`'s rule lists from external input (markdownlint configs,
    /// `.rumdl.toml`, LSP `initializationOptions`, WASM bindings, etc.) so
    /// that downstream consumers (`rules::filter_rules`, the LSP, WASM) can
    /// match against `Rule::name()` with simple string equality.
    pub fn canonicalize_rule_lists(&mut self) {
        use super::registry::canonicalize_rule_list_in_place;
        canonicalize_rule_list_in_place(&mut self.enable);
        canonicalize_rule_list_in_place(&mut self.disable);
        canonicalize_rule_list_in_place(&mut self.extend_enable);
        canonicalize_rule_list_in_place(&mut self.extend_disable);
        canonicalize_rule_list_in_place(&mut self.fixable);
        canonicalize_rule_list_in_place(&mut self.unfixable);
    }
}

/// Names of rumdl-native config files, searched in precedence order when
/// walking up a directory tree.
///
/// This is the single source of truth for config discovery. Both the CLI
/// (`SourcedConfig::discover_config_upward`, `discover_config_for_dir`) and
/// the LSP (`RumdlLanguageServer::resolve_config_for_file`) must use this
/// list; any deviation causes silent config-not-found bugs where the CLI
/// recognises a config but the LSP does not (or vice versa).
///
/// See `src/lsp/tests.rs::test_lsp_cli_resolver_parity_on_fixtures` for
/// the side-by-side resolver parity test that pins this invariant across
/// several directory layouts.
pub const RUMDL_CONFIG_FILES: &[&str] = &[".rumdl.toml", "rumdl.toml", ".config/rumdl.toml", "pyproject.toml"];

pub const MARKDOWNLINT_CONFIG_FILES: &[&str] = &[
    ".markdownlint-cli2.jsonc",
    ".markdownlint-cli2.yaml",
    ".markdownlint-cli2.yml",
    ".markdownlint.json",
    ".markdownlint.jsonc",
    ".markdownlint.yaml",
    ".markdownlint.yml",
    "markdownlint.json",
    "markdownlint.jsonc",
    "markdownlint.yaml",
    "markdownlint.yml",
];

/// Create a default configuration file at the specified path
pub fn create_default_config(path: &str) -> Result<(), ConfigError> {
    create_preset_config("default", path)
}

/// Create a configuration file with a specific style preset
pub fn create_preset_config(preset: &str, path: &str) -> Result<(), ConfigError> {
    if Path::new(path).exists() {
        return Err(ConfigError::FileExists { path: path.to_string() });
    }

    let config_content = match preset {
        "default" => generate_default_preset(),
        "google" => generate_google_preset(),
        "relaxed" => generate_relaxed_preset(),
        _ => {
            return Err(ConfigError::UnknownPreset {
                name: preset.to_string(),
            });
        }
    };

    match fs::write(path, config_content) {
        Ok(_) => Ok(()),
        Err(err) => Err(ConfigError::IoError {
            source: err,
            path: path.to_string(),
        }),
    }
}

/// Generate the default preset configuration content.
/// Returns the same content as `create_default_config`.
fn generate_default_preset() -> String {
    r#"# rumdl configuration file

# Inherit settings from another config file (relative to this file's directory)
# extends = "../base.rumdl.toml"

# Global configuration options
[global]
# List of rules to disable (uncomment and modify as needed)
# disable = ["MD013", "MD033"]

# List of rules to enable exclusively (replaces defaults; only these rules will run)
# enable = ["MD001", "MD003", "MD004"]

# Additional rules to enable on top of defaults (additive, does not replace)
# Use this to activate opt-in rules like MD060, MD063, MD072, MD073, MD074
# extend-enable = ["MD060", "MD063"]

# Additional rules to disable on top of the disable list (additive)
# extend-disable = ["MD041"]

# List of file/directory patterns to include for linting (if provided, only these will be linted)
# include = [
#    "docs/*.md",
#    "src/**/*.md",
#    "README.md"
# ]

# List of file/directory patterns to exclude from linting
exclude = [
    # Common directories to exclude
    ".git",
    ".github",
    "node_modules",
    "vendor",
    "dist",
    "build",

    # Specific files or patterns
    "CHANGELOG.md",
    "LICENSE.md",
]

# Respect .gitignore files when scanning directories (default: true)
respect-gitignore = true

# Markdown flavor/dialect (uncomment to enable)
# Options: standard (default), gfm, commonmark, mkdocs, mdx, quarto
# flavor = "mkdocs"

# Rule-specific configurations (uncomment and modify as needed)

# [MD003]
# style = "atx"  # Heading style (atx, atx_closed, setext)

# [MD004]
# style = "asterisk"  # Unordered list style (asterisk, plus, dash, consistent)

# [MD007]
# indent = 4  # Unordered list indentation

# [MD013]
# line-length = 100  # Line length
# code-blocks = false  # Exclude code blocks from line length check
# tables = false  # Exclude tables from line length check
# headings = true  # Include headings in line length check

# [MD044]
# names = ["rumdl", "Markdown", "GitHub"]  # Proper names that should be capitalized correctly
# code-blocks = false  # Check code blocks for proper names (default: false, skips code blocks)
"#
    .to_string()
}

/// Generate Google developer documentation style preset.
/// Based on https://google.github.io/styleguide/docguide/style.html
fn generate_google_preset() -> String {
    r#"# rumdl configuration - Google developer documentation style
# Based on https://google.github.io/styleguide/docguide/style.html

[global]
exclude = [
    ".git",
    ".github",
    "node_modules",
    "vendor",
    "dist",
    "build",
    "CHANGELOG.md",
    "LICENSE.md",
]
respect-gitignore = true

# ATX-style headings required
[MD003]
style = "atx"

# Unordered list style: dash
[MD004]
style = "dash"

# 4-space indent for nested lists
[MD007]
indent = 4

# Strict mode: no trailing spaces allowed (Google uses backslash for line breaks)
[MD009]
strict = true

# 80-character line length
[MD013]
line-length = 80
code-blocks = false
tables = false

# No trailing punctuation in headings
[MD026]
punctuation = ".,;:!。，；：！"

# Fenced code blocks only (no indented code blocks)
[MD046]
style = "fenced"

# Emphasis with underscores
[MD049]
style = "underscore"

# Strong with asterisks
[MD050]
style = "asterisk"
"#
    .to_string()
}

/// Generate relaxed preset for existing projects adopting rumdl incrementally.
/// Longer line lengths, fewer rules, lenient settings to minimize initial warnings.
fn generate_relaxed_preset() -> String {
    r#"# rumdl configuration - Relaxed preset
# Lenient settings for existing projects adopting rumdl incrementally.
# Minimizes initial warnings while still catching important issues.

[global]
exclude = [
    ".git",
    ".github",
    "node_modules",
    "vendor",
    "dist",
    "build",
    "CHANGELOG.md",
    "LICENSE.md",
]
respect-gitignore = true

# Disable rules that produce the most noise on existing projects
disable = [
    "MD013",  # Line length - most existing files exceed 80 chars
    "MD033",  # Inline HTML - commonly used in real-world markdown
    "MD041",  # First line heading - not all files need it
]

# Consistent heading style (any style, just be consistent)
[MD003]
style = "consistent"

# Consistent list style
[MD004]
style = "consistent"

# Consistent emphasis style
[MD049]
style = "consistent"

# Consistent strong style
[MD050]
style = "consistent"
"#
    .to_string()
}

/// Errors that can occur when loading configuration
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read the configuration file
    #[error("Failed to read config file at {path}: {source}")]
    IoError { source: io::Error, path: String },

    /// Failed to parse the configuration content (TOML or JSON)
    #[error("Failed to parse config: {0}")]
    ParseError(String),

    /// Configuration file already exists
    #[error("Configuration file already exists at {path}")]
    FileExists { path: String },

    /// Circular extends reference detected
    #[error("Circular extends reference: {path} already in chain {chain:?}")]
    CircularExtends { path: String, chain: Vec<String> },

    /// Extends chain exceeds maximum depth
    #[error("Extends chain exceeds maximum depth of {max_depth} at {path}")]
    ExtendsDepthExceeded { path: String, max_depth: usize },

    /// Extends target file not found
    #[error("extends target not found: {path} (referenced from {from})")]
    ExtendsNotFound { path: String, from: String },

    /// Unknown preset name
    #[error("Unknown preset: {name}. Valid presets: default, google, relaxed")]
    UnknownPreset { name: String },
}

/// Get a rule-specific configuration value
/// Automatically tries both the original key and normalized variants (kebab-case ↔ snake_case)
/// for better markdownlint compatibility
pub fn get_rule_config_value<T: serde::de::DeserializeOwned>(config: &Config, rule_name: &str, key: &str) -> Option<T> {
    let norm_rule_name = rule_name.to_ascii_uppercase(); // Use uppercase for lookup

    let rule_config = config.rules.get(&norm_rule_name)?;

    // Try multiple key variants to support both underscore and kebab-case formats
    let key_variants = [
        key.to_string(),       // Original key as provided
        normalize_key(key),    // Normalized key (lowercase, kebab-case)
        key.replace('-', "_"), // Convert kebab-case to snake_case
        key.replace('_', "-"), // Convert snake_case to kebab-case
    ];

    // Try each variant until we find a match
    for variant in &key_variants {
        if let Some(value) = rule_config.values.get(variant)
            && let Ok(result) = T::deserialize(value.clone())
        {
            return Some(result);
        }
    }

    None
}

/// Generate preset configuration for pyproject.toml format.
/// Converts the .rumdl.toml preset to pyproject.toml section format.
pub fn generate_pyproject_preset_config(preset: &str) -> Result<String, ConfigError> {
    match preset {
        "default" => Ok(generate_pyproject_config()),
        other => {
            let rumdl_config = match other {
                "google" => generate_google_preset(),
                "relaxed" => generate_relaxed_preset(),
                _ => {
                    return Err(ConfigError::UnknownPreset {
                        name: other.to_string(),
                    });
                }
            };
            Ok(convert_rumdl_to_pyproject(&rumdl_config))
        }
    }
}

/// Convert a .rumdl.toml config string to pyproject.toml format.
/// Rewrites `[global]` → `[tool.rumdl]` and `[MDXXX]` → `[tool.rumdl.MDXXX]`.
fn convert_rumdl_to_pyproject(rumdl_config: &str) -> String {
    let mut output = String::with_capacity(rumdl_config.len() + 128);
    for line in rumdl_config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') && !trimmed.starts_with("# [") {
            let section = &trimmed[1..trimmed.len() - 1];
            if section == "global" {
                output.push_str("[tool.rumdl]");
            } else {
                output.push_str(&format!("[tool.rumdl.{section}]"));
            }
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }
    output
}

/// Generate default rumdl configuration for pyproject.toml
pub fn generate_pyproject_config() -> String {
    let config_content = r#"
[tool.rumdl]
# Global configuration options
line-length = 100
disable = []
# extend-enable = ["MD060"]  # Add opt-in rules (additive, keeps defaults)
# extend-disable = []  # Additional rules to disable (additive)
exclude = [
    # Common directories to exclude
    ".git",
    ".github",
    "node_modules",
    "vendor",
    "dist",
    "build",
]
respect-gitignore = true

# Rule-specific configurations (uncomment and modify as needed)

# [tool.rumdl.MD003]
# style = "atx"  # Heading style (atx, atx_closed, setext)

# [tool.rumdl.MD004]
# style = "asterisk"  # Unordered list style (asterisk, plus, dash, consistent)

# [tool.rumdl.MD007]
# indent = 4  # Unordered list indentation

# [tool.rumdl.MD013]
# line-length = 100  # Line length
# code-blocks = false  # Exclude code blocks from line length check
# tables = false  # Exclude tables from line length check
# headings = true  # Include headings in line length check

# [tool.rumdl.MD044]
# names = ["rumdl", "Markdown", "GitHub"]  # Proper names that should be capitalized correctly
# code-blocks = false  # Check code blocks for proper names (default: false, skips code blocks)
"#;

    config_content.to_string()
}
