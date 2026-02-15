//! Handler for the `check` command.

use colored::*;

use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;

use crate::cli_utils::{apply_cli_overrides, load_config_with_cli_error_handling_with_dir};
use crate::{CheckArgs, FailOn, FixMode};

/// Run the check/lint/fmt command.
pub fn run_check(args: &CheckArgs, global_config_path: Option<&str>, isolated: bool) {
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
        crate::watch::run_watch_mode(args, global_config_path, isolated, quiet);
        return;
    }

    // 1. Determine the directory for config discovery
    // Use the first target path for config discovery if it's a directory
    // Otherwise use current directory to ensure config files are found
    // when pre-commit or other tools pass relative file paths
    let discovery_dir = if !args.paths.is_empty() {
        let first_path = std::path::Path::new(&args.paths[0]);
        if first_path.is_dir() {
            Some(first_path)
        } else {
            first_path.parent().filter(|&parent| parent.is_dir())
        }
    } else {
        None
    };

    // 2. Load sourced config (for provenance and validation)
    let mut sourced = load_config_with_cli_error_handling_with_dir(global_config_path, isolated, discovery_dir);

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
            // Wrap in Arc<Mutex<>> for thread-safe sharing across parallel workers
            Some(std::sync::Arc::new(std::sync::Mutex::new(cache_instance)))
        }
    } else {
        None
    };

    // Use the same cache directory for workspace index cache (when cache is enabled)
    let workspace_cache_dir = if cache_enabled { Some(cache_dir.as_path()) } else { None };

    let ctx = crate::watch::CheckRunContext {
        args,
        config: &config,
        quiet,
        cache,
        workspace_cache_dir,
        project_root: project_root.as_deref(),
        explicit_config: global_config_path.is_some(),
        isolated,
    };

    let (has_issues, has_warnings, has_errors, total_issues_fixed) = crate::watch::perform_check_run(&ctx);

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
