//! MkDocs/Python-Markdown Definition Lists extension support
//!
//! This module provides support for the Python-Markdown Definition Lists extension,
//! which allows creating definition lists with terms and their definitions.
//!
//! ## Syntax
//!
//! ```markdown
//! Term 1
//! :   Definition for term 1
//!
//! Term 2
//! :   Definition for term 2
//!     Continuation of the definition
//!
//! Term with multiple definitions
//! :   First definition
//! :   Second definition
//! ```
//!
//! ## Format Requirements
//!
//! - Term appears on its own line (no leading whitespace required for the term)
//! - Definition starts with `:` followed by whitespace (typically 3 spaces after `:`)
//! - Multiple definitions for a term use separate `:` lines
//! - Continuation lines are indented (typically 4 spaces)
//!
//! ## MkDocs Material Specifics
//!
//! MkDocs Material supports definition lists via the Python-Markdown Definition Lists extension,
//! which is enabled by default in the Material theme.
//!
//! ## References
//!
//! - [Python-Markdown Definition Lists](https://python-markdown.github.io/extensions/definition_lists/)
//! - [MkDocs Material - Lists](https://squidfunk.github.io/mkdocs-material/reference/lists/#using-definition-lists)

/// Check if a line is a definition (starts with `:` followed by whitespace)
///
/// Reexported from utils::mod.rs for compatibility
#[inline]
pub fn is_definition_line(line: &str) -> bool {
    crate::utils::is_definition_list_item(line)
}

/// Check if a line could be a term (precedes a definition)
///
/// A term line is a non-empty line that:
/// - Doesn't start with whitespace (or has consistent indentation for nested terms)
/// - Is followed by a definition line (checked by caller)
/// - Is not a definition line itself
/// - Is not a blank line
#[inline]
pub fn could_be_term_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && !is_definition_line(line) && !line.starts_with(' ')
}

/// Check if a line is a definition continuation (indented after a definition)
///
/// Continuation lines are typically indented 4 spaces
#[inline]
pub fn is_definition_continuation(line: &str) -> bool {
    // Continuation is indented (at least 4 spaces) and not a new definition
    line.starts_with("    ") && !line.trim_start().starts_with(':')
}

/// Parsed definition list entry
#[derive(Debug, Clone, PartialEq)]
pub struct DefinitionEntry {
    /// The term being defined
    pub term: String,
    /// Line number of the term (1-indexed)
    pub term_line: usize,
    /// List of definitions (each definition may span multiple lines)
    pub definitions: Vec<Definition>,
}

/// A single definition within a definition list entry
#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    /// The definition text (may include continuation lines joined)
    pub text: String,
    /// Line number where this definition starts (1-indexed)
    pub line: usize,
}

/// Extract all definition list entries from content
///
/// # Returns
/// A vector of DefinitionEntry structs for each term+definitions found
pub fn extract_definition_lists(content: &str) -> Vec<DefinitionEntry> {
    let lines: Vec<&str> = content.lines().collect();
    let mut entries = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        // Look for a potential term (non-definition, non-blank line)
        let line = lines[i];
        let trimmed = line.trim();

        // Skip blank lines and definition lines
        if trimmed.is_empty() || is_definition_line(line) {
            i += 1;
            continue;
        }

        // Check if next line is a definition
        if i + 1 < lines.len() && is_definition_line(lines[i + 1]) {
            // Found a term with at least one definition
            let term = trimmed.to_string();
            let term_line = i + 1;
            let mut definitions = Vec::new();

            // Collect all definitions for this term
            i += 1;
            while i < lines.len() && is_definition_line(lines[i]) {
                let def_start_line = i + 1;
                let def_line = lines[i].trim_start();
                // Remove the `:` prefix and any following whitespace
                // Common formats: `:   text`, `: text`, `:\ttext`
                let def_text = if let Some(stripped) = def_line.strip_prefix(':') {
                    stripped.trim_start().to_string()
                } else {
                    def_line.to_string()
                };

                // Check for continuation lines
                let mut full_def = def_text;
                while i + 1 < lines.len() && is_definition_continuation(lines[i + 1]) {
                    i += 1;
                    let continuation = lines[i].trim();
                    if !continuation.is_empty() {
                        full_def.push(' ');
                        full_def.push_str(continuation);
                    }
                }

                definitions.push(Definition {
                    text: full_def,
                    line: def_start_line,
                });

                i += 1;
            }

            entries.push(DefinitionEntry {
                term,
                term_line,
                definitions,
            });
        } else {
            i += 1;
        }
    }

    entries
}

/// Check if a position in a line is within a definition marker
pub fn is_in_definition_marker(line: &str, position: usize) -> bool {
    if !is_definition_line(line) {
        return false;
    }

    // The definition marker is the `:` at the beginning (after any leading whitespace)
    let trimmed = line.trim_start();
    let leading_ws = line.len() - trimmed.len();

    // Position is in the marker if it's at the `:` or the space after
    position >= leading_ws && position < leading_ws + 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_definition_line() {
        assert!(is_definition_line(":   Definition text"));
        assert!(is_definition_line(": Definition text"));
        assert!(is_definition_line(":\tDefinition text"));
        assert!(is_definition_line(":    Long definition"));

        assert!(!is_definition_line("Term"));
        assert!(!is_definition_line("  Term"));
        assert!(!is_definition_line(""));
        assert!(!is_definition_line(":NoSpace")); // No space after colon
    }

    #[test]
    fn test_could_be_term_line() {
        assert!(could_be_term_line("Term"));
        assert!(could_be_term_line("Multi Word Term"));
        assert!(could_be_term_line("Term with special chars: like this"));

        assert!(!could_be_term_line("")); // Empty
        assert!(!could_be_term_line("   ")); // Blank
        assert!(!could_be_term_line(":   Definition")); // Definition
        assert!(!could_be_term_line(" Term")); // Leading space
    }

    #[test]
    fn test_is_definition_continuation() {
        assert!(is_definition_continuation("    Continuation text"));
        assert!(is_definition_continuation("    More continuation"));

        assert!(!is_definition_continuation(":   New definition"));
        assert!(!is_definition_continuation("No indent"));
        assert!(!is_definition_continuation("  Only 2 spaces"));
    }

    #[test]
    fn test_extract_definition_lists() {
        let content = r#"First Term
:   Definition of first term

Second Term
:   Definition of second term
    With continuation

Third Term
:   First definition
:   Second definition
"#;
        let entries = extract_definition_lists(content);

        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].term, "First Term");
        assert_eq!(entries[0].definitions.len(), 1);
        assert_eq!(entries[0].definitions[0].text, "Definition of first term");

        assert_eq!(entries[1].term, "Second Term");
        assert_eq!(entries[1].definitions.len(), 1);
        assert_eq!(
            entries[1].definitions[0].text,
            "Definition of second term With continuation"
        );

        assert_eq!(entries[2].term, "Third Term");
        assert_eq!(entries[2].definitions.len(), 2);
        assert_eq!(entries[2].definitions[0].text, "First definition");
        assert_eq!(entries[2].definitions[1].text, "Second definition");
    }

    #[test]
    fn test_extract_definition_lists_complex() {
        let content = r#"# Document

Regular paragraph.

Apple
:   A fruit
:   A technology company

Banana
:   A yellow fruit
    that grows in tropical climates

Not a definition list line.
"#;
        let entries = extract_definition_lists(content);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].term, "Apple");
        assert_eq!(entries[0].definitions.len(), 2);
        assert_eq!(entries[1].term, "Banana");
        assert_eq!(entries[1].definitions.len(), 1);
        assert!(entries[1].definitions[0].text.contains("tropical climates"));
    }

    #[test]
    fn test_is_in_definition_marker() {
        let line = ":   Definition text";
        assert!(is_in_definition_marker(line, 0)); // At ':'
        assert!(is_in_definition_marker(line, 1)); // At first space
        assert!(!is_in_definition_marker(line, 4)); // In text

        let line_with_ws = "  :   Definition";
        assert!(!is_in_definition_marker(line_with_ws, 0)); // Before ':'
        assert!(is_in_definition_marker(line_with_ws, 2)); // At ':'
        assert!(is_in_definition_marker(line_with_ws, 3)); // At first space after ':'

        let not_def = "Regular line";
        assert!(!is_in_definition_marker(not_def, 0));
    }
}
