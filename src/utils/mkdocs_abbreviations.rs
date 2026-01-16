/// MkDocs/Python-Markdown Abbreviations extension support
///
/// This module provides support for the Python-Markdown Abbreviations extension,
/// which allows defining abbreviations that get expanded with `<abbr>` tags.
///
/// ## Syntax
///
/// Abbreviation definitions appear at the end of the document:
/// ```markdown
/// The HTML specification is maintained by the W3C.
///
/// *[HTML]: Hypertext Markup Language
/// *[W3C]: World Wide Web Consortium
/// ```
///
/// When rendered, each occurrence of HTML and W3C in the document text
/// gets wrapped in an `<abbr>` tag with a `title` attribute.
///
/// ## Format Requirements
///
/// - Must start with `*[` followed by the abbreviation
/// - Abbreviation is closed with `]:`
/// - Definition follows after the colon, optionally with whitespace
/// - Typically placed at the end of the document (but can appear anywhere)
///
/// ## References
///
/// - [Python-Markdown Abbreviations](https://python-markdown.github.io/extensions/abbreviations/)
/// - [MkDocs Material - Abbreviations](https://squidfunk.github.io/mkdocs-material/reference/tooltips/#adding-abbreviations)
use regex::Regex;
use std::sync::LazyLock;

/// Pattern to match abbreviation definitions: `*[ABBR]: Definition`
/// Supports:
/// - Simple abbreviations: `*[HTML]: Hypertext Markup Language`
/// - Multi-word abbreviations: `*[W3C]: World Wide Web Consortium`
/// - Abbreviations with numbers: `*[CSS3]: Cascading Style Sheets Level 3`
static ABBREVIATION_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\*\[([^\]]+)\]:\s*(.*)$").unwrap());

/// Parsed abbreviation definition
#[derive(Debug, Clone, PartialEq)]
pub struct Abbreviation {
    /// The abbreviation text (e.g., "HTML")
    pub abbr: String,
    /// The definition/expansion (e.g., "Hypertext Markup Language")
    pub definition: String,
    /// Line number where defined (1-indexed)
    pub line: usize,
}

/// Check if a line is an abbreviation definition
#[inline]
pub fn is_abbreviation_definition(line: &str) -> bool {
    // Fast path: check for distinctive prefix
    if !line.trim_start().starts_with("*[") {
        return false;
    }
    ABBREVIATION_PATTERN.is_match(line)
}

/// Check if a line might be an abbreviation definition (fast check)
#[inline]
pub fn might_be_abbreviation(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("*[") && trimmed.contains("]:")
}

/// Parse an abbreviation definition from a line
///
/// # Returns
/// Some(Abbreviation) if the line is a valid abbreviation definition, None otherwise
///
/// # Examples
/// ```
/// use rumdl_lib::utils::mkdocs_abbreviations::parse_abbreviation;
///
/// let abbr = parse_abbreviation("*[HTML]: Hypertext Markup Language", 1);
/// assert!(abbr.is_some());
/// let abbr = abbr.unwrap();
/// assert_eq!(abbr.abbr, "HTML");
/// assert_eq!(abbr.definition, "Hypertext Markup Language");
/// ```
pub fn parse_abbreviation(line: &str, line_num: usize) -> Option<Abbreviation> {
    if let Some(caps) = ABBREVIATION_PATTERN.captures(line) {
        let abbr = caps.get(1)?.as_str().to_string();
        let definition = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();

        Some(Abbreviation {
            abbr,
            definition,
            line: line_num,
        })
    } else {
        None
    }
}

/// Extract all abbreviation definitions from content
///
/// # Returns
/// A vector of Abbreviation structs for each definition found
pub fn extract_abbreviations(content: &str) -> Vec<Abbreviation> {
    let mut abbreviations = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        if let Some(abbr) = parse_abbreviation(line, line_idx + 1) {
            abbreviations.push(abbr);
        }
    }

    abbreviations
}

/// Check if a position in a line is within an abbreviation definition
pub fn is_in_abbreviation_definition(line: &str, position: usize) -> bool {
    // If the line is an abbreviation definition, all positions are within it
    if is_abbreviation_definition(line) {
        return position < line.len();
    }
    false
}

/// Get all abbreviation terms from content (just the abbreviation part, not definitions)
///
/// # Returns
/// A vector of abbreviation terms (e.g., ["HTML", "CSS", "W3C"])
pub fn get_abbreviation_terms(content: &str) -> Vec<String> {
    extract_abbreviations(content).into_iter().map(|a| a.abbr).collect()
}

/// Check if a word in content matches a defined abbreviation
///
/// This is useful for rules like MD013 that need to know if a word
/// should be treated specially because it's a defined abbreviation.
pub fn is_defined_abbreviation(content: &str, word: &str) -> bool {
    for line in content.lines() {
        if let Some(abbr) = parse_abbreviation(line, 0)
            && abbr.abbr == word
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_abbreviation_definition() {
        // Valid abbreviation definitions
        assert!(is_abbreviation_definition("*[HTML]: Hypertext Markup Language"));
        assert!(is_abbreviation_definition("*[CSS]: Cascading Style Sheets"));
        assert!(is_abbreviation_definition("*[W3C]: World Wide Web Consortium"));
        assert!(is_abbreviation_definition("*[CSS3]: CSS Level 3"));
        assert!(is_abbreviation_definition("*[abbr]: definition"));

        // Empty definition is valid
        assert!(is_abbreviation_definition("*[HTML]:"));
        assert!(is_abbreviation_definition("*[HTML]: "));

        // Invalid patterns
        assert!(!is_abbreviation_definition("# Heading"));
        assert!(!is_abbreviation_definition("Regular text"));
        assert!(!is_abbreviation_definition("[HTML]: Not an abbr"));
        assert!(!is_abbreviation_definition("*HTML: Not an abbr"));
        assert!(!is_abbreviation_definition("*[HTML] Not an abbr"));
    }

    #[test]
    fn test_parse_abbreviation() {
        let abbr = parse_abbreviation("*[HTML]: Hypertext Markup Language", 1);
        assert!(abbr.is_some());
        let abbr = abbr.unwrap();
        assert_eq!(abbr.abbr, "HTML");
        assert_eq!(abbr.definition, "Hypertext Markup Language");
        assert_eq!(abbr.line, 1);

        let abbr = parse_abbreviation("*[CSS3]: CSS Level 3", 5);
        assert!(abbr.is_some());
        let abbr = abbr.unwrap();
        assert_eq!(abbr.abbr, "CSS3");
        assert_eq!(abbr.definition, "CSS Level 3");
        assert_eq!(abbr.line, 5);

        let abbr = parse_abbreviation("Not an abbreviation", 1);
        assert!(abbr.is_none());
    }

    #[test]
    fn test_extract_abbreviations() {
        let content = r#"# Document

The HTML specification is maintained by the W3C.

CSS is used for styling.

*[HTML]: Hypertext Markup Language
*[W3C]: World Wide Web Consortium
*[CSS]: Cascading Style Sheets
"#;
        let abbreviations = extract_abbreviations(content);
        assert_eq!(abbreviations.len(), 3);

        assert_eq!(abbreviations[0].abbr, "HTML");
        assert_eq!(abbreviations[0].definition, "Hypertext Markup Language");

        assert_eq!(abbreviations[1].abbr, "W3C");
        assert_eq!(abbreviations[1].definition, "World Wide Web Consortium");

        assert_eq!(abbreviations[2].abbr, "CSS");
        assert_eq!(abbreviations[2].definition, "Cascading Style Sheets");
    }

    #[test]
    fn test_is_defined_abbreviation() {
        let content = r#"Some text.

*[HTML]: Hypertext Markup Language
*[CSS]: Cascading Style Sheets
"#;
        assert!(is_defined_abbreviation(content, "HTML"));
        assert!(is_defined_abbreviation(content, "CSS"));
        assert!(!is_defined_abbreviation(content, "W3C"));
        assert!(!is_defined_abbreviation(content, "html")); // Case-sensitive
    }

    #[test]
    fn test_get_abbreviation_terms() {
        let content = r#"Text here.

*[HTML]: Hypertext Markup Language
*[CSS]: Cascading Style Sheets
*[W3C]: World Wide Web Consortium
"#;
        let terms = get_abbreviation_terms(content);
        assert_eq!(terms, vec!["HTML", "CSS", "W3C"]);
    }

    #[test]
    fn test_might_be_abbreviation() {
        assert!(might_be_abbreviation("*[HTML]: Definition"));
        assert!(might_be_abbreviation("  *[HTML]: Definition")); // With leading spaces
        assert!(!might_be_abbreviation("*HTML: Not abbr"));
        assert!(!might_be_abbreviation("[HTML]: Not abbr"));
        assert!(!might_be_abbreviation("Regular text"));
    }

    #[test]
    fn test_abbreviation_with_special_characters() {
        // Abbreviations can contain various characters
        let abbr = parse_abbreviation("*[C++]: C Plus Plus", 1);
        assert!(abbr.is_some());
        let abbr = abbr.unwrap();
        assert_eq!(abbr.abbr, "C++");

        let abbr = parse_abbreviation("*[.NET]: Dot NET Framework", 1);
        assert!(abbr.is_some());
        let abbr = abbr.unwrap();
        assert_eq!(abbr.abbr, ".NET");
    }

    #[test]
    fn test_multi_word_definitions() {
        let abbr = parse_abbreviation("*[API]: Application Programming Interface", 1);
        assert!(abbr.is_some());
        let abbr = abbr.unwrap();
        assert_eq!(abbr.definition, "Application Programming Interface");
    }

    #[test]
    fn test_empty_definition() {
        let abbr = parse_abbreviation("*[HTML]:", 1);
        assert!(abbr.is_some());
        let abbr = abbr.unwrap();
        assert_eq!(abbr.abbr, "HTML");
        assert_eq!(abbr.definition, "");
    }
}
