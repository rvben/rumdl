//! Watch mode functionality for continuous linting

use crate::formatter;
use chrono::Local;
use colored::*;
use notify::{Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rayon::prelude::*;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::rule::CrossFileScope;
use rumdl_lib::workspace_index::WorkspaceIndex;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

pub enum ChangeKind {
    Configuration,
    SourceFile,
}

/// Detects what kind of change occurred based on the file extension
pub fn change_detected(event: &Event) -> Option<ChangeKind> {
    // Skip access and other non-modification events
    if !matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) {
        return None;
    }

    let mut source_file = false;
    for path in &event.paths {
        // Check if this is a configuration file
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            // Check for rumdl-specific config files
            if matches!(
                file_name,
                ".rumdl.toml"
                    | "rumdl.toml"
                    | "pyproject.toml"
                    | ".markdownlint.json"
                    | ".markdownlint.jsonc"
                    | ".markdownlint.yaml"
                    | ".markdownlint.yml"
                    | "markdownlint.json"
                    | "markdownlint.jsonc"
                    | "markdownlint.yaml"
                    | "markdownlint.yml"
            ) {
                return Some(ChangeKind::Configuration);
            }
        }

        // Check for markdown files
        if let Some(extension) = path.extension()
            && matches!(extension.to_str(), Some("md" | "markdown" | "mdown" | "mkd" | "mdx"))
        {
            source_file = true;
        }
    }

    if source_file {
        Some(ChangeKind::SourceFile)
    } else {
        None
    }
}

/// Clear the terminal screen
pub fn clear_screen() {
    // ANSI escape sequence to clear screen and move cursor to top-left
    print!("\x1B[2J\x1B[1;1H");
    let _ = io::stdout().flush();
}

/// Perform a single check run (extracted from run_check for reuse in watch mode)
/// Returns (has_issues, has_warnings, has_errors, total_issues_fixed):
///   - has_issues: any violations (info, warning, or error)
///   - has_warnings: any Warning or Error severity violations
///   - has_errors: any Error-severity violations
///   - total_issues_fixed: number of issues fixed (or would be fixed in diff mode)
pub fn perform_check_run(
    args: &crate::CheckArgs,
    config: &rumdl_config::Config,
    quiet: bool,
    cache: Option<Arc<std::sync::Mutex<crate::cache::LintCache>>>,
    workspace_cache_dir: Option<&Path>,
    project_root: Option<&Path>,
) -> (bool, bool, bool, usize) {
    use rumdl_lib::output::{OutputFormat, OutputWriter};
    use rumdl_lib::rule::Severity;

    // Create output writer for linting results
    let output_writer = OutputWriter::new(args.stderr, quiet, args.silent);

    // Read RUMDL_OUTPUT_FORMAT env var (if set)
    let env_output_format = std::env::var("RUMDL_OUTPUT_FORMAT").ok();

    // Determine output format with precedence: CLI → env var → config → legacy → default
    let output_format_str = args
        .output_format
        .as_deref()
        .or(env_output_format.as_deref())
        .or(config.global.output_format.as_deref())
        .or_else(|| {
            // Legacy support: map --output json to --output-format json
            if args.output == "json" { Some("json") } else { None }
        })
        .unwrap_or("text");

    let output_format = match OutputFormat::from_str(output_format_str) {
        Ok(fmt) => fmt,
        Err(e) => {
            eprintln!("{}: {}", "Error".red().bold(), e);
            return (true, true, true, 0);
        }
    };

    // Initialize rules with configuration
    let enabled_rules = crate::file_processor::get_enabled_rules_from_checkargs(args, config);

    // Handle stdin input - either explicit --stdin flag or "-" as file argument
    if args.stdin || (args.paths.len() == 1 && args.paths[0] == "-") {
        crate::stdin_processor::process_stdin(&enabled_rules, args, config);
        return (false, false, false, 0);
    }

    let cache_hashes = cache
        .as_ref()
        .map(|_| Arc::new(crate::file_processor::CacheHashes::new(config, &enabled_rules)));

    // Find all markdown files to check
    let file_paths = match crate::file_processor::find_markdown_files(&args.paths, args, config, project_root) {
        Ok(paths) => paths,
        Err(e) => {
            if !args.silent {
                eprintln!("{}: Failed to find markdown files: {}", "Error".red().bold(), e);
            }
            return (true, true, true, 0);
        }
    };
    if file_paths.is_empty() {
        if !quiet {
            println!("No markdown files found to check.");
        }
        return (false, false, false, 0);
    }

    // Check if any enabled rule needs cross-file analysis
    let needs_cross_file = enabled_rules
        .iter()
        .any(|r| r.cross_file_scope() != CrossFileScope::None);

    // For formats that need to collect all warnings first
    let needs_collection = matches!(
        output_format,
        OutputFormat::Json | OutputFormat::GitLab | OutputFormat::Sarif | OutputFormat::Junit
    );

    if needs_collection {
        let start_time = Instant::now();
        let mut all_file_warnings = Vec::new();
        let mut has_issues = false;
        let mut has_warnings = false;
        let mut has_errors = false;
        let mut _files_with_issues = 0;
        let mut _total_issues = 0;

        // Phase 1: Lint all files and collect FileIndex data (no second pass needed)
        let mut file_indices: HashMap<PathBuf, rumdl_lib::workspace_index::FileIndex> = HashMap::new();

        for file_path in &file_paths {
            let result = crate::file_processor::process_file_with_index(
                file_path,
                &enabled_rules,
                args.verbose && !args.silent,
                quiet,
                args.silent,
                config,
                cache.as_ref().map(Arc::clone),
                cache_hashes.as_deref(),
            );

            if !result.warnings.is_empty() {
                has_issues = true;
                _files_with_issues += 1;
                _total_issues += result.warnings.len();
                if result
                    .warnings
                    .iter()
                    .any(|w| matches!(w.severity, Severity::Warning | Severity::Error))
                {
                    has_warnings = true;
                }
                if result.warnings.iter().any(|w| w.severity == Severity::Error) {
                    has_errors = true;
                }
                // Transform path for display (relative by default, absolute with --show-full-path)
                let display_path = if args.show_full_path {
                    file_path.clone()
                } else {
                    crate::file_processor::to_display_path(file_path, project_root)
                };
                all_file_warnings.push((display_path, result.warnings));
            }

            // Store FileIndex for cross-file analysis (extracted from single linting pass)
            if needs_cross_file {
                // Canonicalize path for consistent cache key matching
                let canonical = std::fs::canonicalize(file_path).unwrap_or_else(|_| PathBuf::from(file_path));
                file_indices.insert(canonical, result.file_index);
            }
        }

        // Phase 2: Run cross-file checks if needed
        if needs_cross_file && !file_indices.is_empty() {
            let index_start = Instant::now();

            // Load workspace index from cache if available, otherwise start fresh
            let mut workspace_index = workspace_cache_dir
                .and_then(WorkspaceIndex::load_from_cache)
                .unwrap_or_default();

            let loaded_from_cache = workspace_index.file_count() > 0;
            if args.verbose && !args.silent && loaded_from_cache {
                eprintln!(
                    "Loaded workspace index from cache with {} files",
                    workspace_index.file_count()
                );
            }

            // Incremental update: only update files that have changed (stale)
            let mut updated_count = 0;
            let mut skipped_count = 0;
            for (path, file_index) in file_indices {
                if workspace_index.is_file_stale(&path, &file_index.content_hash) {
                    workspace_index.update_file(&path, file_index);
                    updated_count += 1;
                } else {
                    skipped_count += 1;
                }
            }

            // Prune deleted files from workspace index (use canonical paths for matching)
            let current_files: std::collections::HashSet<PathBuf> = file_paths
                .iter()
                .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p)))
                .collect();
            let pruned_count = workspace_index.retain_only(&current_files);

            if args.verbose && !args.silent {
                eprintln!(
                    "Workspace index: {} updated, {} unchanged, {} pruned ({} total) in {:?}",
                    updated_count,
                    skipped_count,
                    pruned_count,
                    workspace_index.file_count(),
                    index_start.elapsed()
                );
            }

            // Run cross-file checks for each file using the FileIndex (no re-parsing needed)
            for (file_path, file_index) in workspace_index.files() {
                if let Ok(cross_file_warnings) = rumdl_lib::run_cross_file_checks(
                    file_path,
                    file_index,
                    &enabled_rules,
                    &workspace_index,
                    Some(config),
                ) && !cross_file_warnings.is_empty()
                {
                    // Transform path for display (must match format used in all_file_warnings)
                    let display_path = if args.show_full_path {
                        file_path.to_string_lossy().to_string()
                    } else {
                        crate::file_processor::to_display_path(&file_path.to_string_lossy(), project_root)
                    };
                    if cross_file_warnings
                        .iter()
                        .any(|w| matches!(w.severity, Severity::Warning | Severity::Error))
                    {
                        has_warnings = true;
                    }
                    if cross_file_warnings.iter().any(|w| w.severity == Severity::Error) {
                        has_errors = true;
                    }
                    // Find existing entry or create new one
                    if let Some((_, warnings)) = all_file_warnings.iter_mut().find(|(p, _)| p == &display_path) {
                        warnings.extend(cross_file_warnings);
                    } else {
                        has_issues = true;
                        _files_with_issues += 1;
                        _total_issues += cross_file_warnings.len();
                        all_file_warnings.push((display_path, cross_file_warnings));
                    }
                }
            }

            // Save workspace index to cache
            if let Some(cache_dir) = workspace_cache_dir {
                if let Err(e) = workspace_index.save_to_cache(cache_dir) {
                    log::warn!("Failed to save workspace index cache: {e}");
                } else if args.verbose && !args.silent {
                    eprintln!(
                        "Saved workspace index cache with {} files",
                        workspace_index.file_count()
                    );
                }
            }
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Format output based on type
        let output = match output_format {
            OutputFormat::Json => rumdl_lib::output::formatters::json::format_all_warnings_as_json(&all_file_warnings),
            OutputFormat::GitLab => rumdl_lib::output::formatters::gitlab::format_gitlab_report(&all_file_warnings),
            OutputFormat::Sarif => rumdl_lib::output::formatters::sarif::format_sarif_report(&all_file_warnings),
            OutputFormat::Junit => {
                rumdl_lib::output::formatters::junit::format_junit_report(&all_file_warnings, duration_ms)
            }
            _ => unreachable!("needs_collection check above guarantees only batch formats here"),
        };

        output_writer.writeln(&output).unwrap_or_else(|e| {
            eprintln!("Error writing output: {e}");
        });

        return (has_issues, has_warnings, has_errors, 0);
    }

    let start_time = Instant::now();

    // Enable parallel processing for both check and fix modes when there are multiple files
    // Each file is processed independently (with all its fix iterations), so parallel processing is safe
    // Single files cannot be parallelized at the file level (would need rule-level parallelization)
    // Cache is thread-safe (Arc<Mutex<>>) so parallel processing works with caching enabled
    let use_parallel = file_paths.len() > 1;

    // Collect all warnings for statistics if requested
    let mut all_warnings_for_stats = Vec::new();

    // For cross-file analysis, we collect FileIndex data during linting (no second pass needed)
    let mut file_indices: HashMap<PathBuf, rumdl_lib::workspace_index::FileIndex> = HashMap::new();

    let (
        mut has_issues,
        mut has_warnings,
        mut has_errors,
        mut files_with_issues,
        files_fixed,
        mut total_issues,
        total_issues_fixed,
        total_fixable_issues,
        total_files_processed,
    ) = if use_parallel {
        // Parallel processing for multiple files with thread-safe cache
        // Each worker locks the mutex ONLY for brief cache get/set operations
        let enabled_rules_arc = Arc::new(enabled_rules.clone());
        let cache_hashes = cache_hashes.clone();

        // Process files in parallel - now includes FileIndex in the result (no second pass needed)
        let results: Vec<_> = file_paths
            .par_iter()
            .map(|file_path| {
                // Clone Arc (cheap - just increments reference count)
                // process_file_with_formatter locks mutex briefly for cache operations
                let result = crate::file_processor::process_file_with_formatter(
                    file_path,
                    &enabled_rules_arc,
                    args.fix_mode,
                    args.diff,
                    args.verbose && !args.silent,
                    quiet,
                    args.silent,
                    &output_format,
                    &output_writer,
                    config,
                    cache.as_ref().map(Arc::clone),
                    project_root,
                    args.show_full_path,
                    cache_hashes.as_deref(),
                );
                (file_path.clone(), result)
            })
            .collect();

        // Aggregate results and extract FileIndex for cross-file analysis
        let mut has_issues = false;
        let mut has_warnings = false;
        let mut has_errors = false;
        let mut files_with_issues = 0;
        let mut files_fixed = 0;
        let mut total_issues = 0;
        let mut total_issues_fixed = 0;
        let mut total_fixable_issues = 0;
        let total_files_processed = results.len();

        for (file_path, (file_has_issues, issues_found, issues_fixed, fixable_issues, warnings, file_index)) in results
        {
            total_issues_fixed += issues_fixed;
            total_fixable_issues += fixable_issues;
            // Always accumulate total_issues from initial count (issues_found), regardless of whether
            // all issues were fixed. This is needed for the summary message "Fixed X/Y issues".
            total_issues += issues_found;

            // Track files that had at least one fix applied (for "Fixed X issues in Y files" message)
            if issues_fixed > 0 {
                files_fixed += 1;
            }

            if file_has_issues {
                has_issues = true;
                files_with_issues += 1;
            }

            if warnings
                .iter()
                .any(|w| matches!(w.severity, Severity::Warning | Severity::Error))
            {
                has_warnings = true;
            }

            if warnings.iter().any(|w| w.severity == Severity::Error) {
                has_errors = true;
            }

            if args.statistics {
                all_warnings_for_stats.extend(warnings);
            }

            // Store FileIndex for cross-file analysis (no second pass needed!)
            if needs_cross_file {
                // Canonicalize path for consistent cache key matching
                let canonical = std::fs::canonicalize(&file_path).unwrap_or_else(|_| PathBuf::from(&file_path));
                file_indices.insert(canonical, file_index);
            }
        }

        (
            has_issues,
            has_warnings,
            has_errors,
            files_with_issues,
            files_fixed,
            total_issues,
            total_issues_fixed,
            total_fixable_issues,
            total_files_processed,
        )
    } else {
        // Sequential processing for single files or when fixing
        let mut has_issues = false;
        let mut has_warnings = false;
        let mut has_errors = false;
        let mut files_with_issues = 0;
        let mut files_fixed = 0;
        let mut total_issues = 0;
        let mut total_issues_fixed = 0;
        let mut total_fixable_issues = 0;
        let mut total_files_processed = 0;

        for file_path in &file_paths {
            // process_file_with_formatter now returns FileIndex (no second pass needed)
            let (file_has_issues, issues_found, issues_fixed, fixable_issues, warnings, file_index) =
                crate::file_processor::process_file_with_formatter(
                    file_path,
                    &enabled_rules,
                    args.fix_mode,
                    args.diff,
                    args.verbose && !args.silent,
                    quiet,
                    args.silent,
                    &output_format,
                    &output_writer,
                    config,
                    cache.as_ref().map(Arc::clone),
                    project_root,
                    args.show_full_path,
                    cache_hashes.as_deref(),
                );

            // Store FileIndex for cross-file analysis (extracted from first pass)
            if needs_cross_file {
                // Canonicalize path for consistent cache key matching
                let canonical = std::fs::canonicalize(file_path).unwrap_or_else(|_| PathBuf::from(file_path));
                file_indices.insert(canonical, file_index);
            }

            total_files_processed += 1;
            total_issues_fixed += issues_fixed;
            total_fixable_issues += fixable_issues;
            // Always accumulate total_issues from initial count (issues_found), regardless of whether
            // all issues were fixed. This is needed for the summary message "Fixed X/Y issues".
            total_issues += issues_found;

            // Track files that had at least one fix applied (for "Fixed X issues in Y files" message)
            if issues_fixed > 0 {
                files_fixed += 1;
            }

            if file_has_issues {
                has_issues = true;
                files_with_issues += 1;
            }

            if warnings
                .iter()
                .any(|w| matches!(w.severity, Severity::Warning | Severity::Error))
            {
                has_warnings = true;
            }

            if warnings.iter().any(|w| w.severity == Severity::Error) {
                has_errors = true;
            }

            if args.statistics {
                all_warnings_for_stats.extend(warnings);
            }
        }

        (
            has_issues,
            has_warnings,
            has_errors,
            files_with_issues,
            files_fixed,
            total_issues,
            total_issues_fixed,
            total_fixable_issues,
            total_files_processed,
        )
    };

    // Phase 2: Run cross-file checks if needed
    if needs_cross_file && !file_indices.is_empty() {
        let index_start = Instant::now();

        // Load workspace index from cache if available, otherwise start fresh
        let mut workspace_index = workspace_cache_dir
            .and_then(WorkspaceIndex::load_from_cache)
            .unwrap_or_default();

        let loaded_from_cache = workspace_index.file_count() > 0;
        if args.verbose && !args.silent && loaded_from_cache {
            eprintln!(
                "Loaded workspace index from cache with {} files",
                workspace_index.file_count()
            );
        }

        // Incremental update: only update files that have changed (stale)
        let mut updated_count = 0;
        let mut skipped_count = 0;
        for (path, file_index) in file_indices {
            if workspace_index.is_file_stale(&path, &file_index.content_hash) {
                workspace_index.update_file(&path, file_index);
                updated_count += 1;
            } else {
                skipped_count += 1;
            }
        }

        // Prune deleted files from workspace index (use canonical paths for matching)
        let current_files: std::collections::HashSet<PathBuf> = file_paths
            .iter()
            .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p)))
            .collect();
        let pruned_count = workspace_index.retain_only(&current_files);

        if args.verbose && !args.silent {
            eprintln!(
                "Workspace index: {} updated, {} unchanged, {} pruned ({} total) in {:?}",
                updated_count,
                skipped_count,
                pruned_count,
                workspace_index.file_count(),
                index_start.elapsed()
            );
        }

        // Run cross-file checks using FileIndex (no re-parsing needed)
        let formatter = output_format.create_formatter();
        for (file_path, file_index) in workspace_index.files() {
            if let Ok(cross_file_warnings) =
                rumdl_lib::run_cross_file_checks(file_path, file_index, &enabled_rules, &workspace_index, Some(config))
                && !cross_file_warnings.is_empty()
            {
                has_issues = true;
                files_with_issues += 1;
                total_issues += cross_file_warnings.len();

                // Check for warning-or-higher severity
                if cross_file_warnings
                    .iter()
                    .any(|w| matches!(w.severity, Severity::Warning | Severity::Error))
                {
                    has_warnings = true;
                }

                // Check for error-severity warnings
                if cross_file_warnings.iter().any(|w| w.severity == Severity::Error) {
                    has_errors = true;
                }

                // Output cross-file warnings
                if !args.silent {
                    let display_path = if args.show_full_path {
                        file_path.to_string_lossy().to_string()
                    } else {
                        crate::file_processor::to_display_path(&file_path.to_string_lossy(), project_root)
                    };
                    let formatted = formatter.format_warnings(&cross_file_warnings, &display_path);
                    if !formatted.is_empty() {
                        output_writer.writeln(&formatted).unwrap_or_else(|e| {
                            eprintln!("Error writing output: {e}");
                        });
                    }
                }

                if args.statistics {
                    all_warnings_for_stats.extend(cross_file_warnings);
                }
            }
        }

        // Save workspace index to cache
        if let Some(cache_dir) = workspace_cache_dir {
            if let Err(e) = workspace_index.save_to_cache(cache_dir) {
                log::warn!("Failed to save workspace index cache: {e}");
            } else if args.verbose && !args.silent {
                eprintln!(
                    "Saved workspace index cache with {} files",
                    workspace_index.file_count()
                );
            }
        }
    }

    let duration = start_time.elapsed();
    let duration_ms = duration.as_secs() * 1000 + duration.subsec_millis() as u64;

    // Print results summary if not in quiet or silent mode
    if !quiet && !args.silent {
        formatter::print_results_from_checkargs(formatter::PrintResultsArgs {
            args,
            has_issues,
            files_with_issues,
            files_fixed,
            total_issues,
            total_issues_fixed,
            total_fixable_issues,
            total_files_processed,
            duration_ms,
        });
    }

    // Print statistics if enabled and not in quiet or silent mode
    if args.statistics && !quiet && !args.silent && !all_warnings_for_stats.is_empty() {
        formatter::print_statistics(&all_warnings_for_stats);
    }

    // Print profiling information if enabled and not in quiet or silent mode
    if args.profile && !quiet && !args.silent {
        match std::panic::catch_unwind(rumdl_lib::profiling::get_report) {
            Ok(report) => {
                output_writer.writeln(&format!("\n{report}")).ok();
            }
            Err(_) => {
                output_writer.writeln("\nProfiling information not available").ok();
            }
        }
    }

    (has_issues, has_warnings, has_errors, total_issues_fixed)
}

/// Run the linter in watch mode, re-running on file changes
pub fn run_watch_mode(args: &crate::CheckArgs, global_config_path: Option<&str>, isolated: bool, quiet: bool) {
    // Always use current directory for config discovery to ensure config files are found
    // when pre-commit or other tools pass relative file paths
    let discovery_dir = None;

    // Load initial configuration
    let mut sourced = crate::load_config_with_cli_error_handling_with_dir(global_config_path, isolated, discovery_dir);

    // Apply CLI argument overrides (e.g., --flavor)
    crate::apply_cli_overrides(&mut sourced, args);

    // Validate configuration
    let all_rules = rumdl_lib::rules::all_rules(&rumdl_config::Config::default());
    let registry = rumdl_config::RuleRegistry::from_rules(&all_rules);
    let validation_warnings = rumdl_config::validate_config_sourced(&sourced, &registry);
    if !validation_warnings.is_empty() && !args.silent {
        for warn in &validation_warnings {
            eprintln!("\x1b[33m[config warning]\x1b[0m {}", warn.message);
        }
    }

    // Extract project_root before converting to Config (for exclude pattern resolution)
    let mut project_root = sourced.project_root.clone();

    // Convert to Config (watch mode doesn't need validation warnings)
    let mut config: rumdl_config::Config = sourced.clone().into_validated_unchecked().into();

    // Configure the file watcher
    let (tx, rx) = channel();

    let mut watcher = match RecommendedWatcher::new(
        tx,
        NotifyConfig::default().with_poll_interval(Duration::from_millis(500)),
    ) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("{}: Failed to create file watcher: {}", "Error".red().bold(), e);
            crate::exit::tool_error();
        }
    };

    // Watch directories for markdown and config files
    let watch_paths = if args.paths.is_empty() {
        vec![".".to_string()]
    } else {
        args.paths.clone()
    };

    for path_str in &watch_paths {
        let path = Path::new(path_str);
        if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
            eprintln!("{}: Failed to watch {}: {}", "Warning".yellow().bold(), path_str, e);
        }
    }

    // Also watch configuration files
    if let Some(config_path) = global_config_path
        && let Err(e) = watcher.watch(Path::new(config_path), RecursiveMode::NonRecursive)
    {
        eprintln!("{}: Failed to watch config file: {}", "Warning".yellow().bold(), e);
    }

    // Perform initial run
    clear_screen();
    let timestamp = Local::now().format("%H:%M:%S");
    println!("[{}] {}...", timestamp, "Starting linter in watch mode".green().bold());
    println!("{}", "Press Ctrl-C to exit".cyan());
    println!();

    let _has_issues = perform_check_run(args, &config, quiet, None, None, project_root.as_deref());
    if !quiet {
        println!("\n{}", "Watching for file changes...".cyan());
    }

    // Main watch loop with improved debouncing
    let debounce_duration = Duration::from_millis(100); // 100ms debounce - responsive while catching most duplicate events

    loop {
        match rx.recv() {
            Ok(event_result) => {
                match event_result {
                    Ok(first_event) => {
                        // Check what kind of change occurred
                        let Some(mut change_kind) = change_detected(&first_event) else {
                            continue;
                        };

                        // Collect all events that occur within the debounce window
                        let start = Instant::now();
                        while start.elapsed() < debounce_duration {
                            // Try to receive more events with a short timeout
                            if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(10)) {
                                // If we get a config change, that takes priority
                                if let Some(kind) = change_detected(&event)
                                    && matches!(kind, ChangeKind::Configuration)
                                {
                                    change_kind = ChangeKind::Configuration;
                                }
                            }
                        }

                        // Handle configuration changes if needed
                        if matches!(change_kind, ChangeKind::Configuration) {
                            // Reload configuration
                            sourced = crate::load_config_with_cli_error_handling_with_dir(
                                global_config_path,
                                isolated,
                                discovery_dir,
                            );

                            // Re-apply CLI argument overrides (e.g., --flavor)
                            crate::apply_cli_overrides(&mut sourced, args);

                            // Re-validate configuration
                            let validation_warnings = rumdl_config::validate_config_sourced(&sourced, &registry);
                            if !validation_warnings.is_empty() && !args.silent {
                                for warn in &validation_warnings {
                                    eprintln!("\x1b[33m[config warning]\x1b[0m {}", warn.message);
                                }
                            }

                            // Update project_root from reloaded config
                            project_root = sourced.project_root.clone();
                            config = sourced.clone().into_validated_unchecked().into();
                        }

                        // Build the header message before clearing
                        let timestamp = chrono::Local::now().format("%H:%M:%S");
                        let header = match change_kind {
                            ChangeKind::Configuration => {
                                format!(
                                    "[{}] {}...\n\n",
                                    timestamp,
                                    "Configuration change detected".yellow().bold()
                                )
                            }
                            ChangeKind::SourceFile => {
                                format!("[{}] {}...\n\n", timestamp, "File change detected".cyan().bold())
                            }
                        };

                        // Clear and immediately print header
                        clear_screen();
                        print!("{header}");
                        let _ = io::stdout().flush();

                        // Re-run the check
                        let _has_issues = perform_check_run(args, &config, quiet, None, None, project_root.as_deref());
                        if !quiet {
                            println!("\n{}", "Watching for file changes...".cyan());
                        }
                    }
                    Err(e) => {
                        eprintln!("{}: Watch error: {}", "Error".red().bold(), e);
                    }
                }
            }
            Err(e) => {
                eprintln!("{}: Failed to receive watch event: {}", "Error".red().bold(), e);
                crate::exit::tool_error();
            }
        }
    }
}
