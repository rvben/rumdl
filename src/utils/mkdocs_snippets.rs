use regex::Regex;
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
use std::sync::LazyLock;

/// Pattern to match valid snippet markers: -{1,}8<-{1,}
/// Based on PyMdown Extensions Snippets specification
static BARE_SNIPPET_MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^;*-+8<-+$", // Optional semicolons, then dashes-8<-dashes only
    )
    .unwrap()
});

/// Pattern to match snippet with quoted file path
/// Lenient: accepts unclosed quotes for detection (can warn later)
static SNIPPET_WITH_FILE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"-+8<-+\s+["']"#, // Just check for quote after snippet marker
    )
    .unwrap()
});

/// Pattern to match section markers
static SECTION_MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-+8<-+\s*\[(start|end):[^\]]*\]").unwrap());

/// Pattern to match invalid asymmetric marker
static INVALID_ASYMMETRIC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?:^|\s)--8<-\s+["']"#, // --8<- followed by quote is invalid
    )
    .unwrap()
});

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
    // Also check with comment prefixes: # -8<- [start:name]
    if SECTION_MARKER.is_match(trimmed) {
        return true;
    }

    // Check for comment-prefixed section markers (# -8<- [start:name])
    let without_comment = trimmed.trim_start_matches(['#', ';', '/', '*']).trim_start();
    if (without_comment.starts_with("-8<-")
        || without_comment.starts_with("--8<--")
        || without_comment.starts_with("---8<---"))
        && (without_comment.contains("[start:") || without_comment.contains("[end:"))
    {
        return true;
    }

    // Block format: bare marker is valid for multi-line snippet blocks
    // According to PyMdown Extensions spec, bare markers like --8<-- on their own line
    // are valid when used as opening/closing delimiters for multi-file blocks:
    // --8<--
    // file1.md
    // file2.md
    // --8<--
    // Check for trailing whitespace (space or tab) BEFORE trimming
    let trimmed_start = line.trim_start();
    let has_trailing_whitespace = trimmed_start.ends_with(' ') || trimmed_start.ends_with('\t');
    if BARE_SNIPPET_MARKER.is_match(trimmed) && !has_trailing_whitespace {
        return true; // Valid bare marker for block format
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
    // # -8<- [start:section_name]  (comment format for source files)
    // ; -8<- [start:section_name]  (comment format for ini files)

    if !line.contains("start:") {
        return false;
    }

    // Must have proper bracket structure
    if let Some(_start_idx) = line.find("[start:")
        && let Some(_end_idx) = line[_start_idx..].find(']')
    {
        // Empty section names are allowed (lenient for detection)
        let trimmed = line.trim();

        // Handle HTML comments specially
        let content_to_check = if trimmed.starts_with("<!--") && trimmed.ends_with("-->") {
            // Extract content from HTML comment
            trimmed.trim_start_matches("<!--").trim_end_matches("-->").trim()
        } else {
            // For other comment styles (# ; / *)
            let without_comment = trimmed.trim_start_matches(['#', ';', '/', '*']);
            without_comment.trim_start()
        };

        return content_to_check.starts_with("--8<--")
            || content_to_check.starts_with("-8<-")
            || content_to_check.starts_with("---8<---");
    }

    false
}

/// Check if a line is a snippet section end marker
pub fn is_snippet_section_end(line: &str) -> bool {
    // Check for patterns like:
    // <!-- --8<-- [end:section_name] -->
    // --8<-- [end:section_name]
    // -8<- [end:section_name]
    // # -8<- [end:section_name]  (comment format for source files)
    // ; -8<- [end:section_name]  (comment format for ini files)

    if !line.contains("end:") {
        return false;
    }

    // Must have proper bracket structure
    if let Some(_start_idx) = line.find("[end:")
        && let Some(_end_idx) = line[_start_idx..].find(']')
    {
        // Empty section names are allowed (lenient for detection)
        let trimmed = line.trim();

        // Handle HTML comments specially
        let content_to_check = if trimmed.starts_with("<!--") && trimmed.ends_with("-->") {
            // Extract content from HTML comment
            trimmed.trim_start_matches("<!--").trim_end_matches("-->").trim()
        } else {
            // For other comment styles (# ; / *)
            let without_comment = trimmed.trim_start_matches(['#', ';', '/', '*']);
            without_comment.trim_start()
        };

        return content_to_check.starts_with("--8<--")
            || content_to_check.starts_with("-8<-")
            || content_to_check.starts_with("---8<---");
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

/// Check if a line is a bare snippet block delimiter (for multi-line blocks)
pub fn is_snippet_block_delimiter(line: &str) -> bool {
    let trimmed = line.trim();
    // Check for trailing whitespace (space or tab) BEFORE full trim
    let trimmed_start = line.trim_start();
    let has_trailing_whitespace = trimmed_start.ends_with(' ') || trimmed_start.ends_with('\t');
    // Bare markers without trailing whitespace are valid block delimiters
    BARE_SNIPPET_MARKER.is_match(trimmed) && !has_trailing_whitespace
}

/// Check if a position is within a multi-line snippet block
/// Multi-line blocks have the format:
/// --8<--
/// file1.md
/// file2.md
/// --8<--
pub fn is_within_snippet_block(content: &str, position: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    let mut byte_pos = 0;
    let mut in_block = false;

    for line in lines {
        let line_end = byte_pos + line.len();

        // Check if this is a block delimiter that toggles state
        if is_snippet_block_delimiter(line) {
            if byte_pos <= position && position <= line_end {
                // The position is on the delimiter itself
                return true;
            }
            in_block = !in_block;
        }

        // Check if position is within this line and we're in a block
        if in_block && byte_pos <= position && position <= line_end {
            return true;
        }

        // Move to next line (account for newline character)
        byte_pos = line_end + 1;
    }

    false
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

        // Bare markers are valid for multi-line blocks
        assert!(is_snippet_marker("--8<--")); // Valid block delimiter
        assert!(is_snippet_marker("-8<-")); // Shorter form
        assert!(is_snippet_marker("---8<---")); // Longer form

        // Invalid snippets with trailing spaces
        assert!(!is_snippet_marker("--8<-- ")); // Trailing space suggests missing file
        assert!(!is_snippet_marker("<!-- --8<-- -->")); // Empty HTML comment snippet

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
        assert!(is_snippet_section_start("# -8<- [start:remote-content]")); // Comment style

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

    #[test]
    fn test_multi_line_snippet_blocks() {
        let content = r#"# Document

Some content before.

--8<--
file1.md
file2.md
https://raw.githubusercontent.com/example/repo/main/file.md
--8<--

Some content after.

-8<-
another_file.txt
-8<-

More content.
"#;

        // Test positions within the first block
        let file1_pos = content.find("file1.md").unwrap();
        let file2_pos = content.find("file2.md").unwrap();
        let url_pos = content.find("https://raw.githubusercontent.com").unwrap();

        // Test positions outside blocks
        let before_pos = content.find("Some content before").unwrap();
        let after_pos = content.find("Some content after").unwrap();
        let more_pos = content.find("More content").unwrap();

        // Test positions on delimiters
        let first_delimiter = content.find("--8<--").unwrap();
        let second_delimiter = content.rfind("--8<--").unwrap();

        // Test position in second block
        let another_file_pos = content.find("another_file.txt").unwrap();

        // Assert content within blocks is detected
        assert!(
            is_within_snippet_block(content, file1_pos),
            "file1.md should be in block"
        );
        assert!(
            is_within_snippet_block(content, file2_pos),
            "file2.md should be in block"
        );
        assert!(is_within_snippet_block(content, url_pos), "URL should be in block");
        assert!(
            is_within_snippet_block(content, another_file_pos),
            "another_file.txt should be in block"
        );

        // Assert delimiters themselves are detected
        assert!(
            is_within_snippet_block(content, first_delimiter),
            "First delimiter should be detected"
        );
        assert!(
            is_within_snippet_block(content, second_delimiter),
            "Second delimiter should be detected"
        );

        // Assert content outside blocks is not detected
        assert!(
            !is_within_snippet_block(content, before_pos),
            "Content before block should not be detected"
        );
        assert!(
            !is_within_snippet_block(content, after_pos),
            "Content between blocks should not be detected"
        );
        assert!(
            !is_within_snippet_block(content, more_pos),
            "Content after blocks should not be detected"
        );
    }

    #[test]
    fn test_snippet_block_delimiter() {
        // Valid block delimiters
        assert!(is_snippet_block_delimiter("--8<--"));
        assert!(is_snippet_block_delimiter("-8<-"));
        assert!(is_snippet_block_delimiter("---8<---"));
        assert!(!is_snippet_block_delimiter("  --8<--  ")); // With trailing whitespace = invalid
        assert!(!is_snippet_block_delimiter("\t-8<-\t")); // With trailing tabs = invalid
        assert!(is_snippet_block_delimiter("  --8<--")); // Leading whitespace only is OK
        assert!(is_snippet_block_delimiter("\t--8<--")); // Leading tabs only is OK

        // Invalid delimiters
        assert!(!is_snippet_block_delimiter("--8<-- ")); // Trailing space after trim
        assert!(!is_snippet_block_delimiter("--8<-- file.md")); // With content
        assert!(!is_snippet_block_delimiter("<!-- --8<-- -->")); // In HTML comment
    }
}
