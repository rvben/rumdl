//! Per-directory configuration resolution.
//!
//! Groups files by their effective config, enabling subdirectory configs
//! to override the root config for files within their scope. This follows
//! the Ruff model: subdirectory configs are standalone by default, and
//! users can use `extends` for inheritance.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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

/// Check whether a config path is at a root-level location.
///
/// Root-level means the config lives directly in the project root
/// or in `project_root/.config/`. Both are considered the "root config"
/// and should not create a separate subdirectory group.
fn is_root_level_config(config_path: &Path, project_root: &Path) -> bool {
    if let Some(parent) = config_path.parent() {
        // Direct child of project root: .rumdl.toml, rumdl.toml, pyproject.toml
        if parent == project_root {
            return true;
        }
        // Config in .config/ subdirectory: .config/rumdl.toml
        if parent == project_root.join(".config") {
            return true;
        }
    }
    false
}

/// Resolve files into config groups based on per-directory config discovery.
///
/// In auto-discovery mode, files in subdirectories that contain their own
/// config files will use that config instead of the root config.
///
/// Fast path: when `explicit_config` or `isolated` is set, or there is no
/// project root, all files use the root config (zero overhead).
pub fn resolve_config_groups(
    file_paths: &[String],
    root_config: &rumdl_config::Config,
    args: &crate::CheckArgs,
    project_root: Option<&Path>,
    cache: &Option<Arc<Mutex<LintCache>>>,
    explicit_config: bool,
    isolated: bool,
) -> Vec<ConfigGroup> {
    // Fast path: explicit config, isolated mode, or no project root
    // All files use the root config
    if explicit_config || isolated || project_root.is_none() {
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

    let project_root = project_root.unwrap();

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
            _ => project_root.to_path_buf(),
        };

        // Look up or discover the config for this directory
        let config_path = discover_with_cache(&parent_dir, project_root, &mut dir_config_cache);

        // Configs at the project root level use the already-loaded root config
        let effective_config = config_path.filter(|cp| !is_root_level_config(cp, project_root));

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
                // Subdirectory config group
                match rumdl_config::SourcedConfig::load_config_for_path(&path, project_root) {
                    Ok(mut subdir_config) => {
                        // Apply CLI overrides that should take effect everywhere
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
    if let Some(ref flavor_str) = args.flavor
        && let Ok(flavor) = flavor_str.parse::<rumdl_config::MarkdownFlavor>()
    {
        config.global.flavor = flavor;
    }

    if let Some(respect_gitignore) = args.respect_gitignore {
        config.global.respect_gitignore = respect_gitignore;
    }
}
