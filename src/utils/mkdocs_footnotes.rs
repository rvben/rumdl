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
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Pattern to match footnote references in text [^1] or [^name]
    static ref FOOTNOTE_REF: Regex = Regex::new(
        r"\[\^[a-zA-Z0-9_-]+\]"
    ).unwrap();

    /// Pattern to match footnote definitions at start of line
    /// [^1]: Definition text
    static ref FOOTNOTE_DEF: Regex = Regex::new(
        r"^(\s*)\[\^([a-zA-Z0-9_-]+)\]:\s+"
    ).unwrap();
}

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
    if let Some(caps) = FOOTNOTE_DEF.captures(line)
        && let Some(indent) = caps.get(1)
    {
        return Some(indent.as_str().len());
    }
    None
}

/// Check if a line is part of a multi-line footnote definition
pub fn is_footnote_continuation(line: &str, base_indent: usize) -> bool {
    // Continuation lines must be indented at least 4 spaces more than the definition
    let line_indent = line.chars().take_while(|&c| c == ' ' || c == '\t').count();

    // Empty lines within footnotes are allowed
    if line.trim().is_empty() {
        return true;
    }

    // Content must be indented at least 4 spaces from the footnote definition
    line_indent >= base_indent + 4
}

/// Check if content at a byte position is within a footnote definition
pub fn is_within_footnote_definition(content: &str, position: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    let mut byte_pos = 0;
    let mut in_footnote = false;
    let mut footnote_indent = 0;

    for line in lines {
        let line_end = byte_pos + line.len();

        // Check if we're starting a footnote definition
        if is_footnote_definition(line) {
            in_footnote = true;
            footnote_indent = get_footnote_indent(line).unwrap_or(0);
        } else if in_footnote && !line.trim().is_empty() && !is_footnote_continuation(line, footnote_indent) {
            // Non-empty line that's not properly indented ends the footnote
            in_footnote = false;
            footnote_indent = 0;

            // Check if this line starts a new footnote
            if is_footnote_definition(line) {
                in_footnote = true;
                footnote_indent = get_footnote_indent(line).unwrap_or(0);
            }
        }

        // Check if the position is within this line and we're in a footnote
        if byte_pos <= position && position <= line_end && in_footnote {
            return true;
        }

        // Account for newline character
        byte_pos = line_end + 1;
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
