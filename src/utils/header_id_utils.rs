//! Utilities for extracting custom header IDs from various Markdown flavors
//!
//! This module supports multiple syntax formats for custom header IDs:
//!
//! ## Kramdown Format
//! - `{#custom-id}` - Simple ID without colon
//! - Example: `# Header {#my-id}`
//!
//! ## Python-markdown attr-list Format
//! - `{:#custom-id}` - ID with colon, no spaces
//! - `{: #custom-id}` - ID with colon and spaces
//! - `{: #custom-id .class}` - ID with classes
//! - `{: #custom-id .class data="value"}` - ID with full attributes
//! - Example: `# Header {: #my-id .highlight}`
//!
//! ## Position Support
//! - Inline: `# Header {#id}` (all formats)
//! - Next-line: Jekyll/kramdown style where attr-list appears on the line after the header
//!   ```markdown
//!   # Header
//!   {#next-line-id}
//!   ```
//!
//! The module provides functions to detect and extract IDs from both inline
//! and standalone (next-line) attr-list syntax.

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Pattern for custom header IDs supporting both kramdown and python-markdown attr-list formats
    /// Supports: {#id}, { #id }, {:#id}, {: #id } and full attr-list with classes/attributes
    /// Must contain #id but can have other attributes: {: #id .class data="value" }
    /// More conservative: only matches when there's actually a hash followed by valid ID characters
    static ref HEADER_ID_PATTERN: Regex = Regex::new(r"\s*\{\s*:?\s*([^}]*?#[^}]*?)\s*\}\s*$").unwrap();

    /// Pattern to extract and validate ID from attr-list content
    /// Finds #id and validates it contains only valid characters (no dots, etc.)
    static ref ID_EXTRACT_PATTERN: Regex = Regex::new(r"#([a-zA-Z0-9_\-:]+)(?:\s|$|[^a-zA-Z0-9_\-:])").unwrap();

    /// Pattern to validate that an ID contains only valid characters
    static ref ID_VALIDATE_PATTERN: Regex = Regex::new(r"^[a-zA-Z0-9_\-:]+$").unwrap();

    /// Pattern for standalone attr-list lines (Jekyll/kramdown style on line after heading)
    /// Matches lines that are just attr-list syntax: {#id}, {: #id .class }, etc.
    static ref STANDALONE_ATTR_LIST_PATTERN: Regex = Regex::new(r"^\s*\{\s*:?\s*([^}]*#[a-zA-Z0-9_\-:]+[^}]*)\s*\}\s*$").unwrap();
}

/// Extract custom header ID from a line if present, returning clean text and ID
///
/// Supports multiple formats:
/// - Kramdown: `{#id}`
/// - Python-markdown: `{:#id}`, `{: #id}`, `{: #id .class}`
///
/// # Examples
/// ```
/// use rumdl_lib::utils::header_id_utils::extract_header_id;
///
/// // Kramdown format
/// let (text, id) = extract_header_id("# Header {#custom-id}");
/// assert_eq!(text, "# Header");
/// assert_eq!(id, Some("custom-id".to_string()));
///
/// // Python-markdown attr-list format
/// let (text, id) = extract_header_id("# Header {: #my-id .highlight}");
/// assert_eq!(text, "# Header");
/// assert_eq!(id, Some("my-id".to_string()));
/// ```
pub fn extract_header_id(line: &str) -> (String, Option<String>) {
    if let Some(captures) = HEADER_ID_PATTERN.captures(line)
        && let Some(full_match) = captures.get(0)
        && let Some(attr_content) = captures.get(1)
    {
        let attr_str = attr_content.as_str().trim();

        // First, find all potential ID matches in the attr-list
        if let Some(hash_pos) = attr_str.find('#') {
            // Extract everything after the hash
            let after_hash = &attr_str[hash_pos + 1..];

            // For simple cases like {#id}, the ID goes to the end
            // For complex cases like {: #id .class}, we need to find where the ID ends

            // First check if this looks like a simple kramdown ID: {#id} with no spaces or attributes
            let is_simple_format = !attr_str.contains(' ') && !attr_str.contains('=') && attr_str.starts_with('#');

            if is_simple_format {
                // Simple format: entire content after # should be the ID
                let potential_id = after_hash;
                if ID_VALIDATE_PATTERN.is_match(potential_id) && !potential_id.is_empty() {
                    let clean_text = line[..full_match.start()].trim_end().to_string();
                    return (clean_text, Some(potential_id.to_string()));
                }
                // If validation fails, reject the entire attr-list
            } else {
                // Complex format: find proper delimiters (space for next attribute, dot for class)
                if let Some(delimiter_pos) = after_hash.find(|c: char| c.is_whitespace() || c == '.' || c == '=') {
                    let potential_id = &after_hash[..delimiter_pos];
                    if ID_VALIDATE_PATTERN.is_match(potential_id) && !potential_id.is_empty() {
                        let clean_text = line[..full_match.start()].trim_end().to_string();
                        return (clean_text, Some(potential_id.to_string()));
                    }
                } else {
                    // No delimiter found in complex format, ID goes to end
                    let potential_id = after_hash;
                    if ID_VALIDATE_PATTERN.is_match(potential_id) && !potential_id.is_empty() {
                        let clean_text = line[..full_match.start()].trim_end().to_string();
                        return (clean_text, Some(potential_id.to_string()));
                    }
                }
            }
        }
    }
    (line.to_string(), None)
}

/// Check if a line is a standalone attr-list (Jekyll/kramdown style)
///
/// This detects attr-list syntax that appears on its own line, typically
/// the line after a header to provide additional attributes.
///
/// # Examples
/// ```
/// use rumdl_lib::utils::header_id_utils::is_standalone_attr_list;
///
/// assert!(is_standalone_attr_list("{#custom-id}"));
/// assert!(is_standalone_attr_list("{: #spaced .class }"));
/// assert!(!is_standalone_attr_list("Some text {#not-standalone}"));
/// assert!(!is_standalone_attr_list(""));
/// ```
pub fn is_standalone_attr_list(line: &str) -> bool {
    STANDALONE_ATTR_LIST_PATTERN.is_match(line)
}

/// Extract ID from a standalone attr-list line
///
/// Returns the ID if the line is a valid standalone attr-list with an ID.
///
/// # Examples
/// ```
/// use rumdl_lib::utils::header_id_utils::extract_standalone_attr_list_id;
///
/// assert_eq!(extract_standalone_attr_list_id("{#custom-id}"), Some("custom-id".to_string()));
/// assert_eq!(extract_standalone_attr_list_id("{: #spaced .class }"), Some("spaced".to_string()));
/// assert_eq!(extract_standalone_attr_list_id("not an attr-list"), None);
/// ```
pub fn extract_standalone_attr_list_id(line: &str) -> Option<String> {
    if let Some(captures) = STANDALONE_ATTR_LIST_PATTERN.captures(line)
        && let Some(attr_content) = captures.get(1)
    {
        let attr_str = attr_content.as_str().trim();

        // Use the same logic as extract_header_id for consistency
        if let Some(hash_pos) = attr_str.find('#') {
            let after_hash = &attr_str[hash_pos + 1..];

            // Check if this looks like a simple kramdown ID: {#id} with no spaces or attributes
            let is_simple_format = !attr_str.contains(' ') && !attr_str.contains('=') && attr_str.starts_with('#');

            if is_simple_format {
                // Simple format: entire content after # should be the ID
                let potential_id = after_hash;
                if ID_VALIDATE_PATTERN.is_match(potential_id) && !potential_id.is_empty() {
                    return Some(potential_id.to_string());
                }
            } else {
                // Complex format: find proper delimiters (space for next attribute, dot for class)
                if let Some(delimiter_pos) = after_hash.find(|c: char| c.is_whitespace() || c == '.' || c == '=') {
                    let potential_id = &after_hash[..delimiter_pos];
                    if ID_VALIDATE_PATTERN.is_match(potential_id) && !potential_id.is_empty() {
                        return Some(potential_id.to_string());
                    }
                } else {
                    // No delimiter found in complex format, ID goes to end
                    let potential_id = after_hash;
                    if ID_VALIDATE_PATTERN.is_match(potential_id) && !potential_id.is_empty() {
                        return Some(potential_id.to_string());
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kramdown_format_extraction() {
        // Simple kramdown format
        let (text, id) = extract_header_id("# Header {#simple}");
        assert_eq!(text, "# Header");
        assert_eq!(id, Some("simple".to_string()));

        let (text, id) = extract_header_id("## Section {#section-id}");
        assert_eq!(text, "## Section");
        assert_eq!(id, Some("section-id".to_string()));
    }

    #[test]
    fn test_python_markdown_attr_list_extraction() {
        // Python-markdown formats
        let (text, id) = extract_header_id("# Header {:#colon-id}");
        assert_eq!(text, "# Header");
        assert_eq!(id, Some("colon-id".to_string()));

        let (text, id) = extract_header_id("# Header {: #spaced-id }");
        assert_eq!(text, "# Header");
        assert_eq!(id, Some("spaced-id".to_string()));
    }

    #[test]
    fn test_extended_attr_list_extraction() {
        // ID with single class
        let (text, id) = extract_header_id("# Header {: #with-class .highlight }");
        assert_eq!(text, "# Header");
        assert_eq!(id, Some("with-class".to_string()));

        // ID with multiple classes
        let (text, id) = extract_header_id("## Section {: #multi .class1 .class2 }");
        assert_eq!(text, "## Section");
        assert_eq!(id, Some("multi".to_string()));

        // ID with key-value attributes
        let (text, id) = extract_header_id("### Subsection {: #with-attrs data-test=\"value\" style=\"color: red\" }");
        assert_eq!(text, "### Subsection");
        assert_eq!(id, Some("with-attrs".to_string()));

        // Complex combination
        let (text, id) = extract_header_id("#### Complex {: #complex .highlight data-role=\"button\" title=\"Test\" }");
        assert_eq!(text, "#### Complex");
        assert_eq!(id, Some("complex".to_string()));

        // ID with quotes in attributes
        let (text, id) = extract_header_id("##### Quotes {: #quotes title=\"Has \\\"nested\\\" quotes\" }");
        assert_eq!(text, "##### Quotes");
        assert_eq!(id, Some("quotes".to_string()));
    }

    #[test]
    fn test_attr_list_detection_edge_cases() {
        // Attr-list without ID should not match
        let (text, id) = extract_header_id("# Header {: .class-only }");
        assert_eq!(text, "# Header {: .class-only }");
        assert_eq!(id, None);

        // Malformed attr-list should not match
        let (text, id) = extract_header_id("# Header { no-hash }");
        assert_eq!(text, "# Header { no-hash }");
        assert_eq!(id, None);

        // Empty ID should not match
        let (text, id) = extract_header_id("# Header {: # }");
        assert_eq!(text, "# Header {: # }");
        assert_eq!(id, None);

        // ID in middle (not at end) should not match
        let (text, id) = extract_header_id("# Header {: #middle } with more text");
        assert_eq!(text, "# Header {: #middle } with more text");
        assert_eq!(id, None);
    }

    #[test]
    fn test_standalone_attr_list_detection() {
        // Simple ID formats
        assert!(is_standalone_attr_list("{#custom-id}"));
        assert!(is_standalone_attr_list("{ #spaced-id }"));
        assert!(is_standalone_attr_list("{:#colon-id}"));
        assert!(is_standalone_attr_list("{: #full-format }"));

        // With classes and attributes
        assert!(is_standalone_attr_list("{: #with-class .highlight }"));
        assert!(is_standalone_attr_list("{: #multi .class1 .class2 }"));
        assert!(is_standalone_attr_list("{: #complex .highlight data-test=\"value\" }"));

        // Should not match
        assert!(!is_standalone_attr_list("Some text {#not-standalone}"));
        assert!(!is_standalone_attr_list("Text before {#id}"));
        assert!(!is_standalone_attr_list("{#id} text after"));
        assert!(!is_standalone_attr_list(""));
        assert!(!is_standalone_attr_list("   ")); // just spaces
        assert!(!is_standalone_attr_list("{: .class-only }")); // no ID
    }

    #[test]
    fn test_standalone_attr_list_id_extraction() {
        // Basic formats
        assert_eq!(extract_standalone_attr_list_id("{#simple}"), Some("simple".to_string()));
        assert_eq!(
            extract_standalone_attr_list_id("{ #spaced }"),
            Some("spaced".to_string())
        );
        assert_eq!(extract_standalone_attr_list_id("{:#colon}"), Some("colon".to_string()));
        assert_eq!(extract_standalone_attr_list_id("{: #full }"), Some("full".to_string()));

        // With additional attributes
        assert_eq!(
            extract_standalone_attr_list_id("{: #with-class .highlight }"),
            Some("with-class".to_string())
        );
        assert_eq!(
            extract_standalone_attr_list_id("{: #complex .class1 .class2 data=\"value\" }"),
            Some("complex".to_string())
        );

        // Should return None
        assert_eq!(extract_standalone_attr_list_id("Not an attr-list"), None);
        assert_eq!(extract_standalone_attr_list_id("Text {#not-standalone}"), None);
        assert_eq!(extract_standalone_attr_list_id("{: .class-only }"), None);
        assert_eq!(extract_standalone_attr_list_id(""), None);
    }

    #[test]
    fn test_backward_compatibility() {
        // Ensure all original kramdown formats still work
        let test_cases = vec![
            ("# Header {#a}", "# Header", Some("a".to_string())),
            ("# Header {#simple-id}", "# Header", Some("simple-id".to_string())),
            ("## Heading {#heading-2}", "## Heading", Some("heading-2".to_string())),
            (
                "### With-Hyphens {#with-hyphens}",
                "### With-Hyphens",
                Some("with-hyphens".to_string()),
            ),
        ];

        for (input, expected_text, expected_id) in test_cases {
            let (text, id) = extract_header_id(input);
            assert_eq!(text, expected_text, "Text mismatch for input: {input}");
            assert_eq!(id, expected_id, "ID mismatch for input: {input}");
        }
    }

    #[test]
    fn test_invalid_id_with_dots() {
        // IDs with dots should not be extracted (dots are not valid ID characters)
        let (text, id) = extract_header_id("## Another. {#id.with.dots}");
        assert_eq!(text, "## Another. {#id.with.dots}"); // Should not strip invalid ID
        assert_eq!(id, None); // Should not extract invalid ID

        // Test that only the part before the dot would be extracted if it was valid standalone
        // But since it's in an invalid format, the whole thing should be rejected
        let (text, id) = extract_header_id("## Another. {#id.more.dots}");
        assert_eq!(text, "## Another. {#id.more.dots}");
        assert_eq!(id, None);
    }
}
