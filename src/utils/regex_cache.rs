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
    pub fn get_fancy_regex(&mut self, pattern: &str) -> Result<Arc<FancyRegex>, Box<fancy_regex::Error>> {
        if let Some(regex) = self.fancy_cache.get(pattern) {
            *self.usage_stats.entry(pattern.to_string()).or_insert(0) += 1;
            return Ok(regex.clone());
        }

        match FancyRegex::new(pattern) {
            Ok(regex) => {
                let arc_regex = Arc::new(regex);
                self.fancy_cache.insert(pattern.to_string(), arc_regex.clone());
                *self.usage_stats.entry(pattern.to_string()).or_insert(0) += 1;
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
/// use rumdl_lib::regex_lazy;
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
    ($pattern:expr) => {{ $crate::utils::regex_cache::get_cached_regex($pattern).expect("Failed to compile regex") }};
}

/// Macro for getting fancy regex from global cache
#[macro_export]
macro_rules! fancy_regex_cached {
    ($pattern:expr) => {{ $crate::utils::regex_cache::get_cached_fancy_regex($pattern).expect("Failed to compile fancy regex") }};
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

    // MD013 specific patterns
    pub static ref IMAGE_REF_PATTERN: Regex = Regex::new(r"^!\[.*?\]\[.*?\]$").unwrap();
    pub static ref LINK_REF_PATTERN: Regex = Regex::new(r"^\[.*?\]:\s*https?://\S+$").unwrap();
    pub static ref URL_IN_TEXT: Regex = Regex::new(r"https?://\S+").unwrap();
    pub static ref SENTENCE_END: Regex = Regex::new(r"[.!?]\s+[A-Z]").unwrap();
    pub static ref ABBREVIATION: Regex = Regex::new(r"\b(?:Mr|Mrs|Ms|Dr|Prof|Sr|Jr|vs|etc|i\.e|e\.g|Inc|Corp|Ltd|Co|St|Ave|Blvd|Rd|Ph\.D|M\.D|B\.A|M\.A|Ph\.D|U\.S|U\.K|U\.N|N\.Y|L\.A|D\.C)\.\s+[A-Z]").unwrap();
    pub static ref DECIMAL_NUMBER: Regex = Regex::new(r"\d+\.\s*\d+").unwrap();
    pub static ref LIST_ITEM: Regex = Regex::new(r"^\s*\d+\.\s+").unwrap();
    pub static ref REFERENCE_LINK: Regex = Regex::new(r"\[([^\]]*)\]\[([^\]]*)\]").unwrap();

    // Email pattern
    pub static ref EMAIL_PATTERN: Regex = Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
}

// Third lazy_static block for link and image patterns used by MD052 and text_reflow
lazy_static! {
    // Reference link patterns (shared by MD052 and text_reflow)
    // Pattern to match reference links: [text][reference] or [text][]
    pub static ref REF_LINK_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)\[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\]\[([^\]]*)\]").unwrap();

    // Pattern for shortcut reference links: [reference]
    // Must not be preceded by ] or ) (to avoid matching second part of [text][ref])
    // Must not be followed by [ or ( (to avoid matching first part of [text][ref] or [text](url))
    pub static ref SHORTCUT_REF_REGEX: FancyRegex = FancyRegex::new(r"(?<![\\)\]])\[([^\]]+)\](?!\s*[\[\(])").unwrap();

    // Inline link with fancy regex for better escaping handling (used by text_reflow)
    pub static ref INLINE_LINK_FANCY_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)\[([^\]]+)\]\(([^)]+)\)").unwrap();

    // Inline image with fancy regex (used by MD052 and text_reflow)
    pub static ref INLINE_IMAGE_FANCY_REGEX: FancyRegex = FancyRegex::new(r"!\[([^\]]*)\]\(([^)]+)\)").unwrap();

    // Reference image: ![alt][ref] or ![alt][]
    pub static ref REF_IMAGE_REGEX: FancyRegex = FancyRegex::new(r"!\[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\]\[([^\]]*)\]").unwrap();

    // Footnote reference: [^note]
    pub static ref FOOTNOTE_REF_REGEX: FancyRegex = FancyRegex::new(r"\[\^([^\]]+)\]").unwrap();

    // Strikethrough with fancy regex: ~~text~~
    pub static ref STRIKETHROUGH_FANCY_REGEX: FancyRegex = FancyRegex::new(r"~~([^~]+)~~").unwrap();

    // Wiki-style links: [[wiki]] or [[wiki|display text]]
    pub static ref WIKI_LINK_REGEX: FancyRegex = FancyRegex::new(r"\[\[([^\]]+)\]\]").unwrap();

    // Math formulas: $inline$ or $$display$$
    pub static ref INLINE_MATH_REGEX: FancyRegex = FancyRegex::new(r"(?<!\$)\$(?!\$)([^\$]+)\$(?!\$)").unwrap();
    pub static ref DISPLAY_MATH_REGEX: FancyRegex = FancyRegex::new(r"\$\$([^\$]+)\$\$").unwrap();

    // Emoji shortcodes: :emoji:
    pub static ref EMOJI_SHORTCODE_REGEX: FancyRegex = FancyRegex::new(r":([a-zA-Z0-9_+-]+):").unwrap();

    // HTML tags (opening, closing, self-closing)
    pub static ref HTML_TAG_PATTERN: FancyRegex = FancyRegex::new(r"</?[a-zA-Z][^>]*>|<[a-zA-Z][^>]*/\s*>").unwrap();

    // HTML entities: &nbsp; &mdash; etc
    pub static ref HTML_ENTITY_REGEX: FancyRegex = FancyRegex::new(r"&[a-zA-Z][a-zA-Z0-9]*;|&#\d+;|&#x[0-9a-fA-F]+;").unwrap();
}

// Fourth lazy_static block for additional patterns
lazy_static! {
    // HTML comment patterns
    pub static ref HTML_COMMENT_START: Regex = Regex::new(r"<!--").unwrap();
    pub static ref HTML_COMMENT_END: Regex = Regex::new(r"-->").unwrap();
    pub static ref HTML_COMMENT_PATTERN: Regex = Regex::new(r"<!--[\s\S]*?-->").unwrap();

    // HTML heading pattern (matches <h1> through <h6> tags)
    pub static ref HTML_HEADING_PATTERN: FancyRegex = FancyRegex::new(r"^\s*<h([1-6])(?:\s[^>]*)?>.*</h\1>\s*$").unwrap();

    // Heading quick check pattern
    pub static ref HEADING_CHECK: Regex = Regex::new(r"(?m)^(?:\s*)#").unwrap();

    // Horizontal rule patterns
    pub static ref HR_DASH: Regex = Regex::new(r"^\-{3,}\s*$").unwrap();
    pub static ref HR_ASTERISK: Regex = Regex::new(r"^\*{3,}\s*$").unwrap();
    pub static ref HR_UNDERSCORE: Regex = Regex::new(r"^_{3,}\s*$").unwrap();
    pub static ref HR_SPACED_DASH: Regex = Regex::new(r"^(\-\s+){2,}\-\s*$").unwrap();
    pub static ref HR_SPACED_ASTERISK: Regex = Regex::new(r"^(\*\s+){2,}\*\s*$").unwrap();
    pub static ref HR_SPACED_UNDERSCORE: Regex = Regex::new(r"^(_\s+){2,}_\s*$").unwrap();
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
    let special_chars = ['.', '+', '*', '?', '^', '$', '(', ')', '[', ']', '{', '}', '|', '\\'];
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
    fn test_regex_cache_new() {
        let cache = RegexCache::new();
        assert!(cache.cache.is_empty());
        assert!(cache.fancy_cache.is_empty());
        assert!(cache.usage_stats.is_empty());
    }

    #[test]
    fn test_regex_cache_default() {
        let cache = RegexCache::default();
        assert!(cache.cache.is_empty());
        assert!(cache.fancy_cache.is_empty());
        assert!(cache.usage_stats.is_empty());
    }

    #[test]
    fn test_get_regex_compilation() {
        let mut cache = RegexCache::new();

        // First call compiles and caches
        let regex1 = cache.get_regex(r"\d+").unwrap();
        assert_eq!(cache.cache.len(), 1);
        assert_eq!(cache.usage_stats.get(r"\d+"), Some(&1));

        // Second call returns cached version
        let regex2 = cache.get_regex(r"\d+").unwrap();
        assert_eq!(cache.cache.len(), 1);
        assert_eq!(cache.usage_stats.get(r"\d+"), Some(&2));

        // Both should be the same Arc
        assert!(Arc::ptr_eq(&regex1, &regex2));
    }

    #[test]
    fn test_get_regex_invalid_pattern() {
        let mut cache = RegexCache::new();
        let result = cache.get_regex(r"[unterminated");
        assert!(result.is_err());
        assert!(cache.cache.is_empty());
    }

    #[test]
    fn test_get_fancy_regex_compilation() {
        let mut cache = RegexCache::new();

        // First call compiles and caches
        let regex1 = cache.get_fancy_regex(r"(?<=foo)bar").unwrap();
        assert_eq!(cache.fancy_cache.len(), 1);
        assert_eq!(cache.usage_stats.get(r"(?<=foo)bar"), Some(&1));

        // Second call returns cached version
        let regex2 = cache.get_fancy_regex(r"(?<=foo)bar").unwrap();
        assert_eq!(cache.fancy_cache.len(), 1);
        assert_eq!(cache.usage_stats.get(r"(?<=foo)bar"), Some(&2));

        // Both should be the same Arc
        assert!(Arc::ptr_eq(&regex1, &regex2));
    }

    #[test]
    fn test_get_fancy_regex_invalid_pattern() {
        let mut cache = RegexCache::new();
        let result = cache.get_fancy_regex(r"(?<=invalid");
        assert!(result.is_err());
        assert!(cache.fancy_cache.is_empty());
    }

    #[test]
    fn test_get_stats() {
        let mut cache = RegexCache::new();

        // Use some patterns
        let _ = cache.get_regex(r"\d+").unwrap();
        let _ = cache.get_regex(r"\d+").unwrap();
        let _ = cache.get_regex(r"\w+").unwrap();
        let _ = cache.get_fancy_regex(r"(?<=foo)bar").unwrap();

        let stats = cache.get_stats();
        assert_eq!(stats.get(r"\d+"), Some(&2));
        assert_eq!(stats.get(r"\w+"), Some(&1));
        assert_eq!(stats.get(r"(?<=foo)bar"), Some(&1));
    }

    #[test]
    fn test_clear_cache() {
        let mut cache = RegexCache::new();

        // Add some patterns
        let _ = cache.get_regex(r"\d+").unwrap();
        let _ = cache.get_fancy_regex(r"(?<=foo)bar").unwrap();

        assert!(!cache.cache.is_empty());
        assert!(!cache.fancy_cache.is_empty());
        assert!(!cache.usage_stats.is_empty());

        // Clear cache
        cache.clear();

        assert!(cache.cache.is_empty());
        assert!(cache.fancy_cache.is_empty());
        assert!(cache.usage_stats.is_empty());
    }

    #[test]
    fn test_global_cache_functions() {
        // Test get_cached_regex
        let regex1 = get_cached_regex(r"\d{3}").unwrap();
        let regex2 = get_cached_regex(r"\d{3}").unwrap();
        assert!(Arc::ptr_eq(&regex1, &regex2));

        // Test get_cached_fancy_regex
        let fancy1 = get_cached_fancy_regex(r"(?<=test)ing").unwrap();
        let fancy2 = get_cached_fancy_regex(r"(?<=test)ing").unwrap();
        assert!(Arc::ptr_eq(&fancy1, &fancy2));

        // Test stats
        let stats = get_cache_stats();
        assert!(stats.contains_key(r"\d{3}"));
        assert!(stats.contains_key(r"(?<=test)ing"));
    }

    #[test]
    fn test_regex_lazy_macro() {
        let re = regex_lazy!(r"^test.*end$");
        assert!(re.is_match("test something end"));
        assert!(!re.is_match("test something"));

        // The macro creates a new static for each invocation location,
        // so we can't test pointer equality across different invocations
        // But we can test that the regex works correctly
        let re2 = regex_lazy!(r"^start.*finish$");
        assert!(re2.is_match("start and finish"));
        assert!(!re2.is_match("start without end"));
    }

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
    fn test_has_emphasis_markers() {
        assert!(has_emphasis_markers("*emphasis*"));
        assert!(has_emphasis_markers("_emphasis_"));
        assert!(has_emphasis_markers("**bold**"));
        assert!(has_emphasis_markers("__bold__"));
        assert!(!has_emphasis_markers("no emphasis"));
    }

    #[test]
    fn test_has_html_tags() {
        assert!(has_html_tags("<div>content</div>"));
        assert!(has_html_tags("<br/>"));
        assert!(has_html_tags("<img src='test.jpg'>"));
        assert!(!has_html_tags("no html tags"));
        assert!(!has_html_tags("less than < but no tag"));
    }

    #[test]
    fn test_has_link_markers() {
        assert!(has_link_markers("[text](url)"));
        assert!(has_link_markers("[reference][1]"));
        assert!(has_link_markers("http://example.com"));
        assert!(has_link_markers("https://example.com"));
        assert!(has_link_markers("ftp://example.com"));
        assert!(!has_link_markers("no links here"));
    }

    #[test]
    fn test_has_image_markers() {
        assert!(has_image_markers("![alt text](image.png)"));
        assert!(has_image_markers("![](image.png)"));
        assert!(!has_image_markers("[link](url)"));
        assert!(!has_image_markers("no images"));
    }

    #[test]
    fn test_contains_url() {
        assert!(contains_url("http://example.com"));
        assert!(contains_url("Text with https://example.com link"));
        assert!(contains_url("ftp://example.com"));
        assert!(!contains_url("Text without URL"));
        assert!(!contains_url("http not followed by ://"));

        // Edge cases
        assert!(!contains_url("http"));
        assert!(!contains_url("https"));
        assert!(!contains_url("://"));
        assert!(contains_url("Visit http://site.com now"));
        assert!(contains_url("See https://secure.site.com/path"));
    }

    #[test]
    fn test_contains_url_performance() {
        // Test early exit for strings without "://"
        let long_text = "a".repeat(10000);
        assert!(!contains_url(&long_text));

        // Test with URL at the end
        let text_with_url = format!("{long_text}https://example.com");
        assert!(contains_url(&text_with_url));
    }

    #[test]
    fn test_escape_regex() {
        assert_eq!(escape_regex("a.b"), "a\\.b");
        assert_eq!(escape_regex("a+b*c"), "a\\+b\\*c");
        assert_eq!(escape_regex("(test)"), "\\(test\\)");
        assert_eq!(escape_regex("[a-z]"), "\\[a-z\\]");
        assert_eq!(escape_regex("normal text"), "normal text");

        // Test all special characters
        assert_eq!(escape_regex(".$^{[(|)*+?\\"), "\\.\\$\\^\\{\\[\\(\\|\\)\\*\\+\\?\\\\");

        // Test empty string
        assert_eq!(escape_regex(""), "");

        // Test mixed content
        assert_eq!(escape_regex("test.com/path?query=1"), "test\\.com/path\\?query=1");
    }

    #[test]
    fn test_static_regex_patterns() {
        // Test URL patterns
        assert!(URL_REGEX.is_match("https://example.com"));
        assert!(URL_REGEX.is_match("http://test.org/path"));
        assert!(URL_REGEX.is_match("ftp://files.com"));
        assert!(!URL_REGEX.is_match("not a url"));

        // Test heading patterns
        assert!(ATX_HEADING_REGEX.is_match("# Heading"));
        assert!(ATX_HEADING_REGEX.is_match("  ## Indented"));
        assert!(ATX_HEADING_REGEX.is_match("### "));
        assert!(!ATX_HEADING_REGEX.is_match("Not a heading"));

        // Test list patterns
        assert!(UNORDERED_LIST_MARKER_REGEX.is_match("* Item"));
        assert!(UNORDERED_LIST_MARKER_REGEX.is_match("- Item"));
        assert!(UNORDERED_LIST_MARKER_REGEX.is_match("+ Item"));
        assert!(ORDERED_LIST_MARKER_REGEX.is_match("1. Item"));
        assert!(ORDERED_LIST_MARKER_REGEX.is_match("99. Item"));

        // Test code block patterns
        assert!(FENCED_CODE_BLOCK_START_REGEX.is_match("```"));
        assert!(FENCED_CODE_BLOCK_START_REGEX.is_match("```rust"));
        assert!(FENCED_CODE_BLOCK_START_REGEX.is_match("~~~"));
        assert!(FENCED_CODE_BLOCK_END_REGEX.is_match("```"));
        assert!(FENCED_CODE_BLOCK_END_REGEX.is_match("~~~"));

        // Test emphasis patterns
        assert!(BOLD_ASTERISK_REGEX.is_match("**bold**"));
        assert!(BOLD_UNDERSCORE_REGEX.is_match("__bold__"));
        assert!(ITALIC_ASTERISK_REGEX.is_match("*italic*"));
        assert!(ITALIC_UNDERSCORE_REGEX.is_match("_italic_"));

        // Test HTML patterns
        assert!(HTML_TAG_REGEX.is_match("<div>"));
        assert!(HTML_TAG_REGEX.is_match("<span class='test'>"));
        assert!(HTML_SELF_CLOSING_TAG_REGEX.is_match("<br/>"));
        assert!(HTML_SELF_CLOSING_TAG_REGEX.is_match("<img src='test'/>"));

        // Test whitespace patterns
        assert!(TRAILING_WHITESPACE_REGEX.is_match("line with spaces   "));
        assert!(TRAILING_WHITESPACE_REGEX.is_match("tabs\t\t"));
        assert!(MULTIPLE_BLANK_LINES_REGEX.is_match("\n\n\n"));
        assert!(MULTIPLE_BLANK_LINES_REGEX.is_match("\n\n\n\n"));

        // Test blockquote pattern
        assert!(BLOCKQUOTE_PREFIX_RE.is_match("> Quote"));
        assert!(BLOCKQUOTE_PREFIX_RE.is_match("  > Indented quote"));
        assert!(BLOCKQUOTE_PREFIX_RE.is_match(">> Nested"));
    }

    #[test]
    fn test_thread_safety() {
        use std::thread;

        let handles: Vec<_> = (0..10)
            .map(|i| {
                thread::spawn(move || {
                    let pattern = format!(r"\d{{{i}}}");
                    let regex = get_cached_regex(&pattern).unwrap();
                    assert!(regex.is_match(&"1".repeat(i)));
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }
}
