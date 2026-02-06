//! Embedded markdown formatting and checking.
//!
//! Handles detecting, linting, and formatting markdown content
//! embedded inside fenced code blocks with `markdown` or `md` language.

use rumdl_lib::config as rumdl_config;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::utils::code_block_utils::CodeBlockUtils;

/// Maximum recursion depth for formatting nested markdown blocks.
///
/// This prevents stack overflow from deeply nested or maliciously crafted content.
/// The value of 5 is chosen because:
/// - Real-world usage rarely exceeds 2-3 levels (e.g., docs showing example markdown)
/// - 5 levels provides headroom for legitimate use cases
/// - Beyond 5 levels, the content is likely either malicious or unintentional
pub(super) const MAX_EMBEDDED_DEPTH: usize = 5;

pub(super) fn has_fenced_code_blocks(content: &str) -> bool {
    content.contains("```") || content.contains("~~~")
}

/// Format markdown content embedded in fenced code blocks with `markdown` or `md` language.
///
/// This function detects markdown code blocks and recursively applies formatting to their content.
/// The formatting preserves indentation for blocks inside lists or blockquotes.
///
/// Returns the number of blocks that were formatted.
pub fn format_embedded_markdown_blocks(
    content: &mut String,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
) -> usize {
    format_embedded_markdown_blocks_recursive(content, rules, config, 0)
}

/// Internal recursive implementation with depth tracking.
fn format_embedded_markdown_blocks_recursive(
    content: &mut String,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
    depth: usize,
) -> usize {
    // Prevent excessive recursion
    if depth >= MAX_EMBEDDED_DEPTH {
        return 0;
    }
    if !has_fenced_code_blocks(content) {
        return 0;
    }

    let blocks = CodeBlockUtils::detect_markdown_code_blocks(content);

    if blocks.is_empty() {
        return 0;
    }

    // Parse inline config from the parent content to respect disable/enable directives
    let inline_config = rumdl_lib::inline_config::InlineConfig::from_content(content);

    let mut formatted_count = 0;

    // Process blocks in reverse order to maintain byte offsets
    for block in blocks.into_iter().rev() {
        // Extract the content between the fences
        let block_content = &content[block.content_start..block.content_end];

        // Skip empty blocks
        if block_content.trim().is_empty() {
            continue;
        }

        // Compute the line number of the block's opening fence
        // The inline config state at this line determines which rules are disabled
        let block_line = content[..block.content_start].matches('\n').count() + 1;

        // Filter rules based on inline config at this block's location
        let block_rules: Vec<Box<dyn Rule>> = rules
            .iter()
            .filter(|rule| !inline_config.is_rule_disabled(rule.name(), block_line))
            .map(|r| dyn_clone::clone_box(&**r))
            .collect();

        // Strip common indentation from all lines
        let (stripped_content, common_indent) = strip_common_indent(block_content);

        // Apply formatting to the stripped content
        let mut formatted = stripped_content;

        // First, recursively format any nested markdown blocks
        let nested_formatted =
            format_embedded_markdown_blocks_recursive(&mut formatted, &block_rules, config, depth + 1);

        // Create a context and collect warnings for the embedded content
        let ctx = LintContext::new(&formatted, config.markdown_flavor(), None);
        let mut warnings = Vec::new();
        for rule in &block_rules {
            if let Ok(rule_warnings) = rule.check(&ctx) {
                warnings.extend(rule_warnings);
            }
        }

        // Apply fixes
        // Note: file_path is None for embedded blocks since they're synthetic content
        if !warnings.is_empty() {
            let _fixed = super::processing::apply_fixes_coordinated(
                &block_rules,
                &warnings,
                &mut formatted,
                true,
                true,
                config,
                None,
            );
        }

        // Remove trailing newline that MD047 may have added if original didn't have one
        // This prevents extra blank lines before the closing fence
        let original_had_trailing_newline = block_content.ends_with('\n');
        if !original_had_trailing_newline && formatted.ends_with('\n') {
            formatted.pop();
        }

        // Restore indentation
        let restored = restore_indent(&formatted, &common_indent);

        // Replace the block content if it changed
        if restored != block_content {
            content.replace_range(block.content_start..block.content_end, &restored);
            formatted_count += 1;
        }

        formatted_count += nested_formatted;
    }

    formatted_count
}

/// Check if embedded markdown linting is enabled via code-block-tools configuration.
///
/// Returns true if the special "rumdl" tool is configured for markdown/md language,
/// indicating that rumdl's built-in markdown linting should be applied to markdown code blocks.
pub(super) fn should_lint_embedded_markdown(config: &rumdl_lib::code_block_tools::CodeBlockToolsConfig) -> bool {
    if !config.enabled {
        return false;
    }

    // Check if markdown language is configured with the built-in rumdl tool
    // Also check "md" since it's a common alias
    for lang_key in ["markdown", "md"] {
        if let Some(lang_config) = config.languages.get(lang_key)
            && lang_config
                .lint
                .iter()
                .any(|tool| tool == rumdl_lib::code_block_tools::RUMDL_BUILTIN_TOOL)
        {
            return true;
        }
    }

    false
}

/// Check markdown content embedded in fenced code blocks with `markdown` or `md` language.
///
/// This function detects markdown code blocks and runs lint checks on their content,
/// returning warnings with adjusted line numbers that point to the correct location
/// in the parent file.
///
/// Returns a vector of warnings from all embedded markdown blocks.
pub fn check_embedded_markdown_blocks(
    content: &str,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
) -> Vec<rumdl_lib::rule::LintWarning> {
    check_embedded_markdown_blocks_recursive(content, rules, config, 0)
}

/// Internal recursive implementation with depth tracking.
fn check_embedded_markdown_blocks_recursive(
    content: &str,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
    depth: usize,
) -> Vec<rumdl_lib::rule::LintWarning> {
    // Prevent excessive recursion
    if depth >= MAX_EMBEDDED_DEPTH {
        return Vec::new();
    }
    if !has_fenced_code_blocks(content) {
        return Vec::new();
    }

    let blocks = CodeBlockUtils::detect_markdown_code_blocks(content);

    if blocks.is_empty() {
        return Vec::new();
    }

    // Parse inline config from the parent content to respect disable/enable directives
    let inline_config = rumdl_lib::inline_config::InlineConfig::from_content(content);

    let mut all_warnings = Vec::new();

    for block in blocks {
        // Extract the content between the fences
        let block_content = &content[block.content_start..block.content_end];

        // Skip empty blocks
        if block_content.trim().is_empty() {
            continue;
        }

        // Calculate the line offset for this block
        // Count newlines before content_start to get the starting line number
        let line_offset = content[..block.content_start].matches('\n').count();

        // Compute the line number of the block's opening fence (1-indexed)
        // The inline config state at this line determines which rules are disabled
        let block_line = line_offset + 1;

        // Filter rules based on inline config at this block's location
        let block_rules: Vec<&Box<dyn Rule>> = rules
            .iter()
            .filter(|rule| !inline_config.is_rule_disabled(rule.name(), block_line))
            .collect();

        // Strip common indentation from all lines
        let (stripped_content, _common_indent) = strip_common_indent(block_content);

        // First, recursively check any nested markdown blocks
        // Clone rules for recursion since we need owned values
        let block_rules_owned: Vec<Box<dyn Rule>> = block_rules.iter().map(|r| dyn_clone::clone_box(&***r)).collect();
        let nested_warnings =
            check_embedded_markdown_blocks_recursive(&stripped_content, &block_rules_owned, config, depth + 1);

        // Adjust nested warning line numbers and add to results
        for mut warning in nested_warnings {
            warning.line += line_offset;
            warning.end_line += line_offset;
            // Clear fix since byte offsets won't be valid for parent file
            warning.fix = None;
            all_warnings.push(warning);
        }

        // Create a context and collect warnings for the embedded content
        // Skip file-scoped rules that don't apply to embedded snippets
        let ctx = LintContext::new(&stripped_content, config.markdown_flavor(), None);
        for rule in &block_rules {
            // Skip file-scoped rules for embedded content
            match rule.name() {
                "MD041" => continue, // "First line in file should be heading" - not a file
                "MD047" => continue, // "File should end with newline" - not a file
                _ => {}
            }

            if let Ok(rule_warnings) = rule.check(&ctx) {
                for warning in rule_warnings {
                    // Create adjusted warning with correct line numbers
                    let adjusted_warning = rumdl_lib::rule::LintWarning {
                        message: warning.message.clone(),
                        line: warning.line + line_offset,
                        column: warning.column,
                        end_line: warning.end_line + line_offset,
                        end_column: warning.end_column,
                        severity: warning.severity,
                        fix: None, // Clear fix since byte offsets won't be valid
                        rule_name: warning.rule_name,
                    };
                    all_warnings.push(adjusted_warning);
                }
            }
        }
    }

    all_warnings
}

/// Strip common leading indentation from all non-empty lines.
/// Returns the stripped content and the common indent string.
pub(super) fn strip_common_indent(content: &str) -> (String, String) {
    let lines: Vec<&str> = content.lines().collect();
    let has_trailing_newline = content.ends_with('\n');

    // Find minimum indentation among non-empty lines
    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    // Build the stripped content
    let mut stripped: String = lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                // Preserve empty lines as empty (no spaces)
                ""
            } else if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                // Fallback: strip what we can
                line.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Preserve trailing newline if original had one
    if has_trailing_newline && !stripped.ends_with('\n') {
        stripped.push('\n');
    }

    // Return the common indent string (spaces)
    let indent_str = " ".repeat(min_indent);

    (stripped, indent_str)
}

/// Restore indentation to all non-empty lines.
/// Preserves trailing newline if present in the original content.
pub(super) fn restore_indent(content: &str, indent: &str) -> String {
    let has_trailing_newline = content.ends_with('\n');

    let mut result: String = content
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Preserve trailing newline
    if has_trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }

    result
}
