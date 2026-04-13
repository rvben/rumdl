//! Core file processing, fix application, and fixability checks.

use crate::cache::LintCache;
use crate::formatter;
use colored::*;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::{FixCapability, LintWarning, Rule};
use rumdl_lib::utils::code_block_utils::CodeBlockUtils;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use rumdl_lib::code_block_tools::executor::ExecutorError;
use rumdl_lib::code_block_tools::processor::ProcessorError;

use super::discovery::to_display_path;
use super::embedded::{
    check_embedded_markdown_blocks, format_embedded_markdown_blocks, has_fenced_code_blocks,
    should_lint_embedded_markdown,
};

/// Result of processing a file through lint and optional fix passes.
pub struct FileProcessResult {
    pub has_issues: bool,
    pub issues_found: usize,
    pub issues_fixed: usize,
    pub summary_issues_fixed: usize,
    pub fixable_issues: usize,
    /// In fix mode, contains only remaining (unfixed) warnings.
    /// In check mode, contains all warnings.
    pub warnings: Vec<rumdl_lib::rule::LintWarning>,
    pub file_index: rumdl_lib::workspace_index::FileIndex,
}

pub fn is_rule_actually_fixable(config: &rumdl_config::Config, rule_name: &str) -> bool {
    // Check unfixable list
    if config
        .global
        .unfixable
        .iter()
        .any(|r| r.eq_ignore_ascii_case(rule_name))
    {
        return false;
    }

    // Check fixable list if specified
    if !config.global.fixable.is_empty() {
        return config.global.fixable.iter().any(|r| r.eq_ignore_ascii_case(rule_name));
    }

    true
}

/// Check if a rule is fixable via CLI (considers both config AND rule's fix_capability)
///
/// A rule is CLI-fixable if:
/// 1. It's not in the unfixable config list
/// 2. It's in the fixable config list (if specified)
/// 3. The rule itself doesn't declare FixCapability::Unfixable
///
/// This replaces hardcoded rule name checks (e.g., `&& name != "MD033"`) with
/// capability-based checks that are future-proof for any rule.
pub fn is_rule_cli_fixable(rules: &[Box<dyn Rule>], config: &rumdl_config::Config, rule_name: &str) -> bool {
    // First check config-based fixability
    if !is_rule_actually_fixable(config, rule_name) {
        return false;
    }

    // Then check if the rule declares itself as Unfixable
    // Rules like MD033 have LSP-only fixes (for VS Code quick actions) but
    // their fix() method returns content unchanged, so CLI shouldn't count them
    rules
        .iter()
        .find(|r| r.name().eq_ignore_ascii_case(rule_name))
        .is_none_or(|r| r.fix_capability() != FixCapability::Unfixable)
}

#[allow(clippy::too_many_arguments)]
pub fn process_file_with_formatter(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    fix_mode: crate::FixMode,
    diff: bool,
    verbose: bool,
    quiet: bool,
    silent: bool,
    output_format: &rumdl_lib::output::OutputFormat,
    output_writer: &rumdl_lib::output::OutputWriter,
    config: &rumdl_config::Config,
    cache: Option<std::sync::Arc<std::sync::Mutex<LintCache>>>,
    project_root: Option<&Path>,
    show_full_path: bool,
    cache_hashes: Option<&CacheHashes>,
) -> FileProcessResult {
    let formatter = output_format.create_formatter();

    // Convert to display path (relative) unless --show-full-path is set
    let display_path = if show_full_path {
        file_path.to_string()
    } else {
        to_display_path(file_path, project_root)
    };

    // Call the original process_file_inner to get warnings, original line ending, and FileIndex
    let (all_warnings, mut content, total_warnings, fixable_warnings, original_line_ending, file_index) =
        process_file_inner(file_path, rules, verbose, quiet, silent, config, cache, cache_hashes);

    // Compute filtered rules based on per-file-ignores for embedded markdown formatting
    // This ensures embedded markdown formatting respects per-file-ignores just like linting does
    let ignored_rules_for_file = config.get_ignored_rules_for_file(Path::new(file_path));
    let filtered_rules: Vec<Box<dyn Rule>> = if !ignored_rules_for_file.is_empty() {
        rules
            .iter()
            .filter(|rule| !ignored_rules_for_file.contains(rule.name()))
            .map(|r| dyn_clone::clone_box(&**r))
            .collect()
    } else {
        rules.to_vec()
    };

    // In check mode with no warnings, return early
    if total_warnings == 0 && fix_mode == crate::FixMode::Check && !diff {
        return FileProcessResult {
            has_issues: false,
            issues_found: 0,
            issues_fixed: 0,
            summary_issues_fixed: 0,
            fixable_issues: 0,
            warnings: Vec::new(),
            file_index,
        };
    }

    // In fix mode with no warnings to fix, check if there are embedded markdown blocks to format
    // or code block tools to run. If not, return early.
    if total_warnings == 0 && fix_mode != crate::FixMode::Check && !diff {
        // Check if there's any embedded markdown to format
        let has_embedded = has_fenced_code_blocks(&content)
            && CodeBlockUtils::detect_markdown_code_blocks(&content)
                .iter()
                .any(|b| !content[b.content_start..b.content_end].trim().is_empty());

        // Check if code block tools are enabled
        let has_code_block_tools = config.code_block_tools.enabled;

        if !has_embedded && !has_code_block_tools {
            return FileProcessResult {
                has_issues: false,
                issues_found: 0,
                issues_fixed: 0,
                summary_issues_fixed: 0,
                fixable_issues: 0,
                warnings: Vec::new(),
                file_index,
            };
        }
    }

    // Format and output warnings (show diagnostics unless silent)
    if !silent && fix_mode == crate::FixMode::Check {
        if diff {
            // In diff mode, only show warnings for unfixable issues
            let unfixable_warnings: Vec<_> = all_warnings.iter().filter(|w| w.fix.is_none()).cloned().collect();

            if !unfixable_warnings.is_empty() {
                let formatted = formatter.format_warnings_with_content(&unfixable_warnings, &display_path, &content);
                if !formatted.is_empty() {
                    output_writer.writeln(&formatted).unwrap_or_else(|e| {
                        eprintln!("Error writing output: {e}");
                    });
                }
            }
        } else {
            // In check mode, show all warnings with [*] for fixable issues
            // Strip fix from warnings where the rule is not CLI-fixable (e.g., LSP-only fixes)
            let display_warnings: Vec<_> = all_warnings
                .iter()
                .map(|w| {
                    let rule_name = w.rule_name.as_deref().unwrap_or("");
                    if !is_rule_cli_fixable(rules, config, rule_name) {
                        LintWarning { fix: None, ..w.clone() }
                    } else {
                        w.clone()
                    }
                })
                .collect();
            let formatted = formatter.format_warnings_with_content(&display_warnings, &display_path, &content);
            if !formatted.is_empty() {
                output_writer.writeln(&formatted).unwrap_or_else(|e| {
                    eprintln!("Error writing output: {e}");
                });
            }
        }
    }

    // Handle diff mode or fix mode
    let mut warnings_fixed = 0;
    if diff {
        // In diff mode, apply fixes to a copy and show diff
        let original_content = content.clone();
        warnings_fixed = apply_fixes_coordinated(
            rules,
            &all_warnings,
            &mut content,
            true,
            true,
            config,
            Some(Path::new(file_path)),
        );

        // Format embedded markdown blocks (recursive formatting)
        // Use filtered_rules to respect per-file-ignores for embedded content
        let embedded_formatted = format_embedded_markdown_blocks(&mut content, &filtered_rules, config);
        warnings_fixed += embedded_formatted;

        // Format doc comments in Rust files
        if Path::new(file_path).extension().is_some_and(|ext| ext == "rs") {
            let doc_formatted = super::doc_comments::format_doc_comment_blocks(&mut content, &filtered_rules, config);
            warnings_fixed += doc_formatted;
        }

        // Format code blocks using external tools if enabled
        if config.code_block_tools.enabled {
            let processor = rumdl_lib::code_block_tools::CodeBlockToolProcessor::new(
                &config.code_block_tools,
                config.get_flavor_for_file(Path::new(file_path)),
            );
            match processor.format(&content) {
                Ok(output) => {
                    if output.content != content {
                        content = output.content;
                        warnings_fixed += 1;
                    }
                    // Report any errors that occurred during formatting
                    if output.had_errors && !silent {
                        for msg in &output.error_messages {
                            eprintln!("Warning: {}", format_tool_warning(msg, &display_path));
                        }
                    }
                }
                Err(e) => {
                    if !silent {
                        eprintln!("Warning: {}", format_tool_error(&e, &display_path));
                    }
                }
            }
        }

        if warnings_fixed > 0 {
            let diff_output = formatter::generate_diff(&original_content, &content, &display_path);
            output_writer.writeln(&diff_output).unwrap_or_else(|e| {
                eprintln!("Error writing diff output: {e}");
            });
        }

        let summary_issues_fixed = if total_warnings > 0 {
            let remaining_warnings = relint_fixed_file_content(&content, file_path, rules, config);
            count_actually_fixed_warnings(rules, config, &all_warnings, &remaining_warnings)
        } else {
            warnings_fixed
        };

        // Don't actually write the file in diff mode, but report how many would be fixed
        return FileProcessResult {
            has_issues: total_warnings > 0 || warnings_fixed > 0,
            issues_found: total_warnings,
            issues_fixed: warnings_fixed,
            summary_issues_fixed,
            fixable_issues: fixable_warnings,
            warnings: all_warnings,
            file_index,
        };
    } else if fix_mode != crate::FixMode::Check {
        // Apply fixes using Fix Coordinator
        warnings_fixed = apply_fixes_coordinated(
            rules,
            &all_warnings,
            &mut content,
            quiet,
            silent,
            config,
            Some(Path::new(file_path)),
        );

        // Format embedded markdown blocks (recursive formatting)
        // Use filtered_rules to respect per-file-ignores for embedded content
        let embedded_formatted = format_embedded_markdown_blocks(&mut content, &filtered_rules, config);
        warnings_fixed += embedded_formatted;

        // Format doc comments in Rust files
        if Path::new(file_path).extension().is_some_and(|ext| ext == "rs") {
            let doc_formatted = super::doc_comments::format_doc_comment_blocks(&mut content, &filtered_rules, config);
            warnings_fixed += doc_formatted;
        }

        // Format code blocks using external tools if enabled
        if config.code_block_tools.enabled {
            let processor = rumdl_lib::code_block_tools::CodeBlockToolProcessor::new(
                &config.code_block_tools,
                config.get_flavor_for_file(Path::new(file_path)),
            );
            match processor.format(&content) {
                Ok(output) => {
                    if output.content != content {
                        content = output.content;
                        warnings_fixed += 1;
                    }
                    // Report any errors that occurred during formatting
                    if output.had_errors && !silent {
                        for msg in &output.error_messages {
                            eprintln!("Warning: {}", format_tool_warning(msg, &display_path));
                        }
                    }
                }
                Err(e) => {
                    if !silent {
                        eprintln!("Warning: {}", format_tool_error(&e, &display_path));
                    }
                }
            }
        }

        // Write fixed content back to file
        if warnings_fixed > 0 {
            // Denormalize back to original line ending before writing
            let content_to_write = rumdl_lib::utils::normalize_line_ending(&content, original_line_ending).into_owned();

            if let Err(err) = std::fs::write(file_path, &content_to_write)
                && !silent
            {
                eprintln!(
                    "{} Failed to write fixed content to file {}: {}",
                    "Error:".red().bold(),
                    file_path,
                    err
                );
            }
        }

        // If there were no original warnings, we only formatted embedded blocks.
        // In this case, return success (no issues) without re-linting, since re-lint
        // doesn't apply per-file-ignores or inline config that the original lint did.
        if total_warnings == 0 {
            return FileProcessResult {
                has_issues: false,
                issues_found: 0,
                issues_fixed: warnings_fixed,
                summary_issues_fixed: warnings_fixed,
                fixable_issues: 0,
                warnings: Vec::new(),
                file_index,
            };
        }

        // Re-lint the fixed content to see which warnings remain.
        let remaining_warnings = relint_fixed_file_content(&content, file_path, rules, config);

        // Compute per-warning fixed status by comparing pre-fix warnings
        // against post-fix remaining warnings
        let fixed_status: Vec<bool> = all_warnings
            .iter()
            .map(|warning| {
                let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");
                let is_fixable = is_rule_cli_fixable(rules, config, rule_name);
                warning.fix.is_some()
                    && is_fixable
                    && !remaining_warnings.iter().any(|w| {
                        w.line == warning.line
                            && w.column == warning.column
                            && w.rule_name == warning.rule_name
                            && w.message == warning.message
                    })
            })
            .collect();
        let summary_issues_fixed = fixed_status.iter().filter(|&&was_fixed| was_fixed).count();

        // Show fix results in streaming output
        if !silent {
            use rumdl_lib::output::OutputFormat;
            match output_format {
                // Human-readable text formats: show all warnings with [fixed] labels
                OutputFormat::Text | OutputFormat::Full => {
                    let mut output = String::new();
                    for (warning, &was_fixed) in all_warnings.iter().zip(&fixed_status) {
                        let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");

                        let fix_indicator = if was_fixed {
                            " [fixed]".green().to_string()
                        } else {
                            String::new()
                        };

                        let line = format!(
                            "{}:{}:{}: {} {}{}",
                            display_path.blue().underline(),
                            warning.line.to_string().cyan(),
                            warning.column.to_string().cyan(),
                            format!("[{rule_name:5}]").yellow(),
                            warning.message,
                            fix_indicator
                        );

                        output.push_str(&line);
                        output.push('\n');
                    }

                    if !output.is_empty() {
                        output.pop(); // Remove trailing newline
                        output_writer.writeln(&output).unwrap_or_else(|e| {
                            eprintln!("Error writing output: {e}");
                        });
                    }
                }
                // Batch formats are handled by check_runner (silent=true suppresses this path)
                OutputFormat::Json | OutputFormat::GitLab | OutputFormat::Sarif | OutputFormat::Junit => {}
                // Other streaming formats: use their formatter with remaining-only warnings
                _ => {
                    if !remaining_warnings.is_empty() {
                        let formatted =
                            formatter.format_warnings_with_content(&remaining_warnings, &display_path, &content);
                        if !formatted.is_empty() {
                            output_writer.writeln(&formatted).unwrap_or_else(|e| {
                                eprintln!("Error writing output: {e}");
                            });
                        }
                    }
                }
            }
        }

        // Return remaining warnings for batch format collection
        // Exit 0 if all violations are fixed (Ruff convention)
        return FileProcessResult {
            has_issues: !remaining_warnings.is_empty(),
            issues_found: total_warnings,
            issues_fixed: warnings_fixed,
            summary_issues_fixed,
            fixable_issues: fixable_warnings,
            warnings: remaining_warnings,
            file_index,
        };
    }

    FileProcessResult {
        has_issues: true,
        issues_found: total_warnings,
        issues_fixed: warnings_fixed,
        summary_issues_fixed: warnings_fixed,
        fixable_issues: fixable_warnings,
        warnings: all_warnings,
        file_index,
    }
}

fn relint_fixed_file_content(
    content: &str,
    file_path: &str,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
) -> Vec<rumdl_lib::rule::LintWarning> {
    let ignored_rules_for_file = config.get_ignored_rules_for_file(Path::new(file_path));
    let filtered_rules: Vec<_> = if !ignored_rules_for_file.is_empty() {
        rules
            .iter()
            .filter(|rule| !ignored_rules_for_file.contains(rule.name()))
            .collect()
    } else {
        rules.iter().collect()
    };

    let flavor = config.get_flavor_for_file(Path::new(file_path));
    let fixed_ctx = LintContext::new(content, flavor, Some(PathBuf::from(file_path)));
    let mut remaining_warnings = Vec::new();

    for rule in &filtered_rules {
        if let Ok(rule_warnings) = rule.check(&fixed_ctx) {
            let filtered_warnings = rule_warnings.into_iter().filter(|warning| {
                let rule_name = warning.rule_name.as_deref().unwrap_or(rule.name());
                let base_rule_name = if let Some(dash_pos) = rule_name.find('-') {
                    &rule_name[..dash_pos]
                } else {
                    rule_name
                };
                !fixed_ctx.inline_config().is_rule_disabled(base_rule_name, warning.line)
            });
            remaining_warnings.extend(filtered_warnings);
        }
    }

    remaining_warnings
}

pub(crate) fn count_actually_fixed_warnings(
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
    all_warnings: &[LintWarning],
    remaining_warnings: &[LintWarning],
) -> usize {
    all_warnings
        .iter()
        .filter(|warning| {
            let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");
            let is_fixable = is_rule_cli_fixable(rules, config, rule_name);
            warning.fix.is_some()
                && is_fixable
                && !remaining_warnings.iter().any(|w| {
                    w.line == warning.line
                        && w.column == warning.column
                        && w.rule_name == warning.rule_name
                        && w.message == warning.message
                })
        })
        .count()
}

/// Result type for file processing that includes index data for cross-file analysis
pub struct ProcessFileResult {
    pub warnings: Vec<rumdl_lib::rule::LintWarning>,
    pub content: String,
    pub total_warnings: usize,
    pub fixable_warnings: usize,
    pub original_line_ending: rumdl_lib::utils::LineEnding,
    pub file_index: rumdl_lib::workspace_index::FileIndex,
}

pub struct CacheHashes {
    pub config_hash: String,
    pub rules_hash: String,
}

impl CacheHashes {
    pub fn new(config: &rumdl_config::Config, rules: &[Box<dyn Rule>]) -> Self {
        Self {
            config_hash: LintCache::hash_config(config),
            rules_hash: LintCache::hash_rules(rules),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn process_file_inner(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    verbose: bool,
    quiet: bool,
    silent: bool,
    config: &rumdl_config::Config,
    cache: Option<std::sync::Arc<std::sync::Mutex<LintCache>>>,
    cache_hashes: Option<&CacheHashes>,
) -> (
    Vec<rumdl_lib::rule::LintWarning>,
    String,
    usize,
    usize,
    rumdl_lib::utils::LineEnding,
    rumdl_lib::workspace_index::FileIndex,
) {
    let result = process_file_with_index(file_path, rules, verbose, quiet, silent, config, cache, cache_hashes);
    (
        result.warnings,
        result.content,
        result.total_warnings,
        result.fixable_warnings,
        result.original_line_ending,
        result.file_index,
    )
}

/// Process a file and return both warnings and FileIndex for cross-file aggregation
#[allow(clippy::too_many_arguments)]
pub fn process_file_with_index(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    verbose: bool,
    quiet: bool,
    silent: bool,
    config: &rumdl_config::Config,
    cache: Option<std::sync::Arc<std::sync::Mutex<LintCache>>>,
    cache_hashes: Option<&CacheHashes>,
) -> ProcessFileResult {
    use std::time::Instant;

    let start_time = Instant::now();
    if verbose && !quiet {
        // Display relative path for better UX, even if file_path is canonical (absolute)
        let display_path = if let Ok(cwd) = std::env::current_dir() {
            Path::new(file_path)
                .strip_prefix(&cwd)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file_path.to_string())
        } else {
            file_path.to_string()
        };
        println!("Processing file: {display_path}");
    }

    let empty_result = ProcessFileResult {
        warnings: Vec::new(),
        content: String::new(),
        total_warnings: 0,
        fixable_warnings: 0,
        original_line_ending: rumdl_lib::utils::LineEnding::Lf,
        file_index: rumdl_lib::workspace_index::FileIndex::new(),
    };

    // Read file content efficiently
    let mut content = match crate::read_file_efficiently(Path::new(file_path)) {
        Ok(content) => content,
        Err(e) => {
            if !silent {
                eprintln!("Error reading file {file_path}: {e}");
            }
            return empty_result;
        }
    };

    // Detect original line ending before any processing
    let original_line_ending = rumdl_lib::utils::detect_line_ending_enum(&content);

    // Normalize to LF for all internal processing
    content = rumdl_lib::utils::normalize_line_ending(&content, rumdl_lib::utils::LineEnding::Lf).into_owned();

    // Route Rust files to doc comment linting instead of regular markdown linting
    if Path::new(file_path).extension().is_some_and(|ext| ext == "rs") {
        return process_rust_file_doc_comments(file_path, &content, rules, config, original_line_ending);
    }

    // Validate inline config comments and warn about unknown rules
    if !silent {
        let inline_warnings = rumdl_lib::inline_config::validate_inline_config_rules(&content);
        for warn in inline_warnings {
            warn.print_warning(file_path);
        }
    }

    // Early content analysis for ultra-fast skip decisions
    if content.is_empty() {
        return ProcessFileResult {
            original_line_ending,
            ..empty_result
        };
    }

    // Compute hashes for cache (Ruff-style: file content + config + enabled rules)
    let (config_hash, rules_hash) = if let Some(hashes) = cache_hashes {
        (Cow::Borrowed(&hashes.config_hash), Cow::Borrowed(&hashes.rules_hash))
    } else {
        (
            Cow::Owned(LintCache::hash_config(config)),
            Cow::Owned(LintCache::hash_rules(rules)),
        )
    };

    // Try to get from cache first (lock briefly for cache read)
    // Note: Cache only stores single-file warnings; cross-file checks must run fresh
    if let Some(ref cache_arc) = cache {
        let cache_result = cache_arc
            .lock()
            .ok()
            .and_then(|mut guard| guard.get(&content, &config_hash, &rules_hash));
        if let Some(cached_warnings) = cache_result {
            if verbose && !quiet {
                println!("Cache hit for {file_path}");
            }
            // Count fixable warnings from cache (using capability-based check)
            let fixable_warnings = cached_warnings
                .iter()
                .filter(|w| {
                    w.fix.is_some()
                        && w.rule_name
                            .as_ref()
                            .is_some_and(|name| is_rule_cli_fixable(rules, config, name))
                })
                .count();

            // Build FileIndex for cross-file analysis on cache hit (lightweight, no rule checking)
            let flavor = config.get_flavor_for_file(Path::new(file_path));
            let file_index =
                rumdl_lib::build_file_index_only(&content, rules, flavor, Some(std::path::PathBuf::from(file_path)));

            let total_warnings = cached_warnings.len();
            return ProcessFileResult {
                warnings: cached_warnings,
                content,
                total_warnings,
                fixable_warnings,
                original_line_ending,
                file_index,
            };
        }
    }

    let lint_start = Instant::now();

    // Filter rules based on per-file-ignores configuration
    let ignored_rules_for_file = config.get_ignored_rules_for_file(Path::new(file_path));
    let filtered_rules: Vec<_> = if !ignored_rules_for_file.is_empty() {
        rules
            .iter()
            .filter(|rule| !ignored_rules_for_file.contains(rule.name()))
            .map(|r| dyn_clone::clone_box(&**r))
            .collect()
    } else {
        rules.to_vec()
    };

    // Determine flavor based on per-file-flavor overrides, global config, or file extension
    let flavor = config.get_flavor_for_file(Path::new(file_path));

    // Use lint_and_index for single-file linting + index contribution
    let source_file = Some(std::path::PathBuf::from(file_path));
    let (warnings_result, file_index) =
        rumdl_lib::lint_and_index(&content, &filtered_rules, verbose, flavor, source_file, Some(config));

    // Combine all warnings
    let mut all_warnings = warnings_result.unwrap_or_default();

    // Check embedded markdown blocks if configured in code-block-tools
    // The special tool "rumdl" in [code-block-tools.languages.markdown] enables this
    if should_lint_embedded_markdown(&config.code_block_tools) {
        let embedded_warnings = check_embedded_markdown_blocks(&content, &filtered_rules, config);
        all_warnings.extend(embedded_warnings);
    }

    // Run code block tools linting if enabled
    if config.code_block_tools.enabled {
        let processor = rumdl_lib::code_block_tools::CodeBlockToolProcessor::new(
            &config.code_block_tools,
            config.get_flavor_for_file(Path::new(file_path)),
        );
        match processor.lint(&content) {
            Ok(diagnostics) => {
                let tool_warnings: Vec<_> = diagnostics.iter().map(|d| d.to_lint_warning()).collect();
                all_warnings.extend(tool_warnings);
            }
            Err(e) => {
                // Convert processor error to a warning so it counts toward exit code
                all_warnings.push(rumdl_lib::rule::LintWarning {
                    message: e.to_string(),
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 1,
                    severity: rumdl_lib::rule::Severity::Error,
                    fix: None,
                    rule_name: Some("code-block-tools".to_string()),
                });
            }
        }
    }

    // Sort warnings by line number, then column
    all_warnings.sort_by(|a, b| {
        if a.line == b.line {
            a.column.cmp(&b.column)
        } else {
            a.line.cmp(&b.line)
        }
    });

    let total_warnings = all_warnings.len();

    // Count fixable issues (using capability-based check)
    let fixable_warnings = all_warnings
        .iter()
        .filter(|w| {
            w.fix.is_some()
                && w.rule_name
                    .as_ref()
                    .is_some_and(|name| is_rule_cli_fixable(rules, config, name))
        })
        .count();

    let lint_end_time = Instant::now();
    let lint_time = lint_end_time.duration_since(lint_start);

    if verbose && !quiet {
        println!("Linting took: {lint_time:?}");
    }

    let total_time = start_time.elapsed();
    if verbose && !quiet {
        println!("Total processing time for {file_path}: {total_time:?}");
    }

    // Store in cache before returning (ignore if mutex is poisoned)
    if let Some(ref cache_arc) = cache
        && let Ok(mut cache_guard) = cache_arc.lock()
    {
        cache_guard.set(&content, &config_hash, &rules_hash, all_warnings.clone());
    }

    ProcessFileResult {
        warnings: all_warnings,
        content,
        total_warnings,
        fixable_warnings,
        original_line_ending,
        file_index,
    }
}

pub fn apply_fixes_coordinated(
    rules: &[Box<dyn Rule>],
    all_warnings: &[rumdl_lib::rule::LintWarning],
    content: &mut String,
    _quiet: bool,
    silent: bool,
    config: &rumdl_config::Config,
    file_path: Option<&std::path::Path>,
) -> usize {
    use rumdl_lib::fix_coordinator::FixCoordinator;
    use std::time::Instant;

    let start = Instant::now();
    let coordinator = FixCoordinator::new();

    // Apply fixes iteratively (up to 100 iterations to ensure convergence, same as Ruff)
    // Pass file_path to enable per-file flavor resolution
    match coordinator.apply_fixes_iterative(rules, all_warnings, content, config, 100, file_path) {
        Ok(result) => {
            let elapsed = start.elapsed();

            if std::env::var("RUMDL_DEBUG_FIX_PERF").is_ok() {
                eprintln!("DEBUG: Fix Coordinator used");
                eprintln!("DEBUG: Iterations: {}", result.iterations);
                eprintln!("DEBUG: Rules applied: {}", result.rules_fixed);
                eprintln!("DEBUG: LintContext creations: {}", result.context_creations);
                eprintln!("DEBUG: Converged: {}", result.converged);
                eprintln!("DEBUG: Total time: {elapsed:?}");
            }

            // Warn if convergence failed (Ruff-style)
            if !result.converged && !silent {
                for line in build_non_convergence_warning_lines(&result, file_path) {
                    eprintln!("{line}");
                }
            }

            // Count warnings for the rules that were successfully applied
            all_warnings
                .iter()
                .filter(|w| {
                    w.rule_name
                        .as_ref()
                        .map(|name| result.fixed_rule_names.contains(name.as_str()))
                        .unwrap_or(false)
                })
                .count()
        }
        Err(e) => {
            if !silent {
                eprintln!("Warning: Fix coordinator failed: {e}");
            }
            0
        }
    }
}

/// Format an error_messages string (from OnError::Warn path) for user display.
///
/// Input format: `"line 15 (shell): Tool 'shfmt' failed: Exit code 1: <standard input>:3:27: msg"`
/// Output format: `"docs/guide.md:18:27: [shfmt] msg"`
fn format_tool_warning(msg: &str, display_path: &str) -> String {
    // Parse "line N (lang): rest" prefix
    let Some(rest) = msg.strip_prefix("line ") else {
        return format!("{display_path}: {msg}");
    };
    let Some(space_pos) = rest.find(' ') else {
        return format!("{display_path}: {msg}");
    };
    let Ok(fence_line) = rest[..space_pos].parse::<usize>() else {
        return format!("{display_path}: {msg}");
    };
    // Extract "(lang): rest_of_message"
    let after_line = &rest[space_pos + 1..];
    let Some(paren_end) = after_line.find("): ") else {
        return format!("{display_path}: {msg}");
    };
    let error_msg = &after_line[paren_end + 3..];

    // Extract tool name from "Tool 'name' failed: ..." and strip boilerplate
    let (tool_bracket, clean_error) = if let Some(tool_start) = error_msg.find("Tool '") {
        let name_start = tool_start + 6;
        if let Some(name_end) = error_msg[name_start..].find("' failed: ") {
            let tool = &error_msg[name_start..name_start + name_end];
            let after_failed = &error_msg[name_start + name_end + 10..];
            let stripped = strip_exit_code_prefix(after_failed);
            (format!("[{tool}]"), stripped.to_string())
        } else {
            (String::new(), error_msg.to_string())
        }
    } else {
        (String::new(), error_msg.to_string())
    };

    let (location, cleaned) = extract_stdin_location(&clean_error, fence_line);
    let loc = location.unwrap_or_else(|| format!("{fence_line}"));
    if tool_bracket.is_empty() {
        format!("{display_path}:{loc}: {cleaned}")
    } else {
        format!("{display_path}:{loc}: {tool_bracket} {cleaned}")
    }
}

/// Format a code-block-tools ProcessorError for user display.
///
/// For `ToolErrorAt` errors, produces `file:line:col: [tool] message` format matching
/// rumdl's own lint output style. Translates `<standard input>:N:` references to
/// absolute file line numbers and strips boilerplate like exit codes.
fn format_tool_error(err: &ProcessorError, display_path: &str) -> String {
    match err {
        ProcessorError::ToolErrorAt {
            error,
            line: fence_line,
            ..
        } => match error {
            ExecutorError::ExecutionFailed { tool, message } => {
                let stripped = strip_exit_code_prefix(message);
                let (location, cleaned) = extract_stdin_location(stripped, *fence_line);
                let loc = location.unwrap_or_else(|| format!("{fence_line}"));
                format!("{display_path}:{loc}: [{tool}] {cleaned}")
            }
            ExecutorError::Timeout { tool, timeout_ms } => {
                format!("{display_path}:{fence_line}: [{tool}] timed out after {timeout_ms}ms")
            }
            ExecutorError::ToolNotFound { tool } => {
                format!("{display_path}:{fence_line}: [{tool}] not found in PATH")
            }
            ExecutorError::IoError { message } => {
                format!("{display_path}:{fence_line}: I/O error: {message}")
            }
        },
        _ => format!("{display_path}: {err}"),
    }
}

/// Strip "Exit code N: " prefix from tool error messages.
fn strip_exit_code_prefix(message: &str) -> &str {
    if let Some(rest) = message.strip_prefix("Exit code ")
        && let Some(colon_pos) = rest.find(": ")
    {
        return &rest[colon_pos + 2..];
    }
    message
}

/// Extract `<standard input>:N:M:` from a tool error message, returning the absolute
/// `line:col` string and the cleaned-up message with the stdin reference removed.
///
/// Returns `(Some("18:27"), "Tool 'shfmt' failed: Exit code 1: `>` must be...")` on
/// success, or `(None, original_message)` if no stdin reference is found.
fn extract_stdin_location(message: &str, fence_line: usize) -> (Option<String>, String) {
    const STDIN_PREFIX: &str = "<standard input>:";
    let Some(pos) = message.find(STDIN_PREFIX) else {
        return (None, message.to_string());
    };
    let after = &message[pos + STDIN_PREFIX.len()..];
    // Parse line number
    let Some(first_colon) = after.find(':') else {
        return (None, message.to_string());
    };
    let Ok(tool_line) = after[..first_colon].parse::<usize>() else {
        return (None, message.to_string());
    };
    let absolute_line = fence_line + tool_line;

    // Try to parse column number
    let rest_after_line = &after[first_colon + 1..];
    let (location, remaining_start) = if let Some(second_colon) = rest_after_line.find(':')
        && let Ok(col) = rest_after_line[..second_colon].parse::<usize>()
    {
        // Have both line and column
        let skip = pos + STDIN_PREFIX.len() + first_colon + 1 + second_colon + 1;
        (format!("{absolute_line}:{col}"), skip)
    } else {
        // Only line number
        let skip = pos + STDIN_PREFIX.len() + first_colon + 1;
        (format!("{absolute_line}"), skip)
    };

    // Reconstruct message: everything before the stdin ref + everything after line:col:
    let before = message[..pos].trim_end();
    let after_ref = message[remaining_start..].trim_start();
    let cleaned = if before.is_empty() {
        after_ref.to_string()
    } else if after_ref.is_empty() {
        before.to_string()
    } else {
        format!("{before} {after_ref}")
    };
    (Some(location), cleaned)
}

fn format_loop(cycle: &[String]) -> Option<String> {
    if cycle.is_empty() {
        return None;
    }

    let mut parts = cycle.to_vec();
    if let Some(first) = parts.first().cloned() {
        parts.push(first);
    }
    Some(parts.join(" -> "))
}

fn build_non_convergence_warning_lines(
    result: &rumdl_lib::fix_coordinator::FixResult,
    file_path: Option<&Path>,
) -> Vec<String> {
    let mut lines = Vec::new();
    let location = file_path.map(|p| format!(" for {}", p.display())).unwrap_or_default();

    if !result.conflicting_rules.is_empty() {
        let mut rules = result.conflicting_rules.clone();
        rules.sort();
        let rule_list = rules.join(", ");
        let primary_rule = rules[0].clone();

        lines.push(format!(
            "Warning: Auto-fix detected a rule conflict loop after {} iterations{}.",
            result.iterations, location
        ));
        lines.push(format!("Conflicting rules: {rule_list}"));
        if let Some(loop_str) = format_loop(&result.conflict_cycle) {
            lines.push(format!("Observed cycle: {loop_str}"));
        }
        lines.push("Actionable options:".to_string());
        lines.push(format!(
            "  - Keep linting but stop auto-fixing one rule: [global] unfixable = [\"{primary_rule}\"]"
        ));
        lines.push(format!(
            "  - Disable one rule entirely for this run: rumdl check --fix --disable {primary_rule}"
        ));
        lines.push(format!(
            "  - Disable one rule in config: [global] disable = [\"{primary_rule}\"]"
        ));
        lines.push(
            "If this looks wrong, please report it: https://github.com/rvben/rumdl/issues/new?template=bug_report.yml"
                .to_string(),
        );
        return lines;
    }

    let mut fixed_rules: Vec<String> = result.fixed_rule_names.iter().cloned().collect();
    fixed_rules.sort();
    let fixed_rules_list = if fixed_rules.is_empty() {
        "(none)".to_string()
    } else {
        fixed_rules.join(", ")
    };

    lines.push(format!(
        "Warning: Auto-fix did not converge after {} iterations{}.",
        result.iterations, location
    ));
    lines.push("No repeatable cycle was detected; this is likely a convergence bug.".to_string());
    lines.push(format!("Rules that changed content: {fixed_rules_list}"));
    if !fixed_rules.is_empty() {
        let quoted_rules = fixed_rules
            .iter()
            .map(|r| format!("\"{r}\""))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "Try narrowing auto-fix scope: [global] fixable = [{quoted_rules}]"
        ));
    }
    lines.push("Please report it: https://github.com/rvben/rumdl/issues/new?template=bug_report.yml".to_string());
    lines
}

/// Process a Rust source file by linting markdown in doc comments.
///
/// Returns a `ProcessFileResult` with warnings remapped to their original file
/// positions. No cross-file analysis is performed for doc comments.
fn process_rust_file_doc_comments(
    file_path: &str,
    content: &str,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
    original_line_ending: rumdl_lib::utils::LineEnding,
) -> ProcessFileResult {
    // Filter rules based on per-file-ignores configuration
    let ignored_rules_for_file = config.get_ignored_rules_for_file(Path::new(file_path));
    let filtered_rules: Vec<Box<dyn Rule>> = if !ignored_rules_for_file.is_empty() {
        rules
            .iter()
            .filter(|rule| !ignored_rules_for_file.contains(rule.name()))
            .map(|r| dyn_clone::clone_box(&**r))
            .collect()
    } else {
        rules.to_vec()
    };

    let all_warnings = rumdl_lib::doc_comment_lint::check_doc_comment_blocks(content, &filtered_rules, config);

    let total_warnings = all_warnings.len();
    // Doc comment warnings have fix stripped (fix: None) in check mode, so
    // determine fixability by checking the rule's fix capability instead.
    let fixable_warnings = all_warnings
        .iter()
        .filter(|w| {
            w.rule_name
                .as_ref()
                .is_some_and(|name| is_rule_cli_fixable(rules, config, name))
        })
        .count();

    ProcessFileResult {
        warnings: all_warnings,
        content: content.to_string(),
        total_warnings,
        fixable_warnings,
        original_line_ending,
        file_index: rumdl_lib::workspace_index::FileIndex::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rumdl_lib::fix_coordinator::FixResult;
    use std::collections::HashSet;

    #[test]
    fn test_build_non_convergence_warning_lines_conflict_loop() {
        let result = FixResult {
            rules_fixed: 3,
            iterations: 6,
            context_creations: 6,
            fixed_rule_names: ["MD044".to_string(), "MD063".to_string()].into_iter().collect(),
            converged: false,
            conflicting_rules: vec!["MD063".to_string(), "MD044".to_string()],
            conflict_cycle: vec!["MD044".to_string(), "MD063".to_string()],
        };

        let lines = build_non_convergence_warning_lines(&result, Some(Path::new("docs/guide.md")));
        let rendered = lines.join("\n");

        assert!(rendered.contains("rule conflict loop"));
        assert!(rendered.contains("for docs/guide.md"));
        assert!(rendered.contains("Conflicting rules: MD044, MD063"));
        assert!(rendered.contains("Observed cycle: MD044 -> MD063 -> MD044"));
        assert!(rendered.contains("[global] unfixable = [\"MD044\"]"));
        assert!(rendered.contains("rumdl check --fix --disable MD044"));
    }

    #[test]
    fn test_build_non_convergence_warning_lines_max_iterations() {
        let result = FixResult {
            rules_fixed: 10,
            iterations: 100,
            context_creations: 100,
            fixed_rule_names: ["MD009".to_string(), "MD012".to_string()].into_iter().collect(),
            converged: false,
            conflicting_rules: Vec::new(),
            conflict_cycle: Vec::new(),
        };

        let lines = build_non_convergence_warning_lines(&result, None);
        let rendered = lines.join("\n");

        assert!(rendered.contains("did not converge after 100 iterations"));
        assert!(rendered.contains("Rules that changed content: MD009, MD012"));
        assert!(rendered.contains("[global] fixable = [\"MD009\", \"MD012\"]"));
        assert!(rendered.contains("Please report it"));
    }

    #[test]
    fn test_format_loop_renders_closed_cycle() {
        let cycle = vec!["MD044".to_string(), "MD063".to_string()];
        assert_eq!(format_loop(&cycle).as_deref(), Some("MD044 -> MD063 -> MD044"));
    }

    #[test]
    fn test_format_loop_empty() {
        assert!(format_loop(&[]).is_none());
    }

    #[test]
    fn test_build_non_convergence_warning_lines_handles_empty_rule_set() {
        let result = FixResult {
            rules_fixed: 0,
            iterations: 100,
            context_creations: 100,
            fixed_rule_names: HashSet::new(),
            converged: false,
            conflicting_rules: Vec::new(),
            conflict_cycle: Vec::new(),
        };

        let lines = build_non_convergence_warning_lines(&result, Some(Path::new("README.md")));
        let rendered = lines.join("\n");

        assert!(rendered.contains("for README.md"));
        assert!(rendered.contains("Rules that changed content: (none)"));
    }

    #[test]
    fn extract_stdin_location_with_line_and_col() {
        let msg = "<standard input>:3:27: `>` must be followed by a word";
        let (loc, cleaned) = super::extract_stdin_location(msg, 15);
        assert_eq!(loc.as_deref(), Some("18:27"));
        assert_eq!(cleaned, "`>` must be followed by a word");
    }

    #[test]
    fn extract_stdin_location_line_only() {
        let msg = "<standard input>:5: syntax error";
        let (loc, cleaned) = super::extract_stdin_location(msg, 10);
        assert_eq!(loc.as_deref(), Some("15"));
        assert_eq!(cleaned, "syntax error");
    }

    #[test]
    fn extract_stdin_location_no_stdin_ref() {
        let msg = "Unknown option --foo";
        let (loc, cleaned) = super::extract_stdin_location(msg, 10);
        assert!(loc.is_none());
        assert_eq!(cleaned, msg);
    }

    #[test]
    fn extract_stdin_location_mid_string() {
        let msg = "some prefix <standard input>:3:27: error text";
        let (loc, cleaned) = super::extract_stdin_location(msg, 15);
        assert_eq!(loc.as_deref(), Some("18:27"));
        assert_eq!(cleaned, "some prefix error text");
    }

    #[test]
    fn strip_exit_code_prefix_present() {
        assert_eq!(super::strip_exit_code_prefix("Exit code 1: some error"), "some error");
        assert_eq!(super::strip_exit_code_prefix("Exit code 127: not found"), "not found");
    }

    #[test]
    fn strip_exit_code_prefix_absent() {
        assert_eq!(
            super::strip_exit_code_prefix("some error without prefix"),
            "some error without prefix"
        );
    }

    #[test]
    fn format_tool_error_execution_failed_with_stdin() {
        use rumdl_lib::code_block_tools::executor::ExecutorError;
        use rumdl_lib::code_block_tools::processor::ProcessorError;
        let err = ProcessorError::ToolErrorAt {
            error: ExecutorError::ExecutionFailed {
                tool: "shfmt".to_string(),
                message: "Exit code 1: <standard input>:3:27: `>` must be followed by a word".to_string(),
            },
            line: 15,
            language: "shell".to_string(),
        };
        assert_eq!(
            super::format_tool_error(&err, "docs/guide.md"),
            "docs/guide.md:18:27: [shfmt] `>` must be followed by a word"
        );
    }

    #[test]
    fn format_tool_error_execution_failed_without_stdin() {
        use rumdl_lib::code_block_tools::executor::ExecutorError;
        use rumdl_lib::code_block_tools::processor::ProcessorError;
        let err = ProcessorError::ToolErrorAt {
            error: ExecutorError::ExecutionFailed {
                tool: "black".to_string(),
                message: "Exit code 1: cannot format".to_string(),
            },
            line: 15,
            language: "python".to_string(),
        };
        assert_eq!(
            super::format_tool_error(&err, "readme.md"),
            "readme.md:15: [black] cannot format"
        );
    }

    #[test]
    fn format_tool_error_timeout() {
        use rumdl_lib::code_block_tools::executor::ExecutorError;
        use rumdl_lib::code_block_tools::processor::ProcessorError;
        let err = ProcessorError::ToolErrorAt {
            error: ExecutorError::Timeout {
                tool: "prettier".to_string(),
                timeout_ms: 5000,
            },
            line: 20,
            language: "javascript".to_string(),
        };
        assert_eq!(
            super::format_tool_error(&err, "test.md"),
            "test.md:20: [prettier] timed out after 5000ms"
        );
    }

    #[test]
    fn format_tool_warning_with_stdin_ref() {
        let msg = "line 15 (shell): Tool 'shfmt' failed: Exit code 1: <standard input>:3:27: bad syntax";
        let result = super::format_tool_warning(msg, "docs/guide.md");
        assert_eq!(result, "docs/guide.md:18:27: [shfmt] bad syntax");
    }

    #[test]
    fn format_tool_warning_without_stdin_ref() {
        let msg = "line 15 (python): Tool 'black' failed: Exit code 1: cannot format";
        let result = super::format_tool_warning(msg, "readme.md");
        assert_eq!(result, "readme.md:15: [black] cannot format");
    }

    #[test]
    fn format_tool_warning_no_prefix() {
        let msg = "No format tools configured for language 'ruby' at line 5";
        let result = super::format_tool_warning(msg, "test.md");
        assert_eq!(
            result,
            "test.md: No format tools configured for language 'ruby' at line 5"
        );
    }
}
