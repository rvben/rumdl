//! PyMdown Extensions Blocks detection utilities
//!
//! This module provides detection for PyMdown Extensions "Blocks" syntax which uses
//! `///` markers to create structured content blocks.
//!
//! Common patterns:
//! - `/// caption` - Caption block for figures/tables
//! - `/// details | Summary title` - Collapsible content
//! - `/// admonition | Title` - Admonition with custom title
//! - `/// html | div` - HTML wrapper block
//! - `///` - Closing marker
//!
//! Blocks can have YAML options indented 4 spaces after the header line:
//! ```text
//! /// caption
//!     attrs: {id: my-id}
//! Caption text
//! ///
//! ```
//!
//! Supported block types: caption, figure-caption, details, admonition, html, definition, tab

use regex::Regex;
use std::sync::LazyLock;

use crate::utils::skip_context::ByteRange;

/// Pattern to match block opening markers
/// Matches: /// block-type, /// block-type | args, etc.
/// Does NOT match a closing /// on its own
static BLOCK_OPEN_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)///\s*(?:[a-zA-Z][a-zA-Z0-9_-]*)").unwrap());

/// Pattern to match block closing markers
/// Matches: /// (with optional whitespace before and after)
static BLOCK_CLOSE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)///\s*$").unwrap());

/// Check if a line is a block opening marker
pub fn is_block_open(line: &str) -> bool {
    BLOCK_OPEN_PATTERN.is_match(line)
}

/// Check if a line is a block closing marker (just `///`)
pub fn is_block_close(line: &str) -> bool {
    BLOCK_CLOSE_PATTERN.is_match(line)
}

/// Get the indentation level of a block marker
pub fn get_block_indent(line: &str) -> usize {
    let mut indent = 0;
    for c in line.chars() {
        match c {
            ' ' => indent += 1,
            '\t' => indent += 4, // Tabs expand to 4 spaces (CommonMark)
            _ => break,
        }
    }
    indent
}

/// Track block nesting state for a document
#[derive(Debug, Clone, Default)]
pub struct BlockTracker {
    /// Stack of block indentation levels for nesting tracking
    indent_stack: Vec<usize>,
}

impl BlockTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a line and return whether we're inside a block after processing
    pub fn process_line(&mut self, line: &str) -> bool {
        let trimmed = line.trim_start();

        if trimmed.starts_with("///") {
            let indent = get_block_indent(line);

            if is_block_close(line) {
                // Closing marker - pop the matching block from stack
                // Pop the top block if its indent is >= the closing marker's indent
                if let Some(&top_indent) = self.indent_stack.last()
                    && top_indent >= indent
                {
                    self.indent_stack.pop();
                }
            } else if is_block_open(line) {
                // Opening marker - push to stack
                self.indent_stack.push(indent);
            }
        }

        !self.indent_stack.is_empty()
    }

    /// Check if we're currently inside a block
    pub fn is_inside_block(&self) -> bool {
        !self.indent_stack.is_empty()
    }

    /// Get current nesting depth
    pub fn depth(&self) -> usize {
        self.indent_stack.len()
    }
}

/// Detect PyMdown block ranges in content
/// Returns a vector of byte ranges (start, end) for each block
pub fn detect_block_ranges(content: &str) -> Vec<ByteRange> {
    let mut ranges = Vec::new();
    let mut tracker = BlockTracker::new();
    let mut block_start: Option<usize> = None;
    let mut byte_offset = 0;

    for line in content.lines() {
        let line_len = line.len();
        let was_inside = tracker.is_inside_block();
        let is_inside = tracker.process_line(line);

        // Started a new block
        if !was_inside && is_inside {
            block_start = Some(byte_offset);
        }
        // Exited a block
        else if was_inside
            && !is_inside
            && let Some(start) = block_start.take()
        {
            // End at the end of the closing line
            ranges.push(ByteRange {
                start,
                end: byte_offset + line_len,
            });
        }

        // Account for newline
        byte_offset += line_len + 1;
    }

    // Handle unclosed blocks at end of document
    if let Some(start) = block_start {
        ranges.push(ByteRange {
            start,
            end: content.len(),
        });
    }

    ranges
}

/// Check if a byte position is within a block
pub fn is_within_block_ranges(ranges: &[ByteRange], position: usize) -> bool {
    ranges.iter().any(|r| position >= r.start && position < r.end)
}

/// Extract the block type from an opening line
/// Returns the block type like "caption", "details", "admonition", etc.
pub fn extract_block_type(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("///") {
        return None;
    }

    let after_marker = trimmed[3..].trim_start();
    // Block type is the first word (before any | or whitespace)
    after_marker
        .split(|c: char| c.is_whitespace() || c == '|')
        .next()
        .filter(|s| !s.is_empty())
}

/// Extract arguments from a block opening line (text after |)
pub fn extract_block_args(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("///") {
        return None;
    }

    // Find the | separator
    if let Some(pipe_pos) = trimmed.find('|') {
        let args = trimmed[pipe_pos + 1..].trim();
        if !args.is_empty() {
            return Some(args);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_open_detection() {
        // Valid block openings
        assert!(is_block_open("/// caption"));
        assert!(is_block_open("/// details | Summary"));
        assert!(is_block_open("/// admonition | Custom Title"));
        assert!(is_block_open("/// html | div"));
        assert!(is_block_open("/// figure-caption"));
        assert!(is_block_open("  /// caption")); // Indented

        // Invalid patterns
        assert!(!is_block_open("///")); // Just closing marker
        assert!(!is_block_open("///  ")); // Just closing with trailing space
        assert!(!is_block_open("Regular text"));
        assert!(!is_block_open("# Heading"));
        assert!(!is_block_open("```python")); // Code fence
        assert!(!is_block_open("// comment")); // Not enough slashes
    }

    #[test]
    fn test_block_close_detection() {
        assert!(is_block_close("///"));
        assert!(is_block_close("///  "));
        assert!(is_block_close("  ///"));
        assert!(is_block_close("    ///  "));

        assert!(!is_block_close("/// caption"));
        assert!(!is_block_close("/// details | Summary"));
        assert!(!is_block_close("///caption")); // No space, but this matches opening
    }

    #[test]
    fn test_block_tracker() {
        let mut tracker = BlockTracker::new();

        // Enter a block
        assert!(tracker.process_line("/// caption"));
        assert!(tracker.is_inside_block());
        assert_eq!(tracker.depth(), 1);

        // Inside content
        assert!(tracker.process_line("This is content."));
        assert!(tracker.is_inside_block());

        // Exit the block
        assert!(!tracker.process_line("///"));
        assert!(!tracker.is_inside_block());
        assert_eq!(tracker.depth(), 0);
    }

    #[test]
    fn test_nested_blocks() {
        let mut tracker = BlockTracker::new();

        // Outer block
        assert!(tracker.process_line("/// details | Outer"));
        assert_eq!(tracker.depth(), 1);

        // Inner block
        assert!(tracker.process_line("  /// caption"));
        assert_eq!(tracker.depth(), 2);

        // Content
        assert!(tracker.process_line("    Content"));
        assert!(tracker.is_inside_block());

        // Close inner
        assert!(tracker.process_line("  ///"));
        assert_eq!(tracker.depth(), 1);

        // Close outer
        assert!(!tracker.process_line("///"));
        assert_eq!(tracker.depth(), 0);
    }

    #[test]
    fn test_detect_block_ranges() {
        let content = r#"# Heading

/// caption
Table caption here.
///

Regular text.

/// details | Click to expand
Hidden content.
///
"#;
        let ranges = detect_block_ranges(content);
        assert_eq!(ranges.len(), 2);

        // First block
        let first_block_content = &content[ranges[0].start..ranges[0].end];
        assert!(first_block_content.contains("caption"));
        assert!(first_block_content.contains("Table caption here"));

        // Second block
        let second_block_content = &content[ranges[1].start..ranges[1].end];
        assert!(second_block_content.contains("details"));
        assert!(second_block_content.contains("Hidden content"));
    }

    #[test]
    fn test_extract_block_type() {
        assert_eq!(extract_block_type("/// caption"), Some("caption"));
        assert_eq!(extract_block_type("/// details | Summary"), Some("details"));
        assert_eq!(extract_block_type("/// figure-caption"), Some("figure-caption"));
        assert_eq!(extract_block_type("/// admonition | Title"), Some("admonition"));
        assert_eq!(extract_block_type("  /// html | div"), Some("html"));

        assert_eq!(extract_block_type("///"), None);
        assert_eq!(extract_block_type("Regular text"), None);
    }

    #[test]
    fn test_extract_block_args() {
        assert_eq!(extract_block_args("/// details | Summary Title"), Some("Summary Title"));
        assert_eq!(extract_block_args("/// caption | <"), Some("<"));
        assert_eq!(extract_block_args("/// figure-caption | 12"), Some("12"));
        assert_eq!(extract_block_args("/// html | div"), Some("div"));

        assert_eq!(extract_block_args("/// caption"), None);
        assert_eq!(extract_block_args("///"), None);
    }

    #[test]
    fn test_block_with_yaml_options() {
        let content = r#"/// caption
    attrs: {id: my-id, class: special}
Caption text here.
///
"#;
        let ranges = detect_block_ranges(content);
        assert_eq!(ranges.len(), 1);

        let block_content = &content[ranges[0].start..ranges[0].end];
        assert!(block_content.contains("attrs:"));
        assert!(block_content.contains("Caption text"));
    }

    #[test]
    fn test_unclosed_block() {
        let content = r#"/// caption
This block is never closed.
"#;
        let ranges = detect_block_ranges(content);
        assert_eq!(ranges.len(), 1);
        // Should include all content to end of document
        assert_eq!(ranges[0].end, content.len());
    }

    #[test]
    fn test_prepend_caption() {
        // Caption before content using | <
        let content = r#"![image](./image.jpeg)

/// caption | <
Caption above the image
///
"#;
        let ranges = detect_block_ranges(content);
        assert_eq!(ranges.len(), 1);

        let args = extract_block_args("/// caption | <");
        assert_eq!(args, Some("<"));
    }

    #[test]
    fn test_figure_caption_with_number() {
        let content = r#"/// figure-caption | 12
Figure 12: Description
///
"#;
        let ranges = detect_block_ranges(content);
        assert_eq!(ranges.len(), 1);

        let block_type = extract_block_type("/// figure-caption | 12");
        assert_eq!(block_type, Some("figure-caption"));
    }
}

#[cfg(test)]
mod integration_tests {
    //! Integration tests verifying LintContext correctly marks lines inside PyMdown blocks
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    /// Test line_info flag is correctly set for PyMdown blocks
    #[test]
    fn test_line_info_in_pymdown_block_flag() {
        let content = r#"# Heading
/// caption
Content line
///
Normal line
"#;

        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

        // Line 1 (Heading) - not in block
        assert!(
            !ctx.line_info(1).is_some_and(|info| info.in_pymdown_block),
            "Line 1 should not be in PyMdown block"
        );

        // Line 2 (/// caption) - is in block (opening marker is part of block)
        assert!(
            ctx.line_info(2).is_some_and(|info| info.in_pymdown_block),
            "Line 2 should be in PyMdown block"
        );

        // Line 3 (Content line) - is in block
        assert!(
            ctx.line_info(3).is_some_and(|info| info.in_pymdown_block),
            "Line 3 should be in PyMdown block"
        );

        // Line 4 (///) - is in block (closing marker is part of block)
        assert!(
            ctx.line_info(4).is_some_and(|info| info.in_pymdown_block),
            "Line 4 should be in PyMdown block"
        );

        // Line 5 (Normal line) - not in block
        assert!(
            !ctx.line_info(5).is_some_and(|info| info.in_pymdown_block),
            "Line 5 should not be in PyMdown block"
        );
    }

    /// Test that standard flavor does NOT enable PyMdown block detection
    #[test]
    fn test_standard_flavor_ignores_pymdown_syntax() {
        let content = r#"# Heading
/// caption
Content line
///
Normal line
"#;

        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // In Standard flavor, PyMdown blocks should NOT be detected
        assert!(
            !ctx.line_info(2).is_some_and(|info| info.in_pymdown_block),
            "Standard flavor should NOT recognize PyMdown blocks"
        );
        assert!(
            !ctx.line_info(3).is_some_and(|info| info.in_pymdown_block),
            "Standard flavor should NOT recognize PyMdown blocks"
        );
    }

    /// Test nested PyMdown blocks
    #[test]
    fn test_nested_pymdown_blocks() {
        let content = r#"# Heading
/// details | Outer
Outer content
  /// caption
  Nested content
  ///
More outer content
///
Normal line
"#;

        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

        // All lines 2-8 should be inside a PyMdown block
        for line_num in 2..=8 {
            assert!(
                ctx.line_info(line_num).is_some_and(|info| info.in_pymdown_block),
                "Line {line_num} should be in PyMdown block"
            );
        }

        // Line 9 (Normal line) - not in block
        assert!(
            !ctx.line_info(9).is_some_and(|info| info.in_pymdown_block),
            "Line 9 should not be in PyMdown block"
        );
    }

    /// Test filtered_lines skips PyMdown blocks correctly
    #[test]
    fn test_filtered_lines_skips_pymdown_blocks() {
        use crate::filtered_lines::FilteredLinesExt;

        let content = r#"Line 1
/// caption
Inside block line 3
///
Line 5
"#;

        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

        let filtered: Vec<_> = ctx.filtered_lines().skip_pymdown_blocks().into_iter().collect();

        // Should only contain lines 1 and 5 (not lines 2-4 which are in the block)
        let line_nums: Vec<_> = filtered.iter().map(|l| l.line_num).collect();
        assert_eq!(line_nums, vec![1, 5], "filtered_lines should skip PyMdown block lines");
    }
}
