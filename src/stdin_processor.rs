//! Stdin processing for markdown linting

use crate::file_processor;
use colored::*;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;
use rumdl_lib::rule::{Rule, Severity};
use std::io::{self, Read};

/// Process markdown content from stdin
pub fn process_stdin(rules: &[Box<dyn Rule>], args: &crate::CheckArgs, config: &rumdl_config::Config) {
    use rumdl_lib::output::{OutputFormat, OutputWriter};

    let quiet = args.quiet;
    let silent = args.silent;

    // In check mode, diagnostics go to stderr by default
    // In fix/format modes, fixed content goes to stdout, so diagnostics go to stdout unless --stderr is specified
    let use_stderr = if args.fix_mode != crate::FixMode::Check {
        args.stderr
    } else {
        true
    };
    // Create output writer for linting results
    let output_writer = OutputWriter::new(use_stderr, silent);

    let output_format = match crate::cli_utils::resolve_output_format(args, config) {
        Ok(fmt) => fmt,
        Err(e) => {
            eprintln!("{}: {}", "Error".red().bold(), e);
            exit::tool_error();
        }
    };

    // Read all content from stdin
    let mut content = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut content) {
        if !args.silent {
            eprintln!("Error reading from stdin: {e}");
        }
        exit::violations_found();
    }

    // Detect original line ending before any processing (I/O boundary)
    let original_line_ending = rumdl_lib::utils::detect_line_ending_enum(&content);

    // Normalize to LF for all internal processing
    content = rumdl_lib::utils::normalize_line_ending(&content, rumdl_lib::utils::LineEnding::Lf).into_owned();

    // Validate inline config comments and warn about unknown rules
    if !silent {
        let inline_warnings = rumdl_lib::inline_config::validate_inline_config_rules(&content);
        let display_name = args.stdin_filename.as_deref().unwrap_or("<stdin>");
        for warn in inline_warnings {
            warn.print_warning(display_name);
        }
    }

    // Determine the filename to use for display and context
    let display_filename = args.stdin_filename.as_deref().unwrap_or("<stdin>");

    // Convert stdin-filename to PathBuf for LintContext
    let source_file = args.stdin_filename.as_ref().map(std::path::PathBuf::from);

    // Use per-file flavor if stdin_filename is provided
    let flavor = args
        .stdin_filename
        .as_ref()
        .map(|f| config.get_flavor_for_file(std::path::Path::new(f)))
        .unwrap_or_else(|| config.markdown_flavor());

    // Lint through the same engine as the file path, so inline config
    // overrides, kramdown suppression, inline-disable ranges, and severity
    // overrides behave identically to `rumdl check <file>`.
    let mut all_warnings =
        match rumdl_lib::lint(&content, rules, args.verbose, flavor, source_file.clone(), Some(config)) {
            Ok(warnings) => warnings,
            Err(e) => {
                if !silent {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
                exit::tool_error();
            }
        };

    // Sort warnings by line/column
    all_warnings.sort_by(|a, b| {
        if a.line == b.line {
            a.column.cmp(&b.column)
        } else {
            a.line.cmp(&b.line)
        }
    });

    let has_issues = !all_warnings.is_empty();
    let has_warnings = all_warnings
        .iter()
        .any(|w| matches!(w.severity, Severity::Warning | Severity::Error));
    let has_errors = all_warnings.iter().any(|w| w.severity == Severity::Error);

    // Apply fixes if requested
    if args.fix_mode != crate::FixMode::Check {
        if has_issues {
            let mut fixed_content = content.clone();
            let file_path = args.stdin_filename.as_ref().map(std::path::Path::new);
            let _warnings_fixed = file_processor::apply_fixes_coordinated(
                rules,
                &all_warnings,
                &mut fixed_content,
                quiet,
                silent,
                config,
                file_path,
            );

            // Denormalize back to original line ending before output (I/O boundary)
            let output_content =
                rumdl_lib::utils::normalize_line_ending(&fixed_content, original_line_ending).into_owned();

            // Output the fixed content to stdout
            print!("{output_content}");

            // Re-check the fixed content through the same engine to see if
            // any issues remain. Use same per-file flavor as initial lint.
            let remaining_warnings = rumdl_lib::lint(
                &fixed_content,
                rules,
                args.verbose,
                flavor,
                source_file.clone(),
                Some(config),
            )
            .unwrap_or_default();
            let actual_warnings_fixed =
                file_processor::count_actually_fixed_warnings(rules, config, &all_warnings, &remaining_warnings);

            // Diagnostics always go to stderr in fix mode (stdout has fixed content)
            let fix_writer = OutputWriter::new(true, silent);
            if !remaining_warnings.is_empty() {
                match output_format {
                    // Batch formats: remaining-only warnings
                    OutputFormat::Json | OutputFormat::GitLab | OutputFormat::Sarif | OutputFormat::Junit => {
                        let file_warnings = vec![(display_filename.to_string(), remaining_warnings.clone())];
                        let output = match output_format {
                            OutputFormat::Json => {
                                rumdl_lib::output::formatters::json::format_all_warnings_as_json(&file_warnings)
                            }
                            OutputFormat::GitLab => {
                                rumdl_lib::output::formatters::gitlab::format_gitlab_report(&file_warnings)
                            }
                            OutputFormat::Sarif => {
                                rumdl_lib::output::formatters::sarif::format_sarif_report(&file_warnings)
                            }
                            OutputFormat::Junit => {
                                let all_files = vec![display_filename.to_string()];
                                rumdl_lib::output::formatters::junit::format_junit_report(&file_warnings, &all_files, 0)
                            }
                            _ => unreachable!(),
                        };
                        fix_writer.writeln(&output).unwrap_or_else(|e| {
                            eprintln!("Error writing output: {e}");
                        });
                    }
                    // Human-readable text formats: all warnings with [fixed] labels
                    OutputFormat::Text | OutputFormat::Full => {
                        let mut output = String::new();
                        for warning in &all_warnings {
                            let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");
                            let was_fixed = file_processor::is_rule_cli_fixable(rules, config, rule_name)
                                && warning.fix.is_some()
                                && !remaining_warnings.iter().any(|w| {
                                    w.line == warning.line
                                        && w.column == warning.column
                                        && w.rule_name == warning.rule_name
                                        && w.message == warning.message
                                });

                            let fix_indicator = if was_fixed {
                                " [fixed]".green().to_string()
                            } else {
                                String::new()
                            };

                            use std::fmt::Write;
                            writeln!(
                                output,
                                "{}:{}:{}: {} {}{}",
                                display_filename.blue().underline(),
                                warning.line.to_string().cyan(),
                                warning.column.to_string().cyan(),
                                format!("[{rule_name:5}]").yellow(),
                                warning.message,
                                fix_indicator
                            )
                            .ok();
                        }

                        if output.ends_with('\n') {
                            output.pop();
                        }
                        fix_writer.writeln(&output).unwrap_or_else(|e| {
                            eprintln!("Error writing output: {e}");
                        });
                    }
                    // Other streaming formats: use their formatter with remaining-only
                    _ => {
                        let formatter = output_format.create_formatter();
                        let formatted = formatter.format_warnings_with_content(
                            &remaining_warnings,
                            display_filename,
                            &fixed_content,
                        );
                        fix_writer.writeln(&formatted).unwrap_or_else(|e| {
                            eprintln!("Error writing output: {e}");
                        });
                    }
                }
                if !quiet {
                    fix_writer
                        .writeln(&format!(
                            "\n{} issue(s) fixed, {} issue(s) remaining",
                            actual_warnings_fixed,
                            remaining_warnings.len()
                        ))
                        .ok();
                }
            }

            if args.fix_mode != crate::FixMode::Format {
                let remaining_has_warnings = remaining_warnings
                    .iter()
                    .any(|w| matches!(w.severity, Severity::Warning | Severity::Error));
                let remaining_has_errors = remaining_warnings.iter().any(|w| w.severity == Severity::Error);
                let should_fail = match args.fail_on_mode {
                    crate::FailOn::Never => false,
                    crate::FailOn::Error => remaining_has_errors,
                    crate::FailOn::Warning => remaining_has_warnings,
                    crate::FailOn::Any => !remaining_warnings.is_empty(),
                };
                if should_fail {
                    exit::violations_found();
                }
            }
        } else {
            print!("{content}");
        }

        return;
    }

    // Normal check mode (no fix) - output diagnostics
    // Batch formats need all warnings collected before formatting
    match output_format {
        OutputFormat::Json | OutputFormat::GitLab | OutputFormat::Sarif | OutputFormat::Junit => {
            let file_warnings = vec![(display_filename.to_string(), all_warnings)];
            let output = match output_format {
                OutputFormat::Json => rumdl_lib::output::formatters::json::format_all_warnings_as_json(&file_warnings),
                OutputFormat::GitLab => rumdl_lib::output::formatters::gitlab::format_gitlab_report(&file_warnings),
                OutputFormat::Sarif => rumdl_lib::output::formatters::sarif::format_sarif_report(&file_warnings),
                OutputFormat::Junit => {
                    let all_files = vec![display_filename.to_string()];
                    rumdl_lib::output::formatters::junit::format_junit_report(&file_warnings, &all_files, 0)
                }
                _ => unreachable!("Outer match guarantees only batch formats here"),
            };

            output_writer.writeln(&output).unwrap_or_else(|e| {
                eprintln!("Error writing output: {e}");
            });
        }
        // Streaming formats (Text, Concise, Grouped, JsonLines, GitHub, Pylint, Azure)
        _ => {
            // Use formatter for line-by-line output
            let formatter = output_format.create_formatter();
            if !all_warnings.is_empty() {
                let formatted = formatter.format_warnings_with_content(&all_warnings, display_filename, &content);
                output_writer.writeln(&formatted).unwrap_or_else(|e| {
                    eprintln!("Error writing output: {e}");
                });
            }

            // Print summary if not quiet
            if !quiet {
                if has_issues {
                    output_writer
                        .writeln(&format!(
                            "\nFound {} issue(s) in {}",
                            all_warnings.len(),
                            display_filename
                        ))
                        .ok();
                } else {
                    output_writer
                        .writeln(&format!("No issues found in {display_filename}"))
                        .ok();
                }
            }
        }
    }

    // Exit with error code based on --fail-on setting
    let should_fail = match args.fail_on_mode {
        crate::FailOn::Never => false,
        crate::FailOn::Error => has_errors,
        crate::FailOn::Warning => has_warnings,
        crate::FailOn::Any => has_issues,
    };
    if should_fail {
        exit::violations_found();
    }
}
