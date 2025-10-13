//! Stdin processing for markdown linting

use crate::file_processor;
use colored::*;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use std::io::{self, Read};
use std::str::FromStr;

/// Process markdown content from stdin
pub fn process_stdin(rules: &[Box<dyn Rule>], args: &crate::CheckArgs, config: &rumdl_config::Config) {
    use rumdl_lib::output::{OutputFormat, OutputWriter};

    // If silent mode is enabled, also enable quiet mode
    let quiet = args.quiet || args.silent;

    // In check mode without --fix, diagnostics should go to stderr by default
    // In fix mode, fixed content goes to stdout, so diagnostics also go to stdout unless --stderr is specified
    let use_stderr = if args._fix {
        args.stderr
    } else {
        true // Check mode: diagnostics to stderr by default
    };
    // Create output writer for linting results
    let output_writer = OutputWriter::new(use_stderr, quiet, args.silent);

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

    // Determine the filename to use for display and context
    let display_filename = args.stdin_filename.as_deref().unwrap_or("<stdin>");

    // Set RUMDL_FILE_PATH if stdin-filename is provided
    // This allows rules like MD057 to know the file location for relative path checking
    if let Some(ref filename) = args.stdin_filename {
        unsafe {
            std::env::set_var("RUMDL_FILE_PATH", filename);
        }
    }

    // Create a lint context for the stdin content
    let ctx = LintContext::new(&content, config.markdown_flavor());
    let mut all_warnings = Vec::new();

    // Run all enabled rules on the content
    for rule in rules {
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

    // Apply fixes if requested
    if args._fix {
        if has_issues {
            let mut fixed_content = content.clone();
            let warnings_fixed =
                file_processor::apply_fixes_coordinated(rules, &all_warnings, &mut fixed_content, quiet, config);

            // Denormalize back to original line ending before output (I/O boundary)
            let output_content = rumdl_lib::utils::normalize_line_ending(&fixed_content, original_line_ending);

            // Output the fixed content to stdout
            print!("{output_content}");

            // Re-check the fixed content to see if any issues remain
            let fixed_ctx = LintContext::new(&fixed_content, config.markdown_flavor());
            let mut remaining_warnings = Vec::new();
            for rule in rules {
                if let Ok(warnings) = rule.check(&fixed_ctx) {
                    remaining_warnings.extend(warnings);
                }
            }

            // Only show diagnostics to stderr if not in quiet mode
            if !quiet && !remaining_warnings.is_empty() {
                let formatter = output_format.create_formatter();
                let formatted = formatter.format_warnings(&remaining_warnings, display_filename);
                eprintln!("{formatted}");
                eprintln!(
                    "\n{} issue(s) fixed, {} issue(s) remaining",
                    warnings_fixed,
                    remaining_warnings.len()
                );
            }

            // Exit with success if all issues were fixed, error if issues remain
            if !remaining_warnings.is_empty() {
                exit::violations_found();
            }
        } else {
            // No issues found, output the original content unchanged
            print!("{content}");
        }

        // Clean up environment variable
        if args.stdin_filename.is_some() {
            unsafe {
                std::env::remove_var("RUMDL_FILE_PATH");
            }
        }
        return;
    }

    // Normal check mode (no fix) - output diagnostics
    // For formats that need collection
    if matches!(
        output_format,
        OutputFormat::Json | OutputFormat::GitLab | OutputFormat::Sarif | OutputFormat::Junit
    ) {
        let file_warnings = vec![(display_filename.to_string(), all_warnings)];
        let output = match output_format {
            OutputFormat::Json => rumdl_lib::output::formatters::json::format_all_warnings_as_json(&file_warnings),
            OutputFormat::GitLab => rumdl_lib::output::formatters::gitlab::format_gitlab_report(&file_warnings),
            OutputFormat::Sarif => rumdl_lib::output::formatters::sarif::format_sarif_report(&file_warnings),
            OutputFormat::Junit => rumdl_lib::output::formatters::junit::format_junit_report(&file_warnings, 0),
            _ => unreachable!(),
        };

        output_writer.writeln(&output).unwrap_or_else(|e| {
            eprintln!("Error writing output: {e}");
        });
    } else {
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

    // Clean up environment variable
    if args.stdin_filename.is_some() {
        unsafe {
            std::env::remove_var("RUMDL_FILE_PATH");
        }
    }

    // Exit with error code if issues found
    if has_issues {
        exit::violations_found();
    }
}
