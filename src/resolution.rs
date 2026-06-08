//! Per-directory configuration resolution.
//!
//! Groups files by their effective config, enabling subdirectory configs
//! to override the root config for files within their scope. This follows
//! the Ruff model: subdirectory configs are standalone by default, and
//! users can use `extends` for inheritance.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rumdl_lib::config as rumdl_config;
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
    root_config: &rumdl_config::Config,
    args: &crate::CheckArgs,
    roots: &ResolutionRoots<'_>,
    inline_overrides: &[toml::Table],
    cache: &Option<Arc<LintCache>>,
    bypass_discovery: bool,
) -> Vec<ConfigGroup> {
    // Fast path: discovery bypassed or no grouping root; all files use the root config
    if bypass_discovery || roots.grouping_root.is_none() {
        let enabled_rules = crate::file_processor::get_enabled_rules_from_checkargs(args, root_config);
        let cache_hashes = cache
            .as_ref()
            .map(|_| Arc::new(CacheHashes::new(root_config, &enabled_rules)));

        return vec![ConfigGroup {
            config: root_config.clone(),
            rules: enabled_rules,
            cache_hashes,
            files: file_paths.to_vec(),
        }];
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

    let mut groups = Vec::new();

    for (config_path, files) in file_config_map {
        match config_path {
            None => {
                // Root config group
                let enabled_rules = crate::file_processor::get_enabled_rules_from_checkargs(args, root_config);
                let cache_hashes = cache
                    .as_ref()
                    .map(|_| Arc::new(CacheHashes::new(root_config, &enabled_rules)));

                groups.push(ConfigGroup {
                    config: root_config.clone(),
                    rules: enabled_rules,
                    cache_hashes,
                    files,
                });
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
                        let mut subdir_config: rumdl_config::Config = sourced.into_validated_unchecked().into();
                        apply_cli_config_overrides(&mut subdir_config, args);

                        let enabled_rules =
                            crate::file_processor::get_enabled_rules_from_checkargs(args, &subdir_config);
                        let cache_hashes = cache
                            .as_ref()
                            .map(|_| Arc::new(CacheHashes::new(&subdir_config, &enabled_rules)));

                        groups.push(ConfigGroup {
                            config: subdir_config,
                            rules: enabled_rules,
                            cache_hashes,
                            files,
                        });
                    }
                    Err(e) => {
                        // Config validation error in subdirectory: fall back to root config
                        eprintln!(
                            "\x1b[33m[config warning]\x1b[0m Failed to load config {}: {}. Using root config for affected files.",
                            path.display(),
                            e
                        );

                        let enabled_rules = crate::file_processor::get_enabled_rules_from_checkargs(args, root_config);
                        let cache_hashes = cache
                            .as_ref()
                            .map(|_| Arc::new(CacheHashes::new(root_config, &enabled_rules)));

                        groups.push(ConfigGroup {
                            config: root_config.clone(),
                            rules: enabled_rules,
                            cache_hashes,
                            files,
                        });
                    }
                }
            }
        }
    }

    groups
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
