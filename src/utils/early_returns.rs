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
        || (!content.contains('*')
            && !content.contains('-')
            && !content.contains('+')
            && !content.contains(". "))
}

/// Common early return checks for code block related rules
pub fn should_skip_code_block_rule(content: &str) -> bool {
    content.is_empty()
        || (!content.contains("```") && !content.contains("~~~") && !content.contains("    "))
}

/// Common early return checks for link-related rules
pub fn should_skip_link_rule(content: &str) -> bool {
    content.is_empty()
        || (!content.contains('[') && !content.contains('(') && !content.contains("]:"))
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
    content.contains("http://") || content.contains("https://") || content.contains("ftp://")
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
        if trimmed.len() > 1
            && (trimmed.chars().all(|c| c == '=') || trimmed.chars().all(|c| c == '-'))
        {
            return true;
        }
    }
    false
}

/// Check if content has any list markers
#[inline]
pub fn has_lists(content: &str) -> bool {
    content.contains("* ")
        || content.contains("- ")
        || content.contains("+ ")
        || has_ordered_lists(content)
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
                    || (trimmed.len() > 1
                        && (trimmed.chars().all(|c| c == '=')
                            || trimmed.chars().all(|c| c == '-'))))
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
            if !analysis.has_links
                && line.contains('[')
                && (line.contains("](") || line.contains("]:"))
            {
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
