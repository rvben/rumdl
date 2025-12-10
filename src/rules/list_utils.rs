use fancy_regex::Regex as FancyRegex;
use regex::Regex;
use std::sync::LazyLock;

// Optimized list detection patterns with anchors and non-capturing groups
static UNORDERED_LIST_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)([*+-])(\s+)").unwrap());
static ORDERED_LIST_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)(\d+\.)(\s+)").unwrap());

// Patterns for lists without proper spacing - now excluding emphasis markers
static UNORDERED_LIST_NO_SPACE_PATTERN: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"^(\s*)(?:(?<!\*)\*(?!\*)|[+-])([^\s\*])").unwrap());
static ORDERED_LIST_NO_SPACE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)(\d+\.)([^\s])").unwrap());

// Patterns for lists with multiple spaces
static UNORDERED_LIST_MULTIPLE_SPACE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)([*+-])(\s{2,})").unwrap());
static ORDERED_LIST_MULTIPLE_SPACE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)(\d+\.)(\s{2,})").unwrap());

// Regex to capture list markers and the spaces *after* them
pub static LIST_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)([-*+]|\d+\.)(\s*)").unwrap());

/// Enum representing different types of list markers
#[derive(Debug, Clone, PartialEq)]
pub enum ListMarkerType {
    Asterisk,
    Plus,
    Minus,
    Ordered,
}

/// Struct representing a list item
#[derive(Debug, Clone)]
pub struct ListItem {
    pub indentation: usize,
    pub marker_type: ListMarkerType,
    pub marker: String,
    pub content: String,
    pub spaces_after_marker: usize,
}

/// Utility functions for detecting and handling lists in Markdown documents
pub struct ListUtils;

impl ListUtils {
    /// Calculate indentation level, counting tabs as 4 spaces per CommonMark spec
    pub fn calculate_indentation(s: &str) -> usize {
        s.chars()
            .take_while(|c| c.is_whitespace())
            .map(|c| if c == '\t' { 4 } else { 1 })
            .sum()
    }

    /// Check if a line is a list item
    pub fn is_list_item(line: &str) -> bool {
        // Fast path for common cases
        if line.is_empty() {
            return false;
        }

        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return false;
        }

        // Quick literal check for common list markers
        let Some(first_char) = trimmed.chars().next() else {
            return false;
        };
        match first_char {
            '*' | '+' | '-' => {
                if trimmed.len() > 1 {
                    let mut chars = trimmed.chars();
                    chars.next(); // Skip first char
                    if let Some(second_char) = chars.next() {
                        return second_char.is_whitespace();
                    }
                }
                false
            }
            '0'..='9' => {
                // Check for ordered list pattern using a literal search first
                let dot_pos = trimmed.find('.');
                if let Some(pos) = dot_pos
                    && pos > 0
                    && pos < trimmed.len() - 1
                {
                    let after_dot = &trimmed[pos + 1..];
                    return after_dot.starts_with(' ');
                }
                false
            }
            _ => false,
        }
    }

    /// Check if a line is an unordered list item
    pub fn is_unordered_list_item(line: &str) -> bool {
        // Fast path for common cases
        if line.is_empty() {
            return false;
        }

        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return false;
        }

        // Quick literal check for unordered list markers
        let Some(first_char) = trimmed.chars().next() else {
            return false;
        };
        if (first_char == '*' || first_char == '+' || first_char == '-')
            && trimmed.len() > 1
            && let Some(second_char) = trimmed.chars().nth(1)
        {
            return second_char.is_whitespace();
        }

        false
    }

    /// Check if a line is an ordered list item
    pub fn is_ordered_list_item(line: &str) -> bool {
        // Fast path for common cases
        if line.is_empty() {
            return false;
        }

        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return false;
        }

        let Some(first_char) = trimmed.chars().next() else {
            return false;
        };

        if !first_char.is_ascii_digit() {
            return false;
        }

        // Check for ordered list pattern using a literal search
        let dot_pos = trimmed.find('.');
        if let Some(pos) = dot_pos
            && pos > 0
            && pos < trimmed.len() - 1
        {
            let after_dot = &trimmed[pos + 1..];
            return after_dot.starts_with(' ');
        }

        false
    }

    /// Check if a line is a list item without proper spacing after the marker
    pub fn is_list_item_without_space(line: &str) -> bool {
        // Skip lines that start with double asterisks (bold text)
        if line.trim_start().starts_with("**") {
            return false;
        }

        // Skip lines that have bold/emphasis markers (typically table cells with bold text)
        if line.trim_start().contains("**") || line.trim_start().contains("__") {
            return false;
        }

        // Skip lines that are part of a Markdown table
        if crate::utils::skip_context::is_table_line(line) {
            return false;
        }

        // Skip lines that are horizontal rules or table delimiter rows
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            // Check for horizontal rules (only dashes and whitespace)
            if trimmed.chars().all(|c| c == '-' || c.is_whitespace()) {
                return false;
            }

            // Check for table delimiter rows without pipes (e.g., in cases where pipes are optional)
            // These have dashes and possibly colons for alignment
            if trimmed.contains('-') && trimmed.chars().all(|c| c == '-' || c == ':' || c.is_whitespace()) {
                return false;
            }
        }

        // Skip lines that are part of emphasis/bold text
        if line.trim_start().matches('*').count() >= 2 {
            return false;
        }

        // Handle potential regex errors gracefully
        UNORDERED_LIST_NO_SPACE_PATTERN.is_match(line).unwrap_or(false) || ORDERED_LIST_NO_SPACE_PATTERN.is_match(line)
    }

    /// Check if a line is a list item with multiple spaces after the marker
    pub fn is_list_item_with_multiple_spaces(line: &str) -> bool {
        UNORDERED_LIST_MULTIPLE_SPACE_PATTERN.is_match(line) || ORDERED_LIST_MULTIPLE_SPACE_PATTERN.is_match(line)
    }

    /// Parse a line as a list item
    pub fn parse_list_item(line: &str) -> Option<ListItem> {
        // First try to match unordered list pattern
        if let Some(captures) = UNORDERED_LIST_PATTERN.captures(line) {
            let indentation = captures.get(1).map_or(0, |m| Self::calculate_indentation(m.as_str()));
            let marker = captures.get(2).unwrap().as_str();
            let spaces = captures.get(3).map_or(0, |m| m.as_str().len());
            let raw_indentation = captures.get(1).map_or(0, |m| m.as_str().len());
            let content_start = raw_indentation + marker.len() + spaces;
            let content = if content_start < line.len() {
                line[content_start..].to_string()
            } else {
                String::new()
            };

            let marker_type = match marker {
                "*" => ListMarkerType::Asterisk,
                "+" => ListMarkerType::Plus,
                "-" => ListMarkerType::Minus,
                _ => unreachable!("UNORDERED_LIST_PATTERN regex guarantees marker is [*+-]"),
            };

            return Some(ListItem {
                indentation,
                marker_type,
                marker: marker.to_string(),
                content,
                spaces_after_marker: spaces,
            });
        }

        // Then try to match ordered list pattern
        if let Some(captures) = ORDERED_LIST_PATTERN.captures(line) {
            let indentation = captures.get(1).map_or(0, |m| Self::calculate_indentation(m.as_str()));
            let marker = captures.get(2).unwrap().as_str();
            let spaces = captures.get(3).map_or(0, |m| m.as_str().len());
            let raw_indentation = captures.get(1).map_or(0, |m| m.as_str().len());
            let content_start = raw_indentation + marker.len() + spaces;
            let content = if content_start < line.len() {
                line[content_start..].to_string()
            } else {
                String::new()
            };

            return Some(ListItem {
                indentation,
                marker_type: ListMarkerType::Ordered,
                marker: marker.to_string(),
                content,
                spaces_after_marker: spaces,
            });
        }

        None
    }

    /// Check if a line is a continuation of a list item
    pub fn is_list_continuation(line: &str, prev_list_item: &ListItem) -> bool {
        if line.trim().is_empty() {
            return false;
        }

        // Calculate indentation level properly (tabs = 4 spaces)
        let indentation = Self::calculate_indentation(line);

        // Continuation should be indented at least as much as the content of the previous item
        let min_indent = prev_list_item.indentation + prev_list_item.marker.len() + prev_list_item.spaces_after_marker;
        indentation >= min_indent && !Self::is_list_item(line)
    }

    /// Fix a list item without proper spacing
    pub fn fix_list_item_without_space(line: &str) -> String {
        // Handle unordered list items
        if let Ok(Some(captures)) = UNORDERED_LIST_NO_SPACE_PATTERN.captures(line) {
            let indentation = captures.get(1).map_or("", |m| m.as_str());
            let marker = captures.get(2).map_or("", |m| m.as_str());
            let content = captures.get(3).map_or("", |m| m.as_str());
            return format!("{indentation}{marker} {content}");
        }

        // Handle ordered list items
        if let Some(captures) = ORDERED_LIST_NO_SPACE_PATTERN.captures(line) {
            let indentation = captures.get(1).map_or("", |m| m.as_str());
            let marker = captures.get(2).map_or("", |m| m.as_str());
            let content = captures.get(3).map_or("", |m| m.as_str());
            return format!("{indentation}{marker} {content}");
        }

        line.to_string()
    }

    /// Fix a list item with multiple spaces after the marker
    pub fn fix_list_item_with_multiple_spaces(line: &str) -> String {
        if let Some(captures) = UNORDERED_LIST_MULTIPLE_SPACE_PATTERN.captures(line) {
            let leading_space = captures.get(1).map_or("", |m| m.as_str());
            let marker = captures.get(2).map_or("", |m| m.as_str());
            let spaces = captures.get(3).map_or("", |m| m.as_str());

            // Get content after multiple spaces
            let start_pos = leading_space.len() + marker.len() + spaces.len();
            let content = if start_pos < line.len() { &line[start_pos..] } else { "" };

            // Replace multiple spaces with a single space
            return format!("{leading_space}{marker} {content}");
        }

        if let Some(captures) = ORDERED_LIST_MULTIPLE_SPACE_PATTERN.captures(line) {
            let leading_space = captures.get(1).map_or("", |m| m.as_str());
            let marker = captures.get(2).map_or("", |m| m.as_str());
            let spaces = captures.get(3).map_or("", |m| m.as_str());

            // Get content after multiple spaces
            let start_pos = leading_space.len() + marker.len() + spaces.len();
            let content = if start_pos < line.len() { &line[start_pos..] } else { "" };

            // Replace multiple spaces with a single space
            return format!("{leading_space}{marker} {content}");
        }

        // Return the original line if no pattern matched
        line.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListType {
    Unordered,
    Ordered,
}

/// Returns (ListType, matched string, number of spaces after marker) if the line is a list item
pub fn is_list_item(line: &str) -> Option<(ListType, String, usize)> {
    let trimmed_line = line.trim();
    if trimmed_line.is_empty() {
        return None;
    }
    // Horizontal rule check (--- or ***)
    if trimmed_line.chars().all(|c| c == '-' || c == ' ') && trimmed_line.chars().filter(|&c| c == '-').count() >= 3 {
        return None;
    }
    if trimmed_line.chars().all(|c| c == '*' || c == ' ') && trimmed_line.chars().filter(|&c| c == '*').count() >= 3 {
        return None;
    }
    if let Some(cap) = LIST_REGEX.captures(line) {
        let marker = &cap[2];
        let spaces = cap[3].len();
        let list_type = if marker.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            ListType::Ordered
        } else {
            ListType::Unordered
        };
        return Some((list_type, cap[0].to_string(), spaces));
    }
    None
}

/// Returns true if the list item at lines[current_idx] is a multi-line item
pub fn is_multi_line_item(lines: &[&str], current_idx: usize) -> bool {
    if current_idx >= lines.len() - 1 {
        return false;
    }
    let next_line = lines[current_idx + 1].trim();
    if next_line.is_empty() {
        return false;
    }
    if is_list_item(next_line).is_some() {
        return false;
    }
    let curr_indent = ListUtils::calculate_indentation(lines[current_idx]);
    let next_indent = ListUtils::calculate_indentation(lines[current_idx + 1]);
    next_indent > curr_indent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_list_item_without_space() {
        // Valid list item with space after marker
        assert!(!ListUtils::is_list_item_without_space("- Item with space"));
        assert!(!ListUtils::is_list_item_without_space("* Item with space"));
        assert!(!ListUtils::is_list_item_without_space("+ Item with space"));
        assert!(!ListUtils::is_list_item_without_space("1. Item with space"));

        // Invalid list items without space after marker (should return true)
        assert!(ListUtils::is_list_item_without_space("-No space"));
        assert!(ListUtils::is_list_item_without_space("*No space"));
        assert!(ListUtils::is_list_item_without_space("+No space"));
        assert!(ListUtils::is_list_item_without_space("1.No space"));

        // Not list items (should return false)
        assert!(!ListUtils::is_list_item_without_space("Regular text"));
        assert!(!ListUtils::is_list_item_without_space(""));
        assert!(!ListUtils::is_list_item_without_space("    "));
        assert!(!ListUtils::is_list_item_without_space("# Heading"));

        // Bold/emphasis text that might be confused with list items (should return false)
        assert!(!ListUtils::is_list_item_without_space("**Bold text**"));
        assert!(!ListUtils::is_list_item_without_space("__Bold text__"));
        assert!(!ListUtils::is_list_item_without_space("*Italic text*"));
        assert!(!ListUtils::is_list_item_without_space("_Italic text_"));

        // Table cells with bold/emphasis (should return false)
        assert!(!ListUtils::is_list_item_without_space("| **Heading** | Content |"));
        assert!(!ListUtils::is_list_item_without_space("**Bold** | Normal"));
        assert!(!ListUtils::is_list_item_without_space("| Cell 1 | **Bold** |"));

        // Horizontal rules (should return false)
        assert!(!ListUtils::is_list_item_without_space("---"));
        assert!(!ListUtils::is_list_item_without_space("----------"));
        assert!(!ListUtils::is_list_item_without_space("   ---   "));

        // Table delimiter rows (should return false)
        assert!(!ListUtils::is_list_item_without_space("|--------|---------|"));
        assert!(!ListUtils::is_list_item_without_space("|:-------|:-------:|"));
        assert!(!ListUtils::is_list_item_without_space("| ------ | ------- |"));
        assert!(!ListUtils::is_list_item_without_space("---------|----------|"));
        assert!(!ListUtils::is_list_item_without_space(":--------|:--------:"));
    }

    #[test]
    fn test_is_list_item() {
        // Valid list items
        assert!(ListUtils::is_list_item("- Item"));
        assert!(ListUtils::is_list_item("* Item"));
        assert!(ListUtils::is_list_item("+ Item"));
        assert!(ListUtils::is_list_item("1. Item"));
        assert!(ListUtils::is_list_item("  - Indented item"));

        // Not list items
        assert!(!ListUtils::is_list_item("Regular text"));
        assert!(!ListUtils::is_list_item(""));
        assert!(!ListUtils::is_list_item("    "));
        assert!(!ListUtils::is_list_item("# Heading"));
        assert!(!ListUtils::is_list_item("**Bold text**"));
        assert!(!ListUtils::is_list_item("| Cell 1 | Cell 2 |"));
    }

    #[test]
    fn test_complex_nested_lists() {
        // Various indentation levels
        assert!(ListUtils::is_list_item("- Level 1"));
        assert!(ListUtils::is_list_item("  - Level 2"));
        assert!(ListUtils::is_list_item("    - Level 3"));
        assert!(ListUtils::is_list_item("      - Level 4"));
        assert!(ListUtils::is_list_item("        - Level 5"));

        // Mixed markers in nested lists
        assert!(ListUtils::is_list_item("* Main item"));
        assert!(ListUtils::is_list_item("  - Sub item"));
        assert!(ListUtils::is_list_item("    + Sub-sub item"));
        assert!(ListUtils::is_list_item("      * Deep item"));

        // Ordered lists nested in unordered
        assert!(ListUtils::is_list_item("- Unordered"));
        assert!(ListUtils::is_list_item("  1. First ordered"));
        assert!(ListUtils::is_list_item("  2. Second ordered"));
        assert!(ListUtils::is_list_item("    - Back to unordered"));

        // Tab indentation
        assert!(ListUtils::is_list_item("\t- Tab indented"));
        assert!(ListUtils::is_list_item("\t\t- Double tab"));
        assert!(ListUtils::is_list_item("\t  - Tab plus spaces"));
        assert!(ListUtils::is_list_item("  \t- Spaces plus tab"));
    }

    #[test]
    fn test_parse_list_item_edge_cases() {
        // Unicode content
        let unicode_item = ListUtils::parse_list_item("- 测试项目 🚀").unwrap();
        assert_eq!(unicode_item.content, "测试项目 🚀");

        // Empty content after marker
        let empty_item = ListUtils::parse_list_item("- ").unwrap();
        assert_eq!(empty_item.content, "");

        // Multiple spaces after marker
        let multi_space = ListUtils::parse_list_item("-   Multiple spaces").unwrap();
        assert_eq!(multi_space.spaces_after_marker, 3);
        assert_eq!(multi_space.content, "Multiple spaces");

        // Very long ordered list numbers
        let long_number = ListUtils::parse_list_item("999999. Item").unwrap();
        assert_eq!(long_number.marker, "999999.");
        assert_eq!(long_number.marker_type, ListMarkerType::Ordered);

        // List with only marker - might not parse as valid list
        if let Some(marker_only) = ListUtils::parse_list_item("*") {
            assert_eq!(marker_only.content, "");
            assert_eq!(marker_only.spaces_after_marker, 0);
        }
    }

    #[test]
    fn test_nested_list_detection() {
        // Test detection of list items at various nesting levels
        let lines = vec![
            ("- Item 1", 0),
            ("  - Item 1.1", 2),
            ("    - Item 1.1.1", 4),
            ("      - Item 1.1.1.1", 6),
            ("    - Item 1.1.2", 4),
            ("  - Item 1.2", 2),
            ("- Item 2", 0),
        ];

        for (line, expected_indent) in lines {
            let item = ListUtils::parse_list_item(line).unwrap();
            assert_eq!(item.indentation, expected_indent, "Failed for line: {line}");
        }
    }

    #[test]
    fn test_mixed_list_markers() {
        // Test different marker types
        let markers = vec![
            ("* Asterisk", ListMarkerType::Asterisk),
            ("+ Plus", ListMarkerType::Plus),
            ("- Minus", ListMarkerType::Minus),
            ("1. Ordered", ListMarkerType::Ordered),
            ("42. Ordered", ListMarkerType::Ordered),
        ];

        for (line, expected_type) in markers {
            let item = ListUtils::parse_list_item(line).unwrap();
            assert_eq!(item.marker_type, expected_type, "Failed for line: {line}");
        }
    }

    #[test]
    fn test_list_item_without_space_edge_cases() {
        // Edge cases for missing spaces
        assert!(ListUtils::is_list_item_without_space("*a"));
        assert!(ListUtils::is_list_item_without_space("+b"));
        assert!(ListUtils::is_list_item_without_space("-c"));
        assert!(ListUtils::is_list_item_without_space("1.d"));

        // Single character lines
        assert!(!ListUtils::is_list_item_without_space("*"));
        assert!(!ListUtils::is_list_item_without_space("+"));
        assert!(!ListUtils::is_list_item_without_space("-"));

        // Markers at end of line
        assert!(!ListUtils::is_list_item_without_space("Text ends with -"));
        assert!(!ListUtils::is_list_item_without_space("Text ends with *"));
        assert!(!ListUtils::is_list_item_without_space("Number ends with 1."));
    }

    #[test]
    fn test_list_item_with_multiple_spaces() {
        // Test detection of multiple spaces after marker
        assert!(ListUtils::is_list_item_with_multiple_spaces("-  Two spaces"));
        assert!(ListUtils::is_list_item_with_multiple_spaces("*   Three spaces"));
        assert!(ListUtils::is_list_item_with_multiple_spaces("+    Four spaces"));
        assert!(ListUtils::is_list_item_with_multiple_spaces("1.  Two spaces"));

        // Should not match single space
        assert!(!ListUtils::is_list_item_with_multiple_spaces("- One space"));
        assert!(!ListUtils::is_list_item_with_multiple_spaces("* One space"));
        assert!(!ListUtils::is_list_item_with_multiple_spaces("+ One space"));
        assert!(!ListUtils::is_list_item_with_multiple_spaces("1. One space"));
    }

    #[test]
    fn test_complex_content_in_lists() {
        // List items with inline formatting
        let bold_item = ListUtils::parse_list_item("- **Bold** content").unwrap();
        assert_eq!(bold_item.content, "**Bold** content");

        let link_item = ListUtils::parse_list_item("* [Link](url) in list").unwrap();
        assert_eq!(link_item.content, "[Link](url) in list");

        let code_item = ListUtils::parse_list_item("+ Item with `code`").unwrap();
        assert_eq!(code_item.content, "Item with `code`");

        // List with inline HTML
        let html_item = ListUtils::parse_list_item("- Item with <span>HTML</span>").unwrap();
        assert_eq!(html_item.content, "Item with <span>HTML</span>");

        // List with emoji
        let emoji_item = ListUtils::parse_list_item("1. 🎉 Party time!").unwrap();
        assert_eq!(emoji_item.content, "🎉 Party time!");
    }

    #[test]
    fn test_ambiguous_list_markers() {
        // Test cases that might be ambiguous

        // Arithmetic expressions should not be lists
        assert!(!ListUtils::is_list_item("2 + 2 = 4"));
        assert!(!ListUtils::is_list_item("5 - 3 = 2"));
        assert!(!ListUtils::is_list_item("3 * 3 = 9"));

        // Emphasis markers should not be lists
        assert!(!ListUtils::is_list_item("*emphasis*"));
        assert!(!ListUtils::is_list_item("**strong**"));
        assert!(!ListUtils::is_list_item("***strong emphasis***"));

        // Date ranges
        assert!(!ListUtils::is_list_item("2023-01-01 - 2023-12-31"));

        // But these should be lists
        assert!(ListUtils::is_list_item("- 2023-01-01 - 2023-12-31"));
        assert!(ListUtils::is_list_item("* emphasis text here"));
    }

    #[test]
    fn test_deeply_nested_complex_lists() {
        let complex_doc = vec![
            "- Top level item",
            "  - Second level with **bold**",
            "    1. Ordered item with `code`",
            "    2. Another ordered item",
            "      - Back to unordered [link](url)",
            "        * Different marker",
            "          + Yet another marker",
            "            - Maximum nesting?",
            "              1. Can we go deeper?",
            "                - Apparently yes!",
        ];

        for line in complex_doc {
            assert!(ListUtils::is_list_item(line), "Failed to recognize: {line}");
            let item = ListUtils::parse_list_item(line).unwrap();
            assert!(
                !item.content.is_empty()
                    || line.trim().ends_with('-')
                    || line.trim().ends_with('*')
                    || line.trim().ends_with('+')
            );
        }
    }

    #[test]
    fn test_parse_list_item_comprehensive() {
        // Test the comprehensive parsing with expected values
        let test_cases = vec![
            ("- Simple item", 0, ListMarkerType::Minus, "-", "Simple item"),
            ("  * Indented", 2, ListMarkerType::Asterisk, "*", "Indented"),
            ("    1. Ordered", 4, ListMarkerType::Ordered, "1.", "Ordered"),
            ("\t+ Tab indent", 4, ListMarkerType::Plus, "+", "Tab indent"), // Tab counts as 4 spaces per CommonMark
        ];

        for (line, expected_indent, expected_type, expected_marker, expected_content) in test_cases {
            let item = ListUtils::parse_list_item(line);
            assert!(item.is_some(), "Failed to parse: {line}");
            let item = item.unwrap();
            assert_eq!(item.indentation, expected_indent, "Wrong indentation for: {line}");
            assert_eq!(item.marker_type, expected_type, "Wrong marker type for: {line}");
            assert_eq!(item.marker, expected_marker, "Wrong marker for: {line}");
            assert_eq!(item.content, expected_content, "Wrong content for: {line}");
        }
    }

    #[test]
    fn test_special_characters_in_lists() {
        // Test with special characters that might break regex
        let special_cases = vec![
            "- Item with $ dollar sign",
            "* Item with ^ caret",
            "+ Item with \\ backslash",
            "- Item with | pipe",
            "1. Item with ( ) parentheses",
            "2. Item with [ ] brackets",
            "3. Item with { } braces",
        ];

        for line in special_cases {
            assert!(ListUtils::is_list_item(line), "Failed for: {line}");
            let item = ListUtils::parse_list_item(line);
            assert!(item.is_some(), "Failed to parse: {line}");
        }
    }

    #[test]
    fn test_list_continuations() {
        // Lists that continue on multiple lines (not directly supported but shouldn't crash)
        let continuation = "- This is a very long list item that \
                           continues on the next line";
        assert!(ListUtils::is_list_item(continuation));

        // Indented continuation
        let indented_cont = "  - Another long item that \
                               continues with proper indentation";
        assert!(ListUtils::is_list_item(indented_cont));
    }

    #[test]
    fn test_performance_edge_cases() {
        // Very long lines
        let long_content = "x".repeat(10000);
        let long_line = format!("- {long_content}");
        assert!(ListUtils::is_list_item(&long_line));

        // Many spaces
        let many_spaces = " ".repeat(100);
        let spaced_line = format!("{many_spaces}- Item");
        assert!(ListUtils::is_list_item(&spaced_line));

        // Large ordered number
        let big_number = format!("{}. Item", "9".repeat(20));
        assert!(ListUtils::is_list_item(&big_number));
    }

    #[test]
    fn test_is_unordered_list_item() {
        // Valid unordered list items
        assert!(ListUtils::is_unordered_list_item("- Item"));
        assert!(ListUtils::is_unordered_list_item("* Item"));
        assert!(ListUtils::is_unordered_list_item("+ Item"));

        // Invalid - ordered lists
        assert!(!ListUtils::is_unordered_list_item("1. Item"));
        assert!(!ListUtils::is_unordered_list_item("99. Item"));

        // Invalid - no space after marker
        assert!(!ListUtils::is_unordered_list_item("-Item"));
        assert!(!ListUtils::is_unordered_list_item("*Item"));
        assert!(!ListUtils::is_unordered_list_item("+Item"));
    }

    #[test]
    fn test_calculate_indentation() {
        // Test that tabs are counted as 4 spaces
        assert_eq!(ListUtils::calculate_indentation(""), 0);
        assert_eq!(ListUtils::calculate_indentation("    "), 4);
        assert_eq!(ListUtils::calculate_indentation("\t"), 4);
        assert_eq!(ListUtils::calculate_indentation("\t\t"), 8);
        assert_eq!(ListUtils::calculate_indentation("  \t"), 6); // 2 spaces + 1 tab
        assert_eq!(ListUtils::calculate_indentation("\t  "), 6); // 1 tab + 2 spaces
        assert_eq!(ListUtils::calculate_indentation("\t\t  "), 10); // 2 tabs + 2 spaces
        assert_eq!(ListUtils::calculate_indentation("  \t  \t"), 12); // 2 spaces + tab + 2 spaces + tab
    }

    #[test]
    fn test_is_ordered_list_item() {
        // Valid ordered list items
        assert!(ListUtils::is_ordered_list_item("1. Item"));
        assert!(ListUtils::is_ordered_list_item("99. Item"));
        assert!(ListUtils::is_ordered_list_item("1234567890. Item"));

        // Invalid - unordered lists
        assert!(!ListUtils::is_ordered_list_item("- Item"));
        assert!(!ListUtils::is_ordered_list_item("* Item"));
        assert!(!ListUtils::is_ordered_list_item("+ Item"));

        // Invalid - no space after period
        assert!(!ListUtils::is_ordered_list_item("1.Item"));
        assert!(!ListUtils::is_ordered_list_item("99.Item"));
    }
}
