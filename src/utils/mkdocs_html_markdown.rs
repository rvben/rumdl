//! HTML with a `markdown` attribute detection
//!
//! Both Python-Markdown's `md_in_html` extension (MkDocs) and kramdown (Jekyll)
//! let an element opt its content into Markdown parsing with a `markdown`
//! attribute:
//! - `<div class="grid cards" markdown>` - Material grid cards
//! - `<div markdown="1">`, `<details markdown="block">`, `<div markdown="span">`
//! - `<p markdown="1">` and `<h5 markdown="1">` - common in Jekyll docs
//!
//! `<div markdown="0">` is kramdown's opposite and declares the content to be
//! raw HTML, so it does not open a block here.

/// Elements a `markdown` attribute can open a Markdown block on.
///
/// Derived from Python-Markdown's `markdown.util.BLOCK_LEVEL_ELEMENTS`, minus
/// the two groups `md_in_html` excludes from `span_and_blocks_tags`:
///
/// - its raw tags (`canvas`, `math`, `option`, `pre`, `script`, `style`,
///   `textarea`), whose content is never parsed - and four of which CommonMark
///   itself treats as raw-text HTML blocks under every flavor;
/// - its empty tag `hr`, which holds no content, so a tracked block opened on it
///   would never find a closing tag and would swallow the rest of the document.
///
/// kramdown accepts the attribute on any element at all, so this is the narrower
/// of the two rules.
const MARKDOWN_ATTRIBUTE_ELEMENTS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "body",
    "colgroup",
    "dd",
    "details",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "group",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "iframe",
    "legend",
    "li",
    "main",
    "map",
    "menu",
    "nav",
    "noscript",
    "object",
    "ol",
    "output",
    "p",
    "progress",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "ul",
    "video",
];

/// Name of the element a line opens, when that element declares its content to
/// be Markdown.
///
/// Attributes are read with their quoting honoured, so the word `markdown`
/// sitting inside another attribute's value is not an opt-in: neither
/// `<div class="markdown-body">` nor `<video title="editing markdown files">`
/// opens a block. The tag has to close on this line to open anything.
fn markdown_html_open_tag(line: &str) -> Option<String> {
    let line = line.trim_start();
    let bytes = line.as_bytes();
    if bytes.first() != Some(&b'<') || !bytes.get(1).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }

    let mut i = 1;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
        i += 1;
    }
    let tag = line[1..i].to_ascii_lowercase();
    if !MARKDOWN_ATTRIBUTE_ELEMENTS.contains(&tag.as_str()) {
        return None;
    }

    // The last `markdown` attribute wins, matching how a browser resolves a
    // repeated attribute.
    let mut declared: Option<&str> = None;
    while i < bytes.len() {
        match bytes[i] {
            b'>' => {
                return declared.filter(|value| *value != "0").map(|_| tag);
            }
            b' ' | b'\t' | b'/' => i += 1,
            _ => {
                let name_start = i;
                while i < bytes.len() && !matches!(bytes[i], b' ' | b'\t' | b'=' | b'>' | b'/') {
                    i += 1;
                }
                let name = &line[name_start..i];

                let mut value = "";
                let mut j = i;
                while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
                    j += 1;
                }
                if bytes.get(j) == Some(&b'=') {
                    j += 1;
                    while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
                        j += 1;
                    }
                    match bytes.get(j) {
                        Some(&quote @ (b'"' | b'\'')) => {
                            let start = j + 1;
                            // An unterminated quote runs off the end of the line, so
                            // there is no complete opening tag here.
                            let end = start + line[start..].find(quote as char)?;
                            value = &line[start..end];
                            j = end + 1;
                        }
                        _ => {
                            let start = j;
                            while j < bytes.len() && !matches!(bytes[j], b' ' | b'\t' | b'>') {
                                j += 1;
                            }
                            value = &line[start..j];
                        }
                    }
                    i = j;
                }

                if name.eq_ignore_ascii_case("markdown") {
                    declared = Some(value);
                }
            }
        }
    }
    None
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
        if let Some(tag) = markdown_html_open_tag(line) {
            // Check if this line also closes the tag (self-contained)
            let closes_here = Self::count_closes_lowered(&line.to_lowercase(), &tag) > 0;

            self.depth += 1;
            self.tag_stack.push((tag, self.depth));
            if closes_here {
                self.depth -= 1;
                self.tag_stack.pop();
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

    fn opens(line: &str) -> bool {
        markdown_html_open_tag(line).is_some()
    }

    #[test]
    fn test_markdown_html_detection() {
        // Basic patterns
        assert!(opens("<div markdown>"));
        assert!(opens("<div class=\"grid cards\" markdown>"));
        assert!(opens("<div markdown=\"1\">"));
        assert!(opens("<div markdown=\"block\">"));

        // Attribute order variations
        assert!(opens("<div markdown class=\"test\">"));
        assert!(opens("<div id=\"foo\" markdown>"));

        // Case insensitivity
        assert!(opens("<DIV markdown>"));
        assert!(opens("<Div Markdown>"));

        // With indentation
        assert!(opens("  <div markdown>"));
        assert!(opens("    <div class=\"grid\" markdown>"));

        // Other valid HTML5 elements
        assert!(opens("<section markdown>"));
        assert!(opens("<article markdown>"));
        assert!(opens("<details markdown>"));

        // Should NOT match
        assert!(!opens("<div class=\"test\">"));
        assert!(!opens("<span markdown>")); // span is not a block-level element
        assert!(!opens("text with markdown word"));
        assert!(!opens("<div>markdown</div>"));
    }

    #[test]
    fn test_attribute_value_quoting() {
        assert_eq!(markdown_html_open_tag("<div markdown=1>").as_deref(), Some("div"));
        assert_eq!(markdown_html_open_tag("<div markdown='block'>").as_deref(), Some("div"));
        assert_eq!(
            markdown_html_open_tag("<div markdown = \"1\" >").as_deref(),
            Some("div")
        );
        assert_eq!(
            markdown_html_open_tag("<div markdown=\"default\"/>").as_deref(),
            Some("div")
        );

        // An unterminated quote runs off the end of the line, so the tag never closes.
        assert!(!opens("<div class=\"unclosed markdown>"));
        // Nor does a tag that simply has no `>` on this line.
        assert!(!opens("<div markdown"));
    }

    #[test]
    fn test_span_level_block_elements_take_the_attribute() {
        // Python-Markdown parses these with span-level rules and kramdown accepts
        // them too; either way their content is Markdown, not raw HTML. Jekyll's
        // own docs open notices with `<h5 markdown="1">` / `<p markdown="1">`.
        for line in [
            "<p markdown=\"1\">",
            "<h1 markdown=\"1\">",
            "<h5 markdown=\"1\">Diving in</h5>",
            "<h6 markdown>",
            "<li markdown=\"1\">",
            "<td markdown=\"1\">",
            "<th markdown=\"1\">",
            "<dd markdown=\"1\">",
            "<summary markdown=\"span\">",
            "<blockquote markdown=\"1\">",
            "<table markdown=\"block\">",
        ] {
            assert!(opens(line), "{line} should open a markdown block");
        }
    }

    #[test]
    fn test_the_word_markdown_in_another_attribute_is_not_an_opt_in() {
        // Each of these matched the tag-name-plus-`\bmarkdown\b` test that this
        // scanner replaces, and none of them declares anything.
        for line in [
            "<div class=\"markdown-body\">",
            "<div id=\"hello-markdown\">",
            "<div class=\"marketplace-extensions-markdown-preview-curated\"></div>",
            "<video title=\"Rendering markdown in the editor\" autoplay controls></video>",
            "<section data-note=\"see markdown docs\">",
        ] {
            assert!(!opens(line), "{line} must not open a markdown block");
        }

        // The attribute itself still counts when it sits beside such a value.
        assert!(opens("<div class=\"markdown-body\" markdown=\"1\">"));
    }

    #[test]
    fn test_markdown_zero_declares_raw_html() {
        // kramdown's `markdown="0"` is the opposite of `markdown="1"`: the content
        // stays raw HTML.
        assert!(!opens("<div markdown=\"0\">"));
        assert!(!opens("<div markdown='0'>"));
        assert!(!opens("<div markdown=0>"));
        assert!(!opens(
            "<div markdown=\"0\"><a href=\"#\" class=\"btn\">Button</a></div>"
        ));

        let mut tracker = MarkdownHtmlTracker::new();
        assert!(!tracker.process_line("<div markdown=\"0\">"));
        assert!(!tracker.is_inside());

        // `0` only speaks for itself; a `10` or a `0.5` is not it.
        assert!(opens("<div markdown=\"10\">"));
    }

    #[test]
    fn test_raw_text_and_void_elements_never_open_a_block() {
        // Python-Markdown never parses the content of these, and CommonMark reads
        // four of them as raw-text HTML blocks under every flavor.
        for line in [
            "<pre markdown=\"1\">",
            "<script markdown=\"1\">",
            "<style markdown>",
            "<textarea markdown=\"1\">",
            "<canvas markdown=\"1\">",
            "<option markdown=\"1\">",
        ] {
            assert!(!opens(line), "{line} must not open a markdown block");
        }

        // `hr` holds no content, so a block opened on it would never be closed and
        // would swallow the rest of the document.
        assert!(!opens("<hr markdown>"));
        assert!(!opens("<img src=\"x.png\" markdown=\"1\">"));
    }

    #[test]
    fn test_tracker_paragraph_block_ends_at_its_closing_tag() {
        // The shape Jekyll's upgrade notices use, and the one that used to leave
        // list-marker and indent rules policing content they do not own.
        let mut tracker = MarkdownHtmlTracker::new();

        assert!(tracker.process_line("<p markdown=\"1\">"));
        assert!(tracker.is_inside());
        assert!(tracker.process_line("-  a list marker with custom spacing"));
        assert!(tracker.is_inside());
        assert!(tracker.process_line("</p>"));
        assert!(!tracker.is_inside());

        assert!(!tracker.process_line("-  back outside the block"));
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
