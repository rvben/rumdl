//! Handler for the `check` command.

use colored::*;

use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;

use crate::cli_utils::{apply_cli_overrides, load_config_with_cli_error_handling_with_dir};
use crate::{CheckArgs, FailOn, FixMode};

/// Run the check/lint/fmt command.
pub fn run_check(args: &CheckArgs, global_config_path: Option<&str>, isolated: bool, inline_overrides: &[toml::Table]) {
    let quiet = args.quiet;
    let silent = args.silent;

    // Validate mutually exclusive options
    if args.diff && args.fix {
        eprintln!("{}: --diff and --fix cannot be used together", "Error".red().bold());
        eprintln!("Use --diff to preview changes, or --fix to apply them");
        exit::tool_error();
    }

    if args.check && args.fix {
        eprintln!("{}: --check and --fix cannot be used together", "Error".red().bold());
        eprintln!("Use --check to verify formatting without changes, or --fix to apply them");
        exit::tool_error();
    }

    // Warn about deprecated --force-exclude flag
    if args.force_exclude {
        eprintln!(
            "{}: --force-exclude is deprecated and has no effect",
            "warning".yellow().bold()
        );
        eprintln!("Exclude patterns are now always respected by default (as of v0.0.156)");
        eprintln!("Use --no-exclude if you want to disable exclusions");
    }

    // Check for watch mode
    if args.watch {
        crate::watch::run_watch_mode(args, global_config_path, isolated, quiet, inline_overrides);
        return;
    }

    // 1. Determine the directory for config discovery.
    //
    // A single path discovers config next to that path (if it's a directory) or
    // next to its parent. This keeps file-scoped runs (and pre-commit passing one
    // relative file) finding the right nearest config.
    //
    // Multiple paths may span several config scopes (e.g. a repo-root config plus
    // nested `.rumdl.toml` files that `extend-disable` a rule). The *global*
    // config is the baseline for every file whose nearest config is the project
    // root, so it must reflect the shared scope of the files being checked, not
    // any single path's own directory. Anchoring at the first path's directory
    // would let that directory's nested config become the baseline for files in
    // sibling directories (silently suppressing a rule there); anchoring at the
    // current working directory would leak an unrelated cwd config onto files
    // elsewhere. So we anchor discovery at the nearest common ancestor of all
    // target paths and let discovery walk up from there to the project root.
    // Per-file grouping still layers each file's own nearest config on top.
    //
    // Zero paths (lint the cwd recursively) keeps the cwd-based discovery.
    let multi_path_root = if args.paths.len() > 1 {
        common_ancestor_dir(&args.paths)
    } else {
        None
    };

    let discovery_dir = if args.paths.len() == 1 {
        let first_path = std::path::Path::new(&args.paths[0]);
        if first_path.is_dir() {
            Some(first_path)
        } else {
            first_path.parent().filter(|&parent| parent.is_dir())
        }
    } else {
        multi_path_root.as_deref()
    };

    // 2. Load sourced config (for provenance and validation)
    let mut sourced = load_config_with_cli_error_handling_with_dir(global_config_path, isolated, discovery_dir);

    // 2b. Apply inline `--config 'RULE.key=value'` overrides at CLI precedence
    // (highest), so they win over both file-loaded values and any later CLI
    // arg overrides that touch top-level globals.
    crate::cli_config_override::apply_inline_overrides(&mut sourced, inline_overrides);

    // 2c. Surface config-discovery warnings (e.g. a `rumdl.toml` shadowed by a
    // sibling `.rumdl.toml`). Resolution is unchanged; this only tells the user
    // which file is winning. Suppressed by --silent, like other config warnings.
    if !sourced.discovery_warnings.is_empty() && !args.silent {
        for warning in &sourced.discovery_warnings {
            eprintln!("\x1b[33m[config warning]\x1b[0m {warning}");
        }
    }

    // 3. Validate configuration
    let registry = rumdl_config::default_registry();
    let validation_warnings = rumdl_config::validate_config_sourced(&sourced, registry);
    if !validation_warnings.is_empty() && !args.silent {
        for warn in &validation_warnings {
            eprintln!("\x1b[33m[config warning]\x1b[0m {}", warn.message);
        }
        // Do NOT exit; continue with valid config
    }

    // 3b. Validate CLI rule names
    let cli_warnings = rumdl_config::validate_cli_rule_names(
        args.enable.as_deref(),
        args.disable.as_deref(),
        args.extend_enable.as_deref(),
        args.extend_disable.as_deref(),
        args.fixable.as_deref(),
        args.unfixable.as_deref(),
    );
    if !cli_warnings.is_empty() && !args.silent {
        for warn in &cli_warnings {
            eprintln!("\x1b[33m[cli warning]\x1b[0m {}", warn.message);
        }
    }

    // 3c. Apply CLI argument overrides (e.g., --flavor)
    apply_cli_overrides(&mut sourced, args);

    // 4. Extract cache_dir and project_root before converting sourced
    let cache_dir_from_config = sourced
        .global
        .cache_dir
        .as_ref()
        .map(|sv| std::path::PathBuf::from(&sv.value));

    let project_root = sourced.project_root.clone();

    // Grouping root: the upper bound for per-directory config grouping. It is the
    // discovered `project_root` when there is one; otherwise, for a multi-path run,
    // it falls back to the common-ancestor anchor so standalone subdirectory
    // configs are still grouped. Unlike `project_root` it does not base the cache
    // dir, per-file globs or displayed paths, so those stay cwd-relative when no
    // project config was found. `discover_config_for_dir` keeps the home boundary,
    // so a grouping root above home never promotes `~/.rumdl.toml`. Isolated and
    // explicit-config runs are unaffected: `resolve_config_groups` fast-paths on
    // those regardless of the grouping root.
    let grouping_root = project_root.clone().or(multi_path_root);

    // 5. Convert to Config for the rest of the linter
    // Validation warnings are already printed above, so we use into_validated_unchecked
    let config: rumdl_config::Config = sourced.into_validated_unchecked().into();

    // 6. Initialize cache if enabled
    // CLI --no-cache flag takes precedence over config
    let cache_enabled = !args.no_cache && config.global.cache;

    // Resolve cache directory with precedence: CLI -> env var -> config -> default
    let mut cache_dir = args
        .cache_dir
        .as_ref()
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var("RUMDL_CACHE_DIR").ok().map(std::path::PathBuf::from))
        .or(cache_dir_from_config)
        .unwrap_or_else(|| std::path::PathBuf::from(".rumdl_cache"));

    // If cache_dir is relative and we have a project root, resolve relative to project root
    if cache_dir.is_relative()
        && let Some(ref root) = project_root
    {
        cache_dir = root.join(&cache_dir);
    }

    let cache = if cache_enabled {
        let cache_instance = crate::cache::LintCache::new(cache_dir.clone(), cache_enabled);

        // Initialize cache directory structure
        if let Err(e) = cache_instance.init() {
            if !silent {
                eprintln!("Warning: Failed to initialize cache: {e}");
            }
            // Continue without cache
            None
        } else {
            // Wrap in Arc for thread-safe sharing across parallel workers.
            Some(std::sync::Arc::new(cache_instance))
        }
    } else {
        None
    };

    // Use the same cache directory for workspace index cache (when cache is enabled)
    let workspace_cache_dir = if cache_enabled { Some(cache_dir.as_path()) } else { None };

    let ctx = crate::check_runner::CheckRunContext {
        args,
        config: &config,
        quiet,
        cache,
        workspace_cache_dir,
        project_root: project_root.as_deref(),
        grouping_root: grouping_root.as_deref(),
        inline_overrides,
        explicit_config: global_config_path.is_some(),
        isolated,
    };

    let (has_issues, has_warnings, has_errors, total_issues_fixed) = crate::check_runner::perform_check_run(&ctx);

    // In --check mode (for fmt), exit with code 1 if any formatting changes would be made
    if args.check && total_issues_fixed > 0 {
        exit::violations_found();
    }

    // Determine if we should fail based on --fail-on setting
    let should_fail = match args.fail_on_mode {
        FailOn::Never => false,
        FailOn::Error => has_errors,
        FailOn::Warning => has_warnings,
        FailOn::Any => has_issues,
    };

    if should_fail && args.fix_mode != FixMode::Format {
        exit::violations_found();
    }
}

/// The nearest common-ancestor directory of every target path, resolved against
/// the current working directory.
///
/// Used to anchor multi-path config discovery so the global baseline reflects the
/// shared scope of the targets rather than any single path's (possibly
/// nested-config) directory. Each path is reduced to its containing directory (the
/// path itself when it is a directory, otherwise its parent); the result is the
/// longest shared component prefix of those directories. Returns `None` when the
/// paths share no common ancestor (e.g. different Windows drives) or the working
/// directory cannot be resolved for a relative path, in which case the caller
/// falls back to cwd-based discovery.
fn common_ancestor_dir(paths: &[String]) -> Option<std::path::PathBuf> {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    let cwd = std::env::current_dir().ok();

    let to_dir = |p: &str| -> Option<PathBuf> {
        let path = Path::new(p);
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.as_ref()?.join(path)
        };
        let dir = if abs.is_dir() {
            abs
        } else {
            abs.parent().map(Path::to_path_buf).unwrap_or(abs)
        };
        // Canonicalize so `..`/`.` components are resolved before the prefix is
        // computed; a literal `../sibling` would otherwise share the first path's
        // directory as a false common prefix. Falls back to the raw directory when
        // the path does not exist (it will be reported later as a missing file).
        Some(std::fs::canonicalize(&dir).unwrap_or(dir))
    };

    let components = |dir: &Path| -> Vec<OsString> { dir.components().map(|c| c.as_os_str().to_owned()).collect() };

    let mut dirs = paths.iter().filter_map(|p| to_dir(p));
    let mut common = components(&dirs.next()?);
    for dir in dirs {
        let comps = components(&dir);
        let shared = common.iter().zip(&comps).take_while(|(a, b)| a == b).count();
        common.truncate(shared);
        if common.is_empty() {
            return None;
        }
    }

    let mut result = PathBuf::new();
    for component in common {
        result.push(component);
    }
    Some(result)
}
