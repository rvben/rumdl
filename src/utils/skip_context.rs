//! Utilities for determining if a position in markdown should be skipped from processing
//!
//! This module provides centralized context detection for various markdown constructs
//! that should typically be skipped when processing rules.

use crate::lint_context::LintContext;
use crate::utils::kramdown_utils::is_math_block_delimiter;
use crate::utils::regex_cache::HTML_COMMENT_PATTERN;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Enhanced inline math pattern that handles both single $ and double $$ delimiters
    static ref INLINE_MATH_REGEX: Regex = Regex::new(r"\$(?:\$)?[^$]+\$(?:\$)?").unwrap();
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

    false
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
}
