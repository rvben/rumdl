//! Quarto div and callout block detection utilities
//!
//! This module provides detection for Quarto/Pandoc fenced div syntax which uses
//! `:::` markers to create structured content blocks.
//!
//! Common patterns:
//! - `::: {.callout-note}` - Callout block with type
//! - `::: {.callout-warning}` - Warning callout
//! - `::: {#myid .class}` - Generic div with id and class
//! - `::: myclass` - Simple div with class (shorthand)
//! - `:::` - Closing marker
//!
//! Callout types: `callout-note`, `callout-warning`, `callout-tip`,
//! `callout-important`, `callout-caution`

use regex::Regex;
use std::sync::LazyLock;

use crate::utils::skip_context::ByteRange;

/// Pattern to match div opening markers
/// Matches: ::: {.class}, ::: {#id .class}, ::: classname, etc.
/// Does NOT match a closing ::: on its own
static DIV_OPEN_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*):::\s*(?:\{[^}]+\}|\S+)").unwrap());

/// Pattern to match div closing markers
/// Matches: ::: (with optional whitespace before and after)
static DIV_CLOSE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*):::\s*$").unwrap());

/// Pattern to match callout blocks specifically
/// Callout types: note, warning, tip, important, caution
static CALLOUT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\s*):::\s*\{[^}]*\.callout-(?:note|warning|tip|important|caution)[^}]*\}").unwrap()
});

/// Pattern to match Pandoc-style attributes on any element
/// Matches: {#id}, {.class}, {#id .class key="value"}, etc.
/// Note: We match the entire attribute block including contents
static PANDOC_ATTR_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{[^}]+\}").unwrap());

/// Check if a line is a div opening marker
pub fn is_div_open(line: &str) -> bool {
    DIV_OPEN_PATTERN.is_match(line)
}

/// Check if a line is a div closing marker (just `:::`)
pub fn is_div_close(line: &str) -> bool {
    DIV_CLOSE_PATTERN.is_match(line)
}

/// Check if a line is a callout block opening
pub fn is_callout_open(line: &str) -> bool {
    CALLOUT_PATTERN.is_match(line)
}

/// Check if a line contains Pandoc-style attributes
pub fn has_pandoc_attributes(line: &str) -> bool {
    PANDOC_ATTR_PATTERN.is_match(line)
}

/// Get the indentation level of a div marker
pub fn get_div_indent(line: &str) -> usize {
    let mut indent = 0;
    for c in line.chars() {
        match c {
            ' ' => indent += 1,
            '\t' => indent += 4, // Tabs expand to 4 spaces (CommonMark)
            _ => break,
        }
    }
    indent
}

/// Track div nesting state for a document
#[derive(Debug, Clone, Default)]
pub struct DivTracker {
    /// Stack of div indentation levels for nesting tracking
    indent_stack: Vec<usize>,
}

impl DivTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a line and return whether we're inside a div after processing
    pub fn process_line(&mut self, line: &str) -> bool {
        let trimmed = line.trim_start();

        if trimmed.starts_with(":::") {
            let indent = get_div_indent(line);

            if is_div_close(line) {
                // Closing marker - pop the matching div from stack
                // Pop the top div if its indent is >= the closing marker's indent
                if let Some(&top_indent) = self.indent_stack.last()
                    && top_indent >= indent
                {
                    self.indent_stack.pop();
                }
            } else if is_div_open(line) {
                // Opening marker - push to stack
                self.indent_stack.push(indent);
            }
        }

        !self.indent_stack.is_empty()
    }

    /// Check if we're currently inside a div
    pub fn is_inside_div(&self) -> bool {
        !self.indent_stack.is_empty()
    }

    /// Get current nesting depth
    pub fn depth(&self) -> usize {
        self.indent_stack.len()
    }
}

/// Detect Quarto div block ranges in content
/// Returns a vector of byte ranges (start, end) for each div block
pub fn detect_div_block_ranges(content: &str) -> Vec<ByteRange> {
    let mut ranges = Vec::new();
    let mut tracker = DivTracker::new();
    let mut div_start: Option<usize> = None;
    let mut byte_offset = 0;

    for line in content.lines() {
        let line_len = line.len();
        let was_inside = tracker.is_inside_div();
        let is_inside = tracker.process_line(line);

        // Started a new div block
        if !was_inside && is_inside {
            div_start = Some(byte_offset);
        }
        // Exited a div block
        else if was_inside
            && !is_inside
            && let Some(start) = div_start.take()
        {
            // End at the start of the closing line
            ranges.push(ByteRange {
                start,
                end: byte_offset + line_len,
            });
        }

        // Account for newline
        byte_offset += line_len + 1;
    }

    // Handle unclosed divs at end of document
    if let Some(start) = div_start {
        ranges.push(ByteRange {
            start,
            end: content.len(),
        });
    }

    ranges
}

/// Check if a byte position is within a div block
pub fn is_within_div_block_ranges(ranges: &[ByteRange], position: usize) -> bool {
    ranges.iter().any(|r| position >= r.start && position < r.end)
}

/// Extract class names from a Pandoc attribute block
/// Returns classes like "callout-note", "bordered", etc.
pub fn extract_classes(line: &str) -> Vec<String> {
    let mut classes = Vec::new();

    // Look for {.class ...} patterns
    if let Some(captures) = PANDOC_ATTR_PATTERN.find(line) {
        let attr_block = captures.as_str();
        // Strip the braces to get the inner content
        let inner = attr_block.trim_start_matches('{').trim_end_matches('}').trim();

        // Extract each .class by splitting on whitespace and looking for . prefix
        for part in inner.split_whitespace() {
            if let Some(class) = part.strip_prefix('.') {
                // Clean up any trailing = if followed by attribute value
                let class = class.split('=').next().unwrap_or(class);
                if !class.is_empty() {
                    classes.push(class.to_string());
                }
            }
        }
    }

    classes
}

/// Extract the ID from a Pandoc attribute block
pub fn extract_id(line: &str) -> Option<String> {
    if let Some(captures) = PANDOC_ATTR_PATTERN.find(line) {
        let attr_block = captures.as_str();
        // Strip the braces to get the inner content
        let inner = attr_block.trim_start_matches('{').trim_end_matches('}').trim();

        // Extract #id by splitting on whitespace and looking for # prefix
        for part in inner.split_whitespace() {
            if let Some(id) = part.strip_prefix('#') {
                // Clean up any trailing = if followed by attribute value
                let id = id.split('=').next().unwrap_or(id);
                if !id.is_empty() {
                    return Some(id.to_string());
                }
            }
        }
    }
    None
}

// ============================================================================
// Pandoc/Quarto Citation Support
// ============================================================================
//
// Pandoc citation syntax:
// - Inline citation: @smith2020
// - Parenthetical citation: [@smith2020]
// - Suppress author: [-@smith2020]
// - With locator: [@smith2020, p. 10]
// - Multiple citations: [@smith2020; @jones2021]
// - With prefix: [see @smith2020]
//
// Citation keys must start with a letter, digit, or underscore, and may contain
// alphanumerics, underscores, hyphens, periods, and colons.

/// Pattern to match bracketed citations: [@key], [-@key], [see @key], [@a; @b]
static BRACKETED_CITATION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // Matches [...] containing at least one @key
    Regex::new(r"\[[^\]]*@[a-zA-Z0-9_][a-zA-Z0-9_:.#$%&\-+?<>~/]*[^\]]*\]").unwrap()
});

/// Pattern to match inline citations: @key (not inside brackets)
/// Citation key: starts with letter/digit/underscore, contains alphanumerics and some punctuation
/// The @ must be preceded by whitespace, start of line, or punctuation (not alphanumeric)
static INLINE_CITATION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // Match @ at start of string, after whitespace, or after non-alphanumeric (except @[)
    Regex::new(r"(?:^|[\s\(\[\{,;:])(@[a-zA-Z0-9_][a-zA-Z0-9_:.#$%&\-+?<>~/]*)").unwrap()
});

/// Quick check if text might contain citations
#[inline]
pub fn has_citations(text: &str) -> bool {
    text.contains('@')
}

/// Find all citation ranges in content (byte ranges)
/// Returns ranges for both bracketed [@key] and inline @key citations
pub fn find_citation_ranges(content: &str) -> Vec<ByteRange> {
    let mut ranges = Vec::new();

    // Find bracketed citations first (higher priority)
    for mat in BRACKETED_CITATION_PATTERN.find_iter(content) {
        ranges.push(ByteRange {
            start: mat.start(),
            end: mat.end(),
        });
    }

    // Find inline citations (but not inside already-found brackets)
    for cap in INLINE_CITATION_PATTERN.captures_iter(content) {
        if let Some(mat) = cap.get(1) {
            let start = mat.start();
            // Skip if this is inside a bracketed citation
            if !ranges.iter().any(|r| start >= r.start && start < r.end) {
                ranges.push(ByteRange { start, end: mat.end() });
            }
        }
    }

    // Sort by start position
    ranges.sort_by_key(|r| r.start);
    ranges
}

/// Check if a byte position is within a citation
pub fn is_in_citation(ranges: &[ByteRange], position: usize) -> bool {
    ranges.iter().any(|r| position >= r.start && position < r.end)
}

/// Extract citation key from a citation string (removes @ prefix)
pub fn extract_citation_key(citation: &str) -> Option<&str> {
    citation.strip_prefix('@').or_else(|| {
        // Handle [-@key] format
        citation.strip_prefix("[-@").and_then(|s| s.strip_suffix(']'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_div_open_detection() {
        // Valid div openings
        assert!(is_div_open("::: {.callout-note}"));
        assert!(is_div_open("::: {.callout-warning}"));
        assert!(is_div_open("::: {#myid .class}"));
        assert!(is_div_open("::: bordered"));
        assert!(is_div_open("  ::: {.note}")); // Indented
        assert!(is_div_open("::: {.callout-tip title=\"My Title\"}"));

        // Invalid patterns
        assert!(!is_div_open(":::")); // Just closing marker
        assert!(!is_div_open(":::  ")); // Just closing with trailing space
        assert!(!is_div_open("Regular text"));
        assert!(!is_div_open("# Heading"));
        assert!(!is_div_open("```python")); // Code fence
    }

    #[test]
    fn test_div_close_detection() {
        assert!(is_div_close(":::"));
        assert!(is_div_close(":::  "));
        assert!(is_div_close("  :::"));
        assert!(is_div_close("    :::  "));

        assert!(!is_div_close("::: {.note}"));
        assert!(!is_div_close("::: class"));
        assert!(!is_div_close(":::note"));
    }

    #[test]
    fn test_callout_detection() {
        assert!(is_callout_open("::: {.callout-note}"));
        assert!(is_callout_open("::: {.callout-warning}"));
        assert!(is_callout_open("::: {.callout-tip}"));
        assert!(is_callout_open("::: {.callout-important}"));
        assert!(is_callout_open("::: {.callout-caution}"));
        assert!(is_callout_open("::: {#myid .callout-note}"));
        assert!(is_callout_open("::: {.callout-note title=\"Title\"}"));

        assert!(!is_callout_open("::: {.note}")); // Not a callout
        assert!(!is_callout_open("::: {.bordered}")); // Not a callout
        assert!(!is_callout_open("::: callout-note")); // Missing braces
    }

    #[test]
    fn test_div_tracker() {
        let mut tracker = DivTracker::new();

        // Enter a div
        assert!(tracker.process_line("::: {.callout-note}"));
        assert!(tracker.is_inside_div());
        assert_eq!(tracker.depth(), 1);

        // Inside content
        assert!(tracker.process_line("This is content."));
        assert!(tracker.is_inside_div());

        // Exit the div
        assert!(!tracker.process_line(":::"));
        assert!(!tracker.is_inside_div());
        assert_eq!(tracker.depth(), 0);
    }

    #[test]
    fn test_nested_divs() {
        let mut tracker = DivTracker::new();

        // Outer div
        assert!(tracker.process_line("::: {.outer}"));
        assert_eq!(tracker.depth(), 1);

        // Inner div
        assert!(tracker.process_line("  ::: {.inner}"));
        assert_eq!(tracker.depth(), 2);

        // Content
        assert!(tracker.process_line("    Content"));
        assert!(tracker.is_inside_div());

        // Close inner
        assert!(tracker.process_line("  :::"));
        assert_eq!(tracker.depth(), 1);

        // Close outer
        assert!(!tracker.process_line(":::"));
        assert_eq!(tracker.depth(), 0);
    }

    #[test]
    fn test_detect_div_block_ranges() {
        let content = r#"# Heading

::: {.callout-note}
This is a note.
:::

Regular text.

::: {.bordered}
Content here.
:::
"#;
        let ranges = detect_div_block_ranges(content);
        assert_eq!(ranges.len(), 2);

        // First div
        let first_div_content = &content[ranges[0].start..ranges[0].end];
        assert!(first_div_content.contains("callout-note"));
        assert!(first_div_content.contains("This is a note"));

        // Second div
        let second_div_content = &content[ranges[1].start..ranges[1].end];
        assert!(second_div_content.contains("bordered"));
        assert!(second_div_content.contains("Content here"));
    }

    #[test]
    fn test_extract_classes() {
        assert_eq!(extract_classes("::: {.callout-note}"), vec!["callout-note"]);
        assert_eq!(
            extract_classes("::: {#myid .bordered .highlighted}"),
            vec!["bordered", "highlighted"]
        );
        assert_eq!(
            extract_classes("::: {.callout-warning title=\"Alert\"}"),
            vec!["callout-warning"]
        );

        assert!(extract_classes("Regular text").is_empty());
        assert!(extract_classes("::: classname").is_empty()); // No braces
    }

    #[test]
    fn test_extract_id() {
        assert_eq!(extract_id("::: {#myid}"), Some("myid".to_string()));
        assert_eq!(extract_id("::: {#myid .class}"), Some("myid".to_string()));
        assert_eq!(extract_id("::: {.class #custom-id}"), Some("custom-id".to_string()));

        assert_eq!(extract_id("::: {.class}"), None);
        assert_eq!(extract_id("Regular text"), None);
    }

    #[test]
    fn test_pandoc_attributes() {
        assert!(has_pandoc_attributes("# Heading {#custom-id}"));
        assert!(has_pandoc_attributes("# Heading {.unnumbered}"));
        assert!(has_pandoc_attributes("![Image](path.png){#fig-1 width=\"50%\"}"));
        assert!(has_pandoc_attributes("{#id .class key=\"value\"}"));

        assert!(!has_pandoc_attributes("# Heading"));
        assert!(!has_pandoc_attributes("Regular text"));
        assert!(!has_pandoc_attributes("{}"));
    }

    #[test]
    fn test_div_with_title_attribute() {
        let content = r#"::: {.callout-note title="Important Note"}
This is the content of the note.
It can span multiple lines.
:::
"#;
        let ranges = detect_div_block_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert!(is_callout_open("::: {.callout-note title=\"Important Note\"}"));
    }

    #[test]
    fn test_unclosed_div() {
        let content = r#"::: {.callout-note}
This note is never closed.
"#;
        let ranges = detect_div_block_ranges(content);
        assert_eq!(ranges.len(), 1);
        // Should include all content to end of document
        assert_eq!(ranges[0].end, content.len());
    }

    #[test]
    fn test_heading_inside_callout() {
        let content = r#"::: {.callout-warning}
## Warning Title

Warning content here.
:::
"#;
        let ranges = detect_div_block_ranges(content);
        assert_eq!(ranges.len(), 1);

        let div_content = &content[ranges[0].start..ranges[0].end];
        assert!(div_content.contains("## Warning Title"));
    }

    // Citation tests
    #[test]
    fn test_has_citations() {
        assert!(has_citations("See @smith2020 for details."));
        assert!(has_citations("[@smith2020]"));
        assert!(has_citations("Multiple [@a; @b] citations"));
        assert!(!has_citations("No citations here"));
        // has_citations is just a quick @ check - emails will pass (intended behavior)
        assert!(has_citations("Email: user@example.com"));
    }

    #[test]
    fn test_bracketed_citation_detection() {
        let content = "See [@smith2020] for more info.";
        let ranges = find_citation_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "[@smith2020]");
    }

    #[test]
    fn test_inline_citation_detection() {
        let content = "As @smith2020 argues, this is true.";
        let ranges = find_citation_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "@smith2020");
    }

    #[test]
    fn test_multiple_citations_in_brackets() {
        let content = "See [@smith2020; @jones2021] for details.";
        let ranges = find_citation_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "[@smith2020; @jones2021]");
    }

    #[test]
    fn test_citation_with_prefix() {
        let content = "[see @smith2020, p. 10]";
        let ranges = find_citation_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "[see @smith2020, p. 10]");
    }

    #[test]
    fn test_suppress_author_citation() {
        let content = "The theory [-@smith2020] states that...";
        let ranges = find_citation_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].start..ranges[0].end], "[-@smith2020]");
    }

    #[test]
    fn test_mixed_citations() {
        let content = "@smith2020 argues that [@jones2021] is wrong.";
        let ranges = find_citation_ranges(content);
        assert_eq!(ranges.len(), 2);
        // Inline citation
        assert_eq!(&content[ranges[0].start..ranges[0].end], "@smith2020");
        // Bracketed citation
        assert_eq!(&content[ranges[1].start..ranges[1].end], "[@jones2021]");
    }

    #[test]
    fn test_citation_key_extraction() {
        assert_eq!(extract_citation_key("@smith2020"), Some("smith2020"));
        assert_eq!(extract_citation_key("@Smith_2020"), Some("Smith_2020"));
        assert_eq!(extract_citation_key("@key:with:colons"), Some("key:with:colons"));
        assert_eq!(extract_citation_key("not-a-citation"), None);
    }

    #[test]
    fn test_is_in_citation() {
        let content = "See [@smith2020] here.";
        let ranges = find_citation_ranges(content);

        // Position inside citation
        assert!(is_in_citation(&ranges, 5)); // Inside [@smith2020]
        // Position outside citation
        assert!(!is_in_citation(&ranges, 0)); // "See "
        assert!(!is_in_citation(&ranges, 17)); // " here."
    }

    #[test]
    fn test_email_not_confused_with_citation() {
        // Email addresses should not match as inline citations when properly filtered
        // The has_citations() is just a quick check, but find_citation_ranges uses more strict patterns
        let content = "Contact user@example.com for help.";
        let ranges = find_citation_ranges(content);
        // Email should not be detected as citation (@ is preceded by alphanumeric)
        assert!(
            ranges.is_empty()
                || !ranges.iter().any(|r| {
                    let s = &content[r.start..r.end];
                    s.contains("example.com")
                })
        );
    }
}
