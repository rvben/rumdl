//! MkDocs HTML with markdown attribute detection
//!
//! Detects HTML elements (primarily divs) with the `markdown` attribute,
//! which tells MkDocs/Python-Markdown to process the content as Markdown.
//!
//! Common patterns:
//! - `<div class="grid cards" markdown>` - Grid cards
//! - `<div markdown="1">` - Explicit markdown processing
//! - `<div markdown="block">` - Block-level markdown

use regex::Regex;
use std::sync::LazyLock;

/// Pattern to detect HTML opening tags with markdown attribute.
/// Handles:
/// - `<div markdown>` or `<div markdown="1">` or `<div markdown="block">`
/// - Attribute can appear anywhere in the tag
/// - Case-insensitive tag names (HTML is case-insensitive)
/// - Various attribute value formats
static MARKDOWN_HTML_OPEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)^(\s*)<(div|section|article|aside|details|figure|footer|header|main|nav)\b[^>]*\bmarkdown\b[^>]*>"#,
    )
    .unwrap()
});

/// Check if a line starts a markdown-enabled HTML block
fn is_markdown_html_start(line: &str) -> bool {
    MARKDOWN_HTML_OPEN.is_match(line)
}

/// Get the tag name from a markdown HTML opening line
fn get_tag_name(line: &str) -> Option<String> {
    MARKDOWN_HTML_OPEN
        .captures(line)
        .map(|caps| caps.get(2).map(|m| m.as_str().to_lowercase()).unwrap_or_default())
}

/// Track state for markdown HTML block parsing
#[derive(Debug, Default)]
pub struct MarkdownHtmlTracker {
    /// Stack of open tags (tag name, depth at that level)
    tag_stack: Vec<(String, usize)>,
    /// Current nesting depth
    depth: usize,
}

impl MarkdownHtmlTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a line and return whether the line is inside a markdown HTML block.
    /// Returns true if:
    /// - This line opens a new markdown HTML block
    /// - This line is part of an existing markdown HTML block (even if it closes it)
    pub fn process_line(&mut self, line: &str) -> bool {
        let trimmed = line.trim();

        // Check for opening tag
        if is_markdown_html_start(line) {
            if let Some(tag) = get_tag_name(line) {
                self.depth += 1;
                self.tag_stack.push((tag.clone(), self.depth));

                // Check if this line also closes the tag (self-contained)
                let line_lower = line.to_lowercase();
                if Self::count_closes_lowered(&line_lower, &tag) > 0 {
                    self.depth -= 1;
                    self.tag_stack.pop();
                }
            }
            return true;
        }

        // If we're inside a markdown HTML block at the start of this line
        if !self.tag_stack.is_empty() {
            // Lowercase the line once for all tag comparisons
            let line_lower = trimmed.to_lowercase();

            // Collect tag names by reference before mutating depth
            let tags: Vec<String> = self.tag_stack.iter().map(|(tag, _)| tag.clone()).collect();
            for tag in &tags {
                let opens = Self::count_opens_lowered(&line_lower, tag);
                let closes = Self::count_closes_lowered(&line_lower, tag);

                self.depth += opens;

                for _ in 0..closes {
                    if self.depth > 0 {
                        self.depth -= 1;
                    }
                }
            }

            // Clean up stack when depth reaches initial level
            while let Some((_, start_depth)) = self.tag_stack.last() {
                if self.depth < *start_depth {
                    self.tag_stack.pop();
                } else {
                    break;
                }
            }

            // Return true because this line was inside the block at the start
            // (even if it also closes the block)
            return true;
        }

        false
    }

    /// Count opening tags of a specific type in a pre-lowercased line.
    /// `tag` is already lowercase (stored that way in `tag_stack`).
    fn count_opens_lowered(line_lower: &str, tag: &str) -> usize {
        let open_pattern = format!("<{tag}");
        let mut count = 0;
        let mut search_start = 0;

        while let Some(pos) = line_lower[search_start..].find(&open_pattern) {
            let abs_pos = search_start + pos;
            let after_tag = abs_pos + open_pattern.len();

            // Verify it's a tag boundary (followed by whitespace, >, or /)
            if after_tag >= line_lower.len()
                || line_lower[after_tag..].starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/')
            {
                count += 1;
            }
            search_start = after_tag;
        }
        count
    }

    /// Count closing tags of a specific type in a pre-lowercased line.
    /// `tag` is already lowercase (stored that way in `tag_stack`).
    fn count_closes_lowered(line_lower: &str, tag: &str) -> usize {
        let close_pattern = format!("</{tag}");
        let mut count = 0;
        let mut search_start = 0;

        while let Some(pos) = line_lower[search_start..].find(&close_pattern) {
            let abs_pos = search_start + pos;
            let after_tag = abs_pos + close_pattern.len();

            // Find the closing > (may have whitespace before it)
            if let Some(rest) = line_lower.get(after_tag..)
                && rest.trim_start().starts_with('>')
            {
                count += 1;
            }
            search_start = after_tag;
        }
        count
    }

    /// Check if currently inside a markdown HTML block
    pub fn is_inside(&self) -> bool {
        !self.tag_stack.is_empty()
    }

    /// Reset the tracker state
    pub fn reset(&mut self) {
        self.tag_stack.clear();
        self.depth = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_html_detection() {
        // Basic patterns
        assert!(is_markdown_html_start("<div markdown>"));
        assert!(is_markdown_html_start("<div class=\"grid cards\" markdown>"));
        assert!(is_markdown_html_start("<div markdown=\"1\">"));
        assert!(is_markdown_html_start("<div markdown=\"block\">"));

        // Attribute order variations
        assert!(is_markdown_html_start("<div markdown class=\"test\">"));
        assert!(is_markdown_html_start("<div id=\"foo\" markdown>"));

        // Case insensitivity
        assert!(is_markdown_html_start("<DIV markdown>"));
        assert!(is_markdown_html_start("<Div Markdown>"));

        // With indentation
        assert!(is_markdown_html_start("  <div markdown>"));
        assert!(is_markdown_html_start("    <div class=\"grid\" markdown>"));

        // Other valid HTML5 elements
        assert!(is_markdown_html_start("<section markdown>"));
        assert!(is_markdown_html_start("<article markdown>"));
        assert!(is_markdown_html_start("<details markdown>"));

        // Should NOT match
        assert!(!is_markdown_html_start("<div class=\"test\">"));
        assert!(!is_markdown_html_start("<span markdown>")); // span not in allowed list
        assert!(!is_markdown_html_start("text with markdown word"));
        assert!(!is_markdown_html_start("<div>markdown</div>"));
    }

    #[test]
    fn test_tracker_basic() {
        let mut tracker = MarkdownHtmlTracker::new();

        assert!(!tracker.is_inside());

        assert!(tracker.process_line("<div class=\"grid cards\" markdown>"));
        assert!(tracker.is_inside());

        assert!(tracker.process_line("-   Content here"));
        assert!(tracker.is_inside());

        assert!(tracker.process_line("    ---"));
        assert!(tracker.is_inside());

        // Close the div
        tracker.process_line("</div>");
        assert!(!tracker.is_inside());
    }

    #[test]
    fn test_tracker_nested() {
        let mut tracker = MarkdownHtmlTracker::new();

        tracker.process_line("<div markdown>");
        assert!(tracker.is_inside());

        tracker.process_line("<div>nested</div>");
        assert!(tracker.is_inside());

        tracker.process_line("</div>");
        assert!(!tracker.is_inside());
    }

    #[test]
    fn test_grid_cards_pattern() {
        let content = r#"<div class="grid cards" markdown>

-   :zap:{ .lg .middle } **Built for speed**

    ---

    Written in Rust.

</div>"#;

        let mut tracker = MarkdownHtmlTracker::new();
        let mut inside_lines = Vec::new();

        for (i, line) in content.lines().enumerate() {
            let inside = tracker.process_line(line);
            if inside {
                inside_lines.push(i);
            }
        }

        // All lines except the last </div> should be marked as inside
        assert!(inside_lines.contains(&0)); // <div ...>
        assert!(inside_lines.contains(&2)); // -   :zap:...
        assert!(inside_lines.contains(&4)); // ---
        assert!(inside_lines.contains(&6)); // Written in Rust.
        assert!(!tracker.is_inside()); // After </div>
    }

    #[test]
    fn test_same_line_open_close() {
        let mut tracker = MarkdownHtmlTracker::new();

        // Single line with both open and close
        let result = tracker.process_line("<div markdown>content</div>");
        assert!(result); // The line itself is part of the block
        assert!(!tracker.is_inside()); // But after processing, we're outside
    }

    #[test]
    fn test_multiple_sequential_blocks() {
        let mut tracker = MarkdownHtmlTracker::new();

        // First block
        assert!(tracker.process_line("<div markdown>"));
        assert!(tracker.is_inside());
        assert!(tracker.process_line("Content 1"));
        tracker.process_line("</div>");
        assert!(!tracker.is_inside());

        // Second block (should work independently)
        assert!(tracker.process_line("<section markdown>"));
        assert!(tracker.is_inside());
        assert!(tracker.process_line("Content 2"));
        tracker.process_line("</section>");
        assert!(!tracker.is_inside());
    }

    #[test]
    fn test_deeply_nested_same_tag() {
        let mut tracker = MarkdownHtmlTracker::new();

        assert!(tracker.process_line("<div markdown>"));
        assert!(tracker.is_inside());

        // Nested div (without markdown attr)
        assert!(tracker.process_line("<div class=\"inner\">"));
        assert!(tracker.is_inside());

        // Close inner div
        assert!(tracker.process_line("</div>"));
        assert!(tracker.is_inside()); // Still inside outer div

        // Close outer div
        tracker.process_line("</div>");
        assert!(!tracker.is_inside());
    }

    #[test]
    fn test_deeply_nested_different_tags() {
        let mut tracker = MarkdownHtmlTracker::new();

        assert!(tracker.process_line("<article markdown>"));
        assert!(tracker.is_inside());

        // Inner section (without markdown)
        assert!(tracker.process_line("<section>"));
        assert!(tracker.is_inside());

        // Close section - tracker only tracks article
        assert!(tracker.process_line("</section>"));
        assert!(tracker.is_inside());

        // Close article
        tracker.process_line("</article>");
        assert!(!tracker.is_inside());
    }

    #[test]
    fn test_multiple_closes_same_line() {
        let mut tracker = MarkdownHtmlTracker::new();

        assert!(tracker.process_line("<div markdown>"));
        assert!(tracker.process_line("<div>inner</div></div>"));
        assert!(!tracker.is_inside());
    }

    #[test]
    fn test_count_opens_boundary_check() {
        // Should match (input is pre-lowercased)
        assert_eq!(MarkdownHtmlTracker::count_opens_lowered("<div>", "div"), 1);
        assert_eq!(MarkdownHtmlTracker::count_opens_lowered("<div class='x'>", "div"), 1);
        assert_eq!(MarkdownHtmlTracker::count_opens_lowered("<div>", "div"), 1);
        assert_eq!(MarkdownHtmlTracker::count_opens_lowered("<div/><div>", "div"), 2);

        // Should NOT match (divider is not div)
        assert_eq!(MarkdownHtmlTracker::count_opens_lowered("<divider>", "div"), 0);
        assert_eq!(MarkdownHtmlTracker::count_opens_lowered("<dividend>", "div"), 0);

        // Case-insensitive via pre-lowercased input
        assert_eq!(
            MarkdownHtmlTracker::count_opens_lowered(&"<DIV>".to_lowercase(), "div"),
            1
        );
    }

    #[test]
    fn test_count_closes_variations() {
        // Input is pre-lowercased
        assert_eq!(MarkdownHtmlTracker::count_closes_lowered("</div>", "div"), 1);
        assert_eq!(
            MarkdownHtmlTracker::count_closes_lowered(&"</DIV>".to_lowercase(), "div"),
            1
        );
        assert_eq!(MarkdownHtmlTracker::count_closes_lowered("</div >", "div"), 1);
        assert_eq!(MarkdownHtmlTracker::count_closes_lowered("</div  >", "div"), 1);
        assert_eq!(MarkdownHtmlTracker::count_closes_lowered("</div></div>", "div"), 2);
        assert_eq!(
            MarkdownHtmlTracker::count_closes_lowered("text</div>more</div>end", "div"),
            2
        );
    }

    #[test]
    fn test_reset() {
        let mut tracker = MarkdownHtmlTracker::new();

        tracker.process_line("<div markdown>");
        assert!(tracker.is_inside());

        tracker.reset();
        assert!(!tracker.is_inside());

        // Should work fresh after reset
        tracker.process_line("<section markdown>");
        assert!(tracker.is_inside());
    }
}
