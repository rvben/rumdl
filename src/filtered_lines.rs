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
//! let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
///     .skip_html_comments()
///     .skip_mkdocstrings()
///     .skip_esm_blocks()
///     .skip_quarto_divs();
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
    /// Skip lines inside mkdocstrings blocks
    pub skip_mkdocstrings: bool,
    /// Skip lines inside ESM (ECMAScript Module) blocks
    pub skip_esm_blocks: bool,
    /// Skip lines inside math blocks ($$ ... $$)
    pub skip_math_blocks: bool,
    /// Skip lines inside Quarto div blocks (::: ... :::)
    pub skip_quarto_divs: bool,
    /// Skip lines containing or inside JSX expressions (MDX: {expression})
    pub skip_jsx_expressions: bool,
    /// Skip lines inside MDX comments ({/* ... */})
    pub skip_mdx_comments: bool,
    /// Skip lines inside MkDocs admonitions (!!! or ???)
    pub skip_admonitions: bool,
    /// Skip lines inside MkDocs content tabs (=== "Tab")
    pub skip_content_tabs: bool,
    /// Skip lines inside definition lists (:  definition)
    pub skip_definition_lists: bool,
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

    /// Skip lines inside mkdocstrings blocks
    ///
    /// Mkdocstrings blocks contain auto-generated documentation and most
    /// markdown rules should not process them.
    #[must_use]
    pub fn skip_mkdocstrings(mut self) -> Self {
        self.skip_mkdocstrings = true;
        self
    }

    /// Skip lines inside ESM (ECMAScript Module) blocks
    ///
    /// ESM blocks contain JavaScript/TypeScript module code and most
    /// markdown rules should not process them.
    #[must_use]
    pub fn skip_esm_blocks(mut self) -> Self {
        self.skip_esm_blocks = true;
        self
    }

    /// Skip lines inside math blocks ($$ ... $$)
    ///
    /// Math blocks contain LaTeX/mathematical notation and markdown rules
    /// should not process them as regular markdown content.
    #[must_use]
    pub fn skip_math_blocks(mut self) -> Self {
        self.skip_math_blocks = true;
        self
    }

    /// Skip lines inside Quarto div blocks (::: ... :::)
    ///
    /// Quarto divs are fenced containers for callouts, panels, and other
    /// structured content. Rules may need to skip them for accurate processing.
    #[must_use]
    pub fn skip_quarto_divs(mut self) -> Self {
        self.skip_quarto_divs = true;
        self
    }

    /// Skip lines containing or inside JSX expressions (MDX: {expression})
    ///
    /// JSX expressions contain JavaScript code and most markdown rules
    /// should not process them as regular markdown content.
    #[must_use]
    pub fn skip_jsx_expressions(mut self) -> Self {
        self.skip_jsx_expressions = true;
        self
    }

    /// Skip lines inside MDX comments ({/* ... */})
    ///
    /// MDX comments are metadata and should not be processed by most
    /// markdown linting rules.
    #[must_use]
    pub fn skip_mdx_comments(mut self) -> Self {
        self.skip_mdx_comments = true;
        self
    }

    /// Skip lines inside MkDocs admonitions (!!! or ???)
    ///
    /// Admonitions are callout blocks and may have special formatting
    /// that rules should not process as regular content.
    #[must_use]
    pub fn skip_admonitions(mut self) -> Self {
        self.skip_admonitions = true;
        self
    }

    /// Skip lines inside MkDocs content tabs (=== "Tab")
    ///
    /// Content tabs contain tabbed content that may need special handling.
    #[must_use]
    pub fn skip_content_tabs(mut self) -> Self {
        self.skip_content_tabs = true;
        self
    }

    /// Skip lines inside definition lists (:  definition)
    ///
    /// Definition lists have special formatting that rules should
    /// not process as regular content.
    #[must_use]
    pub fn skip_definition_lists(mut self) -> Self {
        self.skip_definition_lists = true;
        self
    }

    /// Check if a line should be filtered out based on this configuration
    fn should_filter(&self, line_info: &LineInfo) -> bool {
        (self.skip_front_matter && line_info.in_front_matter)
            || (self.skip_code_blocks && line_info.in_code_block)
            || (self.skip_html_blocks && line_info.in_html_block)
            || (self.skip_html_comments && line_info.in_html_comment)
            || (self.skip_mkdocstrings && line_info.in_mkdocstrings)
            || (self.skip_esm_blocks && line_info.in_esm_block)
            || (self.skip_math_blocks && line_info.in_math_block)
            || (self.skip_quarto_divs && line_info.in_quarto_div)
            || (self.skip_jsx_expressions && line_info.in_jsx_expression)
            || (self.skip_mdx_comments && line_info.in_mdx_comment)
            || (self.skip_admonitions && line_info.in_admonition)
            || (self.skip_content_tabs && line_info.in_content_tab)
            || (self.skip_definition_lists && line_info.in_definition_list)
    }
}

/// Iterator that yields filtered lines based on configuration
pub struct FilteredLinesIter<'a> {
    ctx: &'a LintContext<'a>,
    config: LineFilterConfig,
    current_index: usize,
    content_lines: Vec<&'a str>,
}

impl<'a> FilteredLinesIter<'a> {
    /// Create a new filtered lines iterator
    fn new(ctx: &'a LintContext<'a>, config: LineFilterConfig) -> Self {
        Self {
            ctx,
            config,
            current_index: 0,
            content_lines: ctx.content.lines().collect(),
        }
    }
}

impl<'a> Iterator for FilteredLinesIter<'a> {
    type Item = FilteredLine<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let lines = &self.ctx.lines;

        while self.current_index < lines.len() {
            let idx = self.current_index;
            self.current_index += 1;

            // Check if this line should be filtered
            if self.config.should_filter(&lines[idx]) {
                continue;
            }

            // Get the actual line content from the document
            let line_content = self.content_lines.get(idx).copied().unwrap_or("");

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
    /// let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    /// let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    /// Skip lines inside mkdocstrings blocks
    #[must_use]
    pub fn skip_mkdocstrings(mut self) -> Self {
        self.config = self.config.skip_mkdocstrings();
        self
    }

    /// Skip lines inside ESM (ECMAScript Module) blocks
    #[must_use]
    pub fn skip_esm_blocks(mut self) -> Self {
        self.config = self.config.skip_esm_blocks();
        self
    }

    /// Skip lines inside math blocks ($$ ... $$)
    #[must_use]
    pub fn skip_math_blocks(mut self) -> Self {
        self.config = self.config.skip_math_blocks();
        self
    }

    /// Skip lines inside Quarto div blocks (::: ... :::)
    #[must_use]
    pub fn skip_quarto_divs(mut self) -> Self {
        self.config = self.config.skip_quarto_divs();
        self
    }

    /// Skip lines containing or inside JSX expressions (MDX: {expression})
    #[must_use]
    pub fn skip_jsx_expressions(mut self) -> Self {
        self.config = self.config.skip_jsx_expressions();
        self
    }

    /// Skip lines inside MDX comments ({/* ... */})
    #[must_use]
    pub fn skip_mdx_comments(mut self) -> Self {
        self.config = self.config.skip_mdx_comments();
        self
    }

    /// Skip lines inside MkDocs admonitions (!!! or ???)
    #[must_use]
    pub fn skip_admonitions(mut self) -> Self {
        self.config = self.config.skip_admonitions();
        self
    }

    /// Skip lines inside MkDocs content tabs (=== "Tab")
    #[must_use]
    pub fn skip_content_tabs(mut self) -> Self {
        self.config = self.config.skip_content_tabs();
        self
    }

    /// Skip lines inside definition lists (:  definition)
    #[must_use]
    pub fn skip_definition_lists(mut self) -> Self {
        self.config = self.config.skip_definition_lists();
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
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let line = ctx.content_lines().next().unwrap();
        assert_eq!(line.line_num, 1);
        assert_eq!(line.content, "# Title");
        assert!(!line.line_info.in_front_matter);
    }

    #[test]
    fn test_skip_front_matter_yaml() {
        let content = "---\ntitle: Test\nurl: http://example.com\n---\n\n# Content\n\nMore content";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

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
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let lines: Vec<_> = ctx.content_lines().collect();
        assert_eq!(lines.len(), 2); // Empty line + "# Content"
        assert_eq!(lines[0].line_num, 5);
        assert_eq!(lines[1].line_num, 6);
        assert_eq!(lines[1].content, "# Content");
    }

    #[test]
    fn test_skip_front_matter_json() {
        let content = "{\n\"title\": \"Test\",\n\"url\": \"http://example.com\"\n}\n\n# Content";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let lines: Vec<_> = ctx.content_lines().collect();
        assert_eq!(lines.len(), 2); // Empty line + "# Content"
        assert_eq!(lines[0].line_num, 5);
        assert_eq!(lines[1].line_num, 6);
        assert_eq!(lines[1].content, "# Content");
    }

    #[test]
    fn test_skip_code_blocks() {
        let content = "# Title\n\n```rust\nlet x = 1;\nlet y = 2;\n```\n\nContent";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

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
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // With no filters, all lines should be included
        let lines: Vec<_> = ctx.filtered_lines().into_iter().collect();
        assert_eq!(lines.len(), ctx.lines.len());
    }

    #[test]
    fn test_multiple_filters() {
        let content = "---\ntitle: Test\n---\n\n# Title\n\n```rust\ncode\n```\n\nContent";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

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
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

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
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // content_lines() should automatically skip front matter
        let lines: Vec<_> = ctx.content_lines().collect();
        assert!(!lines.iter().any(|l| l.content.contains("foo")));
        assert!(lines.iter().any(|l| l.content == "Content"));
    }

    #[test]
    fn test_empty_document() {
        let content = "";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let lines: Vec<_> = ctx.content_lines().collect();
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_only_front_matter() {
        let content = "---\ntitle: Test\n---";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

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
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

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
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        for line in ctx.content_lines() {
            // Should be able to access line_info fields
            assert!(!line.line_info.in_front_matter);
            assert!(!line.line_info.in_code_block);
        }
    }

    #[test]
    fn test_skip_mkdocstrings() {
        let content = r#"# API Documentation

::: mymodule.MyClass
    options:
      show_root_heading: true
      show_source: false

Some regular content here.

::: mymodule.function
    options:
      show_signature: true

More content."#;
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let lines: Vec<_> = ctx.filtered_lines().skip_mkdocstrings().into_iter().collect();

        // Verify lines OUTSIDE mkdocstrings blocks are INCLUDED
        assert!(
            lines.iter().any(|l| l.content.contains("# API Documentation")),
            "Should include lines outside mkdocstrings blocks"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("Some regular content")),
            "Should include content between mkdocstrings blocks"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("More content")),
            "Should include content after mkdocstrings blocks"
        );

        // Verify lines INSIDE mkdocstrings blocks are EXCLUDED
        assert!(
            !lines.iter().any(|l| l.content.contains("::: mymodule")),
            "Should exclude mkdocstrings marker lines"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("show_root_heading")),
            "Should exclude mkdocstrings option lines"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("show_signature")),
            "Should exclude all mkdocstrings option lines"
        );

        // Verify line numbers are preserved (1-indexed)
        assert_eq!(lines[0].line_num, 1, "First line should be line 1");
    }

    #[test]
    fn test_skip_esm_blocks() {
        // MDX 2.0+ allows ESM imports/exports anywhere in the document
        let content = r#"import {Chart} from './components.js'
import {Table} from './table.js'
export const year = 2023

# Last year's snowfall

Content about snowfall data.

import {Footer} from './footer.js'

More content."#;
        let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
        let lines: Vec<_> = ctx.filtered_lines().skip_esm_blocks().into_iter().collect();

        // Verify lines OUTSIDE ESM blocks are INCLUDED
        assert!(
            lines.iter().any(|l| l.content.contains("# Last year's snowfall")),
            "Should include markdown headings"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("Content about snowfall")),
            "Should include markdown content"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("More content")),
            "Should include content after ESM blocks"
        );

        // Verify ALL ESM blocks are EXCLUDED (MDX 2.0+ allows imports anywhere)
        assert!(
            !lines.iter().any(|l| l.content.contains("import {Chart}")),
            "Should exclude import statements at top of file"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("import {Table}")),
            "Should exclude all import statements at top of file"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("export const year")),
            "Should exclude export statements at top of file"
        );
        // MDX 2.0+ allows imports anywhere - they should ALL be excluded
        assert!(
            !lines.iter().any(|l| l.content.contains("import {Footer}")),
            "Should exclude import statements even after markdown content (MDX 2.0+ ESM anywhere)"
        );

        // Verify line numbers are preserved
        let heading_line = lines
            .iter()
            .find(|l| l.content.contains("# Last year's snowfall"))
            .unwrap();
        assert_eq!(heading_line.line_num, 5, "Heading should be on line 5");
    }

    #[test]
    fn test_all_filters_combined() {
        let content = r#"---
title: Test
---

# Title

```
code
```

<!-- HTML comment here -->

::: mymodule.Class
    options:
      show_root_heading: true

<div>
HTML block
</div>

Content"#;
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

        let lines: Vec<_> = ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .skip_html_blocks()
            .skip_html_comments()
            .skip_mkdocstrings()
            .into_iter()
            .collect();

        // Verify markdown content is INCLUDED
        assert!(
            lines.iter().any(|l| l.content == "# Title"),
            "Should include markdown headings"
        );
        assert!(
            lines.iter().any(|l| l.content == "Content"),
            "Should include markdown content"
        );

        // Verify all filtered content is EXCLUDED
        assert!(
            !lines.iter().any(|l| l.content == "title: Test"),
            "Should exclude front matter"
        );
        assert!(
            !lines.iter().any(|l| l.content == "code"),
            "Should exclude code block content"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("HTML comment")),
            "Should exclude HTML comments"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("::: mymodule")),
            "Should exclude mkdocstrings blocks"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("show_root_heading")),
            "Should exclude mkdocstrings options"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("HTML block")),
            "Should exclude HTML blocks"
        );
    }

    #[test]
    fn test_skip_math_blocks() {
        let content = r#"# Heading

Some regular text.

$$
A = \left[
\begin{array}{c}
1 \\
-D
\end{array}
\right]
$$

More content after math."#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let lines: Vec<_> = ctx.filtered_lines().skip_math_blocks().into_iter().collect();

        // Verify lines OUTSIDE math blocks are INCLUDED
        assert!(
            lines.iter().any(|l| l.content.contains("# Heading")),
            "Should include markdown headings"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("Some regular text")),
            "Should include regular text before math block"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("More content after math")),
            "Should include content after math block"
        );

        // Verify lines INSIDE math blocks are EXCLUDED
        assert!(
            !lines.iter().any(|l| l.content == "$$"),
            "Should exclude math block delimiters"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("\\left[")),
            "Should exclude LaTeX content inside math block"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("-D")),
            "Should exclude content that looks like list items inside math block"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("\\begin{array}")),
            "Should exclude LaTeX array content"
        );
    }

    #[test]
    fn test_math_blocks_not_confused_with_code_blocks() {
        let content = r#"# Title

```python
# This $$ is inside a code block
x = 1
```

$$
y = 2
$$

Regular text."#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Check that the $$ inside code block doesn't start a math block
        let lines: Vec<_> = ctx.filtered_lines().skip_math_blocks().into_iter().collect();

        // The $$ inside the code block should NOT trigger math block detection
        // So when we skip math blocks, the code block content is still there (until we also skip code blocks)
        assert!(
            lines.iter().any(|l| l.content.contains("# This $$")),
            "Code block content with $$ should not be detected as math block"
        );

        // But the real math block content should be excluded
        assert!(
            !lines.iter().any(|l| l.content == "y = 2"),
            "Actual math block content should be excluded"
        );
    }

    #[test]
    fn test_skip_quarto_divs() {
        let content = r#"# Heading

::: {.callout-note}
This is a callout note.
With multiple lines.
:::

Regular text outside.

::: {.bordered}
Content inside bordered div.
:::

More content."#;
        let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);
        let lines: Vec<_> = ctx.filtered_lines().skip_quarto_divs().into_iter().collect();

        // Verify lines OUTSIDE Quarto divs are INCLUDED
        assert!(
            lines.iter().any(|l| l.content.contains("# Heading")),
            "Should include markdown headings"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("Regular text outside")),
            "Should include content between divs"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("More content")),
            "Should include content after divs"
        );

        // Verify lines INSIDE Quarto divs are EXCLUDED
        assert!(
            !lines.iter().any(|l| l.content.contains("::: {.callout-note}")),
            "Should exclude callout div markers"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("This is a callout note")),
            "Should exclude callout content"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("Content inside bordered")),
            "Should exclude bordered div content"
        );
    }

    #[test]
    fn test_skip_jsx_expressions() {
        let content = r#"# MDX Document

Here is some content with {myVariable} inline.

{items.map(item => (
  <Item key={item.id} />
))}

Regular paragraph after expression.

{/* This should NOT be skipped by jsx_expressions filter */}
{/* MDX comments have their own filter */}

More content."#;
        let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
        let lines: Vec<_> = ctx.filtered_lines().skip_jsx_expressions().into_iter().collect();

        // Verify lines OUTSIDE JSX expressions are INCLUDED
        assert!(
            lines.iter().any(|l| l.content.contains("# MDX Document")),
            "Should include markdown headings"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("Regular paragraph")),
            "Should include regular paragraphs"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("More content")),
            "Should include content after expressions"
        );

        // Verify lines with JSX expressions are EXCLUDED
        assert!(
            !lines.iter().any(|l| l.content.contains("{myVariable}")),
            "Should exclude lines with inline JSX expressions"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("items.map")),
            "Should exclude multi-line JSX expression content"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("<Item key")),
            "Should exclude JSX inside expressions"
        );
    }

    #[test]
    fn test_skip_quarto_divs_nested() {
        let content = r#"# Title

::: {.outer}
Outer content.

::: {.inner}
Inner content.
:::

Back to outer.
:::

Outside text."#;
        let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);
        let lines: Vec<_> = ctx.filtered_lines().skip_quarto_divs().into_iter().collect();

        // Should include content outside all divs
        assert!(
            lines.iter().any(|l| l.content.contains("# Title")),
            "Should include heading"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("Outside text")),
            "Should include text after divs"
        );

        // Should exclude all div content
        assert!(
            !lines.iter().any(|l| l.content.contains("Outer content")),
            "Should exclude outer div content"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("Inner content")),
            "Should exclude inner div content"
        );
    }

    #[test]
    fn test_skip_quarto_divs_not_in_standard_flavor() {
        let content = r#"::: {.callout-note}
This should NOT be skipped in standard flavor.
:::"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let lines: Vec<_> = ctx.filtered_lines().skip_quarto_divs().into_iter().collect();

        // In standard flavor, Quarto divs are not detected, so nothing is skipped
        assert!(
            lines.iter().any(|l| l.content.contains("This should NOT be skipped")),
            "Standard flavor should not detect Quarto divs"
        );
    }

    #[test]
    fn test_skip_mdx_comments() {
        let content = r#"# MDX Document

{/* This is an MDX comment */}

Regular content here.

{/*
  Multi-line
  MDX comment
*/}

More content after comment."#;
        let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
        let lines: Vec<_> = ctx.filtered_lines().skip_mdx_comments().into_iter().collect();

        // Verify lines OUTSIDE MDX comments are INCLUDED
        assert!(
            lines.iter().any(|l| l.content.contains("# MDX Document")),
            "Should include markdown headings"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("Regular content")),
            "Should include regular content"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("More content")),
            "Should include content after comments"
        );

        // Verify lines with MDX comments are EXCLUDED
        assert!(
            !lines.iter().any(|l| l.content.contains("{/* This is")),
            "Should exclude single-line MDX comments"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("Multi-line")),
            "Should exclude multi-line MDX comment content"
        );
    }

    #[test]
    fn test_jsx_expressions_with_nested_braces() {
        // Test that nested braces are handled correctly
        let content = r#"# Document

{props.style || {color: "red", background: "blue"}}

Regular content."#;
        let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
        let lines: Vec<_> = ctx.filtered_lines().skip_jsx_expressions().into_iter().collect();

        // Verify nested braces don't break detection
        assert!(
            !lines.iter().any(|l| l.content.contains("props.style")),
            "Should exclude JSX expression with nested braces"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("Regular content")),
            "Should include content after nested expression"
        );
    }

    #[test]
    fn test_jsx_and_mdx_comments_combined() {
        // Test both filters together
        let content = r#"# Title

{variable}

{/* comment */}

Content."#;
        let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
        let lines: Vec<_> = ctx
            .filtered_lines()
            .skip_jsx_expressions()
            .skip_mdx_comments()
            .into_iter()
            .collect();

        assert!(
            lines.iter().any(|l| l.content.contains("# Title")),
            "Should include heading"
        );
        assert!(
            lines.iter().any(|l| l.content.contains("Content")),
            "Should include regular content"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("{variable}")),
            "Should exclude JSX expression"
        );
        assert!(
            !lines.iter().any(|l| l.content.contains("{/* comment */")),
            "Should exclude MDX comment"
        );
    }

    #[test]
    fn test_jsx_expressions_not_detected_in_standard_flavor() {
        // JSX expressions should only be detected in MDX flavor
        let content = r#"# Document

{this is not JSX in standard markdown}

Content."#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let lines: Vec<_> = ctx.filtered_lines().skip_jsx_expressions().into_iter().collect();

        // In standard markdown, braces are just text - nothing should be filtered
        assert!(
            lines.iter().any(|l| l.content.contains("{this is not JSX")),
            "Should NOT exclude brace content in standard markdown"
        );
    }
}
