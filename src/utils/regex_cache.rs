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
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
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

/// Global regex cache instance
static GLOBAL_REGEX_CACHE: LazyLock<Arc<Mutex<RegexCache>>> = LazyLock::new(|| Arc::new(Mutex::new(RegexCache::new())));

/// Get a regex from the global cache
///
/// If the mutex is poisoned (another thread panicked while holding the lock),
/// this function recovers by clearing the cache and continuing. This ensures
/// the library never panics due to mutex poisoning.
pub fn get_cached_regex(pattern: &str) -> Result<Arc<Regex>, regex::Error> {
    let mut cache = GLOBAL_REGEX_CACHE.lock().unwrap_or_else(|poisoned| {
        // Recover from poisoned mutex by clearing the cache
        let mut guard = poisoned.into_inner();
        guard.clear();
        guard
    });
    cache.get_regex(pattern)
}

/// Get a fancy regex from the global cache
///
/// If the mutex is poisoned (another thread panicked while holding the lock),
/// this function recovers by clearing the cache and continuing. This ensures
/// the library never panics due to mutex poisoning.
pub fn get_cached_fancy_regex(pattern: &str) -> Result<Arc<FancyRegex>, Box<fancy_regex::Error>> {
    let mut cache = GLOBAL_REGEX_CACHE.lock().unwrap_or_else(|poisoned| {
        // Recover from poisoned mutex by clearing the cache
        let mut guard = poisoned.into_inner();
        guard.clear();
        guard
    });
    cache.get_fancy_regex(pattern)
}

/// Get cache usage statistics
///
/// If the mutex is poisoned, returns an empty HashMap rather than panicking.
pub fn get_cache_stats() -> HashMap<String, u64> {
    match GLOBAL_REGEX_CACHE.lock() {
        Ok(cache) => cache.get_stats(),
        Err(_) => HashMap::new(),
    }
}

/// Macro for defining a lazily-initialized, cached regex pattern.
///
/// Use this for ad-hoc regexes that are not already defined in this module.
///
/// # Panics
///
/// This macro will panic at initialization if the regex pattern is invalid.
/// This is intentional for compile-time constant patterns - we want to catch
/// invalid patterns during development, not at runtime.
///
/// # Example
///
/// ```
/// use rumdl_lib::regex_lazy;
/// let my_re = regex_lazy!(r"^foo.*bar$");
/// assert!(my_re.is_match("foobar"));
/// ```
#[macro_export]
macro_rules! regex_lazy {
    ($pattern:expr) => {{
        static REGEX: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new($pattern).unwrap());
        &*REGEX
    }};
}

/// Macro for getting regex from global cache.
///
/// # Panics
///
/// Panics if the regex pattern is invalid. This is acceptable for static patterns
/// where we want to fail fast during development.
#[macro_export]
macro_rules! regex_cached {
    ($pattern:expr) => {{ $crate::utils::regex_cache::get_cached_regex($pattern).expect("Failed to compile regex") }};
}

/// Macro for getting fancy regex from global cache.
///
/// # Panics
///
/// Panics if the regex pattern is invalid. This is acceptable for static patterns
/// where we want to fail fast during development.
#[macro_export]
macro_rules! fancy_regex_cached {
    ($pattern:expr) => {{ $crate::utils::regex_cache::get_cached_fancy_regex($pattern).expect("Failed to compile fancy regex") }};
}

// Also make the macro available directly from this module
pub use crate::regex_lazy;

// =============================================================================
// URL REGEX PATTERNS - Centralized URL Detection
// =============================================================================
//
// ## Pattern Hierarchy (use the most specific pattern for your needs):
//
// | Pattern              | Use Case                                    | Parens | Trailing Punct |
// |----------------------|---------------------------------------------|--------|----------------|
// | URL_STANDARD_REGEX   | MD034 bare URL detection with auto-fix      | Yes    | Captured*      |
// | URL_WWW_REGEX        | www.domain URLs without protocol            | Yes    | Captured*      |
// | URL_IPV6_REGEX       | IPv6 URLs like https://[::1]/path           | Yes    | Captured*      |
// | URL_QUICK_CHECK_REGEX| Fast early-exit check (contains URL?)       | N/A    | N/A            |
// | URL_SIMPLE_REGEX     | Content detection, line length exemption    | No     | Excluded       |
//
// *Trailing punctuation is captured by the regex; use trim_trailing_punctuation() to clean.
//
// ## Design Principles:
// 1. Parentheses in paths are allowed for Wikipedia-style URLs (Issue #240)
// 2. Host portion excludes / so path is captured separately
// 3. Unbalanced trailing parens are handled by trim_trailing_punctuation()
// 4. All patterns exclude angle brackets <> to avoid matching autolinks
//
// ## URL Structure: protocol://host[:port][/path][?query][#fragment]

/// Pattern for standard HTTP(S)/FTP(S) URLs with full path support.
///
/// Use this for bare URL detection where you need the complete URL including
/// Wikipedia-style parentheses in paths. Trailing punctuation like `,;.!?` may
/// be captured and should be trimmed by the caller.
///
/// # Examples
/// - `https://example.com/path_(with_parens)?query#fragment`
/// - `https://en.wikipedia.org/wiki/Rust_(programming_language)`
pub const URL_STANDARD_STR: &str = concat!(
    r#"(?:https?|ftps?|ftp)://"#, // Protocol
    r#"(?:"#,
    r#"\[[0-9a-fA-F:%.\-a-zA-Z]+\]"#, // IPv6 host OR
    r#"|"#,
    r#"[^\s<>\[\]()\\'\"`/]+"#, // Standard host (no parens, no /)
    r#")"#,
    r#"(?::\d+)?"#,                 // Optional port
    r#"(?:/[^\s<>\[\]\\'\"`]*)?"#,  // Optional path (allows parens)
    r#"(?:\?[^\s<>\[\]\\'\"`]*)?"#, // Optional query (allows parens)
    r#"(?:#[^\s<>\[\]\\'\"`]*)?"#,  // Optional fragment (allows parens)
);

/// Pattern for www URLs without protocol.
///
/// Matches URLs starting with `www.` that lack a protocol prefix.
/// These should be converted to proper URLs or flagged as bare URLs.
/// Supports port, path, query string, and fragment like URL_STANDARD_STR.
///
/// # Examples
/// - `www.example.com`
/// - `www.example.com:8080`
/// - `www.example.com/path`
/// - `www.example.com?query=value`
/// - `www.example.com#section`
pub const URL_WWW_STR: &str = concat!(
    r#"www\.(?:[a-zA-Z0-9][-a-zA-Z0-9]*\.)+[a-zA-Z]{2,}"#, // www.domain.tld
    r#"(?::\d+)?"#,                                        // Optional port
    r#"(?:/[^\s<>\[\]\\'\"`]*)?"#,                         // Optional path (allows parens)
    r#"(?:\?[^\s<>\[\]\\'\"`]*)?"#,                        // Optional query (allows parens)
    r#"(?:#[^\s<>\[\]\\'\"`]*)?"#,                         // Optional fragment (allows parens)
);

/// Pattern for IPv6 URLs specifically.
///
/// Matches URLs with IPv6 addresses in brackets, including zone identifiers.
/// Examples: `https://[::1]/path`, `https://[fe80::1%eth0]:8080/`
pub const URL_IPV6_STR: &str = concat!(
    r#"(?:https?|ftps?|ftp)://"#,
    r#"\[[0-9a-fA-F:%.\-a-zA-Z]+\]"#, // IPv6 host in brackets
    r#"(?::\d+)?"#,                   // Optional port
    r#"(?:/[^\s<>\[\]\\'\"`]*)?"#,    // Optional path
    r#"(?:\?[^\s<>\[\]\\'\"`]*)?"#,   // Optional query
    r#"(?:#[^\s<>\[\]\\'\"`]*)?"#,    // Optional fragment
);

/// Pattern for XMPP URIs per GFM extended autolinks specification.
///
/// XMPP URIs use the format `xmpp:user@domain/resource` (without `://`).
/// Reference: <https://github.github.com/gfm/#autolinks-extension->
///
/// # Examples
/// - `xmpp:foo@bar.baz`
/// - `xmpp:foo@bar.baz/txt`
pub const XMPP_URI_STR: &str = r#"xmpp:[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}(?:/[^\s<>\[\]\\'\"`]*)?"#;

/// Quick check pattern for early exits.
///
/// Use this for fast pre-filtering before running more expensive patterns.
/// Matches if the text likely contains a URL or email address.
/// Includes `xmpp:` for GFM extended autolinks.
pub const URL_QUICK_CHECK_STR: &str = r#"(?:https?|ftps?|ftp|xmpp)://|xmpp:|@|www\."#;

/// Simple URL pattern for content detection.
///
/// Less strict pattern that excludes trailing sentence punctuation (.,).
/// Use for line length exemption checks or content characteristic detection
/// where you just need to know if a URL exists, not extract it precisely.
pub const URL_SIMPLE_STR: &str = r#"(?:https?|ftps?|ftp)://[^\s<>]+[^\s<>.,]"#;

// Pre-compiled static patterns for performance

/// Standard URL regex - primary pattern for bare URL detection (MD034).
/// See [`URL_STANDARD_STR`] for documentation.
pub static URL_STANDARD_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(URL_STANDARD_STR).unwrap());

/// WWW URL regex - for URLs starting with www. without protocol.
/// See [`URL_WWW_STR`] for documentation.
pub static URL_WWW_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(URL_WWW_STR).unwrap());

/// IPv6 URL regex - for URLs with IPv6 addresses.
/// See [`URL_IPV6_STR`] for documentation.
pub static URL_IPV6_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(URL_IPV6_STR).unwrap());

/// Quick check regex - fast early-exit test.
/// See [`URL_QUICK_CHECK_STR`] for documentation.
pub static URL_QUICK_CHECK_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(URL_QUICK_CHECK_STR).unwrap());

/// Simple URL regex - for content detection and line length exemption.
/// See [`URL_SIMPLE_STR`] for documentation.
pub static URL_SIMPLE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(URL_SIMPLE_STR).unwrap());

/// Alias for `URL_SIMPLE_REGEX`. Used by MD013 for line length exemption.
pub static URL_PATTERN: LazyLock<Regex> = LazyLock::new(|| URL_SIMPLE_REGEX.clone());

/// XMPP URI regex - for GFM extended autolinks.
/// See [`XMPP_URI_STR`] for documentation.
pub static XMPP_URI_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(XMPP_URI_STR).unwrap());

// Heading patterns
pub static ATX_HEADING_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)(#{1,6})(\s+|$)").unwrap());
pub static CLOSED_ATX_HEADING_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)(#{1,6})(\s+)(.*)(\s+)(#+)(\s*)$").unwrap());
pub static SETEXT_HEADING_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)[^\s]+.*\n(\s*)(=+|-+)\s*$").unwrap());
pub static TRAILING_PUNCTUATION_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[.,:);!?]$").unwrap());

// ATX heading patterns for MD051 and other rules
pub static ATX_HEADING_WITH_CAPTURE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(#{1,6})\s+(.+?)(?:\s+#*\s*)?$").unwrap());
pub static SETEXT_HEADING_WITH_CAPTURE: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"^([^\n]+)\n([=\-])\2+\s*$").unwrap());

// List patterns
pub static UNORDERED_LIST_MARKER_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)([*+-])(\s+)").unwrap());
pub static ORDERED_LIST_MARKER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)(\d+)([.)])(\s+)").unwrap());
pub static LIST_MARKER_ANY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)(?:([*+-])|(\d+)[.)])(\s+)").unwrap());

// Code block patterns
pub static FENCED_CODE_BLOCK_START_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)(```|~~~)(.*)$").unwrap());
pub static FENCED_CODE_BLOCK_END_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)(```|~~~)(\s*)$").unwrap());
pub static INDENTED_CODE_BLOCK_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s{4,})(.*)$").unwrap());
pub static CODE_FENCE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(`{3,}|~{3,})").unwrap());

// Emphasis patterns
pub static EMPHASIS_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"(\s|^)(\*{1,2}|_{1,2})(?=\S)(.+?)(?<=\S)(\2)(\s|$)").unwrap());
pub static SPACE_IN_EMPHASIS_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"(\*|_)(\s+)(.+?)(\s+)(\1)").unwrap());

// MD037 specific emphasis patterns - improved to avoid false positives
// Only match emphasis with spaces that are actually complete emphasis blocks
// Use word boundaries and negative lookbehind/lookahead to avoid matching across emphasis boundaries
pub static ASTERISK_EMPHASIS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[^*])\*(\s+[^*]+\s*|\s*[^*]+\s+)\*(?:[^*]|$)").unwrap());
pub static UNDERSCORE_EMPHASIS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[^_])_(\s+[^_]+\s*|\s*[^_]+\s+)_(?:[^_]|$)").unwrap());
pub static DOUBLE_UNDERSCORE_EMPHASIS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[^_])__(\s+[^_]+\s*|\s*[^_]+\s+)__(?:[^_]|$)").unwrap());
pub static DOUBLE_ASTERISK_EMPHASIS: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"\*\*\s+([^*]+?)\s+\*\*").unwrap());
pub static DOUBLE_ASTERISK_SPACE_START: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"\*\*\s+([^*]+?)\*\*").unwrap());
pub static DOUBLE_ASTERISK_SPACE_END: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"\*\*([^*]+?)\s+\*\*").unwrap());

// Code block patterns
pub static FENCED_CODE_BLOCK_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)```(?:[^`\r\n]*)$").unwrap());
pub static FENCED_CODE_BLOCK_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)```\s*$").unwrap());
pub static ALTERNATE_FENCED_CODE_BLOCK_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)~~~(?:[^~\r\n]*)$").unwrap());
pub static ALTERNATE_FENCED_CODE_BLOCK_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)~~~\s*$").unwrap());
pub static INDENTED_CODE_BLOCK_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s{4,})").unwrap());

// HTML patterns
pub static HTML_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<([a-zA-Z][^>]*)>").unwrap());
pub static HTML_SELF_CLOSING_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<([a-zA-Z][^>]*/)>").unwrap());
pub static HTML_TAG_FINDER: LazyLock<Regex> = LazyLock::new(|| Regex::new("(?i)</?[a-zA-Z][^>]*>").unwrap());
pub static HTML_OPENING_TAG_FINDER: LazyLock<Regex> = LazyLock::new(|| Regex::new("(?i)<[a-zA-Z][^>]*>").unwrap());
pub static HTML_TAG_QUICK_CHECK: LazyLock<Regex> = LazyLock::new(|| Regex::new("(?i)</?[a-zA-Z]").unwrap());

// Link patterns for MD051 and other rules
pub static LINK_REFERENCE_DEFINITION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\[([^\]]+)\]:\s+(.+)$").unwrap());
pub static INLINE_LINK_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap());
pub static LINK_TEXT_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]*)\]").unwrap());
pub static LINK_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"(?<!\\)\[([^\]]*)\]\(([^)#]*)#([^)]+)\)").unwrap());
pub static EXTERNAL_URL_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"^(https?://|ftp://|www\.|[^/]+\.[a-z]{2,})").unwrap());

// Image patterns
pub static IMAGE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").unwrap());

// Whitespace patterns
pub static TRAILING_WHITESPACE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+$").unwrap());
pub static MULTIPLE_BLANK_LINES_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());

// Front matter patterns
pub static FRONT_MATTER_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^---\n.*?\n---\n").unwrap());

// MD051 specific patterns
pub static INLINE_CODE_REGEX: LazyLock<FancyRegex> = LazyLock::new(|| FancyRegex::new(r"`[^`]+`").unwrap());
pub static BOLD_ASTERISK_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*(.+?)\*\*").unwrap());
pub static BOLD_UNDERSCORE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"__(.+?)__").unwrap());
pub static ITALIC_ASTERISK_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*([^*]+?)\*").unwrap());
pub static ITALIC_UNDERSCORE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"_([^_]+?)_").unwrap());
pub static LINK_TEXT_FULL_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());
pub static STRIKETHROUGH_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"~~(.+?)~~").unwrap());
pub static MULTIPLE_HYPHENS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-{2,}").unwrap());
pub static TOC_SECTION_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#+\s*(?:Table of Contents|Contents|TOC)\s*$").unwrap());

// Blockquote patterns
pub static BLOCKQUOTE_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*>+\s*)").unwrap());

/// Check if a line is blank in the context of blockquotes.
///
/// A line is considered "blank" if:
/// - It's empty or contains only whitespace
/// - It's a blockquote continuation line with no content (e.g., ">", ">>", "> ")
///
/// This is essential for rules like MD058 (blanks-around-tables), MD065 (blanks-around-horizontal-rules),
/// and any other rule that needs to detect blank lines that might be inside blockquotes.
///
/// # Examples
/// ```
/// use rumdl_lib::utils::regex_cache::is_blank_in_blockquote_context;
///
/// assert!(is_blank_in_blockquote_context(""));           // Empty line
/// assert!(is_blank_in_blockquote_context("   "));        // Whitespace only
/// assert!(is_blank_in_blockquote_context(">"));          // Blockquote continuation
/// assert!(is_blank_in_blockquote_context("> "));         // Blockquote with trailing space
/// assert!(is_blank_in_blockquote_context(">>"));         // Nested blockquote
/// assert!(is_blank_in_blockquote_context("> > "));       // Spaced nested blockquote
/// assert!(!is_blank_in_blockquote_context("> text"));    // Blockquote with content
/// assert!(!is_blank_in_blockquote_context("text"));      // Regular text
/// ```
pub fn is_blank_in_blockquote_context(line: &str) -> bool {
    if line.trim().is_empty() {
        return true;
    }
    // Check if line is a blockquote prefix with no content after it
    // Handle spaced nested blockquotes like "> > " by recursively checking remainder
    if let Some(m) = BLOCKQUOTE_PREFIX_RE.find(line) {
        let remainder = &line[m.end()..];
        // The remainder should be empty/whitespace OR another blockquote prefix (for spaced nesting)
        is_blank_in_blockquote_context(remainder)
    } else {
        false
    }
}

// MD013 specific patterns
pub static IMAGE_REF_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^!\[.*?\]\[.*?\]$").unwrap());
pub static LINK_REF_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[.*?\]:\s*https?://\S+$").unwrap());
/// Greedy URL pattern for finding URLs in text for length calculation.
///
/// Pattern `https?://\S+` matches until whitespace, which may include trailing
/// punctuation. This is intentional for MD013 line length calculation where
/// we replace URLs with fixed-length placeholders.
///
/// For precise URL extraction, use `URL_STANDARD_REGEX` instead.
pub static URL_IN_TEXT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"https?://\S+").unwrap());
pub static SENTENCE_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[.!?]\s+[A-Z]").unwrap());
pub static ABBREVIATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:Mr|Mrs|Ms|Dr|Prof|Sr|Jr|vs|etc|i\.e|e\.g|Inc|Corp|Ltd|Co|St|Ave|Blvd|Rd|Ph\.D|M\.D|B\.A|M\.A|Ph\.D|U\.S|U\.K|U\.N|N\.Y|L\.A|D\.C)\.\s+[A-Z]").unwrap()
});
pub static DECIMAL_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+\.\s*\d+").unwrap());
pub static LIST_ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\d+\.\s+").unwrap());
pub static REFERENCE_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\[([^\]]*)\]").unwrap());

// Email pattern
pub static EMAIL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap());

// Third lazy_static block for link and image patterns used by MD052 and text_reflow
// Reference link patterns (shared by MD052 and text_reflow)
// Pattern to match reference links: [text][reference] or [text][]
pub static REF_LINK_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"(?<!\\)\[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\]\[([^\]]*)\]").unwrap());

// Pattern for shortcut reference links: [reference]
// Must not be preceded by ] or ) (to avoid matching second part of [text][ref])
// Must not be followed by [ or ( (to avoid matching first part of [text][ref] or [text](url))
// The capturing group handles nested brackets to support cases like [`Union[T, None]`]
pub static SHORTCUT_REF_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"(?<![\\)\]])\[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\](?!\s*[\[\(])").unwrap());

// Inline link with fancy regex for better escaping handling (used by text_reflow)
pub static INLINE_LINK_FANCY_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"(?<!\\)\[([^\]]+)\]\(([^)]+)\)").unwrap());

// Inline image with fancy regex (used by MD052 and text_reflow)
pub static INLINE_IMAGE_FANCY_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"!\[([^\]]*)\]\(([^)]+)\)").unwrap());

// Linked images (clickable badges) - all 4 variants
// Must be detected before inline_image and inline_link to treat as atomic units
//
// Limitation: Alt text containing brackets like [![[v1.0]](img)](link) is not supported.
// The [^\]]* pattern cannot match nested brackets. This is rare in practice.
//
// Pattern 1: Inline image in inline link - [![alt](img-url)](link-url)
pub static LINKED_IMAGE_INLINE_INLINE: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"\[!\[([^\]]*)\]\(([^)]+)\)\]\(([^)]+)\)").unwrap());

// Pattern 2: Reference image in inline link - [![alt][img-ref]](link-url)
pub static LINKED_IMAGE_REF_INLINE: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"\[!\[([^\]]*)\]\[([^\]]*)\]\]\(([^)]+)\)").unwrap());

// Pattern 3: Inline image in reference link - [![alt](img-url)][link-ref]
pub static LINKED_IMAGE_INLINE_REF: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"\[!\[([^\]]*)\]\(([^)]+)\)\]\[([^\]]*)\]").unwrap());

// Pattern 4: Reference image in reference link - [![alt][img-ref]][link-ref]
pub static LINKED_IMAGE_REF_REF: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"\[!\[([^\]]*)\]\[([^\]]*)\]\]\[([^\]]*)\]").unwrap());

// Reference image: ![alt][ref] or ![alt][]
pub static REF_IMAGE_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"!\[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\]\[([^\]]*)\]").unwrap());

// Footnote reference: [^note]
pub static FOOTNOTE_REF_REGEX: LazyLock<FancyRegex> = LazyLock::new(|| FancyRegex::new(r"\[\^([^\]]+)\]").unwrap());

// Strikethrough with fancy regex: ~~text~~
pub static STRIKETHROUGH_FANCY_REGEX: LazyLock<FancyRegex> = LazyLock::new(|| FancyRegex::new(r"~~([^~]+)~~").unwrap());

// Wiki-style links: [[wiki]] or [[wiki|display text]]
pub static WIKI_LINK_REGEX: LazyLock<FancyRegex> = LazyLock::new(|| FancyRegex::new(r"\[\[([^\]]+)\]\]").unwrap());

// Math formulas: $inline$ or $$display$$
pub static INLINE_MATH_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"(?<!\$)\$(?!\$)([^\$]+)\$(?!\$)").unwrap());
pub static DISPLAY_MATH_REGEX: LazyLock<FancyRegex> = LazyLock::new(|| FancyRegex::new(r"\$\$([^\$]+)\$\$").unwrap());

// Emoji shortcodes: :emoji:
pub static EMOJI_SHORTCODE_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r":([a-zA-Z0-9_+-]+):").unwrap());

// HTML tags (opening, closing, self-closing)
pub static HTML_TAG_PATTERN: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"</?[a-zA-Z][^>]*>|<[a-zA-Z][^>]*/\s*>").unwrap());

// HTML entities: &nbsp; &mdash; etc
pub static HTML_ENTITY_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"&[a-zA-Z][a-zA-Z0-9]*;|&#\d+;|&#x[0-9a-fA-F]+;").unwrap());

// Hugo/Go template shortcodes: {{< figure ... >}} and {{% shortcode %}}
// Matches both delimiters: {{< ... >}} (shortcode) and {{% ... %}} (template)
// Handles multi-line content with embedded quotes and newlines
pub static HUGO_SHORTCODE_REGEX: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"\{\{[<%][\s\S]*?[%>]\}\}").unwrap());

// Fourth lazy_static block for additional patterns
// HTML comment patterns
pub static HTML_COMMENT_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<!--").unwrap());
pub static HTML_COMMENT_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-->").unwrap());
pub static HTML_COMMENT_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<!--[\s\S]*?-->").unwrap());

// HTML heading pattern (matches <h1> through <h6> tags)
pub static HTML_HEADING_PATTERN: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"^\s*<h([1-6])(?:\s[^>]*)?>.*</h\1>\s*$").unwrap());

// Heading quick check pattern
pub static HEADING_CHECK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(?:\s*)#").unwrap());

// Horizontal rule patterns
pub static HR_DASH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\-{3,}\s*$").unwrap());
pub static HR_ASTERISK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\*{3,}\s*$").unwrap());
pub static HR_UNDERSCORE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^_{3,}\s*$").unwrap());
pub static HR_SPACED_DASH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\-\s+){2,}\-\s*$").unwrap());
pub static HR_SPACED_ASTERISK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\*\s+){2,}\*\s*$").unwrap());
pub static HR_SPACED_UNDERSCORE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(_\s+){2,}_\s*$").unwrap());

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
    let mut result = String::with_capacity(s.len() * 2);

    for c in s.chars() {
        // Use matches! for O(1) lookup instead of array.contains() which is O(n)
        if matches!(
            c,
            '.' | '+' | '*' | '?' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\'
        ) {
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
        assert!(URL_SIMPLE_REGEX.is_match("https://example.com"));
        assert!(URL_SIMPLE_REGEX.is_match("http://test.org/path"));
        assert!(URL_SIMPLE_REGEX.is_match("ftp://files.com"));
        assert!(!URL_SIMPLE_REGEX.is_match("not a url"));

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

    // ==========================================================================
    // Comprehensive URL Regex Tests
    // ==========================================================================

    #[test]
    fn test_url_standard_basic() {
        // Basic HTTP/HTTPS URLs
        assert!(URL_STANDARD_REGEX.is_match("https://example.com"));
        assert!(URL_STANDARD_REGEX.is_match("http://example.com"));
        assert!(URL_STANDARD_REGEX.is_match("https://example.com/"));
        assert!(URL_STANDARD_REGEX.is_match("https://example.com/path"));
        assert!(URL_STANDARD_REGEX.is_match("ftp://files.example.com"));
        assert!(URL_STANDARD_REGEX.is_match("ftps://secure.example.com"));

        // Should not match non-URLs
        assert!(!URL_STANDARD_REGEX.is_match("not a url"));
        assert!(!URL_STANDARD_REGEX.is_match("example.com"));
        assert!(!URL_STANDARD_REGEX.is_match("www.example.com"));
    }

    #[test]
    fn test_url_standard_with_path() {
        assert!(URL_STANDARD_REGEX.is_match("https://example.com/path/to/page"));
        assert!(URL_STANDARD_REGEX.is_match("https://example.com/path/to/page.html"));
        assert!(URL_STANDARD_REGEX.is_match("https://example.com/path/to/page/"));
    }

    #[test]
    fn test_url_standard_with_query() {
        assert!(URL_STANDARD_REGEX.is_match("https://example.com?query=value"));
        assert!(URL_STANDARD_REGEX.is_match("https://example.com/path?query=value"));
        assert!(URL_STANDARD_REGEX.is_match("https://example.com/path?a=1&b=2"));
    }

    #[test]
    fn test_url_standard_with_fragment() {
        assert!(URL_STANDARD_REGEX.is_match("https://example.com#section"));
        assert!(URL_STANDARD_REGEX.is_match("https://example.com/path#section"));
        assert!(URL_STANDARD_REGEX.is_match("https://example.com/path?query=value#section"));
    }

    #[test]
    fn test_url_standard_with_port() {
        assert!(URL_STANDARD_REGEX.is_match("https://example.com:8080"));
        assert!(URL_STANDARD_REGEX.is_match("https://example.com:443/path"));
        assert!(URL_STANDARD_REGEX.is_match("http://localhost:3000"));
        assert!(URL_STANDARD_REGEX.is_match("https://192.168.1.1:8080/path"));
    }

    #[test]
    fn test_url_standard_wikipedia_style_parentheses() {
        // Wikipedia-style URLs with parentheses in path (Issue #240)
        let url = "https://en.wikipedia.org/wiki/Rust_(programming_language)";
        assert!(URL_STANDARD_REGEX.is_match(url));

        // Verify the full URL is captured
        let cap = URL_STANDARD_REGEX.find(url).unwrap();
        assert_eq!(cap.as_str(), url);

        // Multiple parentheses pairs
        let url2 = "https://example.com/path_(foo)_(bar)";
        let cap2 = URL_STANDARD_REGEX.find(url2).unwrap();
        assert_eq!(cap2.as_str(), url2);
    }

    #[test]
    fn test_url_standard_ipv6() {
        // IPv6 addresses in URLs
        assert!(URL_STANDARD_REGEX.is_match("https://[::1]/path"));
        assert!(URL_STANDARD_REGEX.is_match("https://[2001:db8::1]:8080/path"));
        assert!(URL_STANDARD_REGEX.is_match("http://[fe80::1%eth0]/"));
    }

    #[test]
    fn test_url_www_basic() {
        // www URLs without protocol
        assert!(URL_WWW_REGEX.is_match("www.example.com"));
        assert!(URL_WWW_REGEX.is_match("www.example.co.uk"));
        assert!(URL_WWW_REGEX.is_match("www.sub.example.com"));

        // Should not match plain domains without www
        assert!(!URL_WWW_REGEX.is_match("example.com"));

        // Note: https://www.example.com DOES match because it contains "www."
        // The URL_WWW_REGEX is designed to find www. URLs that lack a protocol
        // Use URL_STANDARD_REGEX for full URLs with protocols
        assert!(URL_WWW_REGEX.is_match("https://www.example.com"));
    }

    #[test]
    fn test_url_www_with_path() {
        assert!(URL_WWW_REGEX.is_match("www.example.com/path"));
        assert!(URL_WWW_REGEX.is_match("www.example.com/path/to/page"));
        assert!(URL_WWW_REGEX.is_match("www.example.com/path_(with_parens)"));
    }

    #[test]
    fn test_url_ipv6_basic() {
        // IPv6 specific patterns
        assert!(URL_IPV6_REGEX.is_match("https://[::1]/"));
        assert!(URL_IPV6_REGEX.is_match("http://[2001:db8::1]/path"));
        assert!(URL_IPV6_REGEX.is_match("https://[fe80::1]:8080/path"));
        assert!(URL_IPV6_REGEX.is_match("ftp://[::ffff:192.168.1.1]/file"));
    }

    #[test]
    fn test_url_ipv6_with_zone_id() {
        // IPv6 with zone identifiers
        assert!(URL_IPV6_REGEX.is_match("https://[fe80::1%eth0]/path"));
        assert!(URL_IPV6_REGEX.is_match("http://[fe80::1%25eth0]:8080/"));
    }

    #[test]
    fn test_url_simple_detection() {
        // Simple pattern for content characteristic detection
        assert!(URL_SIMPLE_REGEX.is_match("https://example.com"));
        assert!(URL_SIMPLE_REGEX.is_match("http://test.org/path"));
        assert!(URL_SIMPLE_REGEX.is_match("ftp://files.com/file.zip"));
        assert!(!URL_SIMPLE_REGEX.is_match("not a url"));
    }

    #[test]
    fn test_url_quick_check() {
        // Quick check pattern for early exits
        assert!(URL_QUICK_CHECK_REGEX.is_match("https://example.com"));
        assert!(URL_QUICK_CHECK_REGEX.is_match("http://example.com"));
        assert!(URL_QUICK_CHECK_REGEX.is_match("ftp://files.com"));
        assert!(URL_QUICK_CHECK_REGEX.is_match("www.example.com"));
        assert!(URL_QUICK_CHECK_REGEX.is_match("user@example.com"));
        assert!(!URL_QUICK_CHECK_REGEX.is_match("just plain text"));
    }

    #[test]
    fn test_url_edge_cases() {
        // URLs with special characters that should be excluded
        let url = "https://example.com/path";
        assert!(URL_STANDARD_REGEX.is_match(url));

        // URL followed by punctuation - the regex captures trailing punctuation
        // because trimming is done by `trim_trailing_punctuation()` in the rule
        let text = "Check https://example.com, it's great!";
        let cap = URL_STANDARD_REGEX.find(text).unwrap();
        // The comma IS captured by the regex - rule-level trimming handles this
        assert!(cap.as_str().ends_with(','));

        // URL in angle brackets should still be found
        let text2 = "See <https://example.com> for more";
        assert!(URL_STANDARD_REGEX.is_match(text2));

        // URL ending at angle bracket should stop at >
        let cap2 = URL_STANDARD_REGEX.find(text2).unwrap();
        assert!(!cap2.as_str().contains('>'));
    }

    #[test]
    fn test_url_with_complex_paths() {
        // Complex real-world URLs
        let urls = [
            "https://github.com/owner/repo/blob/main/src/file.rs#L123",
            "https://docs.example.com/api/v2/endpoint?format=json&page=1",
            "https://cdn.example.com/assets/images/logo.png?v=2023",
            "https://search.example.com/results?q=test+query&filter=all",
        ];

        for url in urls {
            assert!(URL_STANDARD_REGEX.is_match(url), "Should match: {url}");
        }
    }

    #[test]
    fn test_url_pattern_strings_are_valid() {
        // Verify patterns compile into valid regexes by accessing them
        assert!(URL_STANDARD_REGEX.is_match("https://example.com"));
        assert!(URL_WWW_REGEX.is_match("www.example.com"));
        assert!(URL_IPV6_REGEX.is_match("https://[::1]/"));
        assert!(URL_QUICK_CHECK_REGEX.is_match("https://example.com"));
        assert!(URL_SIMPLE_REGEX.is_match("https://example.com"));
    }

    // =========================================================================
    // Tests for is_blank_in_blockquote_context
    // This is a shared utility used by MD058, MD065, and other rules that need
    // to detect blank lines inside blockquotes (Issue #305)
    // =========================================================================

    #[test]
    fn test_is_blank_in_blockquote_context_regular_blanks() {
        // Regular blank lines
        assert!(is_blank_in_blockquote_context(""));
        assert!(is_blank_in_blockquote_context("   "));
        assert!(is_blank_in_blockquote_context("\t"));
        assert!(is_blank_in_blockquote_context("  \t  "));
    }

    #[test]
    fn test_is_blank_in_blockquote_context_blockquote_blanks() {
        // Blockquote continuation lines with no content (should be treated as blank)
        assert!(is_blank_in_blockquote_context(">"));
        assert!(is_blank_in_blockquote_context("> "));
        assert!(is_blank_in_blockquote_context(">  "));
        assert!(is_blank_in_blockquote_context(">>"));
        assert!(is_blank_in_blockquote_context(">> "));
        assert!(is_blank_in_blockquote_context(">>>"));
        assert!(is_blank_in_blockquote_context(">>> "));
    }

    #[test]
    fn test_is_blank_in_blockquote_context_spaced_nested() {
        // Spaced nested blockquotes ("> > " style)
        assert!(is_blank_in_blockquote_context("> > "));
        assert!(is_blank_in_blockquote_context("> > > "));
        assert!(is_blank_in_blockquote_context(">  >  "));
    }

    #[test]
    fn test_is_blank_in_blockquote_context_with_leading_space() {
        // Blockquote with leading whitespace
        assert!(is_blank_in_blockquote_context("  >"));
        assert!(is_blank_in_blockquote_context("  > "));
        assert!(is_blank_in_blockquote_context("  >>"));
    }

    #[test]
    fn test_is_blank_in_blockquote_context_not_blank() {
        // Lines with actual content (should NOT be treated as blank)
        assert!(!is_blank_in_blockquote_context("text"));
        assert!(!is_blank_in_blockquote_context("> text"));
        assert!(!is_blank_in_blockquote_context(">> text"));
        assert!(!is_blank_in_blockquote_context("> | table |"));
        assert!(!is_blank_in_blockquote_context("| table |"));
        assert!(!is_blank_in_blockquote_context("> # Heading"));
        assert!(!is_blank_in_blockquote_context(">text")); // No space after > but has text
    }

    #[test]
    fn test_is_blank_in_blockquote_context_edge_cases() {
        // Edge cases
        assert!(!is_blank_in_blockquote_context(">a")); // Content immediately after >
        assert!(!is_blank_in_blockquote_context("> a")); // Single char content
        assert!(is_blank_in_blockquote_context(">   ")); // Multiple spaces after >
        assert!(!is_blank_in_blockquote_context(">  text")); // Multiple spaces before content
    }
}
