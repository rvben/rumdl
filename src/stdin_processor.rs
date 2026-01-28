//! Stdin processing for markdown linting

use crate::file_processor;
use colored::*;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::{Rule, Severity};
use std::io::{self, Read};
use std::str::FromStr;

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
    let output_writer = OutputWriter::new(use_stderr, quiet, silent);

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
    content = rumdl_lib::utils::normalize_line_ending(&content, rumdl_lib::utils::LineEnding::Lf);

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

    // Create a lint context for the stdin content
    let ctx = LintContext::new(&content, config.markdown_flavor(), source_file.clone());
    let mut all_warnings = Vec::new();

    // Run all enabled rules on the content
    for rule in rules {
        // Skip rules that indicate they should be skipped (opt-in rules, content-based skipping)
        if rule.should_skip(&ctx) {
            continue;
        }
        match rule.check(&ctx) {
            Ok(warnings) => {
                all_warnings.extend(warnings);
            }
            Err(e) => {
                if !args.silent {
                    eprintln!("Error running rule {}: {}", rule.name(), e);
                }
            }
        }
    }

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
            let warnings_fixed = file_processor::apply_fixes_coordinated(
                rules,
                &all_warnings,
                &mut fixed_content,
                quiet,
                silent,
                config,
            );

            // Denormalize back to original line ending before output (I/O boundary)
            let output_content = rumdl_lib::utils::normalize_line_ending(&fixed_content, original_line_ending);

            // Output the fixed content to stdout
            print!("{output_content}");

            // Re-check the fixed content to see if any issues remain
            let fixed_ctx = LintContext::new(&fixed_content, config.markdown_flavor(), source_file.clone());
            let mut remaining_warnings = Vec::new();
            for rule in rules {
                if rule.should_skip(&fixed_ctx) {
                    continue;
                }
                if let Ok(warnings) = rule.check(&fixed_ctx) {
                    remaining_warnings.extend(warnings);
                }
            }

            // Only show diagnostics to stderr unless silent
            if !silent && !remaining_warnings.is_empty() {
                let formatter = output_format.create_formatter();
                let formatted = formatter.format_warnings(&remaining_warnings, display_filename);
                eprintln!("{formatted}");
                eprintln!(
                    "\n{} issue(s) fixed, {} issue(s) remaining",
                    warnings_fixed,
                    remaining_warnings.len()
                );
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
                OutputFormat::Junit => rumdl_lib::output::formatters::junit::format_junit_report(&file_warnings, 0),
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
                let formatted = formatter.format_warnings(&all_warnings, display_filename);
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
