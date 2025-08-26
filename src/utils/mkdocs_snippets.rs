/// MkDocs Snippets extension detection utilities
///
/// The Snippets extension allows including content from external files
/// using ASCII scissors syntax: `--8<--`
///
/// Common patterns:
/// - `--8<-- "filename.md"` - Include entire file
/// - `--8<-- "filename.md:start:end"` - Include specific lines
/// - `<!-- --8<-- [start:section] -->` - Start marker for section
/// - `<!-- --8<-- [end:section] -->` - End marker for section
///
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Pattern to match snippet inclusion with file path
    /// Matches: --8<-- "file.md" or --8<-- 'file.md'
    static ref SNIPPET_WITH_FILE: Regex = Regex::new(
        r#"--8<--\s+["'][^"']+["']"#
    ).unwrap();

    /// Pattern to match section markers
    static ref SECTION_MARKER: Regex = Regex::new(
        r"--8<--\s*\[(start|end):[^\]]*\]"
    ).unwrap();
}

/// Check if a line contains MkDocs snippet syntax
pub fn is_snippet_marker(line: &str) -> bool {
    // Check for the ASCII scissors pattern
    if !line.contains("--8<--") && !line.contains("-8<-") {
        return false;
    }

    let trimmed = line.trim();

    // Check for file inclusion with quotes (required)
    if SNIPPET_WITH_FILE.is_match(trimmed) {
        return true;
    }

    // HTML comment style with file: <!-- --8<-- "file.md" -->
    if line.contains("<!-- --8<--") && (line.contains('"') || line.contains('\'')) {
        return true;
    }

    // Section markers: --8<-- [start:name] or --8<-- [end:name]
    if SECTION_MARKER.is_match(trimmed) {
        return true;
    }

    // HTML comment with section marker
    if line.contains("<!-- --8<--") && line.contains("[") && (line.contains("start:") || line.contains("end:")) {
        return true;
    }

    // Alternative closing marker with file
    if line.contains("-8<-") && (line.contains('"') || line.contains('\'')) {
        return true;
    }

    false
}

/// Check if a line is a snippet section start marker
pub fn is_snippet_section_start(line: &str) -> bool {
    // Check for patterns like:
    // <!-- --8<-- [start:section_name] -->
    // --8<-- [start:section_name]
    // -8<- [start:section_name]

    if !line.contains("start:") {
        return false;
    }

    // Must have proper bracket structure
    if let Some(start_idx) = line.find("[start:")
        && let Some(end_idx) = line[start_idx..].find(']')
    {
        // Section name should not be empty (though empty is technically allowed)
        // and should contain the snippet marker
        return (line.contains("--8<--") || line.contains("-8<-")) && end_idx > 7;
    }

    false
}

/// Check if a line is a snippet section end marker
pub fn is_snippet_section_end(line: &str) -> bool {
    // Check for patterns like:
    // <!-- --8<-- [end:section_name] -->
    // --8<-- [end:section_name]
    // -8<- [end:section_name]

    if !line.contains("end:") {
        return false;
    }

    // Must have proper bracket structure
    if let Some(start_idx) = line.find("[end:")
        && let Some(end_idx) = line[start_idx..].find(']')
    {
        // Section name should match and contain snippet marker
        return (line.contains("--8<--") || line.contains("-8<-")) && end_idx > 5;
    }

    false
}

/// Check if a position is within a snippet section
pub fn is_within_snippet_section(content: &str, position: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    let mut byte_pos = 0;
    let mut section_stack: Vec<String> = Vec::new();

    for line in lines {
        let line_end = byte_pos + line.len();

        // Check if we're starting a snippet section
        if is_snippet_section_start(line) {
            // Extract section name for matching
            if let Some(start) = line.find("[start:")
                && let Some(end) = line[start..].find(']')
            {
                let section_name = line[start + 7..start + end].to_string();
                section_stack.push(section_name);
            }
        }

        // Check if we're ending a snippet section
        if is_snippet_section_end(line) {
            // Check if section names match
            if let Some(start) = line.find("[end:")
                && let Some(end) = line[start..].find(']')
            {
                let end_section_name = &line[start + 5..start + end];
                // Pop the matching section from the stack
                if let Some(last_section) = section_stack.last()
                    && last_section == end_section_name
                {
                    section_stack.pop();
                }
            }
        }

        // Check if position is within this line and we're in any snippet section
        if byte_pos <= position && position <= line_end && !section_stack.is_empty() {
            return true;
        }

        // Account for newline character
        byte_pos = line_end + 1;
    }

    false
}

/// Check if a line contains a snippet reference that could be a broken link
pub fn looks_like_snippet_reference(text: &str) -> bool {
    // More conservative check for link syntax that might be snippets
    text.contains("--8<--") || text.contains("-8<-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snippet_marker_detection() {
        // Valid snippets with file paths
        assert!(is_snippet_marker("--8<-- \"file.md\""));
        assert!(is_snippet_marker("--8<-- 'file.md'"));
        assert!(is_snippet_marker("  --8<-- \"indented.md\"  "));
        assert!(is_snippet_marker("<!-- --8<-- \"file.md\" -->"));

        // Invalid snippets without file paths
        assert!(!is_snippet_marker("--8<--"));
        assert!(!is_snippet_marker("--8<-- "));
        assert!(!is_snippet_marker("<!-- --8<-- -->"));

        // Section markers
        assert!(is_snippet_marker("--8<-- [start:section]"));
        assert!(is_snippet_marker("--8<-- [end:section]"));
        assert!(is_snippet_marker("<!-- --8<-- [start:test] -->"));
    }

    #[test]
    fn test_section_markers() {
        // Valid section start markers
        assert!(is_snippet_section_start("<!-- --8<-- [start:intro] -->"));
        assert!(is_snippet_section_start("--8<-- [start:code]"));
        assert!(is_snippet_section_start("-8<- [start:example]"));

        // Invalid section start markers
        assert!(!is_snippet_section_start("<!-- --8<-- [start:] -->")); // Empty name
        assert!(!is_snippet_section_start("--8<-- [start")); // Missing bracket
        assert!(!is_snippet_section_start("[start:test]")); // Missing snippet marker

        // Valid section end markers
        assert!(is_snippet_section_end("<!-- --8<-- [end:intro] -->"));
        assert!(is_snippet_section_end("--8<-- [end:code]"));

        // Invalid section end markers
        assert!(!is_snippet_section_end("<!-- --8<-- [end:] -->")); // Empty name
        assert!(!is_snippet_section_end("--8<-- [end")); // Missing bracket
    }

    #[test]
    fn test_within_snippet_section() {
        let content = r#"# Document

Normal content here.

<!-- --8<-- [start:example] -->
This content is within a snippet section.
It should be detected as such.
<!-- --8<-- [end:example] -->

This is outside the snippet section.

<!-- --8<-- [start:another] -->
Another snippet section.
<!-- --8<-- [end:another] -->
"#;

        // Test positions within and outside snippet sections
        let within_pos = content.find("within a snippet").unwrap();
        let outside_pos = content.find("outside the snippet").unwrap();
        let another_pos = content.find("Another snippet").unwrap();

        assert!(is_within_snippet_section(content, within_pos));
        assert!(!is_within_snippet_section(content, outside_pos));
        assert!(is_within_snippet_section(content, another_pos));
    }

    #[test]
    fn test_nested_snippet_sections() {
        let content = r#"<!-- --8<-- [start:outer] -->
Outer content.
<!-- --8<-- [start:inner] -->
Inner content.
<!-- --8<-- [end:inner] -->
Back to outer.
<!-- --8<-- [end:outer] -->
Outside."#;

        let outer_pos = content.find("Outer content").unwrap();
        let inner_pos = content.find("Inner content").unwrap();
        let back_pos = content.find("Back to outer").unwrap();
        let outside_pos = content.find("Outside").unwrap();

        assert!(is_within_snippet_section(content, outer_pos));
        assert!(is_within_snippet_section(content, inner_pos));
        assert!(is_within_snippet_section(content, back_pos));
        assert!(!is_within_snippet_section(content, outside_pos));
    }
}
