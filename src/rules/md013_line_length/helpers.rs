/// Check if a line ends with a hard break (either two spaces or backslash)
///
/// CommonMark supports two formats for hard line breaks:
/// 1. Two or more trailing spaces
/// 2. A backslash at the end of the line
pub(crate) fn has_hard_break(line: &str) -> bool {
    let line = line.strip_suffix('\r').unwrap_or(line);
    line.ends_with("  ") || line.ends_with('\\')
}

/// Extract list marker and content from a list item
/// Trim trailing whitespace while preserving hard breaks (two trailing spaces or backslash)
///
/// Hard breaks in Markdown can be indicated by:
/// 1. Two trailing spaces before a newline (traditional)
/// 2. A backslash at the end of the line (mdformat style)
pub(crate) fn trim_preserving_hard_break(s: &str) -> String {
    // Strip trailing \r from CRLF line endings first to handle Windows files
    let s = s.strip_suffix('\r').unwrap_or(s);

    // Check for backslash hard break (mdformat style)
    if s.ends_with('\\') {
        // Preserve the backslash exactly as-is
        return s.to_string();
    }

    // Check if there are at least 2 trailing spaces (traditional hard break)
    if s.ends_with("  ") {
        // Find the position where non-space content ends
        let content_end = s.trim_end().len();
        if content_end == 0 {
            // String is all whitespace
            return String::new();
        }
        // Preserve exactly 2 trailing spaces for hard break
        format!("{}  ", &s[..content_end])
    } else {
        // No hard break, just trim all trailing whitespace
        s.trim_end().to_string()
    }
}

/// Split paragraph lines into segments at hard break boundaries.
/// Each segment is a group of lines that can be reflowed together.
/// Lines with hard breaks (ending with 2+ spaces or backslash) form segment boundaries.
///
/// Example:
///   Input:  ["Line 1", "Line 2  ", "Line 3", "Line 4"]
///   Output: [["Line 1", "Line 2  "], ["Line 3", "Line 4"]]
///
/// The first segment includes "Line 2  " which has a hard break at the end.
/// The second segment starts after the hard break.
pub(crate) fn split_into_segments(para_lines: &[String]) -> Vec<Vec<String>> {
    let mut segments: Vec<Vec<String>> = Vec::new();
    let mut current_segment: Vec<String> = Vec::new();

    for line in para_lines {
        current_segment.push(line.clone());

        // If this line has a hard break, end the current segment
        if has_hard_break(line) {
            segments.push(current_segment.clone());
            current_segment.clear();
        }
    }

    // Add any remaining lines as the final segment
    if !current_segment.is_empty() {
        segments.push(current_segment);
    }

    segments
}

pub(crate) fn extract_list_marker_and_content(line: &str) -> (String, String) {
    // First, find the leading indentation
    let indent_len = line.len() - line.trim_start().len();
    let indent = &line[..indent_len];
    let trimmed = &line[indent_len..];

    // Handle bullet lists
    // Trim trailing whitespace while preserving hard breaks
    if let Some(rest) = trimmed.strip_prefix("- ") {
        return (format!("{indent}- "), trim_preserving_hard_break(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("* ") {
        return (format!("{indent}* "), trim_preserving_hard_break(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("+ ") {
        return (format!("{indent}+ "), trim_preserving_hard_break(rest));
    }

    // Handle numbered lists on trimmed content
    let mut chars = trimmed.chars();
    let mut marker_content = String::new();

    while let Some(c) = chars.next() {
        marker_content.push(c);
        if c == '.' {
            // Check if next char is a space
            if let Some(next) = chars.next()
                && next == ' '
            {
                marker_content.push(next);
                // Trim trailing whitespace while preserving hard breaks
                let content = trim_preserving_hard_break(chars.as_str());
                return (format!("{indent}{marker_content}"), content);
            }
            break;
        }
    }

    // Fallback - shouldn't happen if is_list_item was correct
    (String::new(), line.to_string())
}

// Helper functions for MD013 line length rule
pub(crate) fn is_horizontal_rule(line: &str) -> bool {
    if line.len() < 3 {
        return false;
    }
    // Check if line consists only of -, _, or * characters (at least 3)
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return false;
    }
    let first_char = chars[0];
    if first_char != '-' && first_char != '_' && first_char != '*' {
        return false;
    }
    // All characters should be the same (allowing spaces between)
    for c in &chars {
        if *c != first_char && *c != ' ' {
            return false;
        }
    }
    // Must have at least 3 of the marker character
    chars.iter().filter(|c| **c == first_char).count() >= 3
}

pub(crate) fn is_numbered_list_item(line: &str) -> bool {
    let mut chars = line.chars();
    // Must start with a digit
    if !chars.next().is_some_and(|c| c.is_numeric()) {
        return false;
    }
    // Can have more digits
    while let Some(c) = chars.next() {
        if c == '.' {
            // After period, must have a space (consistent with extract_list_marker_and_content)
            // "2019." alone is NOT treated as a list item to avoid false positives
            return chars.next() == Some(' ');
        }
        if !c.is_numeric() {
            return false;
        }
    }
    false
}

pub(crate) fn is_list_item(line: &str) -> bool {
    // Bullet lists
    if (line.starts_with('-') || line.starts_with('*') || line.starts_with('+'))
        && line.len() > 1
        && line.chars().nth(1) == Some(' ')
    {
        return true;
    }
    // Numbered lists
    is_numbered_list_item(line)
}

/// Check if a line contains only template directives (no other content)
///
/// Detects common template syntax used in static site generators:
/// - Handlebars/mdBook/Mustache: `{{...}}`
/// - Jinja2/Liquid/Jekyll: `{%...%}`
/// - Hugo shortcodes: `{{<...>}}` or `{{%...%}}`
///
/// Template directives are preprocessor directives, not Markdown content,
/// so they should be treated as paragraph boundaries like HTML comments.
pub(crate) fn is_template_directive_only(line: &str) -> bool {
    let trimmed = line.trim();

    // Empty lines are not template directives
    if trimmed.is_empty() {
        return false;
    }

    // Check for various template syntaxes
    // Handlebars/mdBook/Mustache: {{...}}
    if trimmed.starts_with("{{") && trimmed.ends_with("}}") {
        return true;
    }

    // Jinja2/Liquid/Jekyll: {%...%}
    if trimmed.starts_with("{%") && trimmed.ends_with("%}") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test for issue #336: "2019." alone should NOT be treated as a list item
    /// This prevents convergence failures when a year appears at the end of a sentence
    #[test]
    fn test_numbered_list_item_requires_space_after_period() {
        // Valid list items (have space after period)
        assert!(is_numbered_list_item("1. Item"));
        assert!(is_numbered_list_item("10. Item"));
        assert!(is_numbered_list_item("99. Long number"));
        assert!(is_numbered_list_item("123. Triple digits"));

        // Invalid: number+period without space (like years at end of sentences)
        // These should NOT be treated as list items to avoid reflow issues
        assert!(!is_numbered_list_item("2019."));
        assert!(!is_numbered_list_item("1999."));
        assert!(!is_numbered_list_item("2023."));
        assert!(!is_numbered_list_item("1.")); // Even single digit without space

        // Invalid: not starting with digit
        assert!(!is_numbered_list_item("a. Item"));
        assert!(!is_numbered_list_item(". Item"));
        assert!(!is_numbered_list_item("Item"));

        // Invalid: no period
        assert!(!is_numbered_list_item("1 Item"));
        assert!(!is_numbered_list_item("123"));
    }

    #[test]
    fn test_is_list_item_bullet_and_numbered() {
        // Bullet list items
        assert!(is_list_item("- Item"));
        assert!(is_list_item("* Item"));
        assert!(is_list_item("+ Item"));

        // Bullet without space = not a list item
        assert!(!is_list_item("-Item"));
        assert!(!is_list_item("*Item"));

        // Numbered list items
        assert!(is_list_item("1. Item"));
        assert!(is_list_item("99. Item"));

        // Year at end of sentence = not a list item
        assert!(!is_list_item("2019."));
    }
}
