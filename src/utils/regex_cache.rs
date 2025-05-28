//!
//! Cached Regex Patterns and Fast Content Checks for Markdown Linting
//!
//! This module provides a centralized collection of pre-compiled, cached regex patterns
//! for all major Markdown constructs (headings, lists, code blocks, links, images, etc.).
//! It also includes fast-path utility functions for quickly checking if content
//! potentially contains certain Markdown elements, allowing rules to skip expensive
//! processing when unnecessary.
//!
//! # Performance
//!
//! All regexes are compiled once at startup using `lazy_static`, avoiding repeated
//! compilation and improving performance across the linter. Use these shared patterns
//! in rules instead of compiling new regexes.
//!
//! # Usage
//!
//! - Use the provided statics for common Markdown patterns.
//! - Use the `regex_lazy!` macro for ad-hoc regexes that are not predefined.
//! - Use the utility functions for fast content checks before running regexes.

use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Global regex cache for dynamic patterns
#[derive(Debug)]
pub struct RegexCache {
    cache: HashMap<String, Arc<Regex>>,
    fancy_cache: HashMap<String, Arc<FancyRegex>>,
    usage_stats: HashMap<String, u64>,
}

impl Default for RegexCache {
    fn default() -> Self {
        Self::new()
    }
}

impl RegexCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            fancy_cache: HashMap::new(),
            usage_stats: HashMap::new(),
        }
    }

    /// Get or compile a regex pattern
    pub fn get_regex(&mut self, pattern: &str) -> Result<Arc<Regex>, regex::Error> {
        if let Some(regex) = self.cache.get(pattern) {
            *self.usage_stats.entry(pattern.to_string()).or_insert(0) += 1;
            return Ok(regex.clone());
        }

        let regex = Arc::new(Regex::new(pattern)?);
        self.cache.insert(pattern.to_string(), regex.clone());
        *self.usage_stats.entry(pattern.to_string()).or_insert(0) += 1;
        Ok(regex)
    }

    /// Get or compile a fancy regex pattern
    pub fn get_fancy_regex(
        &mut self,
        pattern: &str,
    ) -> Result<Arc<FancyRegex>, Box<fancy_regex::Error>> {
        if let Some(regex) = self.fancy_cache.get(pattern) {
            *self.usage_stats.entry(pattern.to_string()).or_insert(0) += 1;
            return Ok(regex.clone());
        }

        match FancyRegex::new(pattern) {
            Ok(regex) => {
                let arc_regex = Arc::new(regex);
                self.fancy_cache
                    .insert(pattern.to_string(), arc_regex.clone());
                Ok(arc_regex)
            }
            Err(e) => Err(Box::new(e)),
        }
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> HashMap<String, u64> {
        self.usage_stats.clone()
    }

    /// Clear cache (useful for testing)
    pub fn clear(&mut self) {
        self.cache.clear();
        self.fancy_cache.clear();
        self.usage_stats.clear();
    }
}

lazy_static! {
    /// Global regex cache instance
    static ref GLOBAL_REGEX_CACHE: Arc<Mutex<RegexCache>> = Arc::new(Mutex::new(RegexCache::new()));
}

/// Get a regex from the global cache
pub fn get_cached_regex(pattern: &str) -> Result<Arc<Regex>, regex::Error> {
    let mut cache = GLOBAL_REGEX_CACHE.lock().unwrap();
    cache.get_regex(pattern)
}

/// Get a fancy regex from the global cache
pub fn get_cached_fancy_regex(pattern: &str) -> Result<Arc<FancyRegex>, Box<fancy_regex::Error>> {
    let mut cache = GLOBAL_REGEX_CACHE.lock().unwrap();
    cache.get_fancy_regex(pattern)
}

/// Get cache usage statistics
pub fn get_cache_stats() -> HashMap<String, u64> {
    let cache = GLOBAL_REGEX_CACHE.lock().unwrap();
    cache.get_stats()
}

/// Macro for defining a lazily-initialized, cached regex pattern.
/// Use this for ad-hoc regexes that are not already defined in this module.
/// Example:
/// ```
/// use rumdl::regex_lazy;
/// let my_re = regex_lazy!(r"^foo.*bar$");
/// assert!(my_re.is_match("foobar"));
/// ```
#[macro_export]
macro_rules! regex_lazy {
    ($pattern:expr) => {{
        lazy_static::lazy_static! {
            static ref REGEX: regex::Regex = regex::Regex::new($pattern).unwrap();
        }
        &*REGEX
    }};
}

/// Macro for getting regex from global cache
#[macro_export]
macro_rules! regex_cached {
    ($pattern:expr) => {{
        $crate::utils::regex_cache::get_cached_regex($pattern).expect("Failed to compile regex")
    }};
}

/// Macro for getting fancy regex from global cache
#[macro_export]
macro_rules! fancy_regex_cached {
    ($pattern:expr) => {{
        $crate::utils::regex_cache::get_cached_fancy_regex($pattern)
            .expect("Failed to compile fancy regex")
    }};
}

// Also make the macro available directly from this module
pub use crate::regex_lazy;

lazy_static! {
    // URL patterns
    pub static ref URL_REGEX: Regex = Regex::new(r#"(?:https?|ftp)://[^\s<>\[\]()'"]+[^\s<>\[\]()"'.,]"#).unwrap();
    pub static ref BARE_URL_REGEX: Regex = Regex::new(r"(?:https?|ftp)://[^\s<>]+[^\s<>.]").unwrap();
    pub static ref URL_PATTERN: Regex = Regex::new(r"((?:https?|ftp)://[^\s\)<>]+[^\s\)<>.,])").unwrap();

    // Heading patterns
    pub static ref ATX_HEADING_REGEX: Regex = Regex::new(r"^(\s*)(#{1,6})(\s+|$)").unwrap();
    pub static ref CLOSED_ATX_HEADING_REGEX: Regex = Regex::new(r"^(\s*)(#{1,6})(\s+)(.*)(\s+)(#+)(\s*)$").unwrap();
    pub static ref SETEXT_HEADING_REGEX: Regex = Regex::new(r"^(\s*)[^\s]+.*\n(\s*)(=+|-+)\s*$").unwrap();
    pub static ref TRAILING_PUNCTUATION_REGEX: Regex = Regex::new(r"[.,:;!?]$").unwrap();

    // ATX heading patterns for MD051 and other rules
    pub static ref ATX_HEADING_WITH_CAPTURE: Regex = Regex::new(r"^(#{1,6})\s+(.+?)(?:\s+#*\s*)?$").unwrap();
    pub static ref SETEXT_HEADING_WITH_CAPTURE: FancyRegex = FancyRegex::new(r"^([^\n]+)\n([=\-])\2+\s*$").unwrap();

    // List patterns
    pub static ref UNORDERED_LIST_MARKER_REGEX: Regex = Regex::new(r"^(\s*)([*+-])(\s+)").unwrap();
    pub static ref ORDERED_LIST_MARKER_REGEX: Regex = Regex::new(r"^(\s*)(\d+)([.)])(\s+)").unwrap();
    pub static ref LIST_MARKER_ANY_REGEX: Regex = Regex::new(r"^(\s*)(?:([*+-])|(\d+)[.)])(\s+)").unwrap();

    // Code block patterns
    pub static ref FENCED_CODE_BLOCK_START_REGEX: Regex = Regex::new(r"^(\s*)(```|~~~)(.*)$").unwrap();
    pub static ref FENCED_CODE_BLOCK_END_REGEX: Regex = Regex::new(r"^(\s*)(```|~~~)(\s*)$").unwrap();
    pub static ref INDENTED_CODE_BLOCK_REGEX: Regex = Regex::new(r"^(\s{4,})(.*)$").unwrap();
    pub static ref CODE_FENCE_REGEX: Regex = Regex::new(r"^(`{3,}|~{3,})").unwrap();

    // Emphasis patterns
    pub static ref EMPHASIS_REGEX: FancyRegex = FancyRegex::new(r"(\s|^)(\*{1,2}|_{1,2})(?=\S)(.+?)(?<=\S)(\2)(\s|$)").unwrap();
    pub static ref SPACE_IN_EMPHASIS_REGEX: FancyRegex = FancyRegex::new(r"(\*|_)(\s+)(.+?)(\s+)(\1)").unwrap();

    // MD037 specific emphasis patterns - improved to avoid false positives
    // Only match emphasis with spaces that are actually complete emphasis blocks
    // Use word boundaries and negative lookbehind/lookahead to avoid matching across emphasis boundaries
    pub static ref ASTERISK_EMPHASIS: Regex = Regex::new(r"(?:^|[^*])\*(\s+[^*]+\s*|\s*[^*]+\s+)\*(?:[^*]|$)").unwrap();
    pub static ref UNDERSCORE_EMPHASIS: Regex = Regex::new(r"(?:^|[^_])_(\s+[^_]+\s*|\s*[^_]+\s+)_(?:[^_]|$)").unwrap();
    pub static ref DOUBLE_UNDERSCORE_EMPHASIS: Regex = Regex::new(r"(?:^|[^_])__(\s+[^_]+\s*|\s*[^_]+\s+)__(?:[^_]|$)").unwrap();
    pub static ref DOUBLE_ASTERISK_EMPHASIS: FancyRegex = FancyRegex::new(r"\*\*\s+([^*]+?)\s+\*\*").unwrap();
    pub static ref DOUBLE_ASTERISK_SPACE_START: FancyRegex = FancyRegex::new(r"\*\*\s+([^*]+?)\*\*").unwrap();
    pub static ref DOUBLE_ASTERISK_SPACE_END: FancyRegex = FancyRegex::new(r"\*\*([^*]+?)\s+\*\*").unwrap();

    // Code block patterns
    pub static ref FENCED_CODE_BLOCK_START: Regex = Regex::new(r"^(\s*)```(?:[^`\r\n]*)$").unwrap();
    pub static ref FENCED_CODE_BLOCK_END: Regex = Regex::new(r"^(\s*)```\s*$").unwrap();
    pub static ref ALTERNATE_FENCED_CODE_BLOCK_START: Regex = Regex::new(r"^(\s*)~~~(?:[^~\r\n]*)$").unwrap();
    pub static ref ALTERNATE_FENCED_CODE_BLOCK_END: Regex = Regex::new(r"^(\s*)~~~\s*$").unwrap();
    pub static ref INDENTED_CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(\s{4,})").unwrap();

    // HTML patterns
    pub static ref HTML_TAG_REGEX: Regex = Regex::new(r"<([a-zA-Z][^>]*)>").unwrap();
    pub static ref HTML_SELF_CLOSING_TAG_REGEX: Regex = Regex::new(r"<([a-zA-Z][^>]*/)>").unwrap();
    pub static ref HTML_TAG_FINDER: Regex = Regex::new("(?i)</?[a-zA-Z][^>]*>").unwrap();
    pub static ref HTML_TAG_QUICK_CHECK: Regex = Regex::new("(?i)</?[a-zA-Z]").unwrap();

    // Link patterns for MD051 and other rules
    pub static ref LINK_REFERENCE_DEFINITION_REGEX: Regex = Regex::new(r"^\s*\[([^\]]+)\]:\s+(.+)$").unwrap();
    pub static ref INLINE_LINK_REGEX: Regex = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    pub static ref LINK_TEXT_REGEX: Regex = Regex::new(r"\[([^\]]*)\]").unwrap();
    pub static ref LINK_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)\[([^\]]*)\]\(([^)#]*)#([^)]+)\)").unwrap();
    pub static ref EXTERNAL_URL_REGEX: FancyRegex = FancyRegex::new(r"^(https?://|ftp://|www\.|[^/]+\.[a-z]{2,})").unwrap();

    // Image patterns
    pub static ref IMAGE_REGEX: Regex = Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").unwrap();

    // Whitespace patterns
    pub static ref TRAILING_WHITESPACE_REGEX: Regex = Regex::new(r"\s+$").unwrap();
    pub static ref MULTIPLE_BLANK_LINES_REGEX: Regex = Regex::new(r"\n{3,}").unwrap();

    // Front matter patterns
    pub static ref FRONT_MATTER_REGEX: Regex = Regex::new(r"^---\n.*?\n---\n").unwrap();

    // MD051 specific patterns
    pub static ref INLINE_CODE_REGEX: FancyRegex = FancyRegex::new(r"`[^`]+`").unwrap();
    pub static ref BOLD_ASTERISK_REGEX: Regex = Regex::new(r"\*\*(.+?)\*\*").unwrap();
    pub static ref BOLD_UNDERSCORE_REGEX: Regex = Regex::new(r"__(.+?)__").unwrap();
    pub static ref ITALIC_ASTERISK_REGEX: Regex = Regex::new(r"\*([^*]+?)\*").unwrap();
    pub static ref ITALIC_UNDERSCORE_REGEX: Regex = Regex::new(r"_([^_]+?)_").unwrap();
    pub static ref LINK_TEXT_FULL_REGEX: FancyRegex = FancyRegex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap();
    pub static ref STRIKETHROUGH_REGEX: Regex = Regex::new(r"~~(.+?)~~").unwrap();
    pub static ref MULTIPLE_HYPHENS: Regex = Regex::new(r"-{2,}").unwrap();
    pub static ref TOC_SECTION_START: Regex = Regex::new(r"^#+\s*(?:Table of Contents|Contents|TOC)\s*$").unwrap();

    // Blockquote patterns
    pub static ref BLOCKQUOTE_PREFIX_RE: Regex = Regex::new(r"^(\s*>+\s*)").unwrap();
}

/// Utility functions for quick content checks
/// Check if content contains any headings (quick check before regex)
pub fn has_heading_markers(content: &str) -> bool {
    content.contains('#')
}

/// Check if content contains any lists (quick check before regex)
pub fn has_list_markers(content: &str) -> bool {
    content.contains('*')
        || content.contains('-')
        || content.contains('+')
        || (content.contains('.') && content.contains(|c: char| c.is_ascii_digit()))
}

/// Check if content contains any code blocks (quick check before regex)
pub fn has_code_block_markers(content: &str) -> bool {
    content.contains("```") || content.contains("~~~") || content.contains("\n    ")
    // Indented code block potential
}

/// Check if content contains any emphasis markers (quick check before regex)
pub fn has_emphasis_markers(content: &str) -> bool {
    content.contains('*') || content.contains('_')
}

/// Check if content contains any HTML tags (quick check before regex)
pub fn has_html_tags(content: &str) -> bool {
    content.contains('<') && (content.contains('>') || content.contains("/>"))
}

/// Check if content contains any links (quick check before regex)
pub fn has_link_markers(content: &str) -> bool {
    (content.contains('[') && content.contains(']'))
        || content.contains("http://")
        || content.contains("https://")
        || content.contains("ftp://")
}

/// Check if content contains any images (quick check before regex)
pub fn has_image_markers(content: &str) -> bool {
    content.contains("![")
}

/// Optimize URL detection by implementing a character-by-character scanner
/// that's much faster than regex for cases where we know there's no URL
pub fn contains_url(content: &str) -> bool {
    // Fast check - if these substrings aren't present, there's no URL
    if !content.contains("://") {
        return false;
    }

    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Look for the start of a URL protocol
        if i + 2 < chars.len()
            && ((chars[i] == 'h' && chars[i + 1] == 't' && chars[i + 2] == 't')
                || (chars[i] == 'f' && chars[i + 1] == 't' && chars[i + 2] == 'p'))
        {
            // Scan forward to find "://"
            let mut j = i;
            while j + 2 < chars.len() {
                if chars[j] == ':' && chars[j + 1] == '/' && chars[j + 2] == '/' {
                    return true;
                }
                j += 1;

                // Don't scan too far ahead for the protocol
                if j > i + 10 {
                    break;
                }
            }
        }
        i += 1;
    }

    false
}

/// Escapes a string to be used in a regex pattern
pub fn escape_regex(s: &str) -> String {
    let special_chars = [
        '.', '+', '*', '?', '^', '$', '(', ')', '[', ']', '{', '}', '|', '\\',
    ];
    let mut result = String::with_capacity(s.len() * 2);

    for c in s.chars() {
        if special_chars.contains(&c) {
            result.push('\\');
        }
        result.push(c);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_heading_markers() {
        assert!(has_heading_markers("# Heading"));
        assert!(has_heading_markers("Text with # symbol"));
        assert!(!has_heading_markers("Text without heading marker"));
    }

    #[test]
    fn test_has_list_markers() {
        assert!(has_list_markers("* Item"));
        assert!(has_list_markers("- Item"));
        assert!(has_list_markers("+ Item"));
        assert!(has_list_markers("1. Item"));
        assert!(!has_list_markers("Text without list markers"));
    }

    #[test]
    fn test_has_code_block_markers() {
        assert!(has_code_block_markers("```code```"));
        assert!(has_code_block_markers("~~~code~~~"));
        assert!(has_code_block_markers("Text\n    indented code"));
        assert!(!has_code_block_markers("Text without code blocks"));
    }

    #[test]
    fn test_contains_url() {
        assert!(contains_url("http://example.com"));
        assert!(contains_url("Text with https://example.com link"));
        assert!(contains_url("ftp://example.com"));
        assert!(!contains_url("Text without URL"));
        assert!(!contains_url("http not followed by ://"));
    }

    #[test]
    fn test_escape_regex() {
        assert_eq!(escape_regex("a.b"), "a\\.b");
        assert_eq!(escape_regex("a+b*c"), "a\\+b\\*c");
        assert_eq!(escape_regex("(test)"), "\\(test\\)");
        assert_eq!(escape_regex("[a-z]"), "\\[a-z\\]");
        assert_eq!(escape_regex("normal text"), "normal text");
    }
}
