//!
//! Fast-path checks and early return utilities for rule implementations in rumdl.
//! Provides helpers to quickly skip rules based on content analysis.

use crate::rule::LintResult;

/// Trait for implementing early returns in rules
pub trait EarlyReturns {
    /// Check if this rule can be skipped based on content analysis
    fn can_skip(&self, content: &str) -> bool;

    /// Returns the empty result if the rule can be skipped
    fn early_return_if_skippable(&self, content: &str) -> Option<LintResult> {
        if self.can_skip(content) {
            Some(Ok(Vec::new()))
        } else {
            None
        }
    }
}

/// Common early return checks for heading-related rules
pub fn should_skip_heading_rule(content: &str) -> bool {
    content.is_empty() || !content.contains('#')
}

/// Common early return checks for list-related rules
pub fn should_skip_list_rule(content: &str) -> bool {
    content.is_empty()
        || (!content.contains('*') && !content.contains('-') && !content.contains('+') && !content.contains(". "))
}

/// Common early return checks for code block related rules
pub fn should_skip_code_block_rule(content: &str) -> bool {
    content.is_empty() || (!content.contains("```") && !content.contains("~~~") && !content.contains("    "))
}

/// Common early return checks for link-related rules
pub fn should_skip_link_rule(content: &str) -> bool {
    content.is_empty() || (!content.contains('[') && !content.contains('(') && !content.contains("]:"))
}

/// Common early return checks for inline HTML rules
pub fn should_skip_html_rule(content: &str) -> bool {
    content.is_empty() || (!content.contains('<') || !content.contains('>'))
}

/// Common early return checks for emphasis-related rules
pub fn should_skip_emphasis_rule(content: &str) -> bool {
    content.is_empty() || (!content.contains('*') && !content.contains('_'))
}

/// Common early return checks for image-related rules
pub fn should_skip_image_rule(content: &str) -> bool {
    content.is_empty() || !content.contains("![")
}

/// Common early return checks for whitespace-related rules
pub fn should_skip_whitespace_rule(content: &str) -> bool {
    content.is_empty()
}

/// Common early return checks for blockquote-related rules
pub fn should_skip_blockquote_rule(content: &str) -> bool {
    content.is_empty() || !content.contains('>')
}

/// Early return utilities for performance optimization
/// These functions provide fast content analysis to skip expensive processing
/// Check if content has any URLs (http, https, ftp)
#[inline]
pub fn has_urls(content: &str) -> bool {
    // Check for common URL protocols
    if content.contains("http://") || content.contains("https://") || content.contains("ftp://") {
        return true;
    }

    // Also check for URLs with Unicode/internationalized domains using a more permissive check
    // Look for protocol followed by any non-whitespace characters
    for line in content.lines() {
        if let Some(idx) = line.find("://") {
            // Check if there's a valid protocol before ://
            let prefix = &line[..idx];
            if prefix.ends_with("http") || prefix.ends_with("https") || prefix.ends_with("ftp") {
                return true;
            }
        }
    }

    false
}

/// Check if content has any headings (ATX or Setext)
#[inline]
pub fn has_headings(content: &str) -> bool {
    content.contains('#') || has_setext_headings(content)
}

/// Check if content has Setext headings (underlines)
#[inline]
pub fn has_setext_headings(content: &str) -> bool {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.len() > 1 && (trimmed.chars().all(|c| c == '=') || trimmed.chars().all(|c| c == '-')) {
            return true;
        }
    }
    false
}

/// Check if content has any list markers
#[inline]
pub fn has_lists(content: &str) -> bool {
    content.contains("* ") || content.contains("- ") || content.contains("+ ") || has_ordered_lists(content)
}

/// Check if content has ordered lists
#[inline]
pub fn has_ordered_lists(content: &str) -> bool {
    for line in content.lines() {
        let trimmed = line.trim_start();
        if let Some(first_char) = trimmed.chars().next() {
            if first_char.is_ascii_digit() && trimmed.contains(". ") {
                return true;
            }
        }
    }
    false
}

/// Check if content has any links or images
#[inline]
pub fn has_links_or_images(content: &str) -> bool {
    content.contains('[') && (content.contains("](") || content.contains("]:"))
}

/// Check if content has any code blocks or inline code
#[inline]
pub fn has_code(content: &str) -> bool {
    content.contains('`') || content.contains("~~~")
}

/// Check if content has any emphasis markers
#[inline]
pub fn has_emphasis(content: &str) -> bool {
    content.contains('*') || content.contains('_')
}

/// Check if content has any HTML tags
#[inline]
pub fn has_html(content: &str) -> bool {
    content.contains('<') && content.contains('>')
}

/// Check if content has any blockquotes
#[inline]
pub fn has_blockquotes(content: &str) -> bool {
    for line in content.lines() {
        if line.trim_start().starts_with('>') {
            return true;
        }
    }
    false
}

/// Check if content has any tables
#[inline]
pub fn has_tables(content: &str) -> bool {
    content.contains('|')
}

/// Check if content has trailing spaces
#[inline]
pub fn has_trailing_spaces(content: &str) -> bool {
    for line in content.lines() {
        if line.ends_with(' ') || line.ends_with('\t') {
            return true;
        }
    }
    false
}

/// Check if content has hard tabs
#[inline]
pub fn has_hard_tabs(content: &str) -> bool {
    content.contains('\t')
}

/// Check if content has long lines (over threshold)
#[inline]
pub fn has_long_lines(content: &str, threshold: usize) -> bool {
    for line in content.lines() {
        if line.len() > threshold {
            return true;
        }
    }
    false
}

/// Comprehensive content analysis for rule filtering
#[derive(Debug, Default)]
pub struct ContentAnalysis {
    pub has_headings: bool,
    pub has_lists: bool,
    pub has_links: bool,
    pub has_code: bool,
    pub has_emphasis: bool,
    pub has_html: bool,
    pub has_blockquotes: bool,
    pub has_tables: bool,
    pub has_trailing_spaces: bool,
    pub has_hard_tabs: bool,
    pub has_long_lines: bool,
    pub line_count: usize,
    pub char_count: usize,
}

impl ContentAnalysis {
    /// Perform comprehensive analysis of content
    pub fn analyze(content: &str, line_length_threshold: usize) -> Self {
        let mut analysis = Self {
            line_count: content.lines().count(),
            char_count: content.len(),
            ..Default::default()
        };

        // Single pass analysis for maximum efficiency
        for line in content.lines() {
            let trimmed = line.trim();
            let trimmed_start = line.trim_start();

            // Headings
            if !analysis.has_headings
                && (trimmed.starts_with('#')
                    || (trimmed.len() > 1 && (trimmed.chars().all(|c| c == '=') || trimmed.chars().all(|c| c == '-'))))
            {
                analysis.has_headings = true;
            }

            // Lists
            if !analysis.has_lists {
                if line.contains("* ") || line.contains("- ") || line.contains("+ ") {
                    analysis.has_lists = true;
                } else if let Some(first_char) = trimmed_start.chars().next() {
                    if first_char.is_ascii_digit() && line.contains(". ") {
                        analysis.has_lists = true;
                    }
                }
            }

            // Links and images
            if !analysis.has_links && line.contains('[') && (line.contains("](") || line.contains("]:")) {
                analysis.has_links = true;
            }

            // Code
            if !analysis.has_code && (line.contains('`') || line.contains("~~~")) {
                analysis.has_code = true;
            }

            // Emphasis
            if !analysis.has_emphasis && (line.contains('*') || line.contains('_')) {
                analysis.has_emphasis = true;
            }

            // HTML
            if !analysis.has_html && line.contains('<') && line.contains('>') {
                analysis.has_html = true;
            }

            // Blockquotes
            if !analysis.has_blockquotes && trimmed_start.starts_with('>') {
                analysis.has_blockquotes = true;
            }

            // Tables
            if !analysis.has_tables && line.contains('|') {
                analysis.has_tables = true;
            }

            // Whitespace issues
            if !analysis.has_trailing_spaces && (line.ends_with(' ') || line.ends_with('\t')) {
                analysis.has_trailing_spaces = true;
            }

            if !analysis.has_hard_tabs && line.contains('\t') {
                analysis.has_hard_tabs = true;
            }

            // Line length
            if !analysis.has_long_lines && line.len() > line_length_threshold {
                analysis.has_long_lines = true;
            }
        }

        analysis
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_skip_heading_rule() {
        // Should skip empty content
        assert!(should_skip_heading_rule(""));

        // Should skip content without headings
        assert!(should_skip_heading_rule("Just plain text"));
        assert!(should_skip_heading_rule("Some text\nMore text"));

        // Should NOT skip content with headings
        assert!(!should_skip_heading_rule("# Heading"));
        assert!(!should_skip_heading_rule("Text before\n## Heading 2"));
        assert!(!should_skip_heading_rule("###Heading without space"));
    }

    #[test]
    fn test_should_skip_list_rule() {
        // Should skip empty content
        assert!(should_skip_list_rule(""));

        // Should skip content without lists
        assert!(should_skip_list_rule("Just plain text"));
        assert!(should_skip_list_rule("# Heading\nParagraph"));

        // Should NOT skip content with unordered lists
        assert!(!should_skip_list_rule("* Item"));
        assert!(!should_skip_list_rule("- Item"));
        assert!(!should_skip_list_rule("+ Item"));

        // Should NOT skip content with ordered lists
        assert!(!should_skip_list_rule("1. Item"));
        assert!(!should_skip_list_rule("99. Item"));
    }

    #[test]
    fn test_should_skip_code_block_rule() {
        // Should skip empty content
        assert!(should_skip_code_block_rule(""));

        // Should skip content without code blocks
        assert!(should_skip_code_block_rule("Just plain text"));
        assert!(should_skip_code_block_rule("# Heading"));

        // Should NOT skip content with fenced code blocks
        assert!(!should_skip_code_block_rule("```rust\ncode\n```"));
        assert!(!should_skip_code_block_rule("~~~\ncode\n~~~"));

        // Should NOT skip content with indented code blocks
        assert!(!should_skip_code_block_rule("    indented code"));
    }

    #[test]
    fn test_should_skip_link_rule() {
        // Should skip empty content
        assert!(should_skip_link_rule(""));

        // Should skip content without links
        assert!(should_skip_link_rule("Just plain text"));

        // Should NOT skip content with links
        assert!(!should_skip_link_rule("[link](url)"));
        assert!(!should_skip_link_rule("[ref]: url"));
        assert!(!should_skip_link_rule("Text with [link]"));
        assert!(!should_skip_link_rule("Text with (parentheses)"));
    }

    #[test]
    fn test_should_skip_html_rule() {
        // Should skip empty content
        assert!(should_skip_html_rule(""));

        // Should skip content without HTML
        assert!(should_skip_html_rule("Just plain text"));

        // Should skip content with only < or >
        assert!(should_skip_html_rule("a < b"));
        assert!(should_skip_html_rule("a > b"));

        // Should NOT skip content with HTML tags
        assert!(!should_skip_html_rule("<div>content</div>"));
        assert!(!should_skip_html_rule("Text with <span>tag</span>"));
    }

    #[test]
    fn test_should_skip_emphasis_rule() {
        // Should skip empty content
        assert!(should_skip_emphasis_rule(""));

        // Should skip content without emphasis
        assert!(should_skip_emphasis_rule("Just plain text"));

        // Should NOT skip content with emphasis markers
        assert!(!should_skip_emphasis_rule("*emphasis*"));
        assert!(!should_skip_emphasis_rule("_emphasis_"));
        assert!(!should_skip_emphasis_rule("Text with * marker"));
    }

    #[test]
    fn test_should_skip_image_rule() {
        // Should skip empty content
        assert!(should_skip_image_rule(""));

        // Should skip content without images
        assert!(should_skip_image_rule("Just plain text"));
        assert!(should_skip_image_rule("[link](url)"));

        // Should NOT skip content with images
        assert!(!should_skip_image_rule("![alt](image.png)"));
        assert!(!should_skip_image_rule("Text with ![image]"));
    }

    #[test]
    fn test_should_skip_blockquote_rule() {
        // Should skip empty content
        assert!(should_skip_blockquote_rule(""));

        // Should skip content without blockquotes
        assert!(should_skip_blockquote_rule("Just plain text"));

        // Should NOT skip content with blockquotes
        assert!(!should_skip_blockquote_rule("> Quote"));
        assert!(!should_skip_blockquote_rule("Text\n> Quote"));
    }

    #[test]
    fn test_has_urls() {
        assert!(!has_urls(""));
        assert!(!has_urls("Just plain text"));

        assert!(has_urls("http://example.com"));
        assert!(has_urls("https://example.com"));
        assert!(has_urls("ftp://example.com"));
        assert!(has_urls("Text with https://link.com in it"));

        // Unicode/internationalized URLs
        assert!(has_urls("https://例え.jp"));
        assert!(has_urls("http://münchen.de"));
        assert!(has_urls("https://🌐.ws"));
        assert!(has_urls("Visit https://español.example.com for more"));
    }

    #[test]
    fn test_has_headings() {
        assert!(!has_headings(""));
        assert!(!has_headings("Just plain text"));

        // ATX headings
        assert!(has_headings("# Heading"));
        assert!(has_headings("## Heading 2"));

        // Setext headings
        assert!(has_headings("Heading\n======"));
        assert!(has_headings("Heading\n------"));
    }

    #[test]
    fn test_has_setext_headings() {
        assert!(!has_setext_headings(""));
        assert!(!has_setext_headings("Just plain text"));
        assert!(!has_setext_headings("# ATX heading"));

        // Valid setext headings
        assert!(has_setext_headings("Heading\n======"));
        assert!(has_setext_headings("Heading\n------"));
        assert!(has_setext_headings("Heading\n==="));
        assert!(has_setext_headings("Heading\n---"));

        // Not setext headings
        assert!(!has_setext_headings("="));
        assert!(!has_setext_headings("-"));
        assert!(!has_setext_headings("a = b"));
    }

    #[test]
    fn test_has_lists() {
        assert!(!has_lists(""));
        assert!(!has_lists("Just plain text"));

        // Unordered lists
        assert!(has_lists("* Item"));
        assert!(has_lists("- Item"));
        assert!(has_lists("+ Item"));

        // Ordered lists
        assert!(has_lists("1. Item"));
        assert!(has_lists("99. Item"));

        // Not lists - these don't have the required space after marker
        assert!(!has_lists("*emphasis*"));
        // This actually has "- " so it's detected as a list
        // assert!(!has_lists("a - b"));
        assert!(!has_lists("a-b"));
    }

    #[test]
    fn test_has_ordered_lists() {
        assert!(!has_ordered_lists(""));
        assert!(!has_ordered_lists("Just plain text"));
        assert!(!has_ordered_lists("* Unordered"));

        // Valid ordered lists
        assert!(has_ordered_lists("1. Item"));
        assert!(has_ordered_lists("99. Item"));
        assert!(has_ordered_lists("  2. Indented"));

        // Not ordered lists - no space after period
        assert!(!has_ordered_lists("1.Item"));
        // Check for something that doesn't start with a digit
        assert!(!has_ordered_lists("a. Item"));
    }

    #[test]
    fn test_has_links_or_images() {
        assert!(!has_links_or_images(""));
        assert!(!has_links_or_images("Just plain text"));

        // Links
        assert!(has_links_or_images("[link](url)"));
        assert!(has_links_or_images("[ref]: url"));

        // Images
        assert!(has_links_or_images("![alt](img)"));

        // Just brackets not enough
        assert!(!has_links_or_images("[text]"));
        assert!(!has_links_or_images("array[index]"));
    }

    #[test]
    fn test_has_code() {
        assert!(!has_code(""));
        assert!(!has_code("Just plain text"));

        // Inline code
        assert!(has_code("`code`"));
        assert!(has_code("Text with `code` inline"));

        // Fenced code blocks
        assert!(has_code("```rust\ncode\n```"));
        assert!(has_code("~~~\ncode\n~~~"));
    }

    #[test]
    fn test_has_emphasis() {
        assert!(!has_emphasis(""));
        assert!(!has_emphasis("Just plain text"));

        assert!(has_emphasis("*emphasis*"));
        assert!(has_emphasis("_emphasis_"));
        assert!(has_emphasis("**bold**"));
        assert!(has_emphasis("__bold__"));
    }

    #[test]
    fn test_has_html() {
        assert!(!has_html(""));
        assert!(!has_html("Just plain text"));
        assert!(!has_html("a < b"));
        assert!(!has_html("a > b"));

        assert!(has_html("<div>"));
        assert!(has_html("</div>"));
        assert!(has_html("<br/>"));
        assert!(has_html("<span>text</span>"));
    }

    #[test]
    fn test_has_blockquotes() {
        assert!(!has_blockquotes(""));
        assert!(!has_blockquotes("Just plain text"));
        assert!(!has_blockquotes("a > b"));

        assert!(has_blockquotes("> Quote"));
        assert!(has_blockquotes("  > Indented quote"));
        assert!(has_blockquotes("Text\n> Quote"));
    }

    #[test]
    fn test_has_tables() {
        assert!(!has_tables(""));
        assert!(!has_tables("Just plain text"));

        assert!(has_tables("| Header |"));
        assert!(has_tables("a | b | c"));
        assert!(has_tables("Text with | pipe"));
    }

    #[test]
    fn test_has_trailing_spaces() {
        assert!(!has_trailing_spaces(""));
        assert!(!has_trailing_spaces("Clean text"));
        assert!(!has_trailing_spaces("Line 1\nLine 2"));

        assert!(has_trailing_spaces("Trailing space "));
        assert!(has_trailing_spaces("Trailing tab\t"));
        assert!(has_trailing_spaces("Line 1\nLine with space \nLine 3"));
    }

    #[test]
    fn test_has_hard_tabs() {
        assert!(!has_hard_tabs(""));
        assert!(!has_hard_tabs("No tabs here"));
        assert!(!has_hard_tabs("    Four spaces"));

        assert!(has_hard_tabs("\tTab at start"));
        assert!(has_hard_tabs("Tab\tin middle"));
        assert!(has_hard_tabs("Tab at end\t"));
    }

    #[test]
    fn test_has_long_lines() {
        assert!(!has_long_lines("", 80));
        assert!(!has_long_lines("Short line", 80));
        assert!(!has_long_lines("Line 1\nLine 2", 80));

        let long_line = "a".repeat(100);
        assert!(has_long_lines(&long_line, 80));
        assert!(!has_long_lines(&long_line, 100));
        assert!(!has_long_lines(&long_line, 101));
    }

    #[test]
    fn test_early_returns_trait() {
        struct TestRule;

        impl EarlyReturns for TestRule {
            fn can_skip(&self, content: &str) -> bool {
                content.is_empty()
            }
        }

        let rule = TestRule;

        // Should return early for empty content
        let result = rule.early_return_if_skippable("");
        assert!(result.is_some());
        assert!(result.unwrap().unwrap().is_empty());

        // Should not return early for non-empty content
        let result = rule.early_return_if_skippable("content");
        assert!(result.is_none());
    }

    #[test]
    fn test_content_analysis() {
        let analysis = ContentAnalysis::default();
        assert!(!analysis.has_headings);
        assert!(!analysis.has_lists);
        assert_eq!(analysis.line_count, 0);
        assert_eq!(analysis.char_count, 0);
    }

    #[test]
    fn test_unicode_handling() {
        // Test with unicode content
        assert!(!should_skip_heading_rule("# 你好"));
        assert!(!should_skip_emphasis_rule("*émphasis*"));
        // has_urls now supports Unicode domains
        assert!(has_urls("https://example.com"));
        assert!(has_urls("https://例え.jp"));

        // Test with emoji
        assert!(!should_skip_list_rule("* 🎉 Item"));
        assert!(has_emphasis("Text with 🌟 *emphasis*"));
    }

    #[test]
    fn test_edge_cases() {
        // Empty lines
        assert!(!has_headings("\n\n\n"));
        assert!(!has_lists("\n\n\n"));

        // Whitespace only
        assert!(!has_blockquotes("   \n   \n"));
        assert!(!has_code("   \n   \n"));

        // Mixed content
        let mixed = "# Heading\n* List\n> Quote\n`code`\n[link](url)";
        assert!(!should_skip_heading_rule(mixed));
        assert!(!should_skip_list_rule(mixed));
        assert!(!should_skip_blockquote_rule(mixed));
        // should_skip_code_block_rule checks for code blocks, not inline code
        // The mixed content only has inline code, so it would skip
        assert!(should_skip_code_block_rule(mixed));
        assert!(!should_skip_link_rule(mixed));
    }
}
