//! Core file processing, fix application, and fixability checks.

use crate::cache::LintCache;
use crate::formatter;
use colored::*;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::{FixCapability, LintWarning, Rule};
use rumdl_lib::utils::code_block_utils::CodeBlockUtils;
use std::borrow::Cow;
use std::path::Path;

use super::discovery::to_display_path;
use super::embedded::{
    check_embedded_markdown_blocks, format_embedded_markdown_blocks, has_fenced_code_blocks,
    should_lint_embedded_markdown,
};

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
) -> (
    bool,
    usize,
    usize,
    usize,
    Vec<rumdl_lib::rule::LintWarning>,
    rumdl_lib::workspace_index::FileIndex,
) {
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
        return (false, 0, 0, 0, Vec::new(), file_index);
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
            return (false, 0, 0, 0, Vec::new(), file_index);
        }
    }

    // Format and output warnings (show diagnostics unless silent)
    if !silent && fix_mode == crate::FixMode::Check {
        if diff {
            // In diff mode, only show warnings for unfixable issues
            let unfixable_warnings: Vec<_> = all_warnings.iter().filter(|w| w.fix.is_none()).cloned().collect();

            if !unfixable_warnings.is_empty() {
                let formatted = formatter.format_warnings(&unfixable_warnings, &display_path);
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
            let formatted = formatter.format_warnings(&display_warnings, &display_path);
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

        // Format code blocks using external tools if enabled
        if config.code_block_tools.enabled {
            let processor = rumdl_lib::code_block_tools::CodeBlockToolProcessor::new(&config.code_block_tools);
            match processor.format(&content) {
                Ok(output) => {
                    if output.content != content {
                        content = output.content;
                        warnings_fixed += 1;
                    }
                    // Report any errors that occurred during formatting
                    if output.had_errors && !silent {
                        for msg in &output.error_messages {
                            eprintln!("Warning: {msg}");
                        }
                    }
                }
                Err(e) => {
                    if !silent {
                        eprintln!("Warning: Code block tool formatting failed: {e}");
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

        // Don't actually write the file in diff mode, but report how many would be fixed
        return (
            total_warnings > 0 || warnings_fixed > 0,
            total_warnings,
            warnings_fixed,
            fixable_warnings,
            all_warnings,
            file_index,
        );
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

        // Format code blocks using external tools if enabled
        if config.code_block_tools.enabled {
            let processor = rumdl_lib::code_block_tools::CodeBlockToolProcessor::new(&config.code_block_tools);
            match processor.format(&content) {
                Ok(output) => {
                    if output.content != content {
                        content = output.content;
                        warnings_fixed += 1;
                    }
                    // Report any errors that occurred during formatting
                    if output.had_errors && !silent {
                        for msg in &output.error_messages {
                            eprintln!("Warning: {msg}");
                        }
                    }
                }
                Err(e) => {
                    if !silent {
                        eprintln!("Warning: Code block tool formatting failed: {e}");
                    }
                }
            }
        }

        // Write fixed content back to file
        if warnings_fixed > 0 {
            // Denormalize back to original line ending before writing
            let content_to_write = rumdl_lib::utils::normalize_line_ending(&content, original_line_ending);

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
            return (false, 0, warnings_fixed, 0, Vec::new(), file_index);
        }

        // Re-lint the fixed content to see which warnings remain
        // This is needed both for display and to determine exit code (following Ruff's convention)
        //
        // Apply the same filtering as the original lint to ensure consistent behavior:
        // 1. Per-file-ignores: filter rules based on config
        // 2. Inline config: filter warnings based on inline directives
        let ignored_rules_for_file = config.get_ignored_rules_for_file(Path::new(file_path));
        let filtered_rules: Vec<_> = if !ignored_rules_for_file.is_empty() {
            rules
                .iter()
                .filter(|rule| !ignored_rules_for_file.contains(rule.name()))
                .collect()
        } else {
            rules.iter().collect()
        };

        // Use per-file flavor for re-lint (same as initial lint)
        let flavor = config.get_flavor_for_file(Path::new(file_path));
        let fixed_ctx = LintContext::new(&content, flavor, None);
        let inline_config = rumdl_lib::inline_config::InlineConfig::from_content(&content);
        let mut remaining_warnings = Vec::new();

        for rule in &filtered_rules {
            if let Ok(rule_warnings) = rule.check(&fixed_ctx) {
                // Filter warnings based on inline config directives
                let filtered_warnings = rule_warnings.into_iter().filter(|warning| {
                    let rule_name = warning.rule_name.as_deref().unwrap_or(rule.name());
                    // Extract base rule name for sub-rules like "MD029-style" -> "MD029"
                    let base_rule_name = if let Some(dash_pos) = rule_name.find('-') {
                        &rule_name[..dash_pos]
                    } else {
                        rule_name
                    };
                    !inline_config.is_rule_disabled(base_rule_name, warning.line)
                });
                remaining_warnings.extend(filtered_warnings);
            }
        }

        // In fix mode, show warnings with [fixed] for issues that were fixed
        if !silent {
            // Create a custom formatter that shows [fixed] instead of [*]
            let mut output = String::new();
            for warning in &all_warnings {
                let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");

                // Check if the rule is CLI-fixable (config + rule capability)
                let is_fixable = is_rule_cli_fixable(rules, config, rule_name);

                let was_fixed = warning.fix.is_some()
                    && is_fixable
                    && !remaining_warnings.iter().any(|w| {
                        w.line == warning.line && w.column == warning.column && w.rule_name == warning.rule_name
                    });

                // Show [fixed] only for issues that were actually fixed, nothing otherwise
                let fix_indicator = if was_fixed {
                    " [fixed]".green().to_string()
                } else {
                    String::new()
                };

                // Format: file:line:column: [rule] message [fixed/*/]
                // Use colors similar to TextFormatter
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

        // Return false (no issues) if no warnings remain after fixing, true otherwise
        // This follows Ruff's convention: exit 0 if all violations are fixed
        return (
            !remaining_warnings.is_empty(),
            total_warnings,
            warnings_fixed,
            fixable_warnings,
            all_warnings,
            file_index,
        );
    }

    (
        true,
        total_warnings,
        warnings_fixed,
        fixable_warnings,
        all_warnings,
        file_index,
    )
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
    content = rumdl_lib::utils::normalize_line_ending(&content, rumdl_lib::utils::LineEnding::Lf);

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
            let file_index = rumdl_lib::build_file_index_only(&content, rules, flavor);

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
        let processor = rumdl_lib::code_block_tools::CodeBlockToolProcessor::new(&config.code_block_tools);
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
                eprintln!("Warning: Failed to converge after {} iterations.", result.iterations);
                eprintln!("This likely indicates a bug in rumdl.");
                if !result.fixed_rule_names.is_empty() {
                    let rule_codes: Vec<&str> = result.fixed_rule_names.iter().map(|s| s.as_str()).collect();
                    eprintln!("Rule codes: {}", rule_codes.join(", "));
                }
                eprintln!("Please report at: https://github.com/rvben/rumdl/issues/new?template=bug_report.yml");
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
