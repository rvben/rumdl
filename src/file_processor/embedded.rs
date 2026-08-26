//! Embedded markdown formatting and checking.
//!
//! Handles detecting, linting, and formatting markdown content
//! embedded inside fenced code blocks with `markdown` or `md` language.
//!
//! Check/lint functions delegate to `rumdl_lib::embedded_lint` so that
//! both the CLI and the LSP share the same implementation.

use rumdl_lib::config as rumdl_config;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::utils::code_block_utils::CodeBlockUtils;

// Re-export check/lint functions from the library crate
pub use rumdl_lib::embedded_lint::check_embedded_markdown_blocks;
pub(super) use rumdl_lib::embedded_lint::has_fenced_code_blocks;
pub(super) use rumdl_lib::embedded_lint::should_lint_embedded_markdown;

/// Maximum recursion depth for formatting nested markdown blocks.
pub(super) const MAX_EMBEDDED_DEPTH: usize = rumdl_lib::embedded_lint::MAX_EMBEDDED_DEPTH;

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
            super::processing::apply_fixes_coordinated(&block_rules, &mut formatted, true, true, config, None);
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

/// Strip common leading indentation from all non-empty lines.
/// Returns the stripped content and the common indent string.
pub(super) fn strip_common_indent(content: &str) -> (String, String) {
    rumdl_lib::embedded_lint::strip_common_indent(content)
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
