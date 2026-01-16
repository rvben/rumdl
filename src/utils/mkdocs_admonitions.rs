use regex::Regex;
/// MkDocs Admonitions detection utilities
///
/// The Admonitions extension provides specially-styled content blocks for
/// notes, warnings, tips, and other callouts using `!!!` and `???` markers.
///
/// Common patterns:
/// - `!!! note "Title"` - Standard admonition
/// - `??? warning "Title"` - Collapsible admonition (closed by default)
/// - `???+ tip "Title"` - Collapsible admonition (open by default)
/// - `!!! note` - Admonition without title (uses type as title)
/// - `!!! type inline` - Inline admonition (left-aligned)
/// - `!!! type inline end` - Inline admonition (right-aligned)
use std::sync::LazyLock;

/// Pattern to match admonition start markers
/// Matches: !!! type, ??? type, ???+ type, with optional "title" and modifiers
/// Type must be alphanumeric with optional dashes/underscores (no special chars)
/// Lenient: accepts unclosed quotes for real-world markdown handling
static ADMONITION_START: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(\s*)(?:!!!|\?\?\?\+?)\s+([a-zA-Z][a-zA-Z0-9_-]*)(?:\s+(?:inline(?:\s+end)?))?.*$"#).unwrap()
});

/// Pattern to match just the admonition marker without capturing groups
static ADMONITION_MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)(?:!!!|\?\?\?\+?)\s+").unwrap());

/// Pattern to validate admonition type characters
static VALID_TYPE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$").unwrap());

// Common admonition types recognized by MkDocs
// Note: Any word is valid as a custom type, so this list is informational
// Types: note, abstract, info, tip, success, question, warning, failure, danger, bug, example, quote

/// Check if a line is an admonition start marker
pub fn is_admonition_start(line: &str) -> bool {
    // First check with the basic marker
    if !ADMONITION_MARKER.is_match(line) {
        return false;
    }

    // Extract and validate the type
    let trimmed = line.trim_start();
    let after_marker = if let Some(stripped) = trimmed.strip_prefix("!!!") {
        stripped
    } else if let Some(stripped) = trimmed.strip_prefix("???+") {
        stripped
    } else if let Some(stripped) = trimmed.strip_prefix("???") {
        stripped
    } else {
        return false;
    };

    let after_marker = after_marker.trim_start();
    if after_marker.is_empty() {
        return false;
    }

    // Extract the type (first word)
    let type_part = after_marker.split_whitespace().next().unwrap_or("");

    // Validate the type contains only allowed characters
    if !VALID_TYPE.is_match(type_part) {
        return false;
    }

    // Final check with the full regex
    ADMONITION_START.is_match(line)
}

/// Check if a line contains any admonition marker
pub fn is_admonition_marker(line: &str) -> bool {
    ADMONITION_MARKER.is_match(line)
}

/// Extract the indentation level of an admonition (for tracking nested content)
pub fn get_admonition_indent(line: &str) -> Option<usize> {
    if ADMONITION_START.is_match(line) {
        // Use consistent indentation calculation (tabs = 4 spaces)
        return Some(super::mkdocs_common::get_line_indent(line));
    }
    None
}

/// Check if a line is part of admonition content (based on indentation)
pub fn is_admonition_content(line: &str, base_indent: usize) -> bool {
    // Admonition content must be indented at least 4 spaces more than the marker
    let line_indent = super::mkdocs_common::get_line_indent(line);

    // Empty lines within admonitions are allowed
    if line.trim().is_empty() {
        return true;
    }

    // Content must be indented at least 4 spaces from the admonition marker
    line_indent >= base_indent + 4
}

/// Check if content at a byte position is within an admonition block
/// Uses a stack-based approach to properly handle nested admonitions.
pub fn is_within_admonition(content: &str, position: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    let mut byte_pos = 0;
    // Stack of admonition indent levels (supports nesting)
    let mut admonition_stack: Vec<usize> = Vec::new();

    for line in lines {
        let line_end = byte_pos + line.len();
        let line_indent = super::mkdocs_common::get_line_indent(line);

        // Check if we're starting a new admonition
        if is_admonition_start(line) {
            let admon_indent = get_admonition_indent(line).unwrap_or(0);

            // Pop any outer admonitions that this one is not nested within.
            // An admonition is nested within a parent if its marker appears in
            // the parent's content area (indented >= parent_indent + 4)
            while let Some(&parent_indent) = admonition_stack.last() {
                if admon_indent >= parent_indent + 4 {
                    // This admonition is nested inside the parent's content
                    break;
                }
                // Not nested within this parent, pop it
                admonition_stack.pop();
            }

            // Push this admonition onto the stack
            admonition_stack.push(admon_indent);
        } else if !admonition_stack.is_empty() && !line.trim().is_empty() {
            // Non-empty line - check if we're still within any admonition
            // Pop admonitions whose content indent requirement is not met
            while let Some(&admon_indent) = admonition_stack.last() {
                if line_indent >= admon_indent + 4 {
                    // Content is properly indented for this admonition
                    break;
                }
                // Content not indented enough, exit this admonition
                admonition_stack.pop();
            }
        }

        // Check if the position is within this line and we're in any admonition
        if byte_pos <= position && position <= line_end && !admonition_stack.is_empty() {
            return true;
        }

        // Account for newline character
        byte_pos = line_end + 1;
    }

    false
}

/// Get the range of an admonition block starting at the given line index
pub fn get_admonition_range(lines: &[&str], start_line_idx: usize) -> Option<(usize, usize)> {
    if start_line_idx >= lines.len() {
        return None;
    }

    let start_line = lines[start_line_idx];
    if !is_admonition_start(start_line) {
        return None;
    }

    let base_indent = get_admonition_indent(start_line).unwrap_or(0);
    let mut end_line_idx = start_line_idx;

    // Find where the admonition ends
    for (idx, line) in lines.iter().enumerate().skip(start_line_idx + 1) {
        if !line.trim().is_empty() && !is_admonition_content(line, base_indent) {
            break;
        }
        end_line_idx = idx;
    }

    Some((start_line_idx, end_line_idx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_admonition_start_detection() {
        // Valid admonition starts
        assert!(is_admonition_start("!!! note"));
        assert!(is_admonition_start("!!! warning \"Custom Title\""));
        assert!(is_admonition_start("??? tip"));
        assert!(is_admonition_start("???+ danger \"Expanded\""));
        assert!(is_admonition_start("    !!! note")); // Indented
        assert!(is_admonition_start("!!! note inline"));
        assert!(is_admonition_start("!!! note inline end"));

        // Invalid patterns
        assert!(!is_admonition_start("!! note")); // Wrong number of !
        assert!(!is_admonition_start("!!!")); // No type
        assert!(!is_admonition_start("Regular text"));
        assert!(!is_admonition_start("# Heading"));
    }

    #[test]
    fn test_admonition_indent() {
        assert_eq!(get_admonition_indent("!!! note"), Some(0));
        assert_eq!(get_admonition_indent("  !!! note"), Some(2));
        assert_eq!(get_admonition_indent("    !!! warning \"Title\""), Some(4));
        assert_eq!(get_admonition_indent("Regular text"), None);
    }

    #[test]
    fn test_admonition_content() {
        // Base indent 0, content must be indented 4+
        assert!(is_admonition_content("    Content", 0));
        assert!(is_admonition_content("        More indented", 0));
        assert!(is_admonition_content("", 0)); // Empty lines allowed
        assert!(!is_admonition_content("Not indented", 0));
        assert!(!is_admonition_content("  Only 2 spaces", 0));

        // Base indent 4, content must be indented 8+
        assert!(is_admonition_content("        Content", 4));
        assert!(!is_admonition_content("    Not enough", 4));
    }

    #[test]
    fn test_within_admonition() {
        let content = r#"# Document

!!! note "Test Note"
    This is content inside the admonition.
    More content here.

Regular text outside.

??? warning
    Collapsible content.

    Still inside.

Not inside anymore."#;

        // Find positions
        let inside_pos = content.find("inside the admonition").unwrap();
        let outside_pos = content.find("Regular text").unwrap();
        let collapsible_pos = content.find("Collapsible").unwrap();
        let still_inside_pos = content.find("Still inside").unwrap();
        let not_inside_pos = content.find("Not inside anymore").unwrap();

        assert!(is_within_admonition(content, inside_pos));
        assert!(!is_within_admonition(content, outside_pos));
        assert!(is_within_admonition(content, collapsible_pos));
        assert!(is_within_admonition(content, still_inside_pos));
        assert!(!is_within_admonition(content, not_inside_pos));
    }

    #[test]
    fn test_nested_admonitions() {
        let content = r#"!!! note "Outer"
    Content of outer.

    !!! warning "Inner"
        Content of inner.
        More inner content.

    Back to outer.

Outside."#;

        let outer_pos = content.find("Content of outer").unwrap();
        let inner_pos = content.find("Content of inner").unwrap();
        let back_outer_pos = content.find("Back to outer").unwrap();
        let outside_pos = content.find("Outside").unwrap();

        assert!(is_within_admonition(content, outer_pos));
        assert!(is_within_admonition(content, inner_pos));
        // Stack-based approach properly handles returning to outer admonition
        assert!(is_within_admonition(content, back_outer_pos));
        assert!(!is_within_admonition(content, outside_pos));
    }

    #[test]
    fn test_deeply_nested_admonitions() {
        let content = r#"!!! note "Level 1"
    Level 1 content.

    !!! warning "Level 2"
        Level 2 content.

        !!! tip "Level 3"
            Level 3 content.

        Back to level 2.

    Back to level 1.

Outside all."#;

        let level1_pos = content.find("Level 1 content").unwrap();
        let level2_pos = content.find("Level 2 content").unwrap();
        let level3_pos = content.find("Level 3 content").unwrap();
        let back_level2_pos = content.find("Back to level 2").unwrap();
        let back_level1_pos = content.find("Back to level 1").unwrap();
        let outside_pos = content.find("Outside all").unwrap();

        assert!(
            is_within_admonition(content, level1_pos),
            "Level 1 content should be in admonition"
        );
        assert!(
            is_within_admonition(content, level2_pos),
            "Level 2 content should be in admonition"
        );
        assert!(
            is_within_admonition(content, level3_pos),
            "Level 3 content should be in admonition"
        );
        assert!(
            is_within_admonition(content, back_level2_pos),
            "Back to level 2 should be in admonition"
        );
        assert!(
            is_within_admonition(content, back_level1_pos),
            "Back to level 1 should be in admonition"
        );
        assert!(
            !is_within_admonition(content, outside_pos),
            "Outside should not be in admonition"
        );
    }

    #[test]
    fn test_sibling_admonitions() {
        let content = r#"!!! note "First"
    First content.

!!! warning "Second"
    Second content.

Outside."#;

        let first_pos = content.find("First content").unwrap();
        let second_pos = content.find("Second content").unwrap();
        let outside_pos = content.find("Outside").unwrap();

        assert!(is_within_admonition(content, first_pos));
        assert!(is_within_admonition(content, second_pos));
        assert!(!is_within_admonition(content, outside_pos));
    }
}
