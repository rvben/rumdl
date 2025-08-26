/// MkDocs Content Tabs detection utilities
///
/// The Tabbed extension provides support for grouped content tabs
/// using `===` markers for tab labels and content.
///
/// Common patterns:
/// - `=== "Tab 1"` - Tab with label
/// - `=== Tab` - Tab without quotes
/// - Content indented with 4 spaces under each tab
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Pattern to match tab markers
    /// Matches: === "Label" or === Label
    static ref TAB_MARKER: Regex = Regex::new(
        r#"^(\s*)===\s+(?:"([^"]+)"|([^\s]+))\s*$"#
    ).unwrap();

    /// Simple pattern to check for any tab marker
    static ref TAB_START: Regex = Regex::new(
        r"^(\s*)===\s+"
    ).unwrap();
}

/// Check if a line is a tab marker
pub fn is_tab_marker(line: &str) -> bool {
    TAB_MARKER.is_match(line)
}

/// Check if a line starts a tab section
pub fn is_tab_start(line: &str) -> bool {
    TAB_START.is_match(line)
}

/// Get the indentation level of a tab marker
pub fn get_tab_indent(line: &str) -> Option<usize> {
    if let Some(caps) = TAB_MARKER.captures(line)
        && let Some(indent) = caps.get(1)
    {
        return Some(indent.as_str().len());
    }
    None
}

/// Check if a line is part of tab content (based on indentation)
pub fn is_tab_content(line: &str, base_indent: usize) -> bool {
    // Tab content must be indented at least 4 spaces more than the marker
    let line_indent = line.chars().take_while(|&c| c == ' ' || c == '\t').count();

    // Empty lines are not considered content on their own
    // They're handled separately in context
    if line.trim().is_empty() {
        return false;
    }

    // Content must be indented at least 4 spaces from the tab marker
    line_indent >= base_indent + 4
}

/// Check if content at a byte position is within a tab content area
pub fn is_within_tab_content(content: &str, position: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    let mut byte_pos = 0;
    let mut in_tab = false;
    let mut tab_indent = 0;
    let mut in_tab_group = false;

    for line in lines {
        let line_end = byte_pos + line.len();

        // Check if we're starting a new tab
        if is_tab_marker(line) {
            // If this is the first tab, we're starting a tab group
            if !in_tab_group {
                in_tab_group = true;
            }
            in_tab = true;
            tab_indent = get_tab_indent(line).unwrap_or(0);
        } else if in_tab {
            // Check if we're still in tab content
            if !line.trim().is_empty() && !is_tab_content(line, tab_indent) {
                // Check if this is another tab at the same level (continues the group)
                if is_tab_marker(line) && get_tab_indent(line).unwrap_or(0) == tab_indent {
                    // Continue with new tab
                    in_tab = true;
                } else {
                    // Non-tab content that's not properly indented ends the tab group
                    in_tab = false;
                    in_tab_group = false;
                    tab_indent = 0;
                }
            }
        }

        // Check if the position is within this line and we're in a tab
        if byte_pos <= position && position <= line_end && in_tab {
            return true;
        }

        // Account for newline character
        byte_pos = line_end + 1;
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
