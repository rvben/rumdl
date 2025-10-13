//! Watch mode functionality for continuous linting

use crate::formatter;
use chrono::Local;
use colored::*;
use notify::{Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rayon::prelude::*;
use rumdl_lib::config as rumdl_config;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
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
pub fn perform_check_run(args: &crate::CheckArgs, config: &rumdl_config::Config, quiet: bool) -> bool {
    use rumdl_lib::output::{OutputFormat, OutputWriter};

    // Create output writer for linting results
    let output_writer = OutputWriter::new(args.stderr, quiet, args.silent);

    // Determine output format
    let output_format_str = args
        .output_format
        .as_deref()
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
            return true; // Has errors
        }
    };

    // Initialize rules with configuration
    let enabled_rules = crate::file_processor::get_enabled_rules_from_checkargs(args, config);

    // Handle stdin input - either explicit --stdin flag or "-" as file argument
    if args.stdin || (args.paths.len() == 1 && args.paths[0] == "-") {
        crate::stdin_processor::process_stdin(&enabled_rules, args, config);
        return false; // stdin processing handles its own exit codes
    }

    // Find all markdown files to check
    let file_paths = match crate::file_processor::find_markdown_files(&args.paths, args, config) {
        Ok(paths) => paths,
        Err(e) => {
            if !args.silent {
                eprintln!("{}: Failed to find markdown files: {}", "Error".red().bold(), e);
            }
            return true; // Has errors
        }
    };
    if file_paths.is_empty() {
        if !quiet {
            println!("No markdown files found to check.");
        }
        return false;
    }

    // For formats that need to collect all warnings first
    let needs_collection = matches!(
        output_format,
        OutputFormat::Json | OutputFormat::GitLab | OutputFormat::Sarif | OutputFormat::Junit
    );

    if needs_collection {
        let start_time = Instant::now();
        let mut all_file_warnings = Vec::new();
        let mut has_issues = false;
        let mut _files_with_issues = 0;
        let mut _total_issues = 0;

        for file_path in &file_paths {
            let warnings = crate::file_processor::process_file_collect_warnings(
                file_path,
                &enabled_rules,
                args._fix,
                args.verbose && !args.silent,
                quiet,
                config,
            );

            if !warnings.is_empty() {
                has_issues = true;
                _files_with_issues += 1;
                _total_issues += warnings.len();
                all_file_warnings.push((file_path.clone(), warnings));
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
            _ => unreachable!(),
        };

        output_writer.writeln(&output).unwrap_or_else(|e| {
            eprintln!("Error writing output: {e}");
        });

        return has_issues;
    }

    let start_time = Instant::now();

    // Choose processing strategy based on file count and fix mode
    // Also check if it's a single small file to avoid parallel overhead
    let single_small_file = if file_paths.len() == 1 {
        if let Ok(metadata) = fs::metadata(&file_paths[0]) {
            metadata.len() < 10_000 // 10KB threshold
        } else {
            false
        }
    } else {
        false
    };

    let use_parallel = file_paths.len() > 1 && !args._fix && !single_small_file; // Don't parallelize fixes or small files

    // Collect all warnings for statistics if requested
    let mut all_warnings_for_stats = Vec::new();

    let (has_issues, files_with_issues, total_issues, total_issues_fixed, total_fixable_issues, total_files_processed) =
        if use_parallel {
            // Parallel processing for multiple files without fixes
            let enabled_rules_arc = Arc::new(enabled_rules);

            let results: Vec<_> = file_paths
                .par_iter()
                .map(|file_path| {
                    crate::file_processor::process_file_with_formatter(
                        file_path,
                        &enabled_rules_arc,
                        args._fix,
                        args.diff,
                        args.verbose && !args.silent,
                        quiet,
                        &output_format,
                        &output_writer,
                        config,
                    )
                })
                .collect();

            // Aggregate results
            let mut has_issues = false;
            let mut files_with_issues = 0;
            let mut total_issues = 0;
            let mut total_issues_fixed = 0;
            let mut total_fixable_issues = 0;
            let total_files_processed = results.len();

            for (file_has_issues, issues_found, issues_fixed, fixable_issues, warnings) in results {
                total_issues_fixed += issues_fixed;
                total_fixable_issues += fixable_issues;

                if file_has_issues {
                    has_issues = true;
                    files_with_issues += 1;
                    total_issues += issues_found;
                }

                if args.statistics {
                    all_warnings_for_stats.extend(warnings);
                }
            }

            (
                has_issues,
                files_with_issues,
                total_issues,
                total_issues_fixed,
                total_fixable_issues,
                total_files_processed,
            )
        } else {
            // Sequential processing for single files or when fixing
            let mut has_issues = false;
            let mut files_with_issues = 0;
            let mut total_issues = 0;
            let mut total_issues_fixed = 0;
            let mut total_fixable_issues = 0;
            let mut total_files_processed = 0;

            for file_path in &file_paths {
                let (file_has_issues, issues_found, issues_fixed, fixable_issues, warnings) =
                    crate::file_processor::process_file_with_formatter(
                        file_path,
                        &enabled_rules,
                        args._fix,
                        args.diff,
                        args.verbose && !args.silent,
                        quiet,
                        &output_format,
                        &output_writer,
                        config,
                    );

                total_files_processed += 1;
                total_issues_fixed += issues_fixed;
                total_fixable_issues += fixable_issues;

                if file_has_issues {
                    has_issues = true;
                    files_with_issues += 1;
                    total_issues += issues_found;
                }

                if args.statistics {
                    all_warnings_for_stats.extend(warnings);
                }
            }

            (
                has_issues,
                files_with_issues,
                total_issues,
                total_issues_fixed,
                total_fixable_issues,
                total_files_processed,
            )
        };

    let duration = start_time.elapsed();
    let duration_ms = duration.as_secs() * 1000 + duration.subsec_millis() as u64;

    // Print results summary if not in quiet mode
    if !quiet {
        formatter::print_results_from_checkargs(formatter::PrintResultsArgs {
            args,
            has_issues,
            files_with_issues,
            total_issues,
            total_issues_fixed,
            total_fixable_issues,
            total_files_processed,
            duration_ms,
        });
    }

    // Print statistics if enabled and not in quiet mode
    if args.statistics && !quiet && !all_warnings_for_stats.is_empty() {
        formatter::print_statistics(&all_warnings_for_stats);
    }

    // Print profiling information if enabled and not in quiet mode
    if args.profile && !quiet {
        match std::panic::catch_unwind(rumdl_lib::profiling::get_report) {
            Ok(report) => {
                output_writer.writeln(&format!("\n{report}")).ok();
            }
            Err(_) => {
                output_writer.writeln("\nProfiling information not available").ok();
            }
        }
    }

    has_issues
}

/// Run the linter in watch mode, re-running on file changes
pub fn run_watch_mode(args: &crate::CheckArgs, global_config_path: Option<&str>, isolated: bool, quiet: bool) {
    // Always use current directory for config discovery to ensure config files are found
    // when pre-commit or other tools pass relative file paths
    let discovery_dir = None;

    // Load initial configuration
    let mut sourced = crate::load_config_with_cli_error_handling_with_dir(global_config_path, isolated, discovery_dir);

    // Validate configuration
    let all_rules = rumdl_lib::rules::all_rules(&rumdl_config::Config::default());
    let registry = rumdl_config::RuleRegistry::from_rules(&all_rules);
    let validation_warnings = rumdl_config::validate_config_sourced(&sourced, &registry);
    if !validation_warnings.is_empty() && !args.silent {
        for warn in &validation_warnings {
            eprintln!("\x1b[33m[config warning]\x1b[0m {}", warn.message);
        }
    }

    let mut config: rumdl_config::Config = sourced.clone().into();

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

    let _has_issues = perform_check_run(args, &config, quiet);
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

                            // Re-validate configuration
                            let validation_warnings = rumdl_config::validate_config_sourced(&sourced, &registry);
                            if !validation_warnings.is_empty() && !args.silent {
                                for warn in &validation_warnings {
                                    eprintln!("\x1b[33m[config warning]\x1b[0m {}", warn.message);
                                }
                            }

                            config = sourced.clone().into();
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
                        let _has_issues = perform_check_run(args, &config, quiet);
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
