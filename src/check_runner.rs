//! Core check/fix execution logic.
//!
//! This module contains the main check run logic shared by both
//! the `check` command and watch mode.

use crate::formatter;
use colored::*;
use rayon::prelude::*;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::rule::CrossFileScope;
use rumdl_lib::workspace_index::WorkspaceIndex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

/// Context for a single check run, grouping parameters to avoid too many function arguments.
pub struct CheckRunContext<'a> {
    pub args: &'a crate::CheckArgs,
    pub config: &'a rumdl_config::Config,
    pub quiet: bool,
    pub cache: Option<Arc<std::sync::Mutex<crate::cache::LintCache>>>,
    pub workspace_cache_dir: Option<&'a Path>,
    pub project_root: Option<&'a Path>,
    pub explicit_config: bool,
    pub isolated: bool,
}

/// Perform a single check run.
/// Returns (has_issues, has_warnings, has_errors, total_issues_fixed):
///   - has_issues: any violations (info, warning, or error)
///   - has_warnings: any Warning or Error severity violations
///   - has_errors: any Error-severity violations
///   - total_issues_fixed: number of issues fixed (or would be fixed in diff mode)
pub fn perform_check_run(ctx: &CheckRunContext<'_>) -> (bool, bool, bool, usize) {
    let CheckRunContext {
        args,
        config,
        quiet,
        ref cache,
        workspace_cache_dir,
        project_root,
        explicit_config,
        isolated,
    } = *ctx;
    use rumdl_lib::output::{OutputFormat, OutputWriter};
    use rumdl_lib::rule::Severity;

    // Create output writer for linting results
    let output_writer = OutputWriter::new(args.stderr, quiet, args.silent);

    // Read RUMDL_OUTPUT_FORMAT env var (if set)
    let env_output_format = std::env::var("RUMDL_OUTPUT_FORMAT").ok();

    // Determine output format with precedence: CLI → env var → config → legacy → default
    let output_format = if let Some(fmt) = args.output_format {
        fmt.into()
    } else {
        let output_format_str = env_output_format
            .as_deref()
            .or(config.global.output_format.as_deref())
            .or({
                // Legacy support: map --output json to --output-format json
                match args.output {
                    crate::cli_types::Output::Json => Some("json"),
                    crate::cli_types::Output::Text => None,
                }
            })
            .unwrap_or("text");

        match OutputFormat::from_str(output_format_str) {
            Ok(fmt) => fmt,
            Err(e) => {
                eprintln!("{}: {}", "Error".red().bold(), e);
                return (true, true, true, 0);
            }
        }
    };

    // Handle stdin input - either explicit --stdin flag or "-" as file argument
    if args.stdin || (args.paths.len() == 1 && args.paths[0] == "-") {
        let enabled_rules = crate::file_processor::get_enabled_rules_from_checkargs(args, config);
        crate::stdin_processor::process_stdin(&enabled_rules, args, config);
        return (false, false, false, 0);
    }

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

    // Resolve files into config groups (per-directory config discovery)
    let config_groups = crate::resolution::resolve_config_groups(
        &file_paths,
        config,
        args,
        project_root,
        cache,
        explicit_config,
        isolated,
    );

    // Build file → group index mapping for cross-file analysis (Phase 2)
    let file_group_map: HashMap<PathBuf, usize> = config_groups
        .iter()
        .enumerate()
        .flat_map(|(gi, g)| {
            g.files.iter().map(move |f| {
                let canonical = std::fs::canonicalize(f).unwrap_or_else(|_| PathBuf::from(f));
                (canonical, gi)
            })
        })
        .collect();

    // Check if any enabled rule across any group needs cross-file analysis
    let needs_cross_file = config_groups
        .iter()
        .any(|g| g.rules.iter().any(|r| r.cross_file_scope() != CrossFileScope::None));

    // Batch output formats need to collect all warnings before formatting
    let needs_collection = matches!(
        output_format,
        OutputFormat::Json | OutputFormat::GitLab | OutputFormat::Sarif | OutputFormat::Junit
    );

    // Use a silent output writer for batch formats so per-file output is suppressed
    // (warnings are collected and formatted as a batch at the end)
    let batch_output_writer;
    let effective_output_writer = if needs_collection {
        batch_output_writer = OutputWriter::new(false, true, true);
        &batch_output_writer
    } else {
        &output_writer
    };

    let start_time = Instant::now();

    // Enable parallel processing for both check and fix modes when there are multiple files
    let use_parallel = file_paths.len() > 1;

    // Collect all warnings for statistics if requested
    let mut all_warnings_for_stats = Vec::new();

    // For cross-file analysis, we collect FileIndex data during linting (no second pass needed)
    let mut file_indices: HashMap<PathBuf, rumdl_lib::workspace_index::FileIndex> = HashMap::new();

    // Track files that already have issues from Phase 1 to avoid double-counting in Phase 2
    let mut files_already_with_issues: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    // Build flat list of (file_path, group_index) for parallel processing
    let file_tasks: Vec<(usize, &str)> = config_groups
        .iter()
        .enumerate()
        .flat_map(|(gi, g)| g.files.iter().map(move |f| (gi, f.as_str())))
        .collect();

    // For batch formats, collect (display_path, warnings) tuples
    let mut batch_file_warnings: Vec<(String, Vec<rumdl_lib::rule::LintWarning>)> = Vec::new();

    let (
        mut has_issues,
        mut has_warnings,
        mut has_errors,
        mut files_with_issues,
        files_fixed,
        mut total_issues,
        summary_issues_fixed,
        total_issues_fixed,
        total_fixable_issues,
        total_files_processed,
    ) = if use_parallel {
        // Parallel processing for multiple files with thread-safe cache
        let results: Vec<_> = file_tasks
            .par_iter()
            .map(|(gi, file_path)| {
                let group = &config_groups[*gi];
                let result = crate::file_processor::process_file_with_formatter(
                    file_path,
                    &group.rules,
                    args.fix_mode,
                    args.diff,
                    args.verbose && !args.silent,
                    quiet,
                    args.silent,
                    &output_format,
                    effective_output_writer,
                    &group.config,
                    cache.as_ref().map(Arc::clone),
                    project_root,
                    args.show_full_path,
                    group.cache_hashes.as_deref(),
                );
                (file_path.to_string(), result)
            })
            .collect();

        // Aggregate results and extract FileIndex for cross-file analysis
        let mut has_issues = false;
        let mut has_warnings = false;
        let mut has_errors = false;
        let mut files_with_issues = 0;
        let mut files_fixed = 0;
        let mut total_issues = 0;
        let mut summary_issues_fixed = 0;
        let mut total_issues_fixed = 0;
        let mut total_fixable_issues = 0;
        let total_files_processed = results.len();

        for (file_path, result) in results {
            let crate::file_processor::FileProcessResult {
                has_issues: file_has_issues,
                issues_found,
                issues_fixed,
                summary_issues_fixed: file_summary_issues_fixed,
                fixable_issues,
                warnings,
                file_index,
            } = result;

            summary_issues_fixed += file_summary_issues_fixed;
            total_issues_fixed += issues_fixed;
            total_fixable_issues += fixable_issues;
            total_issues += issues_found;

            if issues_fixed > 0 {
                files_fixed += 1;
            }

            let canonical = std::fs::canonicalize(&file_path).unwrap_or_else(|_| PathBuf::from(&file_path));

            if file_has_issues {
                has_issues = true;
                files_with_issues += 1;
                files_already_with_issues.insert(canonical.clone());
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

            // Collect warnings for batch output formats
            if needs_collection && !warnings.is_empty() {
                let display_path = if args.show_full_path {
                    file_path.clone()
                } else {
                    crate::file_processor::to_display_path(&file_path, project_root)
                };
                batch_file_warnings.push((display_path, warnings.clone()));
            }

            if args.statistics {
                all_warnings_for_stats.extend(warnings);
            }

            if needs_cross_file {
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
            summary_issues_fixed,
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
        let mut summary_issues_fixed = 0;
        let mut total_issues_fixed = 0;
        let mut total_fixable_issues = 0;
        let mut total_files_processed = 0;

        for &(gi, file_path) in &file_tasks {
            let group = &config_groups[gi];
            let crate::file_processor::FileProcessResult {
                has_issues: file_has_issues,
                issues_found,
                issues_fixed,
                summary_issues_fixed: file_summary_issues_fixed,
                fixable_issues,
                warnings,
                file_index,
            } = crate::file_processor::process_file_with_formatter(
                file_path,
                &group.rules,
                args.fix_mode,
                args.diff,
                args.verbose && !args.silent,
                quiet,
                args.silent,
                &output_format,
                effective_output_writer,
                &group.config,
                cache.as_ref().map(Arc::clone),
                project_root,
                args.show_full_path,
                group.cache_hashes.as_deref(),
            );

            if needs_cross_file {
                let canonical = std::fs::canonicalize(file_path).unwrap_or_else(|_| PathBuf::from(file_path));
                file_indices.insert(canonical, file_index);
            }

            total_files_processed += 1;
            summary_issues_fixed += file_summary_issues_fixed;
            total_issues_fixed += issues_fixed;
            total_fixable_issues += fixable_issues;
            total_issues += issues_found;

            if issues_fixed > 0 {
                files_fixed += 1;
            }

            if file_has_issues {
                has_issues = true;
                files_with_issues += 1;
                let canonical = std::fs::canonicalize(file_path).unwrap_or_else(|_| PathBuf::from(file_path));
                files_already_with_issues.insert(canonical);
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

            // Collect warnings for batch output formats
            if needs_collection && !warnings.is_empty() {
                let display_path = if args.show_full_path {
                    file_path.to_string()
                } else {
                    crate::file_processor::to_display_path(file_path, project_root)
                };
                batch_file_warnings.push((display_path, warnings.clone()));
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
            summary_issues_fixed,
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

        // Run cross-file checks using per-file config group rules
        let formatter = output_format.create_formatter();
        for (file_path, file_index) in workspace_index.files() {
            // Use the file's own config group for cross-file rules
            let (cf_rules, cf_config) = match file_group_map.get(file_path) {
                Some(&gi) => (&config_groups[gi].rules, &config_groups[gi].config),
                None => continue,
            };

            if let Ok(cross_file_warnings) =
                rumdl_lib::run_cross_file_checks(file_path, file_index, cf_rules, &workspace_index, Some(cf_config))
                && !cross_file_warnings.is_empty()
            {
                has_issues = true;
                if !files_already_with_issues.contains(file_path) {
                    files_with_issues += 1;
                }
                total_issues += cross_file_warnings.len();

                if cross_file_warnings
                    .iter()
                    .any(|w| matches!(w.severity, Severity::Warning | Severity::Error))
                {
                    has_warnings = true;
                }

                if cross_file_warnings.iter().any(|w| w.severity == Severity::Error) {
                    has_errors = true;
                }

                let display_path = if args.show_full_path {
                    file_path.to_string_lossy().to_string()
                } else {
                    crate::file_processor::to_display_path(&file_path.to_string_lossy(), project_root)
                };

                if needs_collection {
                    // Collect cross-file warnings for batch output
                    if let Some((_, warnings)) = batch_file_warnings.iter_mut().find(|(p, _)| p == &display_path) {
                        warnings.extend(cross_file_warnings.clone());
                    } else {
                        batch_file_warnings.push((display_path, cross_file_warnings.clone()));
                    }
                } else {
                    // Stream cross-file warnings immediately
                    if !args.silent {
                        let file_content = std::fs::read_to_string(file_path).unwrap_or_default();
                        let formatted =
                            formatter.format_warnings_with_content(&cross_file_warnings, &display_path, &file_content);
                        if !formatted.is_empty() {
                            output_writer.writeln(&formatted).unwrap_or_else(|e| {
                                eprintln!("Error writing output: {e}");
                            });
                        }
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

    // Emit batch output for collection formats
    if needs_collection {
        let duration_ms = start_time.elapsed().as_millis() as u64;

        let output = match output_format {
            OutputFormat::Json => {
                rumdl_lib::output::formatters::json::format_all_warnings_as_json(&batch_file_warnings)
            }
            OutputFormat::GitLab => rumdl_lib::output::formatters::gitlab::format_gitlab_report(&batch_file_warnings),
            OutputFormat::Sarif => rumdl_lib::output::formatters::sarif::format_sarif_report(&batch_file_warnings),
            OutputFormat::Junit => {
                rumdl_lib::output::formatters::junit::format_junit_report(&batch_file_warnings, duration_ms)
            }
            _ => unreachable!("needs_collection check above guarantees only batch formats here"),
        };

        output_writer.writeln(&output).unwrap_or_else(|e| {
            eprintln!("Error writing output: {e}");
        });
    }

    let duration = start_time.elapsed();
    let duration_ms = duration.as_secs() * 1000 + duration.subsec_millis() as u64;

    // Print results summary if not in quiet or silent mode
    // Skip for batch formats to keep stdout as pure structured output
    if !quiet && !args.silent && !needs_collection && !output_format.is_machine_readable() {
        formatter::print_results_from_checkargs(formatter::PrintResultsArgs {
            args,
            has_issues,
            files_with_issues,
            files_fixed,
            total_issues,
            summary_issues_fixed,
            total_issues_fixed,
            total_fixable_issues,
            total_files_processed,
            duration_ms,
        });
    }

    // Print statistics if enabled and not in quiet or silent mode
    if args.statistics
        && !quiet
        && !args.silent
        && !needs_collection
        && !output_format.is_machine_readable()
        && !all_warnings_for_stats.is_empty()
    {
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
