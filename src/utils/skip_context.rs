//! Utilities for determining if a position in markdown should be skipped from processing
//!
//! This module provides centralized context detection for various markdown constructs
//! that should typically be skipped when processing rules.

use crate::config::MarkdownFlavor;
use crate::lint_context::LintContext;
use crate::utils::kramdown_utils::is_math_block_delimiter;
use crate::utils::mkdocs_admonitions;
use crate::utils::mkdocs_critic;
use crate::utils::mkdocs_extensions;
use crate::utils::mkdocs_footnotes;
use crate::utils::mkdocs_icons;
use crate::utils::mkdocs_snippets;
use crate::utils::mkdocs_tabs;
use crate::utils::mkdocstrings_refs;
use crate::utils::regex_cache::HTML_COMMENT_PATTERN;
use regex::Regex;
use std::sync::LazyLock;

/// Enhanced inline math pattern that handles both single $ and double $$ delimiters.
/// Matches:
/// - Display math: $$...$$ (zero or more non-$ characters)
/// - Inline math: $...$ (zero or more non-$ non-newline characters)
///
/// The display math pattern is tried first to correctly handle $$content$$.
/// Critically, both patterns allow ZERO characters between delimiters,
/// so empty math like $$ or $ $ is consumed and won't pair with other $ signs.
static INLINE_MATH_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\$[^$]*\$\$|\$[^$\n]*\$").unwrap());

/// Range representing a span of bytes (start inclusive, end exclusive)
#[derive(Debug, Clone, Copy)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

/// Pre-compute all HTML comment ranges in the content
/// Returns a sorted vector of byte ranges for efficient lookup
pub fn compute_html_comment_ranges(content: &str) -> Vec<ByteRange> {
    HTML_COMMENT_PATTERN
        .find_iter(content)
        .map(|m| ByteRange {
            start: m.start(),
            end: m.end(),
        })
        .collect()
}

/// Check if a byte position is within any of the pre-computed HTML comment ranges
/// Uses binary search for O(log n) complexity
pub fn is_in_html_comment_ranges(ranges: &[ByteRange], byte_pos: usize) -> bool {
    // Binary search to find a range that might contain byte_pos
    ranges
        .binary_search_by(|range| {
            if byte_pos < range.start {
                std::cmp::Ordering::Greater
            } else if byte_pos >= range.end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Check if a line is ENTIRELY within a single HTML comment
/// Returns true only if both the line start AND end are within the same comment range
pub fn is_line_entirely_in_html_comment(ranges: &[ByteRange], line_start: usize, line_end: usize) -> bool {
    for range in ranges {
        // If line start is within this range, check if line end is also within it
        if line_start >= range.start && line_start < range.end {
            return line_end <= range.end;
        }
    }
    false
}

/// Check if a line is within front matter (both YAML and TOML)
pub fn is_in_front_matter(content: &str, line_num: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();

    // Check YAML front matter (---) at the beginning
    if !lines.is_empty() && lines[0] == "---" {
        for (i, line) in lines.iter().enumerate().skip(1) {
            if *line == "---" {
                return line_num <= i;
            }
        }
    }

    // Check TOML front matter (+++) at the beginning
    if !lines.is_empty() && lines[0] == "+++" {
        for (i, line) in lines.iter().enumerate().skip(1) {
            if *line == "+++" {
                return line_num <= i;
            }
        }
    }

    false
}

/// Check if a byte position is within any context that should be skipped
pub fn is_in_skip_context(ctx: &LintContext, byte_pos: usize) -> bool {
    // Check standard code contexts
    if ctx.is_in_code_block_or_span(byte_pos) {
        return true;
    }

    // Check HTML comments
    if is_in_html_comment(ctx.content, byte_pos) {
        return true;
    }

    // Check math contexts
    if is_in_math_context(ctx, byte_pos) {
        return true;
    }

    // Check if in HTML tag
    if is_in_html_tag(ctx, byte_pos) {
        return true;
    }

    // Check MDX-specific contexts
    if ctx.flavor == MarkdownFlavor::MDX {
        // Check JSX expressions
        if ctx.is_in_jsx_expression(byte_pos) {
            return true;
        }
        // Check MDX comments
        if ctx.is_in_mdx_comment(byte_pos) {
            return true;
        }
    }

    // Check MkDocs snippet sections and multi-line blocks
    if ctx.flavor == MarkdownFlavor::MkDocs {
        if mkdocs_snippets::is_within_snippet_section(ctx.content, byte_pos) {
            return true;
        }
        if mkdocs_snippets::is_within_snippet_block(ctx.content, byte_pos) {
            return true;
        }
    }

    // Check MkDocs admonition blocks
    if ctx.flavor == MarkdownFlavor::MkDocs && mkdocs_admonitions::is_within_admonition(ctx.content, byte_pos) {
        return true;
    }

    // Check MkDocs footnote definitions
    if ctx.flavor == MarkdownFlavor::MkDocs && mkdocs_footnotes::is_within_footnote_definition(ctx.content, byte_pos) {
        return true;
    }

    // Check MkDocs content tabs
    if ctx.flavor == MarkdownFlavor::MkDocs && mkdocs_tabs::is_within_tab_content(ctx.content, byte_pos) {
        return true;
    }

    // Check MkDocstrings autodoc blocks
    if ctx.flavor == MarkdownFlavor::MkDocs && mkdocstrings_refs::is_within_autodoc_block(ctx.content, byte_pos) {
        return true;
    }

    // Check MkDocs Critic Markup
    if ctx.flavor == MarkdownFlavor::MkDocs && mkdocs_critic::is_within_critic_markup(ctx.content, byte_pos) {
        return true;
    }

    false
}

/// Check if a byte position is within a JSX expression (MDX: {expression})
#[inline]
pub fn is_in_jsx_expression(ctx: &LintContext, byte_pos: usize) -> bool {
    ctx.flavor == MarkdownFlavor::MDX && ctx.is_in_jsx_expression(byte_pos)
}

/// Check if a byte position is within an MDX comment ({/* ... */})
#[inline]
pub fn is_in_mdx_comment(ctx: &LintContext, byte_pos: usize) -> bool {
    ctx.flavor == MarkdownFlavor::MDX && ctx.is_in_mdx_comment(byte_pos)
}

/// Check if a line should be skipped due to MkDocs snippet syntax
pub fn is_mkdocs_snippet_line(line: &str, flavor: MarkdownFlavor) -> bool {
    flavor == MarkdownFlavor::MkDocs && mkdocs_snippets::is_snippet_marker(line)
}

/// Check if a line is a MkDocs admonition marker
pub fn is_mkdocs_admonition_line(line: &str, flavor: MarkdownFlavor) -> bool {
    flavor == MarkdownFlavor::MkDocs && mkdocs_admonitions::is_admonition_marker(line)
}

/// Check if a line is a MkDocs footnote definition
pub fn is_mkdocs_footnote_line(line: &str, flavor: MarkdownFlavor) -> bool {
    flavor == MarkdownFlavor::MkDocs && mkdocs_footnotes::is_footnote_definition(line)
}

/// Check if a line is a MkDocs tab marker
pub fn is_mkdocs_tab_line(line: &str, flavor: MarkdownFlavor) -> bool {
    flavor == MarkdownFlavor::MkDocs && mkdocs_tabs::is_tab_marker(line)
}

/// Check if a line is a MkDocstrings autodoc marker
pub fn is_mkdocstrings_autodoc_line(line: &str, flavor: MarkdownFlavor) -> bool {
    flavor == MarkdownFlavor::MkDocs && mkdocstrings_refs::is_autodoc_marker(line)
}

/// Check if a line contains MkDocs Critic Markup
pub fn is_mkdocs_critic_line(line: &str, flavor: MarkdownFlavor) -> bool {
    flavor == MarkdownFlavor::MkDocs && mkdocs_critic::contains_critic_markup(line)
}

/// Check if a byte position is within an HTML comment
pub fn is_in_html_comment(content: &str, byte_pos: usize) -> bool {
    for m in HTML_COMMENT_PATTERN.find_iter(content) {
        if m.start() <= byte_pos && byte_pos < m.end() {
            return true;
        }
    }
    false
}

/// Check if a byte position is within an HTML tag
pub fn is_in_html_tag(ctx: &LintContext, byte_pos: usize) -> bool {
    for html_tag in ctx.html_tags().iter() {
        if html_tag.byte_offset <= byte_pos && byte_pos < html_tag.byte_end {
            return true;
        }
    }
    false
}

/// Check if a byte position is within a math context (block or inline)
pub fn is_in_math_context(ctx: &LintContext, byte_pos: usize) -> bool {
    let content = ctx.content;

    // Check if we're in a math block
    if is_in_math_block(content, byte_pos) {
        return true;
    }

    // Check if we're in inline math
    if is_in_inline_math(content, byte_pos) {
        return true;
    }

    false
}

/// Check if a byte position is within a math block ($$...$$)
pub fn is_in_math_block(content: &str, byte_pos: usize) -> bool {
    let mut in_math_block = false;
    let mut current_pos = 0;

    for line in content.lines() {
        let line_start = current_pos;
        let line_end = current_pos + line.len();

        // Check if this line is a math block delimiter
        if is_math_block_delimiter(line) {
            if byte_pos >= line_start && byte_pos <= line_end {
                // Position is on the delimiter line itself
                return true;
            }
            in_math_block = !in_math_block;
        } else if in_math_block && byte_pos >= line_start && byte_pos <= line_end {
            // Position is inside a math block
            return true;
        }

        current_pos = line_end + 1; // +1 for newline
    }

    false
}

/// Check if a byte position is within inline math ($...$)
pub fn is_in_inline_math(content: &str, byte_pos: usize) -> bool {
    // Find all inline math spans
    for m in INLINE_MATH_REGEX.find_iter(content) {
        if m.start() <= byte_pos && byte_pos < m.end() {
            return true;
        }
    }
    false
}

/// Check if a position is within a table cell
pub fn is_in_table_cell(ctx: &LintContext, line_num: usize, _col: usize) -> bool {
    // Check if this line is part of a table
    for table_row in ctx.table_rows().iter() {
        if table_row.line == line_num {
            // This line is part of a table
            // For now, we'll skip the entire table row
            // Future enhancement: check specific column boundaries
            return true;
        }
    }
    false
}

/// Check if a line contains table syntax
pub fn is_table_line(line: &str) -> bool {
    let trimmed = line.trim();

    // Check for table separator line
    if trimmed
        .chars()
        .all(|c| c == '|' || c == '-' || c == ':' || c.is_whitespace())
        && trimmed.contains('|')
        && trimmed.contains('-')
    {
        return true;
    }

    // Check for table content line (starts and/or ends with |)
    if (trimmed.starts_with('|') || trimmed.ends_with('|')) && trimmed.matches('|').count() >= 2 {
        return true;
    }

    false
}

/// Check if a byte position is within an MkDocs icon shortcode
/// Icon shortcodes use format like `:material-check:`, `:octicons-mark-github-16:`
pub fn is_in_icon_shortcode(line: &str, position: usize, _flavor: MarkdownFlavor) -> bool {
    // Only skip for MkDocs flavor, but check pattern for all flavors
    // since emoji shortcodes are universal
    mkdocs_icons::is_in_any_shortcode(line, position)
}

/// Check if a byte position is within PyMdown extension markup
/// Includes: Keys (++ctrl+alt++), Caret (^text^), Insert (^^text^^), Mark (==text==)
pub fn is_in_pymdown_markup(line: &str, position: usize, flavor: MarkdownFlavor) -> bool {
    if flavor != MarkdownFlavor::MkDocs {
        return false;
    }
    mkdocs_extensions::is_in_pymdown_markup(line, position)
}

/// Check if a byte position is within any MkDocs-specific markup
/// Combines icon shortcodes and PyMdown extensions
pub fn is_in_mkdocs_markup(line: &str, position: usize, flavor: MarkdownFlavor) -> bool {
    if is_in_icon_shortcode(line, position, flavor) {
        return true;
    }
    if is_in_pymdown_markup(line, position, flavor) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_comment_detection() {
        let content = "Text <!-- comment --> more text";
        assert!(is_in_html_comment(content, 10)); // Inside comment
        assert!(!is_in_html_comment(content, 0)); // Before comment
        assert!(!is_in_html_comment(content, 25)); // After comment
    }

    #[test]
    fn test_is_line_entirely_in_html_comment() {
        // Test 1: Multi-line comment with content after closing
        let content = "<!--\ncomment\n--> Content after comment";
        let ranges = compute_html_comment_ranges(content);
        // Line 0: "<!--" (bytes 0-4) - entirely in comment
        assert!(is_line_entirely_in_html_comment(&ranges, 0, 4));
        // Line 1: "comment" (bytes 5-12) - entirely in comment
        assert!(is_line_entirely_in_html_comment(&ranges, 5, 12));
        // Line 2: "--> Content after comment" (bytes 13-38) - NOT entirely in comment
        assert!(!is_line_entirely_in_html_comment(&ranges, 13, 38));

        // Test 2: Single-line comment with content after
        let content2 = "<!-- comment --> Not a comment";
        let ranges2 = compute_html_comment_ranges(content2);
        // The entire line is NOT entirely in the comment
        assert!(!is_line_entirely_in_html_comment(&ranges2, 0, 30));

        // Test 3: Single-line comment alone
        let content3 = "<!-- comment -->";
        let ranges3 = compute_html_comment_ranges(content3);
        // The entire line IS entirely in the comment
        assert!(is_line_entirely_in_html_comment(&ranges3, 0, 16));

        // Test 4: Content before comment
        let content4 = "Text before <!-- comment -->";
        let ranges4 = compute_html_comment_ranges(content4);
        // Line start is NOT in the comment range
        assert!(!is_line_entirely_in_html_comment(&ranges4, 0, 28));
    }

    #[test]
    fn test_math_block_detection() {
        let content = "Text\n$$\nmath content\n$$\nmore text";
        assert!(is_in_math_block(content, 8)); // On opening $$
        assert!(is_in_math_block(content, 15)); // Inside math block
        assert!(!is_in_math_block(content, 0)); // Before math block
        assert!(!is_in_math_block(content, 30)); // After math block
    }

    #[test]
    fn test_inline_math_detection() {
        let content = "Text $x + y$ and $$a^2 + b^2$$ here";
        assert!(is_in_inline_math(content, 7)); // Inside first math
        assert!(is_in_inline_math(content, 20)); // Inside second math
        assert!(!is_in_inline_math(content, 0)); // Before math
        assert!(!is_in_inline_math(content, 35)); // After math
    }

    #[test]
    fn test_table_line_detection() {
        assert!(is_table_line("| Header | Column |"));
        assert!(is_table_line("|--------|--------|"));
        assert!(is_table_line("| Cell 1 | Cell 2 |"));
        assert!(!is_table_line("Regular text"));
        assert!(!is_table_line("Just a pipe | here"));
    }

    #[test]
    fn test_is_in_front_matter() {
        // Test YAML frontmatter
        let yaml_content = r#"---
title: "My Post"
tags: ["test", "example"]
---

# Content"#;

        assert!(
            is_in_front_matter(yaml_content, 0),
            "Line 1 should be in YAML front matter"
        );
        assert!(
            is_in_front_matter(yaml_content, 2),
            "Line 3 should be in YAML front matter"
        );
        assert!(
            is_in_front_matter(yaml_content, 3),
            "Line 4 should be in YAML front matter"
        );
        assert!(
            !is_in_front_matter(yaml_content, 4),
            "Line 5 should NOT be in front matter"
        );

        // Test TOML frontmatter
        let toml_content = r#"+++
title = "My Post"
tags = ["test", "example"]
+++

# Content"#;

        assert!(
            is_in_front_matter(toml_content, 0),
            "Line 1 should be in TOML front matter"
        );
        assert!(
            is_in_front_matter(toml_content, 2),
            "Line 3 should be in TOML front matter"
        );
        assert!(
            is_in_front_matter(toml_content, 3),
            "Line 4 should be in TOML front matter"
        );
        assert!(
            !is_in_front_matter(toml_content, 4),
            "Line 5 should NOT be in front matter"
        );

        // Test TOML blocks NOT at beginning (should not be considered front matter)
        let mixed_content = r#"# Content

+++
title = "Not frontmatter"
+++

More content"#;

        assert!(
            !is_in_front_matter(mixed_content, 2),
            "TOML block not at beginning should NOT be front matter"
        );
        assert!(
            !is_in_front_matter(mixed_content, 3),
            "TOML block not at beginning should NOT be front matter"
        );
        assert!(
            !is_in_front_matter(mixed_content, 4),
            "TOML block not at beginning should NOT be front matter"
        );
    }

    #[test]
    fn test_is_in_icon_shortcode() {
        let line = "Click :material-check: to confirm";
        // Position 0-5 is "Click"
        assert!(!is_in_icon_shortcode(line, 0, MarkdownFlavor::MkDocs));
        // Position 6-22 is ":material-check:"
        assert!(is_in_icon_shortcode(line, 6, MarkdownFlavor::MkDocs));
        assert!(is_in_icon_shortcode(line, 15, MarkdownFlavor::MkDocs));
        assert!(is_in_icon_shortcode(line, 21, MarkdownFlavor::MkDocs));
        // Position 22+ is " to confirm"
        assert!(!is_in_icon_shortcode(line, 22, MarkdownFlavor::MkDocs));
    }

    #[test]
    fn test_is_in_pymdown_markup() {
        // Test Keys notation
        let line = "Press ++ctrl+c++ to copy";
        assert!(!is_in_pymdown_markup(line, 0, MarkdownFlavor::MkDocs));
        assert!(is_in_pymdown_markup(line, 6, MarkdownFlavor::MkDocs));
        assert!(is_in_pymdown_markup(line, 10, MarkdownFlavor::MkDocs));
        assert!(!is_in_pymdown_markup(line, 17, MarkdownFlavor::MkDocs));

        // Test Mark notation
        let line2 = "This is ==highlighted== text";
        assert!(!is_in_pymdown_markup(line2, 0, MarkdownFlavor::MkDocs));
        assert!(is_in_pymdown_markup(line2, 8, MarkdownFlavor::MkDocs));
        assert!(is_in_pymdown_markup(line2, 15, MarkdownFlavor::MkDocs));
        assert!(!is_in_pymdown_markup(line2, 23, MarkdownFlavor::MkDocs));

        // Should not match for Standard flavor
        assert!(!is_in_pymdown_markup(line, 10, MarkdownFlavor::Standard));
    }

    #[test]
    fn test_is_in_mkdocs_markup() {
        // Should combine both icon and pymdown
        let line = ":material-check: and ++ctrl++";
        assert!(is_in_mkdocs_markup(line, 5, MarkdownFlavor::MkDocs)); // In icon
        assert!(is_in_mkdocs_markup(line, 23, MarkdownFlavor::MkDocs)); // In keys
        assert!(!is_in_mkdocs_markup(line, 17, MarkdownFlavor::MkDocs)); // In " and "
    }
}
