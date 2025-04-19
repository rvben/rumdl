use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;

/// A module that provides cached regex patterns for use across multiple rules
/// This helps avoid recompiling the same patterns repeatedly, improving performance
#[macro_export]
macro_rules! regex_lazy {
    ($pattern:expr) => {{
        lazy_static::lazy_static! {
            static ref REGEX: regex::Regex = regex::Regex::new($pattern).unwrap();
        }
        &*REGEX
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

    // List patterns
    pub static ref UNORDERED_LIST_MARKER_REGEX: Regex = Regex::new(r"^(\s*)([*+-])(\s+)").unwrap();
    pub static ref ORDERED_LIST_MARKER_REGEX: Regex = Regex::new(r"^(\s*)(\d+)([.)])(\s+)").unwrap();
    pub static ref LIST_MARKER_ANY_REGEX: Regex = Regex::new(r"^(\s*)(?:([*+-])|(\d+)[.)])(\s+)").unwrap();

    // Code block patterns
    pub static ref FENCED_CODE_BLOCK_START_REGEX: Regex = Regex::new(r"^(\s*)(```|~~~)(.*)$").unwrap();
    pub static ref FENCED_CODE_BLOCK_END_REGEX: Regex = Regex::new(r"^(\s*)(```|~~~)(\s*)$").unwrap();
    pub static ref INDENTED_CODE_BLOCK_REGEX: Regex = Regex::new(r"^(\s{4,})(.*)$").unwrap();

    // Emphasis patterns
    pub static ref EMPHASIS_REGEX: FancyRegex = FancyRegex::new(r"(\s|^)(\*{1,2}|_{1,2})(?=\S)(.+?)(?<=\S)(\2)(\s|$)").unwrap();
    pub static ref SPACE_IN_EMPHASIS_REGEX: FancyRegex = FancyRegex::new(r"(\*|_)(\s+)(.+?)(\s+)(\1)").unwrap();

    // HTML patterns
    pub static ref HTML_TAG_REGEX: Regex = Regex::new(r"<([a-zA-Z][^>]*)>").unwrap();
    pub static ref HTML_SELF_CLOSING_TAG_REGEX: Regex = Regex::new(r"<([a-zA-Z][^>]*/)>").unwrap();

    // Link patterns
    pub static ref LINK_REFERENCE_DEFINITION_REGEX: Regex = Regex::new(r"^\s*\[([^\]]+)\]:\s*(.+)$").unwrap();
    pub static ref INLINE_LINK_REGEX: Regex = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    pub static ref LINK_TEXT_REGEX: Regex = Regex::new(r"\[([^\]]*)\]").unwrap();

    // Image patterns
    pub static ref IMAGE_REGEX: Regex = Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").unwrap();

    // Whitespace patterns
    pub static ref TRAILING_WHITESPACE_REGEX: Regex = Regex::new(r"\s+$").unwrap();
    pub static ref MULTIPLE_BLANK_LINES_REGEX: Regex = Regex::new(r"\n{3,}").unwrap();

    // Front matter patterns
    pub static ref FRONT_MATTER_REGEX: Regex = Regex::new(r"^---\n.*?\n---\n").unwrap();
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
