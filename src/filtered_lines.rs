//! Filtered line iteration for markdown linting
//!
//! This module provides a zero-cost abstraction for iterating over markdown lines
//! while automatically filtering out non-content regions like front matter, code blocks,
//! and HTML blocks. This ensures rules only process actual markdown content.
//!
//! # Architecture
//!
//! The filtered iterator approach centralizes the logic of what content should be
//! processed by rules, eliminating error-prone manual checks in each rule implementation.
//!
//! # Examples
//!
//! ```rust
//! use rumdl_lib::lint_context::LintContext;
//! use rumdl_lib::filtered_lines::FilteredLinesExt;
//!
//! let content = "---\nurl: http://example.com\n---\n\n# Title\n\nContent";
//! let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
//!
//! // Simple: get all content lines (skips front matter by default)
//! for line in ctx.content_lines() {
//!     println!("Line {}: {}", line.line_num, line.content);
//! }
//!
//! // Advanced: custom filter configuration
//! for line in ctx.filtered_lines()
//!     .skip_code_blocks()
//!     .skip_front_matter()
//!     .skip_html_blocks() {
//!     println!("Line {}: {}", line.line_num, line.content);
//! }
//! ```

use crate::lint_context::{LineInfo, LintContext};

/// A single line from a filtered iteration, with guaranteed 1-indexed line numbers
#[derive(Debug, Clone)]
pub struct FilteredLine<'a> {
    /// The 1-indexed line number in the original document
    pub line_num: usize,
    /// Reference to the line's metadata
    pub line_info: &'a LineInfo,
    /// The actual line content
    pub content: &'a str,
}

/// Configuration for filtering lines during iteration
///
/// Use the builder pattern to configure which types of content should be skipped:
///
/// ```rust
/// use rumdl_lib::filtered_lines::LineFilterConfig;
///
/// let config = LineFilterConfig::new()
///     .skip_front_matter()
///     .skip_code_blocks()
///     .skip_html_blocks()
///     .skip_html_comments();
/// ```
#[derive(Debug, Clone, Default)]
pub struct LineFilterConfig {
    /// Skip lines inside front matter (YAML/TOML/JSON metadata)
    pub skip_front_matter: bool,
    /// Skip lines inside fenced code blocks
    pub skip_code_blocks: bool,
    /// Skip lines inside HTML blocks
    pub skip_html_blocks: bool,
    /// Skip lines inside HTML comments
    pub skip_html_comments: bool,
}

impl LineFilterConfig {
    /// Create a new filter configuration with all filters disabled
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Skip lines that are part of front matter (YAML/TOML/JSON)
    ///
    /// Front matter is metadata at the start of a markdown file and should
    /// not be processed by markdown linting rules.
    #[must_use]
    pub fn skip_front_matter(mut self) -> Self {
        self.skip_front_matter = true;
        self
    }

    /// Skip lines inside fenced code blocks
    ///
    /// Code blocks contain source code, not markdown, and most rules should
    /// not process them.
    #[must_use]
    pub fn skip_code_blocks(mut self) -> Self {
        self.skip_code_blocks = true;
        self
    }

    /// Skip lines inside HTML blocks
    ///
    /// HTML blocks contain raw HTML and most markdown rules should not
    /// process them.
    #[must_use]
    pub fn skip_html_blocks(mut self) -> Self {
        self.skip_html_blocks = true;
        self
    }

    /// Skip lines inside HTML comments
    ///
    /// HTML comments (<!-- ... -->) are metadata and should not be processed
    /// by most markdown linting rules.
    #[must_use]
    pub fn skip_html_comments(mut self) -> Self {
        self.skip_html_comments = true;
        self
    }

    /// Check if a line should be filtered out based on this configuration
    fn should_filter(&self, line_info: &LineInfo) -> bool {
        (self.skip_front_matter && line_info.in_front_matter)
            || (self.skip_code_blocks && line_info.in_code_block)
            || (self.skip_html_blocks && line_info.in_html_block)
            || (self.skip_html_comments && line_info.in_html_comment)
    }
}

/// Iterator that yields filtered lines based on configuration
pub struct FilteredLinesIter<'a> {
    ctx: &'a LintContext<'a>,
    config: LineFilterConfig,
    current_index: usize,
}

impl<'a> FilteredLinesIter<'a> {
    /// Create a new filtered lines iterator
    fn new(ctx: &'a LintContext<'a>, config: LineFilterConfig) -> Self {
        Self {
            ctx,
            config,
            current_index: 0,
        }
    }
}

impl<'a> Iterator for FilteredLinesIter<'a> {
    type Item = FilteredLine<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let lines = &self.ctx.lines;
        let content_lines: Vec<&str> = self.ctx.content.lines().collect();

        while self.current_index < lines.len() {
            let idx = self.current_index;
            self.current_index += 1;

            // Check if this line should be filtered
            if self.config.should_filter(&lines[idx]) {
                continue;
            }

            // Get the actual line content from the document
            let line_content = content_lines.get(idx).copied().unwrap_or("");

            // Return the filtered line with 1-indexed line number
            return Some(FilteredLine {
                line_num: idx + 1, // Convert 0-indexed to 1-indexed
                line_info: &lines[idx],
                content: line_content,
            });
        }

        None
    }
}

/// Extension trait that adds filtered iteration methods to `LintContext`
///
/// This trait provides convenient methods for iterating over lines while
/// automatically filtering out non-content regions.
pub trait FilteredLinesExt {
    /// Start building a filtered lines iterator
    ///
    /// Returns a `LineFilterConfig` builder that can be used to configure
    /// which types of content should be filtered out.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rumdl_lib::lint_context::LintContext;
    /// use rumdl_lib::filtered_lines::FilteredLinesExt;
    ///
    /// let content = "# Title\n\n```rust\ncode\n```\n\nContent";
    /// let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    ///
    /// for line in ctx.filtered_lines().skip_code_blocks() {
    ///     println!("Line {}: {}", line.line_num, line.content);
    /// }
    /// ```
    fn filtered_lines(&self) -> FilteredLinesBuilder<'_>;

    /// Get an iterator over content lines only
    ///
    /// This is a convenience method that returns an iterator with front matter
    /// filtered out by default. This is the most common use case for rules that
    /// should only process markdown content.
    ///
    /// Equivalent to: `ctx.filtered_lines().skip_front_matter()`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rumdl_lib::lint_context::LintContext;
    /// use rumdl_lib::filtered_lines::FilteredLinesExt;
    ///
    /// let content = "---\ntitle: Test\n---\n\n# Content";
    /// let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    ///
    /// for line in ctx.content_lines() {
    ///     // Front matter is automatically skipped
    ///     println!("Line {}: {}", line.line_num, line.content);
    /// }
    /// ```
    fn content_lines(&self) -> FilteredLinesIter<'_>;
}

/// Builder type that allows chaining filter configuration and converting to an iterator
pub struct FilteredLinesBuilder<'a> {
    ctx: &'a LintContext<'a>,
    config: LineFilterConfig,
}

impl<'a> FilteredLinesBuilder<'a> {
    fn new(ctx: &'a LintContext<'a>) -> Self {
        Self {
            ctx,
            config: LineFilterConfig::new(),
        }
    }

    /// Skip lines that are part of front matter (YAML/TOML/JSON)
    #[must_use]
    pub fn skip_front_matter(mut self) -> Self {
        self.config = self.config.skip_front_matter();
        self
    }

    /// Skip lines inside fenced code blocks
    #[must_use]
    pub fn skip_code_blocks(mut self) -> Self {
        self.config = self.config.skip_code_blocks();
        self
    }

    /// Skip lines inside HTML blocks
    #[must_use]
    pub fn skip_html_blocks(mut self) -> Self {
        self.config = self.config.skip_html_blocks();
        self
    }

    /// Skip lines inside HTML comments
    #[must_use]
    pub fn skip_html_comments(mut self) -> Self {
        self.config = self.config.skip_html_comments();
        self
    }
}

impl<'a> IntoIterator for FilteredLinesBuilder<'a> {
    type Item = FilteredLine<'a>;
    type IntoIter = FilteredLinesIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        FilteredLinesIter::new(self.ctx, self.config)
    }
}

impl<'a> FilteredLinesExt for LintContext<'a> {
    fn filtered_lines(&self) -> FilteredLinesBuilder<'_> {
        FilteredLinesBuilder::new(self)
    }

    fn content_lines(&self) -> FilteredLinesIter<'_> {
        FilteredLinesIter::new(self, LineFilterConfig::new().skip_front_matter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;

    #[test]
    fn test_filtered_line_structure() {
        let content = "# Title\n\nContent";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard);

        let line = ctx.content_lines().next().unwrap();
        assert_eq!(line.line_num, 1);
        assert_eq!(line.content, "# Title");
        assert!(!line.line_info.in_front_matter);
    }

    #[test]
    fn test_skip_front_matter_yaml() {
        let content = "---\ntitle: Test\nurl: http://example.com\n---\n\n# Content\n\nMore content";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard);

        let lines: Vec<_> = ctx.content_lines().collect();
        // After front matter (lines 1-4), we have: empty line, "# Content", empty line, "More content"
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].line_num, 5); // First line after front matter
        assert_eq!(lines[0].content, "");
        assert_eq!(lines[1].line_num, 6);
        assert_eq!(lines[1].content, "# Content");
        assert_eq!(lines[2].line_num, 7);
        assert_eq!(lines[2].content, "");
        assert_eq!(lines[3].line_num, 8);
        assert_eq!(lines[3].content, "More content");
    }

    #[test]
    fn test_skip_front_matter_toml() {
        let content = "+++\ntitle = \"Test\"\nurl = \"http://example.com\"\n+++\n\n# Content";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard);

        let lines: Vec<_> = ctx.content_lines().collect();
        assert_eq!(lines.len(), 2); // Empty line + "# Content"
        assert_eq!(lines[0].line_num, 5);
        assert_eq!(lines[1].line_num, 6);
        assert_eq!(lines[1].content, "# Content");
    }

    #[test]
    fn test_skip_front_matter_json() {
        let content = "{\n\"title\": \"Test\",\n\"url\": \"http://example.com\"\n}\n\n# Content";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard);

        let lines: Vec<_> = ctx.content_lines().collect();
        assert_eq!(lines.len(), 2); // Empty line + "# Content"
        assert_eq!(lines[0].line_num, 5);
        assert_eq!(lines[1].line_num, 6);
        assert_eq!(lines[1].content, "# Content");
    }

    #[test]
    fn test_skip_code_blocks() {
        let content = "# Title\n\n```rust\nlet x = 1;\nlet y = 2;\n```\n\nContent";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard);

        let lines: Vec<_> = ctx.filtered_lines().skip_code_blocks().into_iter().collect();

        // Should have: "# Title", empty line, "```rust" fence, "```" fence, empty line, "Content"
        // Wait, actually code blocks include the fences. Let me check the line_info
        // Looking at the implementation, in_code_block is true for lines INSIDE code blocks
        // The fences themselves are not marked as in_code_block
        assert!(lines.iter().any(|l| l.content == "# Title"));
        assert!(lines.iter().any(|l| l.content == "Content"));
        // The actual code lines should be filtered out
        assert!(!lines.iter().any(|l| l.content == "let x = 1;"));
        assert!(!lines.iter().any(|l| l.content == "let y = 2;"));
    }

    #[test]
    fn test_no_filters() {
        let content = "---\ntitle: Test\n---\n\n# Content";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard);

        // With no filters, all lines should be included
        let lines: Vec<_> = ctx.filtered_lines().into_iter().collect();
        assert_eq!(lines.len(), ctx.lines.len());
    }

    #[test]
    fn test_multiple_filters() {
        let content = "---\ntitle: Test\n---\n\n# Title\n\n```rust\ncode\n```\n\nContent";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard);

        let lines: Vec<_> = ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .into_iter()
            .collect();

        // Should skip front matter (lines 1-3) and code block content (line 8)
        assert!(lines.iter().any(|l| l.content == "# Title"));
        assert!(lines.iter().any(|l| l.content == "Content"));
        assert!(!lines.iter().any(|l| l.content == "title: Test"));
        assert!(!lines.iter().any(|l| l.content == "code"));
    }

    #[test]
    fn test_line_numbering_is_1_indexed() {
        let content = "First\nSecond\nThird";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard);

        let lines: Vec<_> = ctx.content_lines().collect();
        assert_eq!(lines[0].line_num, 1);
        assert_eq!(lines[0].content, "First");
        assert_eq!(lines[1].line_num, 2);
        assert_eq!(lines[1].content, "Second");
        assert_eq!(lines[2].line_num, 3);
        assert_eq!(lines[2].content, "Third");
    }

    #[test]
    fn test_content_lines_convenience_method() {
        let content = "---\nfoo: bar\n---\n\nContent";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard);

        // content_lines() should automatically skip front matter
        let lines: Vec<_> = ctx.content_lines().collect();
        assert!(!lines.iter().any(|l| l.content.contains("foo")));
        assert!(lines.iter().any(|l| l.content == "Content"));
    }

    #[test]
    fn test_empty_document() {
        let content = "";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard);

        let lines: Vec<_> = ctx.content_lines().collect();
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_only_front_matter() {
        let content = "---\ntitle: Test\n---";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard);

        let lines: Vec<_> = ctx.content_lines().collect();
        assert_eq!(
            lines.len(),
            0,
            "Document with only front matter should have no content lines"
        );
    }

    #[test]
    fn test_builder_pattern_ergonomics() {
        let content = "# Title\n\n```\ncode\n```\n\nContent";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard);

        // Test that builder pattern works smoothly
        let _lines: Vec<_> = ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .skip_html_blocks()
            .into_iter()
            .collect();

        // If this compiles and runs, the builder pattern is working
    }

    #[test]
    fn test_filtered_line_access_to_line_info() {
        let content = "# Title\n\nContent";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard);

        for line in ctx.content_lines() {
            // Should be able to access line_info fields
            assert!(!line.line_info.in_front_matter);
            assert!(!line.line_info.in_code_block);
        }
    }
}
