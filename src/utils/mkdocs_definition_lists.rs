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
}
