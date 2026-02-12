//! Utilities for handling Kramdown-specific syntax
//!
//! Kramdown is a superset of Markdown that adds additional features like
//! Inline Attribute Lists (IAL) for adding attributes to elements.

use regex::Regex;
use std::sync::LazyLock;

/// Pattern for Kramdown span IAL: text{:.class #id key="value"}
static SPAN_IAL_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{[:\.#][^}]*\}$").unwrap());

/// Pattern for Kramdown extensions opening: {::comment}, {::nomarkdown}, etc.
static EXTENSION_OPEN_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\{::([a-z]+)(?:\s+[^}]*)?\}\s*$").unwrap());

/// Pattern for Kramdown extensions closing: {:/comment}, {:/}, etc.
static EXTENSION_CLOSE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\{:/([a-z]+)?\}\s*$").unwrap());

/// Pattern for Kramdown options: {::options key="value" /}
static OPTIONS_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\{::options\s+[^}]+/\}\s*$").unwrap());

/// Pattern for math blocks: $$
static MATH_BLOCK_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\$\$").unwrap());

/// Check if a line is a Kramdown block attribute (IAL - Inline Attribute List)
///
/// Kramdown IAL syntax allows adding attributes to block elements:
/// - `{:.class}` - CSS class
/// - `{:#id}` - Element ID
/// - `{:attribute="value"}` - Generic attributes
/// - `{:.class #id attribute="value"}` - Combinations
///
/// # Examples
///
/// ```
/// use rumdl_lib::utils::kramdown_utils::is_kramdown_block_attribute;
///
/// assert!(is_kramdown_block_attribute("{:.wrap}"));
/// assert!(is_kramdown_block_attribute("{:#my-id}"));
/// assert!(is_kramdown_block_attribute("{:.class #id}"));
/// assert!(is_kramdown_block_attribute("{:style=\"color: red\"}"));
///
/// assert!(!is_kramdown_block_attribute("{just text}"));
/// assert!(!is_kramdown_block_attribute("{}"));
/// assert!(!is_kramdown_block_attribute("{"));
/// ```
pub fn is_kramdown_block_attribute(line: &str) -> bool {
    let trimmed = line.trim();

    // Must start with { and end with }
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') || trimmed.len() < 3 {
        return false;
    }

    // Check if it matches Kramdown IAL patterns
    // Valid patterns start with {: or {# or {.
    let second_char = trimmed.chars().nth(1);
    matches!(second_char, Some(':') | Some('#') | Some('.'))
}

/// Check if text ends with a Kramdown span IAL (inline attribute)
///
/// # Examples
/// ```
/// use rumdl_lib::utils::kramdown_utils::has_span_ial;
///
/// assert!(has_span_ial("*emphasized*{:.highlight}"));
/// assert!(has_span_ial("[link](url){:target=\"_blank\"}"));
/// assert!(!has_span_ial("regular text"));
/// ```
pub fn has_span_ial(text: &str) -> bool {
    SPAN_IAL_PATTERN.is_match(text.trim())
}

/// Check if a line is a Kramdown extension opening tag
///
/// Extensions include: comment, nomarkdown, options
pub fn is_kramdown_extension_open(line: &str) -> bool {
    EXTENSION_OPEN_PATTERN.is_match(line)
}

/// Check if a line is a Kramdown extension closing tag
pub fn is_kramdown_extension_close(line: &str) -> bool {
    EXTENSION_CLOSE_PATTERN.is_match(line)
}

/// Check if a line is a Kramdown options directive
pub fn is_kramdown_options(line: &str) -> bool {
    OPTIONS_PATTERN.is_match(line)
}

/// Check if a line starts a math block
pub fn is_math_block_delimiter(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "$$" || MATH_BLOCK_PATTERN.is_match(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kramdown_class_attributes() {
        assert!(is_kramdown_block_attribute("{:.wrap}"));
        assert!(is_kramdown_block_attribute("{:.class-name}"));
        assert!(is_kramdown_block_attribute("{:.multiple .classes}"));
    }

    #[test]
    fn test_kramdown_id_attributes() {
        assert!(is_kramdown_block_attribute("{:#my-id}"));
        assert!(is_kramdown_block_attribute("{:#section-1}"));
    }

    #[test]
    fn test_kramdown_generic_attributes() {
        assert!(is_kramdown_block_attribute("{:style=\"color: red\"}"));
        assert!(is_kramdown_block_attribute("{:data-value=\"123\"}"));
    }

    #[test]
    fn test_kramdown_combined_attributes() {
        assert!(is_kramdown_block_attribute("{:.class #id}"));
        assert!(is_kramdown_block_attribute("{:#id .class style=\"color: blue\"}"));
        assert!(is_kramdown_block_attribute("{:.wrap #my-code .highlight}"));
    }

    #[test]
    fn test_non_kramdown_braces() {
        assert!(!is_kramdown_block_attribute("{just some text}"));
        assert!(!is_kramdown_block_attribute("{not kramdown}"));
        assert!(!is_kramdown_block_attribute("{ spaces }"));
    }

    #[test]
    fn test_edge_cases() {
        assert!(!is_kramdown_block_attribute("{}"));
        assert!(!is_kramdown_block_attribute("{"));
        assert!(!is_kramdown_block_attribute("}"));
        assert!(!is_kramdown_block_attribute(""));
        assert!(!is_kramdown_block_attribute("not braces"));
    }

    #[test]
    fn test_whitespace_handling() {
        assert!(is_kramdown_block_attribute("  {:.wrap}  "));
        assert!(is_kramdown_block_attribute("\t{:#id}\t"));
        assert!(is_kramdown_block_attribute(" {:.class #id} "));
    }
}
