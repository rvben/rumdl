use super::mkdocs_common::{BytePositionTracker, ContextStateMachine, MKDOCS_CONTENT_INDENT, get_line_indent};
use regex::Regex;
/// MkDocs Footnotes detection utilities
///
/// The Footnotes extension provides support for footnotes with references
/// and definitions using `[^ref]` syntax.
///
/// Common patterns:
/// - `[^1]` - Footnote reference
/// - `[^note]` - Named footnote reference
/// - `[^1]: Definition` - Footnote definition
/// - Multi-line footnote definitions with 4-space indentation
use std::sync::LazyLock;

/// Pattern to match footnote references in text `[^1]` or `[^name]`
static FOOTNOTE_REF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\^[a-zA-Z0-9_-]+\]").unwrap());

/// Pattern to match footnote definitions at start of line
/// `[^1]: Definition text`
/// Lenient: accepts empty definitions for real-world markdown
static FOOTNOTE_DEF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(\s*)\[\^([a-zA-Z0-9_-]+)\]:\s*", // \s* instead of \s+ to allow empty
    )
    .unwrap()
});

/// Check if a line contains a footnote definition
pub fn is_footnote_definition(line: &str) -> bool {
    FOOTNOTE_DEF.is_match(line)
}

/// Check if a line contains any footnote references
pub fn contains_footnote_reference(line: &str) -> bool {
    FOOTNOTE_REF.is_match(line)
}

/// Get the indentation level of a footnote definition
pub fn get_footnote_indent(line: &str) -> Option<usize> {
    if FOOTNOTE_DEF.is_match(line) {
        // Use consistent indentation calculation (tabs = 4 spaces)
        return Some(get_line_indent(line));
    }
    None
}

/// Check if a line is part of a multi-line footnote definition
pub fn is_footnote_continuation(line: &str, base_indent: usize) -> bool {
    // Empty lines within footnotes are allowed
    if line.trim().is_empty() {
        return true;
    }

    // Content must be indented at least MKDOCS_CONTENT_INDENT spaces from the footnote definition
    get_line_indent(line) >= base_indent + MKDOCS_CONTENT_INDENT
}

/// Check if content at a byte position is within a footnote definition
pub fn is_within_footnote_definition(content: &str, position: usize) -> bool {
    let tracker = BytePositionTracker::new(content);
    let mut state = ContextStateMachine::new();

    for (_idx, line, start, end) in tracker.iter_with_positions() {
        // Check if we're starting a footnote definition
        if is_footnote_definition(line) {
            let indent = get_footnote_indent(line).unwrap_or(0);
            state.enter_context(indent, "footnote".to_string());
        } else if state.is_in_context() {
            // Check if we're still in the footnote
            if !line.trim().is_empty() && !is_footnote_continuation(line, state.context_indent()) {
                // Non-empty line that's not properly indented ends the footnote
                state.exit_context();

                // Check if this line starts a new footnote
                if is_footnote_definition(line) {
                    let indent = get_footnote_indent(line).unwrap_or(0);
                    state.enter_context(indent, "footnote".to_string());
                }
            }
        }

        // Check if the position is within this line and we're in a footnote
        if start <= position && position <= end && state.is_in_context() {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_footnote_definition_detection() {
        assert!(is_footnote_definition("[^1]: This is a footnote"));
        assert!(is_footnote_definition("[^note]: Named footnote"));
        assert!(is_footnote_definition("  [^2]: Indented footnote"));
        assert!(!is_footnote_definition("[^1] Reference in text"));
        assert!(!is_footnote_definition("Regular text"));
    }

    #[test]
    fn test_footnote_reference_detection() {
        assert!(contains_footnote_reference("Text with [^1] reference"));
        assert!(contains_footnote_reference("Multiple [^1] and [^2] refs"));
        assert!(contains_footnote_reference("[^named-ref]"));
        assert!(!contains_footnote_reference("No references here"));
    }

    #[test]
    fn test_footnote_continuation() {
        assert!(is_footnote_continuation("    Continued content", 0));
        assert!(is_footnote_continuation("        More indented", 0));
        assert!(is_footnote_continuation("", 0)); // Empty lines allowed
        assert!(!is_footnote_continuation("Not indented enough", 0));
        assert!(!is_footnote_continuation("  Only 2 spaces", 0));
    }

    #[test]
    fn test_within_footnote_definition() {
        let content = r#"Regular text here.

[^1]: This is a footnote definition
    with multiple lines
    of content.

More regular text.

[^2]: Another footnote
    Also multi-line.

End text."#;

        let def_pos = content.find("footnote definition").unwrap();
        let multi_pos = content.find("with multiple").unwrap();
        let regular_pos = content.find("More regular").unwrap();
        let end_pos = content.find("End text").unwrap();

        assert!(is_within_footnote_definition(content, def_pos));
        assert!(is_within_footnote_definition(content, multi_pos));
        assert!(!is_within_footnote_definition(content, regular_pos));
        assert!(!is_within_footnote_definition(content, end_pos));
    }
}
