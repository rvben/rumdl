use super::regex_cache;
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

/// Utility module for early returns / fast path checks to quickly skip rules
/// when processing markdown content.

/// Check if the content potentially contains URLs
#[inline]
pub fn has_urls(content: &str) -> bool {
    regex_cache::contains_url(content)
}

/// Check if the content potentially contains headings
#[inline]
pub fn has_headings(content: &str) -> bool {
    regex_cache::has_heading_markers(content)
}

/// Check if the content potentially contains unordered list markers
#[inline]
pub fn has_unordered_list_markers(content: &str) -> bool {
    regex_cache::has_list_markers(content)
        && (content.contains('*') || content.contains('-') || content.contains('+'))
}

/// Check if the content potentially contains ordered list markers
#[inline]
pub fn has_ordered_list_markers(content: &str) -> bool {
    regex_cache::has_list_markers(content)
        && content.contains('.')
        && content.contains(|c: char| c.is_ascii_digit())
}

/// Check if the content potentially contains HTML tags
#[inline]
pub fn has_html_tags(content: &str) -> bool {
    regex_cache::has_html_tags(content)
}

/// Check if the content potentially contains emphasis markers
#[inline]
pub fn has_emphasis(content: &str) -> bool {
    regex_cache::has_emphasis_markers(content)
}

/// Check if the content contains specific characters that could be part
/// of patterns checked by various rules
#[inline]
pub fn contains_any_of(content: &str, chars: &[char]) -> bool {
    chars.iter().any(|&c| content.contains(c))
}

/// Fast check if content is essentially empty or trivial
#[inline]
pub fn is_empty_or_trivial(content: &str) -> bool {
    content.is_empty() || content.trim().is_empty()
}
