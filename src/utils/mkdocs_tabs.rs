use super::mkdocs_common::{BytePositionTracker, ContextStateMachine, MKDOCS_CONTENT_INDENT, get_line_indent};
use regex::Regex;
/// MkDocs Content Tabs detection utilities
///
/// The Tabbed extension provides support for grouped content tabs
/// using `===` markers for tab labels and content.
///
/// Common patterns:
/// - `=== "Tab 1"` - Tab with label
/// - `=== Tab` - Tab without quotes
/// - Content indented with 4 spaces under each tab
use std::sync::LazyLock;

/// Pattern to match tab markers
/// Matches: === "Label" or === Label
/// Lenient: accepts unclosed quotes, escaped quotes within quotes
static TAB_MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(\s*)===\s+.*$", // Just need content after ===
    )
    .unwrap()
});

/// Simple pattern to check for any tab marker
static TAB_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)===\s+").unwrap());

/// Check if a line is a tab marker
pub fn is_tab_marker(line: &str) -> bool {
    // First check if it starts like a tab marker
    let trimmed_start = line.trim_start();
    if !trimmed_start.starts_with("===") {
        return false;
    }

    // Reject double === (like "=== ===")
    // Check what comes after the first ===
    let after_marker = &trimmed_start[3..];
    if after_marker.trim_start().starts_with("===") {
        return false; // Double === is invalid
    }

    let trimmed = line.trim();

    // Must have content after ===
    if trimmed.len() <= 3 || !trimmed.chars().nth(3).is_some_and(|c| c.is_whitespace()) {
        return false;
    }

    // Be lenient with quote matching to handle real-world markdown
    // A future rule can warn about unclosed quotes
    // For now, just ensure there's some content after ===

    // Use the original regex as a final check
    TAB_MARKER.is_match(line)
}

/// Check if a line starts a tab section
pub fn is_tab_start(line: &str) -> bool {
    TAB_START.is_match(line)
}

/// Get the indentation level of a tab marker
pub fn get_tab_indent(line: &str) -> Option<usize> {
    if TAB_MARKER.is_match(line) {
        // Use consistent indentation calculation (tabs = 4 spaces)
        return Some(get_line_indent(line));
    }
    None
}

/// Check if a line is part of tab content (based on indentation)
pub fn is_tab_content(line: &str, base_indent: usize) -> bool {
    // Empty lines are not considered content on their own
    // They're handled separately in context
    if line.trim().is_empty() {
        return false;
    }

    // Content must be indented at least MKDOCS_CONTENT_INDENT spaces from the tab marker
    get_line_indent(line) >= base_indent + MKDOCS_CONTENT_INDENT
}

/// Check if content at a byte position is within a tab content area
pub fn is_within_tab_content(content: &str, position: usize) -> bool {
    let tracker = BytePositionTracker::new(content);
    let mut state = ContextStateMachine::new();
    let mut in_tab_group = false;

    for (_idx, line, start, end) in tracker.iter_with_positions() {
        // Check if we're starting a new tab
        if is_tab_marker(line) {
            // If this is the first tab, we're starting a tab group
            if !in_tab_group {
                in_tab_group = true;
            }
            let indent = get_tab_indent(line).unwrap_or(0);
            state.enter_context(indent, "tab".to_string());
        } else if state.is_in_context() {
            // Check if we're still in tab content
            if !line.trim().is_empty() && !is_tab_content(line, state.context_indent()) {
                // Check if this is another tab at the same level (continues the group)
                if is_tab_marker(line) && get_tab_indent(line).unwrap_or(0) == state.context_indent() {
                    // Continue with new tab
                    let indent = get_tab_indent(line).unwrap_or(0);
                    state.enter_context(indent, "tab".to_string());
                } else {
                    // Non-tab content that's not properly indented ends the tab group
                    state.exit_context();
                    in_tab_group = false;
                }
            }
        }

        // Check if the position is within this line and we're in a tab
        if start <= position && position <= end && state.is_in_context() {
            return true;
        }
    }

    false
}

/// Check if multiple consecutive lines form a tab group
pub fn get_tab_group_range(lines: &[&str], start_line_idx: usize) -> Option<(usize, usize)> {
    if start_line_idx >= lines.len() {
        return None;
    }

    let start_line = lines[start_line_idx];
    if !is_tab_marker(start_line) {
        return None;
    }

    let base_indent = get_tab_indent(start_line).unwrap_or(0);
    let mut end_line_idx = start_line_idx;

    // Find where the tab group ends
    for (idx, line) in lines.iter().enumerate().skip(start_line_idx + 1) {
        if is_tab_marker(line) && get_tab_indent(line).unwrap_or(0) == base_indent {
            // Another tab at the same level continues the group
            end_line_idx = idx;
        } else if is_tab_content(line, base_indent) {
            // Indented content within the tab
            end_line_idx = idx;
        } else {
            // Non-tab, non-content line ends the group
            // Don't include trailing empty lines
            break;
        }
    }

    Some((start_line_idx, end_line_idx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_marker_detection() {
        assert!(is_tab_marker("=== \"Tab 1\""));
        assert!(is_tab_marker("=== \"Complex Tab Label\""));
        assert!(is_tab_marker("=== SimpleTab"));
        assert!(is_tab_marker("  === \"Indented Tab\""));
        assert!(!is_tab_marker("== \"Not a tab\""));
        assert!(!is_tab_marker("==== \"Too many equals\""));
        assert!(!is_tab_marker("Regular text"));
    }

    #[test]
    fn test_tab_indent() {
        assert_eq!(get_tab_indent("=== \"Tab\""), Some(0));
        assert_eq!(get_tab_indent("  === \"Tab\""), Some(2));
        assert_eq!(get_tab_indent("    === \"Tab\""), Some(4));
        assert_eq!(get_tab_indent("Not a tab"), None);
    }

    #[test]
    fn test_tab_content() {
        // Base indent 0, content must be indented 4+
        assert!(is_tab_content("    Content", 0));
        assert!(is_tab_content("        More indented", 0));
        assert!(!is_tab_content("", 0)); // Empty lines not considered content on their own
        assert!(!is_tab_content("Not indented", 0));
        assert!(!is_tab_content("  Only 2 spaces", 0));
    }

    #[test]
    fn test_within_tab_content() {
        let content = r#"# Document

=== "Python"

    ```python
    def hello():
        print("Hello")
    ```

=== "JavaScript"

    ```javascript
    function hello() {
        console.log("Hello");
    }
    ```

Regular text outside tabs."#;

        let python_code_pos = content.find("def hello").unwrap();
        let js_code_pos = content.find("function hello").unwrap();
        let outside_pos = content.find("Regular text").unwrap();

        assert!(is_within_tab_content(content, python_code_pos));
        assert!(is_within_tab_content(content, js_code_pos));
        assert!(!is_within_tab_content(content, outside_pos));
    }

    #[test]
    fn test_tab_group_range() {
        let content = "=== \"Tab 1\"\n    Content 1\n=== \"Tab 2\"\n    Content 2\n\nOutside";
        let lines: Vec<&str> = content.lines().collect();

        let range = get_tab_group_range(&lines, 0);
        assert_eq!(range, Some((0, 3))); // Includes both tabs and their content
    }
}
