//! Per-directory configuration resolution.
//!
//! Groups files by their effective config, enabling subdirectory configs
//! to override the root config for files within their scope. This follows
//! the Ruff model: subdirectory configs are standalone by default, and
//! users can use `extends` for inheritance.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rumdl_lib::config as rumdl_config;
use rumdl_lib::config::editorconfig::{self, EditorConfigSettings, EditorConfigWarning};
use rumdl_lib::rule::Rule;

use crate::cache::LintCache;
use crate::file_processor::CacheHashes;

/// A group of files that share the same configuration.
pub struct ConfigGroup {
    pub config: rumdl_config::Config,
    pub rules: Vec<Box<dyn Rule>>,
    pub cache_hashes: Option<Arc<CacheHashes>>,
    pub files: Vec<String>,
}

/// The run's root configuration, in both the forms grouping needs.
///
/// `config` is what every file falls back to. `sourced` is the same
/// configuration with provenance intact, which `.editorconfig` layers into: it
/// can only fill in a setting no rumdl config mentions, and that distinction
/// exists solely in the sourced form.
pub struct RootConfig<'a> {
    pub config: &'a rumdl_config::Config,
    pub sourced: &'a rumdl_config::SourcedConfig<rumdl_config::ConfigValidated>,
}

/// The two roots that anchor config resolution for a run.
///
/// They coincide for the common case (a project root discovered below the cwd).
/// They diverge only for a multi-path run with no discovered project config: then
/// `grouping_root` is the common-ancestor anchor (so subdirectory configs are still
/// grouped) while `project_root` stays unset (so the cache dir, per-file globs and
/// displayed paths remain cwd-relative).
pub struct ResolutionRoots<'a> {
    /// Upper bound for the per-directory config walk.
    pub grouping_root: Option<&'a Path>,
    /// The run's project root; bases a discovered subdir config's per-file globs.
    pub project_root: Option<&'a Path>,
}

/// The directory a config file governs (its scope).
///
/// For `.rumdl.toml`, `rumdl.toml` and `pyproject.toml` this is the containing
/// directory. A `.config/rumdl.toml` config governs the directory that holds
/// `.config/`, not `.config/` itself, so its scope is the grandparent. Used to base
/// a discovered subdir config's per-file globs on the files it actually governs.
fn config_scope_dir(config_path: &Path) -> Option<&Path> {
    let parent = config_path.parent()?;
    if parent.file_name() == Some(std::ffi::OsStr::new(".config")) {
        parent.parent()
    } else {
        Some(parent)
    }
}

/// Check whether a config path is at a root-level location.
///
/// Root-level means the config lives directly in the project root
/// or in `project_root/.config/`. Both are considered the "root config"
/// and should not create a separate subdirectory group.
///
/// Paths are canonicalized before comparison so platform-specific
/// representations do not cause a false negative. On Windows the discovered
/// `config_path` is a canonical, long-name `\\?\` path while `project_root` may
/// be an 8.3 short name (e.g. `RUNNER~1`); on Unix symlinks can differ. A false
/// negative here misclassifies the root config as a subdirectory config and
/// reloads it without the inline `--config` overrides.
fn is_root_level_config(config_path: &Path, project_root: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let Some(parent) = config_path.parent() else {
        return false;
    };
    let parent = canon(parent);
    // Direct child of project root: .rumdl.toml, rumdl.toml, pyproject.toml
    // or config in a `.config/` subdirectory: .config/rumdl.toml
    parent == canon(project_root) || parent == canon(&project_root.join(".config"))
}

/// Resolve files into config groups based on per-directory config discovery.
///
/// In auto-discovery mode, files in subdirectories that contain their own
/// config files will use that config instead of the root config.
///
/// Fast path: when discovery is bypassed (`bypass_discovery`, i.e. an explicit
/// `--config` or `--isolated`) or there is no grouping root, all files use the root
/// config (zero overhead).
///
/// `inline_overrides` are the inline `--config 'RULE.key=value'` overrides already
/// merged into `root_config`; they are re-applied on top of each discovered
/// subdirectory config so CLI precedence holds across every group, not just the root.
///
/// See [`ResolutionRoots`] for how the grouping root and project root relate.
pub fn resolve_config_groups(
    file_paths: &[String],
    root: &RootConfig<'_>,
    args: &crate::CheckArgs,
    roots: &ResolutionRoots<'_>,
    inline_overrides: &[toml::Table],
    cache: &Option<Arc<LintCache>>,
    bypass_discovery: bool,
) -> ResolvedGroups {
    let mut grouping = Grouping::default();

    // Fast path: discovery bypassed or no grouping root; all files use the root config
    if bypass_discovery || roots.grouping_root.is_none() {
        grouping.push_groups(root.sourced, root.config.clone(), file_paths.to_vec(), args, cache);
        return grouping.finish();
    }

    let grouping_root = roots.grouping_root.unwrap();

    // Cache: directory → Option<config file path>
    // None means "no subdirectory config found, use root"
    let mut dir_config_cache: HashMap<PathBuf, Option<PathBuf>> = HashMap::new();

    // Map each file to its effective config path.
    // BTreeMap ensures deterministic group ordering across runs.
    let mut file_config_map: BTreeMap<Option<PathBuf>, Vec<String>> = BTreeMap::new();

    for file_path in file_paths {
        let path = Path::new(file_path);
        let parent_dir = match path.parent() {
            Some(dir) if dir.is_dir() => dir.to_path_buf(),
            _ => grouping_root.to_path_buf(),
        };

        // Look up or discover the config for this directory
        let config_path = discover_with_cache(&parent_dir, grouping_root, &mut dir_config_cache);

        // Configs at the grouping root level use the already-loaded root config
        let effective_config = config_path.filter(|cp| !is_root_level_config(cp, grouping_root));

        file_config_map
            .entry(effective_config)
            .or_default()
            .push(file_path.clone());
    }

    for (config_path, files) in file_config_map {
        match config_path {
            None => {
                // Root config group
                grouping.push_groups(root.sourced, root.config.clone(), files, args, cache);
            }
            Some(path) => {
                // Subdirectory config group. Base its per-file globs on the real
                // project root, or on the directory the config governs when there is
                // none, never on the grouping anchor (which may sit above its scope).
                let subconfig_root = roots
                    .project_root
                    .or_else(|| config_scope_dir(&path))
                    .unwrap_or(grouping_root);
                match rumdl_config::SourcedConfig::load_sourced_for_path(&path, subconfig_root) {
                    Ok(mut sourced) => {
                        // Layer inline `--config` overrides on top at CLI precedence
                        // (as the global config does), then convert and apply the
                        // flavor / gitignore overrides that take effect everywhere.
                        crate::cli_config_override::apply_inline_overrides(&mut sourced, inline_overrides);
                        let sourced = sourced.into_validated_unchecked();
                        let mut subdir_config: rumdl_config::Config = sourced.clone().into();
                        apply_cli_config_overrides(&mut subdir_config, args);

                        grouping.push_groups(&sourced, subdir_config, files, args, cache);
                    }
                    Err(e) => {
                        // Config validation error in subdirectory: fall back to root config
                        eprintln!(
                            "\x1b[33m[config warning]\x1b[0m Failed to load config {}: {}. Using root config for affected files.",
                            path.display(),
                            e
                        );

                        grouping.push_groups(root.sourced, root.config.clone(), files, args, cache);
                    }
                }
            }
        }
    }

    grouping.finish()
}

/// The config groups a run resolved to, and whether resolving them turned up a
/// configuration problem.
pub struct ResolvedGroups {
    pub groups: Vec<ConfigGroup>,
    /// Set when an `.editorconfig` property was read but could not be applied.
    /// Reported like rumdl's own config warnings, and counted the same way by
    /// `--deny-config-warnings`.
    pub config_warning: bool,
}

/// The groups being built, plus the state that spans them.
#[derive(Default)]
struct Grouping {
    groups: Vec<ConfigGroup>,
    /// The `.editorconfig` messages already printed, so one covering many files
    /// is reported once for the run rather than once per group.
    reported: BTreeSet<String>,
    config_warning: bool,
}

impl Grouping {
    fn finish(self) -> ResolvedGroups {
        ResolvedGroups {
            groups: self.groups,
            config_warning: self.config_warning,
        }
    }

    /// Build the config groups for a set of files that share one rumdl config.
    ///
    /// That is a single group unless the config opts into `.editorconfig`
    /// reading, in which case the files are sub-grouped by the properties
    /// resolved for each: section globs and nested `.editorconfig` files can
    /// give two files in the same directory different settings, so the grouping
    /// has to be per file even though the rules are instantiated per group.
    fn push_groups(
        &mut self,
        base: &rumdl_config::SourcedConfig<rumdl_config::ConfigValidated>,
        base_config: rumdl_config::Config,
        files: Vec<String>,
        args: &crate::CheckArgs,
        cache: &Option<Arc<LintCache>>,
    ) {
        if !base_config.global.editorconfig {
            self.groups.push(build_group(base_config, files, args, cache));
            return;
        }

        // Keyed by the resolved settings so files resolving identically share one
        // config, and by a `BTreeMap` so the group order is the same on every run.
        let mut by_settings: BTreeMap<(EditorConfigSettings, Option<String>), SettingsGroup> = BTreeMap::new();

        for file in files {
            let resolution = editorconfig::resolve(Path::new(&file));
            let group = by_settings.entry((resolution.settings, resolution.origin)).or_default();
            group
                .warnings
                .extend(resolution.warnings.into_iter().map(|warning| (file.clone(), warning)));
            group.files.push(file);
        }

        for ((settings, origin), group) in by_settings {
            let config = config_with_editorconfig(base, &base_config, &settings, origin.as_deref(), args);
            let built = build_group(config, group.files, args, cache);
            self.config_warning |=
                report_editorconfig_warnings(&group.warnings, &built.rules, &built.config, &mut self.reported, args);
            self.groups.push(built);
        }
    }
}

/// The files that resolved to one set of `.editorconfig` settings, along with the
/// warnings those resolutions raised.
///
/// Each warning keeps the file it came from: whether it is worth reporting
/// depends on the rules that run for that one file, and `per-file-ignores` can
/// make those differ from the ones its group-mates run.
#[derive(Default)]
struct SettingsGroup {
    files: Vec<String>,
    warnings: Vec<(String, EditorConfigWarning)>,
}

/// Layer a file's resolved `.editorconfig` settings onto the config it would
/// otherwise use.
fn config_with_editorconfig(
    base: &rumdl_config::SourcedConfig<rumdl_config::ConfigValidated>,
    base_config: &rumdl_config::Config,
    settings: &EditorConfigSettings,
    origin: Option<&str>,
    args: &crate::CheckArgs,
) -> rumdl_config::Config {
    if settings.is_empty() {
        return base_config.clone();
    }

    let mut sourced = base.clone();
    editorconfig::apply(&mut sourced, settings, origin);
    let mut config: rumdl_config::Config = sourced.into();
    apply_cli_config_overrides(&mut config, args);
    config
}

/// Whether the rule a warning names runs for the file that raised it: enabled by
/// that file's config, and left on for its path by `per-file-ignores`.
///
/// This is the decision `filter_rules_for_file` makes before linting, asked
/// without cloning a rule set that is only being consulted.
fn rule_runs_for(rule: &str, rules: &[Box<dyn Rule>], config: &rumdl_config::Config, path: &Path) -> bool {
    rules.iter().any(|candidate| candidate.name() == rule) && !config.get_ignored_rules_for_file(path).contains(rule)
}

/// Report the `.editorconfig` properties rumdl read but does not act on, and
/// answer whether any of them counts as a configuration problem.
///
/// A warning naming a rule is only true while that rule runs for the file that
/// raised it, so it is dropped otherwise. The rest count whether or not they are
/// printed, so `--silent` suppresses the output without changing what
/// `--deny-config-warnings` sees. `reported` carries the messages already
/// printed: one `.editorconfig` typically covers many files, and repeating a
/// message once per file would bury the lint output.
fn report_editorconfig_warnings(
    warnings: &[(String, EditorConfigWarning)],
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
    reported: &mut BTreeSet<String>,
    args: &crate::CheckArgs,
) -> bool {
    let mut problem = false;
    for (file, warning) in warnings {
        if let Some(rule) = warning.rule
            && !rule_runs_for(rule, rules, config, Path::new(file))
        {
            continue;
        }
        problem = true;
        if !args.silent && reported.insert(warning.message.clone()) {
            eprintln!("\x1b[33m[config warning]\x1b[0m {}", warning.message);
        }
    }
    problem
}

/// The configuration content piped in on stdin is linted with.
pub struct StdinConfig {
    pub config: rumdl_config::Config,
    pub rules: Vec<Box<dyn Rule>>,
    /// Set when an `.editorconfig` property was read but could not be applied.
    pub config_warning: bool,
}

/// Resolve the configuration for content piped in on stdin.
///
/// `--stdin-filename` names a file in the project, and the rest of the stdin
/// path already treats it as one for per-file ignores and flavor, so the
/// `.editorconfig` that applies to that file applies to this content too.
/// Without a filename there is no file to resolve properties for. Per-directory
/// rumdl configs are deliberately not discovered here: that is the caller's
/// decision, unchanged by this.
pub fn resolve_stdin_config(root: &RootConfig<'_>, args: &crate::CheckArgs) -> StdinConfig {
    let file = args
        .stdin_filename
        .as_deref()
        .filter(|_| root.config.global.editorconfig);

    let Some(file) = file else {
        return StdinConfig {
            rules: crate::file_processor::get_enabled_rules_from_checkargs(args, root.config),
            config: root.config.clone(),
            config_warning: false,
        };
    };

    let resolution = editorconfig::resolve(Path::new(file));
    let config = config_with_editorconfig(
        root.sourced,
        root.config,
        &resolution.settings,
        resolution.origin.as_deref(),
        args,
    );
    let rules = crate::file_processor::get_enabled_rules_from_checkargs(args, &config);

    let warnings: Vec<(String, EditorConfigWarning)> = resolution
        .warnings
        .into_iter()
        .map(|warning| (file.to_string(), warning))
        .collect();
    let config_warning = report_editorconfig_warnings(&warnings, &rules, &config, &mut BTreeSet::new(), args);

    StdinConfig {
        config,
        rules,
        config_warning,
    }
}

/// Instantiate the rules and cache hashes a config implies.
fn build_group(
    config: rumdl_config::Config,
    files: Vec<String>,
    args: &crate::CheckArgs,
    cache: &Option<Arc<LintCache>>,
) -> ConfigGroup {
    let rules = crate::file_processor::get_enabled_rules_from_checkargs(args, &config);
    let cache_hashes = cache.as_ref().map(|_| Arc::new(CacheHashes::new(&config, &rules)));

    ConfigGroup {
        config,
        rules,
        cache_hashes,
        files,
    }
}

/// Discover the config file for a directory, using and populating the cache.
///
/// Also caches intermediate directories traversed during the upward walk
/// so that sibling files sharing a parent directory get cache hits.
fn discover_with_cache(
    dir: &Path,
    project_root: &Path,
    cache: &mut HashMap<PathBuf, Option<PathBuf>>,
) -> Option<PathBuf> {
    if let Some(cached) = cache.get(dir) {
        return cached.clone();
    }

    // Walk upward collecting directories we traverse, so we can cache them all
    let result = rumdl_config::SourcedConfig::discover_config_for_dir(dir, project_root);

    // Cache the result for this directory
    cache.insert(dir.to_path_buf(), result.clone());

    // Also cache intermediate directories between dir and the config location
    // (or project root if no config found). This prevents redundant walks.
    if let Some(ref config_path) = result {
        if let Some(config_dir) = config_path.parent() {
            let mut intermediate = dir.to_path_buf();
            while intermediate != config_dir && intermediate.starts_with(project_root) {
                cache.entry(intermediate.clone()).or_insert_with(|| result.clone());
                match intermediate.parent() {
                    Some(parent) => intermediate = parent.to_path_buf(),
                    None => break,
                }
            }
        }
    } else {
        // No config found - cache all directories up to project root
        let mut intermediate = dir.to_path_buf();
        while intermediate.starts_with(project_root) {
            cache.entry(intermediate.clone()).or_insert(None);
            if intermediate == project_root.to_path_buf() {
                break;
            }
            match intermediate.parent() {
                Some(parent) => intermediate = parent.to_path_buf(),
                None => break,
            }
        }
    }

    result
}

/// Apply CLI overrides that should be consistent across all config groups.
///
/// When a user passes `--flavor gfm` on the CLI, that should apply to all files
/// regardless of which subdirectory config they use.
fn apply_cli_config_overrides(config: &mut rumdl_config::Config, args: &crate::CheckArgs) {
    if let Some(flavor) = args.flavor {
        config.global.flavor = flavor.into();
    }

    if let Some(respect_gitignore) = args.respect_gitignore {
        config.global.respect_gitignore = respect_gitignore;
    }
}

#[cfg(test)]
mod tests {
    use super::config_scope_dir;
    use std::path::Path;

    #[test]
    fn config_scope_dir_uses_containing_dir_for_plain_configs() {
        for name in ["myproj/.rumdl.toml", "myproj/rumdl.toml", "myproj/pyproject.toml"] {
            assert_eq!(
                config_scope_dir(Path::new(name)),
                Some(Path::new("myproj")),
                "{name} should be scoped to its containing directory"
            );
        }
    }

    #[test]
    fn config_scope_dir_skips_dot_config_directory() {
        // `.config/rumdl.toml` governs the directory that holds `.config`, not
        // `.config` itself, so its per-file globs must resolve one level up.
        assert_eq!(
            config_scope_dir(Path::new("myproj/.config/rumdl.toml")),
            Some(Path::new("myproj"))
        );
    }
}
