use indexmap::IndexSet;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use super::discovery_trace::{DiscoveryTrace, HomeSource, ProbeOutcome, SearchStage, SkipReason, TraceEvent, WalkEnd};
use super::file_source::{ConfigFileSource, FsConfigFiles};
use super::flavor::ConfigLoaded;
use super::flavor::ConfigValidated;
use super::parsers;
use super::registry::RuleRegistry;
use super::source_tracking::{
    ConfigSource, ConfigValidationWarning, SourcedConfig, SourcedConfigFragment, SourcedGlobalConfig, SourcedValue,
};
use super::types::{
    Config, ConfigError, ConfigOrigin, DiscoveredConfigError, GlobalConfig, MARKDOWNLINT_CONFIG_FILES,
    RUMDL_CONFIG_FILES, RuleConfig, WITHHELD,
};
use super::user_paths::{self, Platform, UserCandidate};
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

/// An `extends` value as the user wrote it, so the file it reaches can be named
/// without disclosing what the value expanded to. See [`ConfigOrigin`] for why
/// that matters.
///
/// Nothing here needs the environment variables the value substitutes: the only
/// forms expanded are `$NAME` and `${NAME}`, both of which stand verbatim in the
/// written value, so naming the reference already names them.
struct ExtendsRef {
    /// The value exactly as written in the declaring config file, and `None`
    /// when that file was itself reached through `extends`. An `extends` value is
    /// a line of the file that wrote it, so a file whose unknown keys and invalid
    /// values are withheld does not get to have this one line quoted instead.
    written: Option<String>,
    /// The short name of the config file that declared this `extends`, so a
    /// chain of substituted paths never surfaces at any depth.
    from: String,
}

impl ExtendsRef {
    /// The reference as a message about the file it reaches should name it.
    fn describe(&self) -> String {
        format!("{} (referenced from {})", self.short(), self.from)
    }

    /// The reference alone, for a message that only has to say which file it
    /// means. See [`ConfigOrigin::short_name`].
    ///
    /// A withheld reference names nothing, so two of them in one chain read
    /// alike. What locates the problem is still there: the file that declared it,
    /// which whoever hit the error can open.
    fn short(&self) -> String {
        match &self.written {
            Some(written) => format!("'{written}'"),
            None => WITHHELD.to_string(),
        }
    }
}

/// The `extends` chain walked so far.
///
/// `visited` holds canonicalized paths, which is what cycle detection needs.
/// `names` holds how each file was reached, which is what a message about the
/// chain may show: a path in it can hold expanded environment variables.
#[derive(Default)]
struct ExtendsChain {
    visited: IndexSet<PathBuf>,
    names: Vec<String>,
}

impl ExtendsChain {
    fn contains(&self, canonical: &Path) -> bool {
        self.visited.contains(canonical)
    }

    fn len(&self) -> usize {
        self.visited.len()
    }

    fn push(&mut self, canonical: PathBuf, name: String) {
        self.visited.insert(canonical);
        self.names.push(name);
    }

    fn names(&self) -> Vec<String> {
        self.names.clone()
    }
}

/// Resolve an `extends` value against the config file that declares it, and
/// describe the reference for any message about the file it reaches.
///
/// - `$VAR` / `${VAR}`: expanded from the environment first (see [`expand_env_vars`])
/// - `~/` prefix: expanded to home directory
/// - Relative paths: resolved against the config file's parent directory
/// - Absolute paths: used as-is
///
/// `declared_by` is the origin of the file holding the value, which decides
/// whether the value may be quoted: it is that file's text like any other.
/// `source` supplies the environment and home directory the value is resolved
/// against.
fn resolve_extends(
    extends_value: &str,
    config_file_path: &Path,
    from: &str,
    declared_by: ConfigOrigin<'_>,
    source: &dyn ConfigFileSource,
) -> Result<(PathBuf, ExtendsRef), ConfigError> {
    let expanded = expand_env_vars(extends_value, |key| source.env_var(key)).map_err(|var| {
        // The variable name is written in the same value, so it is quotable
        // exactly when the value is. Withholding it where the value is withheld
        // also keeps a set and an unset variable from telling different amounts.
        ConfigError::ExtendsUndefinedVar {
            var: if declared_by.may_quote_contents() {
                format!("${var}")
            } else {
                WITHHELD.to_string()
            },
            from: from.to_string(),
        }
    })?;

    let reference = ExtendsRef {
        written: declared_by.may_quote_contents().then(|| extends_value.to_string()),
        from: from.to_string(),
    };

    Ok((
        resolve_expanded_extends_path(&expanded, config_file_path, source.home_dir().as_deref()),
        reference,
    ))
}

/// Turn an already-expanded `extends` value into a path. `home` is what `~/`
/// expands to; without one the prefix is kept literal.
fn resolve_expanded_extends_path(expanded: &str, config_file_path: &Path, home: Option<&Path>) -> PathBuf {
    if let Some(suffix) = expanded.strip_prefix("~/") {
        match home {
            Some(home) => home.join(suffix),
            None => PathBuf::from(expanded),
        }
    } else {
        let path = PathBuf::from(expanded);
        if path.is_absolute() {
            path
        } else {
            // Resolve relative to config file's directory
            let config_dir = config_file_path.parent().unwrap_or(Path::new("."));
            config_dir.join(expanded)
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

/// Inspect one candidate config path.
///
/// A path counts as found when it can be inspected at all (a directory named
/// `rumdl.toml` is found, and then fails to load, as it always has). A
/// `pyproject.toml` (`needs_rumdl_section`) is found only when it declares a rumdl
/// section. An error other than the path being absent is reported as such rather
/// than as missing, so a permissions problem does not read as "no config".
pub(crate) fn probe_config_file(path: &Path, needs_rumdl_section: bool) -> ProbeOutcome {
    match std::fs::metadata(path) {
        Ok(_) => {}
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return ProbeOutcome::Missing;
        }
        Err(e) => return ProbeOutcome::AccessError { error: e.to_string() },
    }
    if !needs_rumdl_section {
        return ProbeOutcome::Found;
    }
    match std::fs::read_to_string(path) {
        Ok(content) if pyproject_declares_rumdl_config(&content) => ProbeOutcome::Found,
        Ok(_) => ProbeOutcome::IneligiblePyproject,
        Err(e) => ProbeOutcome::AccessError { error: e.to_string() },
    }
}

fn is_pyproject(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some("pyproject.toml")
}

/// Every rumdl-native config candidate in `dir`, in precedence order, with what
/// was found at each.
///
/// Walks `RUMDL_CONFIG_FILES` (the single source of truth for discovery) joined onto
/// `dir`, so `.config/rumdl.toml` is recognised at the same level as `.rumdl.toml`.
/// `pyproject.toml` counts only when it declares a rumdl section. markdownlint configs
/// are intentionally excluded: they are a separate fallback tier, not a same-tool
/// collision, and projects routinely keep one around while migrating.
fn probe_rumdl_configs_in_dir(dir: &Path) -> Vec<(PathBuf, ProbeOutcome)> {
    RUMDL_CONFIG_FILES
        .iter()
        .map(|name| {
            let path = dir.join(name);
            let outcome = probe_config_file(&path, is_pyproject(&path));
            (path, outcome)
        })
        .collect()
}

/// The paths among `probes` that were found, in order.
fn found_paths(probes: &[(PathBuf, ProbeOutcome)]) -> Vec<PathBuf> {
    probes
        .iter()
        .filter(|(_, outcome)| *outcome == ProbeOutcome::Found)
        .map(|(path, _)| path.clone())
        .collect()
}

/// The rumdl-native config files that actually exist in `dir`, in precedence order.
pub(crate) fn rumdl_configs_in_dir(dir: &Path) -> Vec<PathBuf> {
    found_paths(&probe_rumdl_configs_in_dir(dir))
}

fn record_probes(trace: &mut DiscoveryTrace, stage: SearchStage, probes: &[(PathBuf, ProbeOutcome)]) {
    for (path, outcome) in probes {
        trace.record(TraceEvent::Probe {
            stage,
            path: path.to_string_lossy().into_owned(),
            outcome: outcome.clone(),
            profile_fallback: false,
        });
    }
}

fn record_selected(
    trace: &mut DiscoveryTrace,
    stage: SearchStage,
    winner: &Path,
    shadows: &[PathBuf],
    profile_fallback: bool,
) {
    trace.record(TraceEvent::Selected {
        stage,
        path: winner.to_string_lossy().into_owned(),
        shadows: shadows.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
        profile_fallback,
    });
}

/// Collect project configuration candidates nearest-first.
///
/// Rumdl-native files precede markdownlint files within each directory. The
/// walk includes `workspace_root`, when supplied, and excludes `home_dir`: a
/// config in the home directory is user configuration rather than project
/// configuration. Returning every candidate lets adapters recover from a
/// malformed higher-precedence file without reimplementing discovery policy.
pub(crate) fn collect_project_config_candidates(
    search_dir: &Path,
    workspace_root: Option<&Path>,
    home_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let profile = user_paths::profile_boundary_for(home_dir);
    collect_project_config_candidates_bounded(search_dir, workspace_root, home_dir, profile.as_deref())
}

/// [`collect_project_config_candidates`] with the profile boundary supplied.
pub(crate) fn collect_project_config_candidates_bounded(
    search_dir: &Path,
    workspace_root: Option<&Path>,
    home_dir: Option<&Path>,
    profile_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let spared = |dir: &Path| workspace_root.is_none_or(|root| canonical(dir) != canonical(root));
    let home_boundary = home_dir.filter(|home| spared(home));
    let profile_boundary = profile_dir.filter(|profile| spared(profile));
    let walk = UpwardWalk::new(search_dir)
        .stop_below(home_boundary.map(Path::to_path_buf))
        .stop_below_profile(profile_boundary.map(Path::to_path_buf));
    let walk = match workspace_root {
        Some(root) => walk.stop_at(root),
        None => walk,
    };

    for current_dir in walk {
        candidates.extend(rumdl_configs_in_dir(&current_dir));
        candidates.extend(
            MARKDOWNLINT_CONFIG_FILES
                .iter()
                .map(|name| current_dir.join(name))
                .filter(|path| path.exists()),
        );
    }

    candidates
}

/// File names markdownlint-cli2 reads its own options from, in its precedence order.
const MARKDOWNLINT_CLI2_CONFIG_FILES: &[&str] = &[
    ".markdownlint-cli2.jsonc",
    ".markdownlint-cli2.yaml",
    ".markdownlint-cli2.yml",
];

/// File names markdownlint-cli2 reads a plain markdownlint config from, in its precedence order.
const MARKDOWNLINT_CLI2_RULE_FILES: &[&str] = &[
    ".markdownlint.jsonc",
    ".markdownlint.json",
    ".markdownlint.yaml",
    ".markdownlint.yml",
];

fn first_existing(dir: &Path, names: &[&str]) -> Option<String> {
    names
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
        .map(|path| path.display().to_string())
}

/// Preserve a selected symlink spelling while recognizing equivalent base paths.
fn is_within_config_base(directory: &Path, base: &Path) -> bool {
    let absolute = |path: &Path| std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| absolute(path));
    absolute(directory).starts_with(absolute(base)) || canonical(directory).starts_with(canonical(base))
}

/// The markdownlint files whose rules make up the config markdownlint-cli2 would apply in the
/// directory of the discovered `config_file`, outermost first, for
/// [`parsers::load_from_markdownlint`].
///
/// markdownlint-cli2 resolves a directory's rules from the directories between it and
/// `base_dir`, the directory it runs from (`markdownlint-cli2.mjs`, `createDirInfos` and
/// `lintFiles`):
///
/// - A `.markdownlint.*` file is used as-is. It beats a `.markdownlint-cli2.*` `config` in the
///   same directory, and a nested directory inherits it while no `.markdownlint-cli2.*` file
///   between them sets `config`.
/// - Otherwise the `config` of every `.markdownlint-cli2.*` file up to `base_dir` is merged, a
///   nearer file replacing an inherited rule entry as a whole.
///
/// rumdl reads these files on behalf of markdownlint-cli2, so it resolves them the same way.
/// A config outside `base_dir` (or with no `base_dir`) stands alone. The home directory
/// bounds discovery unless it is the explicitly selected run root.
fn markdownlint_config_chain(
    config_file: &Path,
    base_dir: Option<&Path>,
    home_boundary: Option<PathBuf>,
) -> Vec<String> {
    let path_str = config_file.display().to_string();
    let filename = config_file.file_name().and_then(|name| name.to_str()).unwrap_or("");
    let (Some(config_dir), Some(base_dir)) = (config_file.parent(), base_dir) else {
        return vec![path_str];
    };
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !MARKDOWNLINT_CLI2_CONFIG_FILES.contains(&filename) || !is_within_config_base(config_dir, base_dir) {
        return vec![path_str];
    }

    let mut declares_config = crate::markdownlint_config::declares_cli2_config(&path_str);
    let mut chain = vec![path_str];
    // The explicitly selected run root is project context even when CI sets
    // HOME to the checkout. The home boundary still applies above that root.
    let profile_boundary = user_paths::profile_boundary_for(home_boundary.as_deref())
        .filter(|profile| canonical(profile) != canonical(base_dir));
    let home_boundary = home_boundary.filter(|home| canonical(home) != canonical(base_dir));
    let parents = UpwardWalk::new(config_dir)
        .stop_below(home_boundary)
        .stop_below_profile(profile_boundary)
        .always_yield_start()
        .stop_at(base_dir)
        .skip(1);
    let usable = |file: &String| match parsers::load_from_markdownlint(std::slice::from_ref(file)) {
        Ok(_) => true,
        Err(error) => {
            log::debug!("[rumdl-config] Skipping unusable ancestor rules in {file}: {error}");
            false
        }
    };
    for dir in parents {
        if let Some(cli2_file) = first_existing(&dir, MARKDOWNLINT_CLI2_CONFIG_FILES)
            && usable(&cli2_file)
        {
            declares_config |= crate::markdownlint_config::declares_cli2_config(&cli2_file);
            chain.push(cli2_file);
        }
        if !declares_config
            && let Some(rule_file) = first_existing(&dir, MARKDOWNLINT_CLI2_RULE_FILES)
            && usable(&rule_file)
        {
            return vec![rule_file];
        }
    }
    chain.reverse();
    chain
}

/// Resolve selection options independently of rules: a plain markdownlint
/// file replaces rule configuration but still receives CLI2 ignore options.
fn markdownlint_ignore_scopes(
    config_file: &Path,
    base_dir: Option<&Path>,
    home_boundary: Option<PathBuf>,
) -> (Vec<super::Cli2Ignore>, Vec<String>) {
    let Some(config_dir) = config_file.parent() else {
        return (Vec::new(), Vec::new());
    };
    let base = base_dir.unwrap_or(config_dir);
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut dirs = if is_within_config_base(config_dir, base) {
        let profile_boundary = user_paths::profile_boundary_for(home_boundary.as_deref())
            .filter(|profile| canonical(profile) != canonical(base));
        UpwardWalk::new(config_dir)
            .stop_below(home_boundary.filter(|home| canonical(home) != canonical(base)))
            .stop_below_profile(profile_boundary)
            .always_yield_start()
            .stop_at(base)
            .collect::<Vec<_>>()
    } else {
        vec![config_dir.to_path_buf()]
    };
    dirs.reverse();
    let mut scopes = Vec::new();
    let mut loaded = Vec::new();
    let mut nested_scope = None;
    for dir in dirs {
        let Some(file) = first_existing(&dir, MARKDOWNLINT_CLI2_CONFIG_FILES) else {
            continue;
        };
        let is_root = canonical(&dir) == canonical(base);
        if is_root && !rumdl_configs_in_dir(base).is_empty() {
            continue;
        }
        let directory = if is_root { base } else { config_dir };
        let scope = crate::markdownlint_config::read_cli2_ignores(&file).and_then(|patterns| {
            patterns
                .map(|patterns| super::Cli2Ignore::new(directory.to_path_buf(), patterns, is_root))
                .transpose()
        });
        loaded.push(file.clone());
        match scope {
            Ok(Some(scope)) if is_root => {
                // Root ignores filter discovery and are also inherited as
                // nested options, reapplied relative to this config group.
                if let Ok(nested) = super::Cli2Ignore::new(config_dir.to_path_buf(), scope.patterns.clone(), false) {
                    nested_scope = Some(nested);
                }
                scopes.push(scope);
            }
            Ok(Some(scope)) => nested_scope = Some(scope),
            Ok(None) => {}
            Err(error) => log::debug!("[rumdl-config] Ignoring unusable discovered CLI2 options in {file}: {error}"),
        }
    }
    // CLI2 reapplies inherited nested patterns relative to the directory that
    // owns this config group, even when that group's file is plain markdownlint.
    if let Some(scope) = nested_scope {
        scopes.push(scope);
    }
    (scopes, loaded)
}

fn load_discovered_markdownlint(
    sourced: &mut SourcedConfig,
    config_file: &Path,
    base_dir: Option<&Path>,
    home_boundary: Option<PathBuf>,
) -> Result<(), ConfigError> {
    let chain = markdownlint_config_chain(config_file, base_dir, home_boundary.clone());
    let fragment = parsers::load_from_markdownlint(&chain)?;
    let (ignores, option_files) = markdownlint_ignore_scopes(config_file, base_dir, home_boundary);
    sourced.merge(fragment);
    sourced.cli2_ignores = ignores;
    for file in chain.into_iter().chain(option_files) {
        if !sourced.loaded_files.contains(&file) {
            sourced.loaded_files.push(file);
        }
    }
    Ok(())
}

/// Native nested rules do not undo selection performed by a CLI2 run root.
/// A native config at the run root still takes precedence over CLI2 discovery.
fn load_root_cli2_ignores(sourced: &mut SourcedConfig, config_file: &Path, base_dir: Option<&Path>) {
    let (Some(config_dir), Some(base)) = (config_file.parent(), base_dir) else {
        return;
    };
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if canonical(config_dir) == canonical(base)
        || !is_within_config_base(config_dir, base)
        || !rumdl_configs_in_dir(base).is_empty()
    {
        return;
    }
    load_run_cli2_ignores(sourced, base, true)
}

fn load_run_cli2_ignores(sourced: &mut SourcedConfig, base: &Path, preserve_root_group: bool) {
    if !rumdl_configs_in_dir(base).is_empty() {
        return;
    }
    if let Some(file) = first_existing(base, MARKDOWNLINT_CLI2_CONFIG_FILES) {
        let scope = crate::markdownlint_config::read_cli2_ignores(&file).and_then(|patterns| {
            patterns
                .map(|patterns| super::Cli2Ignore::new(base.to_path_buf(), patterns, true))
                .transpose()
        });
        let scope = match scope {
            Ok(scope) => scope,
            Err(error) => {
                // Like discovered markdownlint rules, unusable discovered
                // options are skipped. Explicit --config still reports errors.
                log::debug!("[rumdl-config] Skipping CLI2 selection options {file}: {error}");
                return;
            }
        };
        if let Some(scope) = scope {
            // Native configs create rumdl groups, but do not create CLI2 groups.
            // Keep the root's micromatch filter at its original directory.
            if preserve_root_group
                && let Ok(group) = super::Cli2Ignore::new(base.to_path_buf(), scope.patterns.clone(), false)
                && !sourced.cli2_ignores.contains(&group)
            {
                sourced.cli2_ignores.push(group);
            }
            if !sourced.cli2_ignores.contains(&scope) {
                sourced.cli2_ignores.push(scope);
            }
        }
        if !sourced.loaded_files.contains(&file) {
            sourced.loaded_files.push(file);
        }
    }
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
#[cfg(test)]
pub(crate) fn detect_shadowed_configs(dir: &Path) -> Option<ShadowedConfigs> {
    shadow_from_found(dir, rumdl_configs_in_dir(dir))
}

/// `detect_shadowed_configs` for a directory whose found configs are already known.
fn shadow_from_found(dir: &Path, mut configs: Vec<PathBuf>) -> Option<ShadowedConfigs> {
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
///
/// `origin` says how this file was reached, and every message about it is
/// phrased through that: a file the recursion reached is named by the `extends`
/// value that names it, never by the path that value expanded to. The resolved
/// path is still recorded in [`SourcedConfig::loaded_files`], which is not a
/// message about the file but the answer to a question someone asked about the
/// configuration. See [`ConfigOrigin`].
fn load_config_with_extends(
    sourced_config: &mut SourcedConfig<ConfigLoaded>,
    config_file_path: &Path,
    chain: &mut ExtendsChain,
    chain_source: ConfigSource,
    origin: ConfigOrigin<'_>,
    source: &dyn ConfigFileSource,
) -> Result<(), ConfigError> {
    // Canonicalize the path for circular reference detection
    let canonical = source.canonicalize(config_file_path);

    let path_str = config_file_path.display().to_string();
    let described = origin.display_name(&path_str);
    let short = origin.short_name(&path_str);

    // Check for circular references
    if chain.contains(&canonical) {
        return Err(ConfigError::CircularExtends {
            path: described,
            chain: chain.names(),
        });
    }

    // Check depth limit
    if chain.len() >= MAX_EXTENDS_DEPTH {
        return Err(ConfigError::ExtendsDepthExceeded {
            path: described,
            max_depth: MAX_EXTENDS_DEPTH,
        });
    }

    // Mark as visited
    chain.push(canonical, short.clone());

    let filename = config_file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Read and parse the config file
    let content = source
        .read_to_string(config_file_path)
        .map_err(|e| ConfigError::IoError {
            source: e,
            path: described.clone(),
        })?;

    let fragment = if filename == "pyproject.toml" {
        match parsers::parse_pyproject_toml(&content, &path_str, chain_source, origin)? {
            Some(f) => f,
            None => return Ok(()), // No [tool.rumdl] section
        }
    } else {
        parsers::parse_rumdl_toml(&content, &path_str, chain_source, origin)?
    };

    // If this fragment has `extends`, load the base config first
    if let Some(ref extends_value) = fragment.extends {
        let (base_path, reference) = resolve_extends(extends_value, config_file_path, &short, origin, source)?;
        let base_described = reference.describe();
        let base_short = reference.short();

        if !source.exists(&base_path) {
            return Err(ConfigError::ExtendsNotFound {
                path: base_short,
                from: short,
            });
        }

        log::debug!(
            "[rumdl-config] Config {} extends {}, loading base first",
            path_str,
            base_path.display()
        );

        // Recursively load the base config
        load_config_with_extends(
            sourced_config,
            &base_path,
            chain,
            chain_source,
            ConfigOrigin::Extends {
                described_as: &base_described,
                short_name: &base_short,
            },
            source,
        )?;
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

        // Whether a message about the include patterns may quote them travels with
        // the patterns: the file supplying the winning list supplies what may be
        // said about it, in either direction.
        if self.global.include.merge_from(fragment.global.include) {
            self.global.include_withheld = fragment.global.include_withheld;
        }
        self.global.exclude.merge_from(fragment.global.exclude);
        self.global
            .respect_gitignore
            .merge_from(fragment.global.respect_gitignore);
        self.global.line_length.merge_from(fragment.global.line_length);
        self.global.fixable.merge_from(fragment.global.fixable);
        self.global.unfixable.merge_from(fragment.global.unfixable);
        self.global.flavor.merge_from(fragment.global.flavor);
        self.global.force_exclude.merge_from(fragment.global.force_exclude);
        self.global.editorconfig.merge_from(fragment.global.editorconfig);

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

            // Merge values. Whether a value may be quoted back travels with the
            // value: a file that takes a key over also takes over what may be
            // said about it, in either direction.
            for (key, sourced_value_fragment) in rule_fragment.values {
                let sv_entry = rule_entry
                    .values
                    .entry(key.clone())
                    .or_insert_with(|| SourcedValue::new(sourced_value_fragment.value.clone(), ConfigSource::Default));
                if sv_entry.merge_from(sourced_value_fragment) {
                    if rule_fragment.withheld_keys.contains(&key) {
                        rule_entry.withheld_keys.insert(key);
                    } else {
                        rule_entry.withheld_keys.remove(&key);
                    }
                }
            }
        }

        // Merge unknown_keys from fragment
        // A file reached twice through two `extends` chains has the same problem
        // both times; the user wants to hear about it once.
        for warning in fragment.load_warnings {
            if !self.discovery_warnings.contains(&warning) {
                self.discovery_warnings.push(warning);
            }
        }

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
    /// `home_override` wins (supplied by tests and the language server); otherwise
    /// the real home is resolved by the shared resolver, the same one every other
    /// consumer of `~` uses (`user_paths`). Wasm has no home/project walk to bound,
    /// so it returns `None` there.
    fn resolve_home_boundary(home_override: Option<&Path>) -> Option<std::path::PathBuf> {
        home_override
            .map(Path::to_path_buf)
            .or_else(user_paths::process_home_path)
    }

    /// The home directory a load uses throughout, recorded in `trace` once.
    ///
    /// Resolved once per load so the project-walk boundary and the user-config
    /// fallback cannot disagree about where `~` is, and so the trace shows the
    /// choice (and any `HOME` value that was set but ignored) exactly once.
    fn effective_home(home_override: Option<&Path>, trace: &mut DiscoveryTrace) -> Option<std::path::PathBuf> {
        if let Some(home) = home_override {
            if !trace.has_home() {
                trace.record(TraceEvent::Home {
                    path: Some(home.to_string_lossy().into_owned()),
                    source: Some(HomeSource::Caller),
                });
            }
            return Some(home.to_path_buf());
        }
        let resolution = user_paths::process_home();
        if !trace.has_home() {
            if let Some(ignored) = &resolution.ignored {
                trace.record(TraceEvent::IgnoredHome {
                    value: ignored.value.clone(),
                    reason: ignored.reason,
                });
            }
            trace.record(TraceEvent::Home {
                path: resolution.home.as_ref().map(|h| h.path.to_string_lossy().into_owned()),
                source: resolution.home.as_ref().map(|h| h.source),
            });
        }
        resolution.home.map(|home| home.path)
    }

    /// Where an upward config walk begins.
    ///
    /// `start_override` is the directory a caller chose as its scope; the CLI has
    /// none and uses the process working directory, the directory the user typed
    /// the command in.
    fn resolve_discovery_start(start_override: Option<&Path>) -> Option<std::path::PathBuf> {
        if let Some(dir) = start_override {
            return Some(dir.to_path_buf());
        }
        match std::env::current_dir() {
            Ok(dir) => Some(dir),
            Err(e) => {
                log::debug!("[rumdl-config] Failed to get current directory: {e}");
                None
            }
        }
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
    /// user-config directory keeps precedence over `~/.rumdl.toml`. The start
    /// directory is exempt from that boundary: it is an explicitly chosen project
    /// context, so its configs apply even when it *is* `$HOME` (pre-commit.ci sets
    /// `HOME` to the git checkout, and `pyproject.toml` has no user-config
    /// fallback). `home_override` supplies the boundary for tests; production
    /// resolves the real home directory.
    fn discover_config_upward(
        start_override: Option<&Path>,
        home_override: Option<&Path>,
        trace: &mut DiscoveryTrace,
    ) -> Option<(std::path::PathBuf, std::path::PathBuf, Option<ShadowedConfigs>)> {
        let home = Self::resolve_home_boundary(home_override);
        let profile = user_paths::profile_boundary_for(home.as_deref());
        Self::discover_config_upward_bounded(start_override, home, profile, trace)
    }

    /// [`Self::discover_config_upward`] with both boundaries supplied: the resolved
    /// home and, when it is a home in its own right, the user profile directory.
    pub(crate) fn discover_config_upward_bounded(
        start_override: Option<&Path>,
        home: Option<std::path::PathBuf>,
        profile: Option<std::path::PathBuf>,
        trace: &mut DiscoveryTrace,
    ) -> Option<(std::path::PathBuf, std::path::PathBuf, Option<ShadowedConfigs>)> {
        let start_dir = Self::resolve_discovery_start(start_override)?;

        // The probes are the single source of truth for "which rumdl configs live
        // here", shared with the LSP and the shadow detector, so the winner and the
        // silently-shadowed siblings are computed identically.
        let mut walk = UpwardWalk::new(&start_dir)
            .stop_below(home)
            .stop_below_profile(profile)
            .always_yield_start()
            .stop_at_git_root();
        let found = walk.find_map(|dir| {
            let probes = probe_rumdl_configs_in_dir(&dir);
            record_probes(trace, SearchStage::Project, &probes);
            let found = found_paths(&probes);
            let winner = found.first()?.clone();
            log::debug!("[rumdl-config] Found config file: {}", winner.display());
            let shadow = shadow_from_found(&dir, found);
            record_selected(
                trace,
                SearchStage::Project,
                &winner,
                shadow.as_ref().map_or(&[][..], |s| &s.shadowed),
                false,
            );
            Some((winner, dir, shadow))
        });
        let Some((config_path, config_dir, shadow)) = found else {
            if let Some(stop) = walk.stop_reason() {
                trace.record(TraceEvent::WalkEnded {
                    stage: SearchStage::Project,
                    end: WalkEnd::from(stop),
                });
            }
            return None;
        };

        // Determine project root by walking up from the config location.
        let project_root = Self::find_project_root_from(&config_dir);
        Some((config_path, project_root, shadow))
    }

    /// Discover markdownlint configuration file by traversing up the directory tree.
    /// Similar to discover_config_upward but for .markdownlint.yaml/json files, and
    /// bounded at the home directory for the same reason: a markdownlint config in
    /// `$HOME` is user-level, not a project config. The start directory is exempt
    /// from the boundary just like rumdl config discovery, and markdownlint files
    /// have no user-config fallback at all, so without the exemption a config in a
    /// checkout that is itself `$HOME` would be ignored entirely.
    fn discover_markdownlint_config_upward(
        start_override: Option<&Path>,
        home_override: Option<&Path>,
        trace: &mut DiscoveryTrace,
    ) -> Option<std::path::PathBuf> {
        let start_dir = Self::resolve_discovery_start(start_override)?;

        let home = Self::resolve_home_boundary(home_override);
        let mut walk = UpwardWalk::new(&start_dir)
            .stop_below_profile(user_paths::profile_boundary_for(home.as_deref()))
            .stop_below(home)
            .always_yield_start()
            .stop_at_git_root();
        let found = walk.find_map(|dir| {
            let probes: Vec<(PathBuf, ProbeOutcome)> = MARKDOWNLINT_CONFIG_FILES
                .iter()
                .map(|name| dir.join(name))
                .map(|path| {
                    let outcome = probe_config_file(&path, false);
                    (path, outcome)
                })
                .collect();
            record_probes(trace, SearchStage::Markdownlint, &probes);
            let winner = found_paths(&probes).into_iter().next()?;
            record_selected(trace, SearchStage::Markdownlint, &winner, &[], false);
            Some(winner)
        });
        if found.is_none()
            && let Some(stop) = walk.stop_reason()
        {
            trace.record(TraceEvent::WalkEnded {
                stage: SearchStage::Markdownlint,
                end: WalkEnd::from(stop),
            });
        }
        found
    }

    /// Load an explicit config file (standalone, no user config merging)
    fn load_explicit_config(sourced_config: &mut Self, path: &str, run_dir: Option<&Path>) -> Result<(), ConfigError> {
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
            let mut chain = ExtendsChain::default();
            let chain_source = source_from_filename(filename);
            load_config_with_extends(
                sourced_config,
                path_obj,
                &mut chain,
                chain_source,
                ConfigOrigin::Direct,
                &FsConfigFiles,
            )?;
        } else if MARKDOWNLINT_FILENAMES.contains(&filename)
            || path_str.ends_with(".json")
            || path_str.ends_with(".jsonc")
            || path_str.ends_with(".yaml")
            || path_str.ends_with(".yml")
        {
            // Parse as markdownlint config (JSON/YAML) - no extends support. An explicit
            // config is standalone, so it inherits nothing from parent directories.
            if MARKDOWNLINT_CLI2_CONFIG_FILES.contains(&filename)
                && let Some(patterns) =
                    crate::markdownlint_config::read_cli2_ignores(&path_str).map_err(ConfigError::ParseError)?
            {
                let base = Self::resolve_discovery_start(run_dir).unwrap_or_else(|| PathBuf::from("."));
                let discovery =
                    super::Cli2Ignore::new(base.clone(), patterns.clone(), true).map_err(ConfigError::ParseError)?;
                let group = super::Cli2Ignore::new(base, patterns, false).map_err(ConfigError::ParseError)?;
                sourced_config.cli2_ignores.extend([discovery, group]);
            }
            let chain = vec![path_str];
            let fragment = parsers::load_from_markdownlint(&chain)?;
            sourced_config.merge(fragment);
            sourced_config.loaded_files.extend(chain);
        } else {
            // Try TOML with extends support
            let mut chain = ExtendsChain::default();
            let chain_source = source_from_filename(filename);
            load_config_with_extends(
                sourced_config,
                path_obj,
                &mut chain,
                chain_source,
                ConfigOrigin::Direct,
                &FsConfigFiles,
            )?;
        }

        Ok(())
    }

    /// Load and merge user-level configuration into this `SourcedConfig`.
    ///
    /// Takes the first existing file among the user-config locations listed in
    /// `user_paths`: the platform user-config directory, then (Windows only)
    /// `~/.config/rumdl`, then the home-directory dotfiles. Override the platform
    /// directory with `user_config_dir` and the home directory with `home_dir`
    /// (tests and the language server).
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
        let home = Self::effective_home(home_dir, &mut sourced_config.discovery_trace);
        let platform_dir = match user_config_dir {
            Some(dir) => Some(dir.to_path_buf()),
            None => user_paths::platform_config_dir(),
        };
        // The fallback belongs to the process's own home, which the loader forwards
        // here as a plain path; a different home chosen by a caller has none.
        let profile_fallback = user_paths::process_home().profile_fallback_for(home.as_deref());
        let candidates = user_paths::user_config_candidates(
            Platform::current(),
            platform_dir.as_deref(),
            home.as_deref(),
            profile_fallback.as_deref(),
        );
        Self::load_user_candidates(sourced_config, &candidates)
    }

    /// Probe `candidates` (highest priority first), record each probe, and load the
    /// first file found. Split from `load_user_config` so the candidate list, which
    /// is where the platform policy lives, can be supplied directly.
    pub(crate) fn load_user_candidates(
        sourced_config: &mut Self,
        candidates: &[UserCandidate],
    ) -> Result<(), ConfigError> {
        let probes: Vec<(PathBuf, ProbeOutcome)> = candidates
            .iter()
            .map(|c| (c.path.clone(), probe_config_file(&c.path, c.requires_rumdl_section)))
            .collect();
        for (candidate, (path, outcome)) in candidates.iter().zip(&probes) {
            sourced_config.discovery_trace.record(TraceEvent::Probe {
                stage: SearchStage::User,
                path: path.to_string_lossy().into_owned(),
                outcome: outcome.clone(),
                profile_fallback: candidate.profile_fallback,
            });
        }

        let mut found = candidates
            .iter()
            .zip(&probes)
            .filter(|(_, (_, outcome))| *outcome == ProbeOutcome::Found)
            .map(|(candidate, (path, _))| (path.clone(), candidate.profile_fallback));
        let Some((user_config_path, from_profile_fallback)) = found.next() else {
            log::debug!("[rumdl-config] No user configuration file found");
            return Ok(());
        };
        let shadows: Vec<PathBuf> = found.map(|(path, _)| path).collect();
        record_selected(
            &mut sourced_config.discovery_trace,
            SearchStage::User,
            &user_config_path,
            &shadows,
            from_profile_fallback,
        );

        log::debug!("[rumdl-config] Loading user config: {}", user_config_path.display());

        // User config fallback also supports extends chains.
        // Use a uniform source across the chain so child overrides are determined by chain order.
        let mut chain = ExtendsChain::default();
        load_config_with_extends(
            sourced_config,
            &user_config_path,
            &mut chain,
            ConfigSource::UserConfig,
            ConfigOrigin::Direct,
            &FsConfigFiles,
        )
    }

    /// Load a project config file that discovery found, as opposed to one the user
    /// named explicitly.
    ///
    /// The two are not interchangeable. An explicit config is standalone by design
    /// (`load_explicit_config`), and so is a discovered rumdl-native config: a
    /// project's ruleset has to be reproducible on any machine. A discovered
    /// *markdownlint* config is the exception. That format cannot express
    /// rumdl-specific settings (flavor, cache, per-file ignores), so the user config
    /// is loaded first as a base and the markdownlint fragment merged on top. The
    /// fragment carries `ConfigSource::ProjectConfig` (precedence 3) against the
    /// base's `ConfigSource::UserConfig` (1), so project settings still win on every
    /// overlapping key.
    ///
    /// Both the CLI (`load_with_discovery_impl`) and the LSP (`load_discovered`,
    /// via `RumdlLanguageServer::resolve_config_for_file`) load discovered files
    /// through here, so a discovered config resolves the same way in an editor as
    /// it does on the command line.
    ///
    /// `base_dir` is the directory the run is scoped to (the CLI's working directory
    /// or the LSP's workspace root). It bounds markdownlint-cli2 config inheritance,
    /// as markdownlint-cli2's own working directory does (`markdownlint_config_chain`).
    fn load_discovered_config(
        sourced_config: &mut Self,
        config_file: &Path,
        user_config_dir: Option<&Path>,
        home_dir: Option<&Path>,
        base_dir: Option<&Path>,
    ) -> Result<(), DiscoveredConfigError> {
        let filename = config_file.file_name().and_then(|name| name.to_str()).unwrap_or("");

        if MARKDOWNLINT_CONFIG_FILES.contains(&filename) {
            Self::load_user_config(sourced_config, user_config_dir, home_dir)
                .map_err(DiscoveredConfigError::UserConfig)?;

            load_discovered_markdownlint(
                sourced_config,
                config_file,
                base_dir,
                Self::resolve_home_boundary(home_dir),
            )
            .map_err(DiscoveredConfigError::ProjectConfig)?;
        } else {
            let mut chain = ExtendsChain::default();
            let chain_source = source_from_filename(filename);
            load_config_with_extends(
                sourced_config,
                config_file,
                &mut chain,
                chain_source,
                ConfigOrigin::Direct,
                &FsConfigFiles,
            )
            .map_err(DiscoveredConfigError::ProjectConfig)?;
            load_root_cli2_ignores(sourced_config, config_file, base_dir);
        }

        Ok(())
    }

    /// Load a config file that the caller discovered by walking the tree itself.
    ///
    /// The LSP cannot use `load_with_discovery`: that walk starts at the process
    /// working directory, while the server resolves a config per document and stops
    /// at the workspace root. It finds the file with its own walk and hands it here,
    /// so the discovered-config rules in `load_discovered_config` still apply.
    ///
    /// `project_root` comes from the config file's own location, which is what
    /// per-file ignore globs are matched against.
    ///
    /// `user_config_dir` and `home_dir` override the platform user-config directory
    /// and the home directory; the server passes the home directory it already
    /// resolved for its walk boundary, and tests pass both.
    ///
    /// The error distinguishes an unusable discovered file from an unusable user
    /// config so a caller walking several candidates can tell "try the next one"
    /// from "nothing here will resolve correctly".
    ///
    /// A discovered `.markdownlint-cli2.*` file loads standalone here; use
    /// `load_discovered_within` to resolve it against its parent directories.
    pub fn load_discovered(
        config_file: &Path,
        user_config_dir: Option<&Path>,
        home_dir: Option<&Path>,
    ) -> Result<Self, DiscoveredConfigError> {
        Self::load_discovered_within(config_file, user_config_dir, home_dir, None)
    }

    /// `load_discovered` for a run scoped to `base_dir` (the server passes the
    /// document's workspace root), which bounds markdownlint-cli2 config
    /// inheritance as described on `load_discovered_config`.
    pub(crate) fn load_discovered_within(
        config_file: &Path,
        user_config_dir: Option<&Path>,
        home_dir: Option<&Path>,
        base_dir: Option<&Path>,
    ) -> Result<Self, DiscoveredConfigError> {
        let mut sourced_config = SourcedConfig::default();

        if let Some(config_parent) = config_file.parent() {
            sourced_config.project_root = Some(Self::find_project_root_from(config_parent));
        }

        Self::load_discovered_config(&mut sourced_config, config_file, user_config_dir, home_dir, base_dir)?;

        Ok(sourced_config)
    }

    /// Load the configuration that applies to a directory, as if the CLI had run there.
    ///
    /// Discovery normally walks up from the process working directory, which is the
    /// scope the user chose when they typed `rumdl check`. A language server has no
    /// such directory: the editor launches it from wherever it happens to be, which
    /// may sit in an unrelated project. The workspace root is the scope the user
    /// chose, so the server passes that here and resolves what `rumdl check` would
    /// resolve inside it.
    ///
    /// `user_config_dir` and `home_dir` override the platform user-config directory
    /// and the home-directory walk boundary; the server passes the home directory it
    /// already resolved for its per-file walk, and tests pass both.
    pub fn load_for_workspace(
        start_dir: &Path,
        config_path: Option<&str>,
        user_config_dir: Option<&Path>,
        home_dir: Option<&Path>,
    ) -> Result<Self, ConfigError> {
        Self::load_with_discovery_from(
            Some(start_dir),
            config_path,
            None,
            false,
            user_config_dir,
            home_dir,
            None,
        )
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
        Self::load_with_discovery_from(
            None,
            config_path,
            cli_overrides,
            skip_auto_discovery,
            user_config_dir,
            home_dir,
            None,
        )
    }

    /// Load a CLI target's nearest config without changing the process cwd.
    /// Discovery starts at the target; CLI2 inheritance is bounded by the run's cwd.
    pub fn load_for_directory(
        start_dir: &Path,
        config_path: Option<&str>,
        skip_auto_discovery: bool,
        run_dir: &Path,
    ) -> Result<Self, ConfigError> {
        let mut sourced = Self::load_with_discovery_from(
            Some(start_dir),
            config_path,
            None,
            skip_auto_discovery,
            None,
            None,
            Some(run_dir),
        )?;
        // Root selection still applies when explicit targets lie outside cwd.
        if !skip_auto_discovery && config_path.is_none() {
            load_run_cli2_ignores(&mut sourced, run_dir, false);
        }
        Ok(sourced)
    }

    /// Shared body of every discovery-based load.
    ///
    /// `start_dir` is where the upward walk begins; `None` means the process
    /// working directory, which is what the CLI wants.
    fn load_with_discovery_from(
        start_dir: Option<&Path>,
        config_path: Option<&str>,
        cli_overrides: Option<&SourcedGlobalConfig>,
        skip_auto_discovery: bool,
        user_config_dir: Option<&Path>,
        home_dir: Option<&Path>,
        inheritance_base: Option<&Path>,
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
            for stage in [SearchStage::Project, SearchStage::User] {
                sourced_config.discovery_trace.record(TraceEvent::Skipped {
                    stage,
                    reason: SkipReason::ExplicitConfig,
                });
            }
            sourced_config.discovery_trace.record(TraceEvent::Selected {
                stage: SearchStage::Explicit,
                path: path.to_string(),
                shadows: Vec::new(),
                profile_fallback: false,
            });
            Self::load_explicit_config(&mut sourced_config, path, inheritance_base.or(start_dir))?;
        } else if skip_auto_discovery {
            log::debug!("[rumdl-config] Skipping config discovery due to --no-config/--isolated flag");
            for stage in [SearchStage::Project, SearchStage::User] {
                sourced_config.discovery_trace.record(TraceEvent::Skipped {
                    stage,
                    reason: SkipReason::Isolated,
                });
            }
            // No config loading, just apply CLI overrides at the end
        } else {
            // No explicit path - try auto-discovery
            log::debug!("[rumdl-config] No explicit config_path, searching default locations");
            let discovery_base = Self::resolve_discovery_start(inheritance_base.or(start_dir));
            if let Some(start) = Self::resolve_discovery_start(start_dir) {
                sourced_config.discovery_trace.record(TraceEvent::StartDirectory {
                    path: start.to_string_lossy().into_owned(),
                });
            }
            // One home for the whole load: the walk boundary and the user-config
            // fallback must agree about where `~` is.
            let home = Self::effective_home(home_dir, &mut sourced_config.discovery_trace);
            let home_dir = home.as_deref();

            // Try to discover project config first
            if let Some((config_file, project_root, shadow)) =
                Self::discover_config_upward(start_dir, home_dir, &mut sourced_config.discovery_trace)
            {
                for stage in [SearchStage::Markdownlint, SearchStage::User] {
                    sourced_config.discovery_trace.record(TraceEvent::Skipped {
                        stage,
                        reason: SkipReason::ProjectConfigFound,
                    });
                }
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

                Self::load_discovered_config(
                    &mut sourced_config,
                    &config_file,
                    user_config_dir,
                    home_dir,
                    discovery_base.as_deref(),
                )?;
            } else {
                // No rumdl project config - try markdownlint config
                log::debug!("[rumdl-config] No rumdl config found, checking markdownlint config");

                if let Some(markdownlint_path) =
                    Self::discover_markdownlint_config_upward(start_dir, home_dir, &mut sourced_config.discovery_trace)
                {
                    log::debug!(
                        "[rumdl-config] Found markdownlint config: {}",
                        markdownlint_path.display()
                    );

                    if let Err(e) = Self::load_discovered_config(
                        &mut sourced_config,
                        &markdownlint_path,
                        user_config_dir,
                        home_dir,
                        discovery_base.as_deref(),
                    ) {
                        match e {
                            // A markdownlint file rumdl cannot parse is skipped rather
                            // than fatal: the user never named it, and rumdl only reads
                            // the format as a courtesy. The user config it would have
                            // merged onto is already loaded, which is the state of the
                            // no-project-config case.
                            DiscoveredConfigError::ProjectConfig(e) => {
                                log::debug!("[rumdl-config] Failed to load markdownlint config: {e}");
                                sourced_config.discovery_trace.reject_selected(
                                    SearchStage::Markdownlint,
                                    &markdownlint_path.to_string_lossy(),
                                    "could not be loaded as a markdownlint config",
                                );
                            }
                            // A broken user config is fatal, as in every other arm.
                            DiscoveredConfigError::UserConfig(e) => return Err(e),
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
            cli2_ignores: self.cli2_ignores,
            per_file_ignores: self.per_file_ignores,
            per_file_flavor: self.per_file_flavor,
            code_block_tools: self.code_block_tools,
            rules: self.rules,
            loaded_files: self.loaded_files,
            unknown_keys: self.unknown_keys,
            project_root: self.project_root,
            discovery_warnings: self.discovery_warnings,
            discovery_trace: self.discovery_trace,
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
            cli2_ignores: self.cli2_ignores,
            per_file_ignores: self.per_file_ignores,
            per_file_flavor: self.per_file_flavor,
            code_block_tools: self.code_block_tools,
            rules: self.rules,
            loaded_files: self.loaded_files,
            unknown_keys: self.unknown_keys,
            project_root: self.project_root,
            discovery_warnings: self.discovery_warnings,
            discovery_trace: self.discovery_trace,
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
        // Windows short names stay as the caller wrote them); only stop checks
        // compare canonically. A relative `dir` is resolved against the current
        // directory, so the returned config path is always absolute. UpwardWalk
        // also retains the shared traversal depth bound.
        //
        // The home boundary keeps the walk from treating `~/.rumdl.toml` as a
        // project config, consistent with `discover_config_upward`. This only has
        // an effect when `project_root` is at or above the home directory (e.g. a
        // multi-path run whose grouping root spans the home boundary); for the
        // usual project root below home the walk stops there first.
        collect_project_config_candidates(dir, Some(project_root), Self::resolve_home_boundary(None).as_deref())
            .into_iter()
            .next()
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
            // Per-directory markdownlint discovery keeps the same rumdl-specific
            // user defaults as initial CLI and LSP discovery.
            Self::load_user_config(&mut sourced_config, None, None)?;
            load_discovered_markdownlint(
                &mut sourced_config,
                config_path,
                Self::resolve_discovery_start(None).as_deref(),
                Self::resolve_home_boundary(None),
            )?;
        } else {
            let mut chain = ExtendsChain::default();
            let chain_source = source_from_filename(filename);
            load_config_with_extends(
                &mut sourced_config,
                config_path,
                &mut chain,
                chain_source,
                ConfigOrigin::Direct,
                &FsConfigFiles,
            )?;
            load_root_cli2_ignores(
                &mut sourced_config,
                config_path,
                Self::resolve_discovery_start(None).as_deref(),
            );
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

    /// Load the rumdl config at `root` and the `extends` chain it declares,
    /// reading every file from `source`, into a fresh still-`Loaded` config.
    ///
    /// This is the entry point for embedders without a filesystem: they supply
    /// an [`InMemoryConfigFiles`](super::file_source::InMemoryConfigFiles) and `root` names an
    /// entry in it. `root` is read as `pyproject.toml` when that is its file
    /// name and as `.rumdl.toml` / `rumdl.toml` otherwise; base configs reached
    /// through `extends` are named by the path their reference resolves to,
    /// relative to the declaring file's directory, exactly as on disk.
    #[cfg(any(feature = "wasm", test))]
    pub(crate) fn load_chain_from(root: &Path, source: &dyn ConfigFileSource) -> Result<Self, ConfigError> {
        let mut sourced_config = SourcedConfig::default();
        let filename = root.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let mut chain = ExtendsChain::default();
        load_config_with_extends(
            &mut sourced_config,
            root,
            &mut chain,
            source_from_filename(filename),
            ConfigOrigin::Direct,
            source,
        )?;
        Ok(sourced_config)
    }
}

/// Convert a validated configuration to the final Config type.
///
/// This implementation only exists for `SourcedConfig<ConfigValidated>`,
/// ensuring that validation must occur before conversion.
impl From<SourcedConfig<ConfigValidated>> for Config {
    fn from(sourced: SourcedConfig<ConfigValidated>) -> Self {
        let mut rules = BTreeMap::new();
        let mut withheld_rule_values = std::collections::BTreeSet::new();
        for (rule_name, sourced_rule_cfg) in sourced.rules {
            // Normalize rule name to uppercase for case-insensitive lookup
            let normalized_rule_name = rule_name.to_ascii_uppercase();
            let severity = sourced_rule_cfg.severity.map(|sv| sv.value);
            let mut values = BTreeMap::new();
            for (key, sourced_val) in sourced_rule_cfg.values {
                values.insert(key, sourced_val.value);
            }
            if values.keys().any(|key| sourced_rule_cfg.withheld_keys.contains(key)) {
                withheld_rule_values.insert(normalized_rule_name.clone());
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
            editorconfig: sourced.global.editorconfig.value,
            enable_is_explicit,
            include_withheld: sourced.global.include_withheld,
        };

        let mut config = Config {
            extends: None,
            global,
            cli2_ignores: sourced.cli2_ignores,
            per_file_ignores: sourced.per_file_ignores.value,
            per_file_flavor: sourced.per_file_flavor.value,
            code_block_tools: sourced.code_block_tools.value,
            rules,
            withheld_rule_values,
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

    #[test]
    #[serial_test::serial]
    fn project_candidates_absolutize_a_relative_start() {
        let cwd = std::env::current_dir().unwrap();
        let temp = tempfile::Builder::new()
            .prefix("rumdl-relative-config-")
            .tempdir_in(&cwd)
            .unwrap();
        let nested = temp.path().join("docs");
        std::fs::create_dir(&nested).unwrap();
        let config = temp.path().join(".rumdl.toml");
        std::fs::write(&config, "").unwrap();

        let relative_root = temp.path().strip_prefix(&cwd).unwrap();
        let relative_nested = nested.strip_prefix(&cwd).unwrap();
        let candidates = super::collect_project_config_candidates(relative_nested, Some(relative_root), None);

        assert_eq!(candidates.first(), Some(&config));
        assert!(candidates.iter().all(|path| path.is_absolute()));
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

#[cfg(test)]
mod cli2_tests {
    use super::*;

    #[test]
    fn inherited_rules_keep_their_actual_sources() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let docs = root.join("docs");
        std::fs::create_dir(&docs).unwrap();
        let parent = root.join(".markdownlint-cli2.yaml");
        let child = docs.join(".markdownlint-cli2.yaml");
        std::fs::write(
            &parent,
            "config:\n  MD013:\n    line_length: 100\n  MD007:\n    indent: 4\n",
        )
        .unwrap();
        std::fs::write(&child, "config:\n  MD007:\n    indent: 2\n").unwrap();
        let mut sourced = SourcedConfig::default();
        load_discovered_markdownlint(&mut sourced, &child, Some(root), Some(root.to_path_buf())).unwrap();
        assert_eq!(
            sourced.rules["MD013"].values["line-length"].origin.as_deref(),
            parent.to_str()
        );
        assert_eq!(
            sourced.rules["MD007"].values["indent"].origin.as_deref(),
            child.to_str()
        );
        assert_eq!(sourced.rules["MD007"].values["style"].origin.as_deref(), child.to_str());
        assert_eq!(
            sourced.loaded_files,
            [parent.display().to_string(), child.display().to_string()]
        );
    }
}
