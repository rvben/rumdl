//! Utilities for handling Kramdown-specific syntax
//!
//! Kramdown is a superset of Markdown that adds additional features like
//! Inline Attribute Lists (IAL) for adding attributes to elements.

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
