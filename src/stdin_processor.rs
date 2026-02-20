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
                exit::tool_error();
            }
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

    // Create a lint context for the stdin content
    // Use per-file flavor if stdin_filename is provided
    let flavor = args
        .stdin_filename
        .as_ref()
        .map(|f| config.get_flavor_for_file(std::path::Path::new(f)))
        .unwrap_or_else(|| config.markdown_flavor());
    let ctx = LintContext::new(&content, flavor, source_file.clone());
    let inline_config = ctx.inline_config();
    let mut all_warnings = Vec::new();

    // Apply inline configure-file overrides by merging them into the config and
    // recreating affected rules — mirrors the file-based path in lib.rs.
    let inline_overrides = inline_config.get_all_rule_configs();
    let merged_config: Option<rumdl_config::Config> = if !inline_overrides.is_empty() {
        Some(config.merge_with_inline_config(inline_config))
    } else {
        None
    };
    let effective_config = merged_config.as_ref().unwrap_or(config);
    let mut recreated_rules: std::collections::HashMap<String, Box<dyn rumdl_lib::rule::Rule>> =
        std::collections::HashMap::new();
    for rule_name in inline_overrides.keys() {
        if let Some(recreated) = rumdl_lib::rules::create_rule_by_name(rule_name, effective_config) {
            recreated_rules.insert(rule_name.clone(), recreated);
        }
    }

    // Run all enabled rules on the content
    for rule in rules {
        // Skip rules that indicate they should be skipped (opt-in rules, content-based skipping)
        if rule.should_skip(&ctx) {
            continue;
        }

        // Use recreated rule if inline configure-file overrides exist for this rule
        let effective_rule: &dyn rumdl_lib::rule::Rule = recreated_rules
            .get(rule.name())
            .map(|r| r.as_ref())
            .unwrap_or(rule.as_ref());

        match effective_rule.check(&ctx) {
            Ok(rule_warnings) => {
                // Filter out warnings for rules disabled via inline comments,
                // and warnings inside kramdown extension blocks.
                let filtered: Vec<_> = rule_warnings
                    .into_iter()
                    .filter(|warning| {
                        if ctx
                            .line_info(warning.line)
                            .is_some_and(|info| info.in_kramdown_extension_block)
                        {
                            return false;
                        }
                        let rule_name_to_check = warning.rule_name.as_deref().unwrap_or(rule.name());
                        let base_rule_name = if let Some(dash_pos) = rule_name_to_check.find('-') {
                            &rule_name_to_check[..dash_pos]
                        } else {
                            rule_name_to_check
                        };
                        !inline_config.is_rule_disabled(base_rule_name, warning.line)
                    })
                    .collect();
                all_warnings.extend(filtered);
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
            let file_path = args.stdin_filename.as_ref().map(std::path::Path::new);
            let warnings_fixed = file_processor::apply_fixes_coordinated(
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

            // Re-check the fixed content to see if any issues remain
            // Use same per-file flavor as initial lint
            let fixed_ctx = LintContext::new(&fixed_content, flavor, source_file.clone());
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
                let formatted = formatter.format_warnings_with_content(&remaining_warnings, display_filename, &content);
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
