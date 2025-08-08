//! Utilities for handling Kramdown-specific syntax
//!
//! Kramdown is a superset of Markdown that adds additional features like
//! Inline Attribute Lists (IAL) for adding attributes to elements.

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Pattern for Kramdown span IAL: text{:.class #id key="value"}
    static ref SPAN_IAL_PATTERN: Regex = Regex::new(r"\{[:\.#][^}]*\}$").unwrap();

    /// Pattern for Kramdown header IDs: # Header {#custom-id}
    static ref HEADER_ID_PATTERN: Regex = Regex::new(r"\s*\{#[a-zA-Z][\w\-:.]*\}\s*$").unwrap();

    /// Pattern for Kramdown extensions opening: {::comment}, {::nomarkdown}, etc.
    static ref EXTENSION_OPEN_PATTERN: Regex = Regex::new(r"^\s*\{::([a-z]+)(?:\s+[^}]*)?\}\s*$").unwrap();

    /// Pattern for Kramdown extensions closing: {:/comment}, {:/}, etc.
    static ref EXTENSION_CLOSE_PATTERN: Regex = Regex::new(r"^\s*\{:/([a-z]+)?\}\s*$").unwrap();

    /// Pattern for Kramdown options: {::options key="value" /}
    static ref OPTIONS_PATTERN: Regex = Regex::new(r"^\s*\{::options\s+[^}]+/\}\s*$").unwrap();

    /// Pattern for footnote references: [^footnote]
    static ref FOOTNOTE_REF_PATTERN: Regex = Regex::new(r"\[\^[a-zA-Z0-9_\-]+\]").unwrap();

    /// Pattern for footnote definitions: [^footnote]: definition
    static ref FOOTNOTE_DEF_PATTERN: Regex = Regex::new(r"^\[\^[a-zA-Z0-9_\-]+\]:").unwrap();

    /// Pattern for abbreviations: *[HTML]: HyperText Markup Language
    static ref ABBREVIATION_PATTERN: Regex = Regex::new(r"^\*\[[^\]]+\]:").unwrap();

    /// Pattern for math blocks: $$ or $
    static ref MATH_BLOCK_PATTERN: Regex = Regex::new(r"^\$\$").unwrap();
    static ref MATH_INLINE_PATTERN: Regex = Regex::new(r"\$[^$]+\$").unwrap();
}

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
/// use rumdl::utils::kramdown_utils::is_kramdown_block_attribute;
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
/// use rumdl::utils::kramdown_utils::has_span_ial;
///
/// assert!(has_span_ial("*emphasized*{:.highlight}"));
/// assert!(has_span_ial("[link](url){:target=\"_blank\"}"));
/// assert!(!has_span_ial("regular text"));
/// ```
pub fn has_span_ial(text: &str) -> bool {
    SPAN_IAL_PATTERN.is_match(text.trim())
}

/// Remove span IAL from text if present
pub fn remove_span_ial(text: &str) -> &str {
    if let Some(captures) = SPAN_IAL_PATTERN.find(text) {
        &text[..captures.start()]
    } else {
        text
    }
}

/// Check if a heading line has a Kramdown header ID
///
/// # Examples
/// ```
/// use rumdl::utils::kramdown_utils::has_header_id;
///
/// assert!(has_header_id("# Header {#custom-id}"));
/// assert!(has_header_id("## Section {#section-1}"));
/// assert!(!has_header_id("# Regular Header"));
/// ```
pub fn has_header_id(line: &str) -> bool {
    HEADER_ID_PATTERN.is_match(line)
}

/// Remove header ID from a heading line if present
pub fn remove_header_id(line: &str) -> String {
    HEADER_ID_PATTERN.replace(line, "").to_string()
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

/// Check if a line is a Kramdown extension (any type)
pub fn is_kramdown_extension(line: &str) -> bool {
    is_kramdown_extension_open(line) || is_kramdown_extension_close(line) || is_kramdown_options(line)
}

/// Check if a line is an End-of-Block (EOB) marker
///
/// In Kramdown, a line containing only `^` ends the current block
pub fn is_eob_marker(line: &str) -> bool {
    line.trim() == "^"
}

/// Check if text contains a footnote reference
pub fn has_footnote_reference(text: &str) -> bool {
    FOOTNOTE_REF_PATTERN.is_match(text)
}

/// Check if a line is a footnote definition
pub fn is_footnote_definition(line: &str) -> bool {
    FOOTNOTE_DEF_PATTERN.is_match(line.trim_start())
}

/// Check if a line is an abbreviation definition
pub fn is_abbreviation_definition(line: &str) -> bool {
    ABBREVIATION_PATTERN.is_match(line.trim_start())
}

/// Check if a line starts a math block
pub fn is_math_block_delimiter(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "$$" || MATH_BLOCK_PATTERN.is_match(trimmed)
}

/// Check if text contains inline math
pub fn has_inline_math(text: &str) -> bool {
    MATH_INLINE_PATTERN.is_match(text)
}

/// Check if a line is a definition list item
///
/// Definition lists in Kramdown use the pattern:
/// ```
/// Term
/// : Definition
/// ```
pub fn is_definition_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with(": ")
        || (trimmed.starts_with(':') && trimmed.len() > 1 && trimmed.chars().nth(1).is_some_and(|c| c.is_whitespace()))
}

/// Check if a line contains any Kramdown-specific syntax
pub fn has_kramdown_syntax(line: &str) -> bool {
    is_kramdown_block_attribute(line)
        || has_span_ial(line)
        || has_header_id(line)
        || is_kramdown_extension(line)
        || is_eob_marker(line)
        || is_footnote_definition(line)
        || is_abbreviation_definition(line)
        || is_math_block_delimiter(line)
        || is_definition_list_item(line)
        || has_footnote_reference(line)
        || has_inline_math(line)
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
