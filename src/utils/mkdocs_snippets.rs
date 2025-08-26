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
/// Check if a line contains MkDocs snippet syntax
pub fn is_snippet_marker(line: &str) -> bool {
    // Check for the ASCII scissors pattern
    if !line.contains("--8<--") && !line.contains("-8<-") {
        return false;
    }

    // Common snippet patterns
    // Direct inclusion: --8<-- "file.md"
    if line.trim().starts_with("--8<--") {
        return true;
    }

    // HTML comment style: <!-- --8<-- ... -->
    if line.contains("<!-- --8<--") || line.contains("<!-- -8<-") {
        return true;
    }

    // Alternative format with single quotes or without quotes
    if line.contains("--8<-- '") || line.contains("-8<- '") {
        return true;
    }

    // Section markers: --8<-- [start:name] or --8<-- [end:name]
    if line.contains("--8<-- [") || line.contains("-8<- [") {
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

    is_snippet_marker(line) && line.contains("[start:")
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

    is_snippet_marker(line) && line.contains("[end:")
}

/// Extract the section name from a snippet marker
pub fn extract_snippet_section_name(line: &str) -> Option<String> {
    // Extract section name from patterns like [start:name] or [end:name]
    if let Some(start_idx) = line.find("[start:").or_else(|| line.find("[end:")) {
        let after_bracket = &line[start_idx..];
        if let Some(colon_idx) = after_bracket.find(':') {
            let after_colon = &after_bracket[colon_idx + 1..];
            if let Some(end_idx) = after_colon.find(']') {
                return Some(after_colon[..end_idx].to_string());
            }
        }
    }
    None
}

/// Check if content is within MkDocs snippet markers
pub fn is_within_snippet_section(content: &str, position: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    let mut byte_pos = 0;
    let mut in_snippet_section = false;

    for line in lines {
        let line_end = byte_pos + line.len();

        if is_snippet_section_start(line) {
            in_snippet_section = true;
        } else if is_snippet_section_end(line) {
            in_snippet_section = false;
        }

        if byte_pos <= position && position <= line_end && in_snippet_section {
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
    fn test_snippet_marker_detection() {
        // Valid snippet markers
        assert!(is_snippet_marker("--8<-- \"file.md\""));
        assert!(is_snippet_marker("--8<-- 'file.md'"));
        assert!(is_snippet_marker("    --8<-- \"nested.md\""));
        assert!(is_snippet_marker("<!-- --8<-- \"file.md\" -->"));
        assert!(is_snippet_marker("<!-- -8<- [start:section] -->"));
        assert!(is_snippet_marker("--8<-- [end:section]"));

        // Invalid patterns
        assert!(!is_snippet_marker("Regular text"));
        assert!(!is_snippet_marker("# Heading"));
        assert!(!is_snippet_marker("8<-- not valid"));
    }

    #[test]
    fn test_section_markers() {
        // Start markers
        assert!(is_snippet_section_start("<!-- --8<-- [start:remote-content] -->"));
        assert!(is_snippet_section_start("-8<- [start:example]"));
        assert!(is_snippet_section_start("--8<-- [start:docs]"));

        // End markers
        assert!(is_snippet_section_end("<!-- --8<-- [end:remote-content] -->"));
        assert!(is_snippet_section_end("-8<- [end:example]"));
        assert!(is_snippet_section_end("--8<-- [end:docs]"));

        // Not section markers
        assert!(!is_snippet_section_start("--8<-- \"file.md\""));
        assert!(!is_snippet_section_end("--8<-- \"file.md\""));
    }

    #[test]
    fn test_extract_section_name() {
        assert_eq!(
            extract_snippet_section_name("<!-- --8<-- [start:remote-content] -->"),
            Some("remote-content".to_string())
        );
        assert_eq!(
            extract_snippet_section_name("--8<-- [end:example]"),
            Some("example".to_string())
        );
        assert_eq!(
            extract_snippet_section_name("-8<- [start:my_section]"),
            Some("my_section".to_string())
        );
        assert_eq!(extract_snippet_section_name("--8<-- \"file.md\""), None);
    }

    #[test]
    fn test_within_snippet_section() {
        let content = r#"# Document

<!-- --8<-- [start:example] -->
This is included content
More content here
<!-- --8<-- [end:example] -->

Regular content"#;

        // Find positions
        let included_pos = content.find("included").unwrap();
        let regular_pos = content.find("Regular").unwrap();

        assert!(is_within_snippet_section(content, included_pos));
        assert!(!is_within_snippet_section(content, regular_pos));
    }
}
