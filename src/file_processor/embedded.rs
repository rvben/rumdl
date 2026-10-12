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

// Re-export check/lint functions from the library crate
pub use rumdl_lib::embedded_lint::check_embedded_markdown_blocks;
pub(super) use rumdl_lib::embedded_lint::has_fenced_code_blocks;
pub(super) use rumdl_lib::embedded_lint::markdown_code_blocks;
pub(super) use rumdl_lib::embedded_lint::should_format_embedded_markdown;
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
    if rumdl_lib::merge_conflict::detect_for_rules(content, rules, config, None).is_some() {
        return 0;
    }
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

    let blocks = markdown_code_blocks(content);

    if blocks.is_empty() {
        return 0;
    }

    // Parse inline config from the parent content to respect disable/enable directives
    let inline_config = rumdl_lib::inline_config::InlineConfig::from_content(content);

    let mut formatted_count = 0;

    // Process blocks in reverse order to maintain byte offsets
    for block in blocks.into_iter().rev() {
        // Skip empty blocks
        if block.code.trim().is_empty() {
            continue;
        }

        // Filter rules based on inline config at the block's first line of code
        let block_rules: Vec<Box<dyn Rule>> = rules
            .iter()
            .filter(|rule| !inline_config.is_rule_disabled(rule.name(), block.start_line + 2))
            .map(|r| dyn_clone::clone_box(&**r))
            .collect();

        let had_trailing_newline = block.code.ends_with('\n');

        // Format the code as CommonMark reads it, container prefixes removed
        let mut formatted = block.code.clone();

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
        if !had_trailing_newline && formatted.ends_with('\n') {
            formatted.pop();
        }

        // Write the code back inside the block's containers if it changed
        if formatted != block.code {
            content.replace_range(block.content_start..block.content_end, &block.source_for(&formatted));
            formatted_count += 1;
        }

        formatted_count += nested_formatted;
    }

    formatted_count
}
