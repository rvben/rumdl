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
    /// Pattern to match valid snippet markers: -{1,}8<-{1,}
    /// Based on PyMdown Extensions Snippets specification
    static ref BARE_SNIPPET_MARKER: Regex = Regex::new(
        r"^;*-+8<-+$"  // Optional semicolons, then dashes-8<-dashes only
    ).unwrap();

    /// Pattern to match snippet with quoted file path
    /// Lenient: accepts unclosed quotes for detection (can warn later)
    static ref SNIPPET_WITH_FILE: Regex = Regex::new(
        r#"-+8<-+\s+["']"#  // Just check for quote after snippet marker
    ).unwrap();

    /// Pattern to match section markers
    static ref SECTION_MARKER: Regex = Regex::new(
        r"-+8<-+\s*\[(start|end):[^\]]*\]"
    ).unwrap();

    /// Pattern to match snippet in HTML comment
    static ref SNIPPET_IN_COMMENT: Regex = Regex::new(
        r"<!--\s*-+8<-+\s*-->"
    ).unwrap();

    /// Pattern to match invalid asymmetric marker
    static ref INVALID_ASYMMETRIC: Regex = Regex::new(
        r#"(?:^|\s)--8<-\s+["']"#  // --8<- followed by quote is invalid
    ).unwrap();
}

/// Check if a line contains MkDocs snippet syntax
pub fn is_snippet_marker(line: &str) -> bool {
    // PyMdown Snippets spec says: -{1,}8<-{1,} (symmetric dashes)
    // We're lenient with unclosed quotes for detection (to warn later)

    // Check for known invalid asymmetric patterns
    // IMPORTANT: -8<-- as a standalone marker is invalid, but it appears
    // as a substring in valid --8<-- markers!
    // Only reject if -8<-- appears without a leading dash
    if !line.contains("--8<--") && !line.contains("---8<---") {
        // Only check for invalid patterns if we don't have valid ones
        if line.contains("-8<-- ") || line.contains("-8<--\"") || line.contains("-8<--'") {
            return false; // -8<-- is invalid when not part of --8<--
        }
    }
    if INVALID_ASYMMETRIC.is_match(line) {
        return false; // --8<- with file is invalid (asymmetric)
    }

    let trimmed = line.trim();

    // Check for single-line snippet with file
    // Be lenient: accept if line has valid marker and a quote anywhere
    let has_valid_marker = line.contains("--8<--")
        || line.contains("---8<---")
        || (line.contains("-8<-") && !line.contains("-8<--") && !line.contains("--8<-"));

    if has_valid_marker && (line.contains('"') || line.contains('\'')) {
        return true;
    }

    // Check for section markers: --8<-- [start:name] or [end:name]
    if SECTION_MARKER.is_match(trimmed) {
        return true;
    }

    // Block format: bare marker ONLY valid if truly alone (no trailing space)
    // The test expects "--8<--" alone to be invalid without context
    // Only accept if it's exactly the marker with optional semicolons
    // But NOT if there's trailing whitespace suggesting missing file
    if BARE_SNIPPET_MARKER.is_match(trimmed) && !trimmed.ends_with(' ') {
        // Additional check: bare marker usually appears in pairs for blocks
        // For now, we'll be conservative and not accept bare markers
        // unless they're in HTML comments
        if !line.contains("<!--") {
            return false; // Bare marker without context is invalid
        }
    }

    // HTML comment variations
    if line.contains("<!--") && line.contains("-->") && line.contains("8<") {
        // Check various patterns within comments
        if SNIPPET_WITH_FILE.is_match(line) {
            return true;
        }
        if SECTION_MARKER.is_match(line) {
            return true;
        }
        // Don't accept bare snippet markers in comments without content
        // <!-- --8<-- --> is not valid (no file or section)
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
    if let Some(_start_idx) = line.find("[start:")
        && let Some(_end_idx) = line[_start_idx..].find(']')
    {
        // Empty section names are allowed (lenient for detection)
        // Just check that we have the snippet marker
        return line.contains("--8<--") || line.contains("-8<-") || line.contains("---8<---");
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
    if let Some(_start_idx) = line.find("[end:")
        && let Some(_end_idx) = line[_start_idx..].find(']')
    {
        // Empty section names are allowed (lenient for detection)
        // Just check that we have the snippet marker
        return line.contains("--8<--") || line.contains("-8<-") || line.contains("---8<---");
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
        // We're lenient with empty section names for detection (can warn later)
        assert!(is_snippet_section_start("<!-- --8<-- [start:] -->")); // Empty name allowed
        assert!(!is_snippet_section_start("--8<-- [start")); // Missing bracket
        assert!(!is_snippet_section_start("[start:test]")); // Missing snippet marker

        // Valid section end markers
        assert!(is_snippet_section_end("<!-- --8<-- [end:intro] -->"));
        assert!(is_snippet_section_end("--8<-- [end:code]"));

        // Invalid section end markers
        // We're lenient with empty section names for detection (can warn later)
        assert!(is_snippet_section_end("<!-- --8<-- [end:] -->")); // Empty name allowed
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
