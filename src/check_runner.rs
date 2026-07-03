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
use std::sync::Arc;
use std::time::Instant;

/// Context for a single check run, grouping parameters to avoid too many function arguments.
pub struct CheckRunContext<'a> {
    pub args: &'a crate::CheckArgs,
    pub config: &'a rumdl_config::Config,
    pub quiet: bool,
    pub cache: Option<Arc<crate::cache::LintCache>>,
    pub workspace_cache_dir: Option<&'a Path>,
    pub project_root: Option<&'a Path>,
    /// Upper bound for per-directory config grouping. Equals `project_root` for
    /// single/zero-path runs; for multi-path runs with no discovered project
    /// config it is the common-ancestor anchor, so standalone subdirectory configs
    /// are still grouped while `project_root` stays unset (keeping the cache dir,
    /// per-file globs and displayed paths cwd-relative).
    pub grouping_root: Option<&'a Path>,
    /// Inline `--config 'RULE.key=value'` overrides, re-applied to each discovered
    /// subdirectory config so CLI precedence holds across all config groups.
    pub inline_overrides: &'a [toml::Table],
    pub explicit_config: bool,
    pub isolated: bool,
}

/// Perform a single check run.
/// Returns (has_issues, has_warnings, has_errors, total_issues_fixed, had_tool_error):
///   - has_issues: any violations (info, warning, or error)
///   - has_warnings: any Warning or Error severity violations
///   - has_errors: any Error-severity violations
///   - total_issues_fixed: number of issues fixed (or would be fixed in diff mode)
///   - had_tool_error: a file could not be read (missing/unreadable/invalid UTF-8);
///     a tool error that must exit with code 2, not a lint result
pub fn perform_check_run(ctx: &CheckRunContext<'_>) -> (bool, bool, bool, usize, bool) {
    let CheckRunContext {
        args,
        config,
        quiet,
        ref cache,
        workspace_cache_dir,
        project_root,
        grouping_root,
        inline_overrides,
        explicit_config,
        isolated,
    } = *ctx;
    use rumdl_lib::output::OutputWriter;
    use rumdl_lib::rule::Severity;

    // Create output writer for linting results
    let output_writer = OutputWriter::new(args.stderr, args.silent);

    let output_format = match crate::cli_utils::resolve_output_format(args, config) {
        Ok(fmt) => fmt,
        Err(e) => {
            eprintln!("{}: {}", "Error".red().bold(), e);
            // An invalid --output-format is a tool error (exit code 2).
            return (false, false, false, 0, true);
        }
    };

    // Handle stdin input - either explicit --stdin flag or "-" as file argument
    if args.stdin || (args.paths.len() == 1 && args.paths[0] == "-") {
        let enabled_rules = crate::file_processor::get_enabled_rules_from_checkargs(args, config);
        crate::stdin_processor::process_stdin(&enabled_rules, args, config);
        return (false, false, false, 0, false);
    }

    // Find all markdown files to check
    let file_paths = match rumdl_lib::time_function!(
        "check: discover markdown files",
        crate::file_processor::find_markdown_files(&args.paths, args, config, project_root)
    ) {
        Ok(paths) => paths,
        Err(e) => {
            if !args.silent {
                eprintln!("{}: Failed to find markdown files: {}", "Error".red().bold(), e);
            }
            // A target path that does not exist (or is otherwise unreadable) is a
            // tool error (exit code 2), not a lint finding: flag had_tool_error
            // rather than reporting phantom violations.
            return (false, false, false, 0, true);
        }
    };
    if file_paths.is_empty() {
        if !quiet {
            println!("No markdown files found to check.");
        }
        return (false, false, false, 0, false);
    }

    // Resolve files into config groups (per-directory config discovery)
    let config_groups = rumdl_lib::time_function!(
        "check: resolve config groups",
        crate::resolution::resolve_config_groups(
            &file_paths,
            config,
            args,
            &crate::resolution::ResolutionRoots {
                grouping_root,
                project_root,
            },
            inline_overrides,
            cache,
            explicit_config || isolated,
        )
    );

    // Build file → group index mapping for cross-file analysis (Phase 2)
    let file_group_map: HashMap<PathBuf, usize> = rumdl_lib::time_function!(
        "check: build file group map",
        config_groups
            .iter()
            .enumerate()
            .flat_map(|(gi, g)| {
                g.files.iter().map(move |f| {
                    let canonical = std::fs::canonicalize(f).unwrap_or_else(|_| PathBuf::from(f));
                    (canonical, gi)
                })
            })
            .collect()
    );

    // Check if any enabled rule across any group needs cross-file analysis
    let needs_cross_file = config_groups
        .iter()
        .any(|g| g.rules.iter().any(|r| r.cross_file_scope() != CrossFileScope::None));

    // Load the workspace index before file processing so cache-hit files can reuse
    // their existing FileIndex when the content hash still matches.
    let cached_workspace_index = if needs_cross_file {
        Some(Arc::new(rumdl_lib::time_function!(
            "workspace: load index cache",
            workspace_cache_dir
                .and_then(WorkspaceIndex::load_from_cache)
                .unwrap_or_default()
        )))
    } else {
        None
    };

    // Batch output formats need to collect all warnings before formatting
    let needs_collection = output_format.is_batch();

    // Some batch formats report passing files too and need every checked
    // file's path, not just the ones with warnings.
    let collect_all_files = output_format.needs_all_files();

    // Use a silent output writer for batch formats so per-file output is suppressed
    // (warnings are collected and formatted as a batch at the end)
    let batch_output_writer;
    let effective_output_writer = if needs_collection {
        batch_output_writer = OutputWriter::new(false, true);
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
    let mut file_indices: HashMap<PathBuf, (rumdl_lib::workspace_index::FileIndex, bool)> = HashMap::new();

    // Track files that already have issues from Phase 1 to avoid double-counting in Phase 2
    let mut files_already_with_issues: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    // Build flat list of (file_path, group_index) for parallel processing
    let file_tasks: Vec<(usize, &str)> = rumdl_lib::time_function!(
        "check: build file tasks",
        config_groups
            .iter()
            .enumerate()
            .flat_map(|(gi, g)| g.files.iter().map(move |f| (gi, f.as_str())))
            .collect()
    );

    // For batch formats, collect (display_path, warnings) tuples
    let mut batch_file_warnings: Vec<(String, Vec<rumdl_lib::rule::LintWarning>)> = Vec::new();
    // For JUnit, the display paths of every checked file (clean and dirty).
    let mut batch_all_files: Vec<String> = Vec::new();

    // Set when a file could not be read (missing, unreadable, invalid UTF-8).
    // This is a tool error (exit code 2), distinct from lint violations, so it
    // is tracked separately from `has_issues`/`has_errors` and reported to the
    // caller regardless of which processing branch ran.
    let mut had_tool_error = false;

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
        let results: Vec<_> = rumdl_lib::time_function!(
            "check: process files parallel",
            file_tasks
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
                        cached_workspace_index.as_ref().map(Arc::clone),
                        project_root,
                        args.show_full_path,
                        group.cache_hashes.as_deref(),
                    );
                    (file_path.to_string(), result)
                })
                .collect()
        );

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

        rumdl_lib::time_section!("check: aggregate file results", {
            for (file_path, result) in results {
                let crate::file_processor::FileProcessResult {
                    has_issues: file_has_issues,
                    issues_found,
                    issues_fixed,
                    summary_issues_fixed: file_summary_issues_fixed,
                    fixable_issues,
                    warnings,
                    file_index,
                    file_index_reused,
                    errored: file_errored,
                } = result;

                if file_errored {
                    had_tool_error = true;
                }

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

                // Collect warnings for batch output formats; for JUnit also record every
                // checked file so passing files appear in the report.
                if needs_collection && (collect_all_files || !warnings.is_empty()) {
                    let display_path =
                        crate::file_processor::resolve_display_path(&file_path, args.show_full_path, project_root);
                    if collect_all_files {
                        batch_all_files.push(display_path.clone());
                    }
                    if !warnings.is_empty() {
                        batch_file_warnings.push((display_path, warnings.clone()));
                    }
                }

                if args.statistics {
                    all_warnings_for_stats.extend(warnings);
                }

                if needs_cross_file {
                    file_indices.insert(canonical, (file_index, file_index_reused));
                }
            }
        });

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

        rumdl_lib::time_section!("check: process files sequential", {
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
                    file_index_reused,
                    errored: file_errored,
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
                    cached_workspace_index.as_ref().map(Arc::clone),
                    project_root,
                    args.show_full_path,
                    group.cache_hashes.as_deref(),
                );

                if file_errored {
                    had_tool_error = true;
                }

                if needs_cross_file {
                    let canonical = std::fs::canonicalize(file_path).unwrap_or_else(|_| PathBuf::from(file_path));
                    file_indices.insert(canonical, (file_index, file_index_reused));
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

                // Collect warnings for batch output formats; for JUnit also record every
                // checked file so passing files appear in the report.
                if needs_collection && (collect_all_files || !warnings.is_empty()) {
                    let display_path =
                        crate::file_processor::resolve_display_path(file_path, args.show_full_path, project_root);
                    if collect_all_files {
                        batch_all_files.push(display_path.clone());
                    }
                    if !warnings.is_empty() {
                        batch_file_warnings.push((display_path, warnings.clone()));
                    }
                }

                if args.statistics {
                    all_warnings_for_stats.extend(warnings);
                }
            }
        });

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

        // Reuse the workspace index snapshot loaded before file processing.
        let mut workspace_index = cached_workspace_index
            .as_ref()
            .map(|index| (**index).clone())
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
        rumdl_lib::time_section!("workspace: update stale file indexes", {
            for (path, (file_index, file_index_reused)) in file_indices {
                if !file_index_reused || workspace_index.is_file_stale(&path, &file_index.content_hash) {
                    workspace_index.update_file(&path, file_index);
                    updated_count += 1;
                } else {
                    skipped_count += 1;
                }
            }
        });

        // Prune deleted files from workspace index (use canonical paths for matching)
        let current_files: std::collections::HashSet<PathBuf> = rumdl_lib::time_function!(
            "workspace: canonicalize current files",
            file_paths
                .iter()
                .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p)))
                .collect()
        );
        let pruned_count = rumdl_lib::time_function!(
            "workspace: prune deleted files",
            workspace_index.retain_only(&current_files)
        );

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
        rumdl_lib::time_section!("workspace: run cross-file checks", {
            // Iterate in path order so cross-file diagnostics are emitted in a
            // stable order across runs (the workspace index is a HashMap).
            for (file_path, file_index) in workspace_index.files_sorted() {
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

                    let display_path = crate::file_processor::resolve_display_path(
                        &file_path.to_string_lossy(),
                        args.show_full_path,
                        project_root,
                    );

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
                            let formatted = formatter.format_warnings_with_content(
                                &cross_file_warnings,
                                &display_path,
                                &file_content,
                            );
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
        });

        // Save workspace index to cache
        if let Some(cache_dir) = workspace_cache_dir {
            if let Err(e) =
                rumdl_lib::time_function!("workspace: save index cache", workspace_index.save_to_cache(cache_dir))
            {
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
    if let Some(output) = output_format.format_batch(
        &batch_file_warnings,
        &batch_all_files,
        start_time.elapsed().as_millis() as u64,
    ) {
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
            had_tool_error,
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

    // Print profiling information when explicitly requested. This intentionally
    // ignores --silent because --profile is itself an explicit output request.
    if args.profile && !quiet {
        match std::panic::catch_unwind(rumdl_lib::profiling::get_report) {
            Ok(report) => {
                if args.stderr {
                    eprintln!("\n{report}");
                } else {
                    println!("\n{report}");
                }
            }
            Err(_) => {
                if args.stderr {
                    eprintln!("\nProfiling information not available");
                } else {
                    println!("\nProfiling information not available");
                }
            }
        }
    }

    (has_issues, has_warnings, has_errors, total_issues_fixed, had_tool_error)
}
