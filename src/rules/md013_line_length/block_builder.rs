//! Block-level state machine for MD013 reflow.
//!
//! Turns a stream of [`LineType`](super::LineType) events into a `Vec<Block>`
//! that the reflow renderer can consume. The builder centralises bookkeeping
//! that would otherwise be 20+ scattered locals plus repeated "flush admonition
//! + flush table + flush the active mutex group" sequences in every standalone-line arm.
//!
//! ## State machine layers
//!
//! 1. **Container blocks** that nest other content: `admonition`, `table`.
//!    These flush independently and never coexist.
//! 2. **Mutex group**: at most one of `code`, `html`, `paragraph` is in flight
//!    at a time. [`BlockBuilder::flush_mutex_group`] closes whichever is active.
//! 3. **Standalone lines** (`SemanticLine`, `SnippetLine`, `DivMarker`) flush
//!    everything via [`BlockBuilder::flush_for_new_block`] before pushing themselves.
//!
//! ## Public API
//!
//! Callers feed one [`LineType`](super::LineType) at a time via the `feed_*`
//! methods, then call [`BlockBuilder::finalize`] to drain remaining state into
//! a `Vec<Block>`. All HTML state-machine bookkeeping is encapsulated inside
//! [`BlockBuilder::feed_content`].

/// A semantic block in a list item, ready for the reflow renderer.
///
/// `Block` is a transport DTO between two collaborators in the same parent
/// module: [`BlockBuilder`] is responsible for *construction* (deciding
/// which variant a stream of [`LineType`](super::LineType) events
/// produces, and bookkeeping HTML/admonition/table state), and the reflow
/// renderer in `mod.rs` is responsible for *consumption* (pattern
/// matching to apply per-variant rendering rules). The two halves
/// deliberately speak the same tagged-sum-type vocabulary because the
/// rendering logic depends on rendering context (`LineLengthConfig`,
/// `MdConfig`, fix state, the parent line index) that does not belong
/// inside this submodule.
///
/// The encapsulation boundary is therefore "BlockBuilder owns construction;
/// downstream owns consumption", not "Block is opaque". Adding a new
/// variant requires updating both halves.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Block {
    Paragraph(Vec<(String, usize)>),
    Code {
        /// `(content, indent)` pairs preserving original indentation.
        lines: Vec<(String, usize)>,
        /// Whether a blank line preceded this block in the source.
        has_preceding_blank: bool,
    },
    /// A semantic marker (NOTE:, WARNING:, …) preserved on its own line.
    SemanticLine(String),
    /// An MkDocs snippet delimiter (`-8<-`) preserved verbatim with no extra spacing.
    SnippetLine(String),
    /// A Quarto/Pandoc div marker (`:::` opening or closing) preserved verbatim.
    DivMarker(String),
    Html {
        /// HTML lines preserved exactly as-is.
        lines: Vec<String>,
        has_preceding_blank: bool,
    },
    Admonition {
        /// e.g. `!!! note` or `??? warning "Title"`.
        header: String,
        /// Original indent of the header line.
        header_indent: usize,
        /// `(text, original_indent)` pairs for body lines.
        content_lines: Vec<(String, usize)>,
    },
    Table {
        /// `(row_text, original_indent)` pairs preserved verbatim.
        lines: Vec<(String, usize)>,
        has_preceding_blank: bool,
    },
}

/// Block-level HTML tags whose presence triggers HTML block detection.
const BLOCK_LEVEL_TAGS: &[&str] = &[
    "div",
    "details",
    "summary",
    "section",
    "article",
    "header",
    "footer",
    "nav",
    "aside",
    "main",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "td",
    "th",
    "ul",
    "ol",
    "li",
    "dl",
    "dt",
    "dd",
    "pre",
    "blockquote",
    "figure",
    "figcaption",
    "form",
    "fieldset",
    "legend",
    "hr",
    "p",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "style",
    "script",
    "noscript",
];

/// If `line` opens a block-level HTML tag (or HTML comment), return the
/// lowercased tag name. Returns `Some("!--")` for the comment sentinel.
fn block_html_opening_tag(line: &str) -> Option<String> {
    let trimmed = line.trim();

    if trimmed.starts_with("<!--") {
        return Some("!--".to_string());
    }

    if trimmed.starts_with('<') && !trimmed.starts_with("</") && !trimmed.starts_with("<!") {
        let after_bracket = &trimmed[1..];
        if let Some(end) = after_bracket.find(|c: char| c.is_whitespace() || c == '>' || c == '/') {
            let tag_name = after_bracket[..end].to_lowercase();
            if BLOCK_LEVEL_TAGS.contains(&tag_name.as_str()) {
                return Some(tag_name);
            }
        }
    }
    None
}

/// Whether `line` is a closing tag for `tag_name`. The sentinel `"!--"`
/// matches the `-->` comment terminator.
fn is_html_closing_tag(line: &str, tag_name: &str) -> bool {
    let trimmed = line.trim();

    if tag_name == "!--" {
        return trimmed.ends_with("-->");
    }

    trimmed.starts_with(&format!("</{tag_name}>"))
        || trimmed.starts_with(&format!("</{tag_name}  "))
        || (trimmed.starts_with("</") && trimmed[2..].trim_start().starts_with(tag_name))
}

/// Whether `line` is a self-closing tag (`<foo/>`).
fn is_self_closing_tag(line: &str) -> bool {
    line.trim().ends_with("/>")
}

/// State machine that consumes line events and produces a `Vec<Block>`.
///
/// Construct via [`BlockBuilder::new`], feed one line at a time via the
/// `feed_*` methods, then call [`BlockBuilder::finalize`] to extract the
/// blocks.
pub(super) struct BlockBuilder {
    blocks: Vec<Block>,

    current_paragraph: Vec<(String, usize)>,
    current_code_block: Vec<(String, usize)>,
    current_html_block: Vec<String>,
    html_tag_stack: Vec<String>,
    current_table: Vec<(String, usize)>,

    in_code: bool,
    in_html_block: bool,
    in_table: bool,
    in_admonition_block: bool,

    admonition_header: Option<(String, usize)>,
    admonition_content: Vec<(String, usize)>,

    had_preceding_blank: bool,
    code_block_has_preceding_blank: bool,
    html_block_has_preceding_blank: bool,
    table_has_preceding_blank: bool,

    current_line: usize,
}

impl BlockBuilder {
    #[cfg(test)]
    pub(super) fn new() -> Self {
        Self::new_with_start_line(1)
    }

    pub(super) fn new_with_start_line(start_line: usize) -> Self {
        Self {
            blocks: Vec::new(),
            current_paragraph: Vec::new(),
            current_code_block: Vec::new(),
            current_html_block: Vec::new(),
            html_tag_stack: Vec::new(),
            current_table: Vec::new(),
            in_code: false,
            in_html_block: false,
            in_table: false,
            in_admonition_block: false,
            admonition_header: None,
            admonition_content: Vec::new(),
            had_preceding_blank: false,
            code_block_has_preceding_blank: false,
            html_block_has_preceding_blank: false,
            table_has_preceding_blank: false,
            current_line: start_line,
        }
    }

    // ------------------------------------------------------------------------
    // Feeders — one per LineType variant. Callers do `match line { ... }` and
    // dispatch to a single feeder method; all state mutation lives here.
    // ------------------------------------------------------------------------

    /// Feed a blank line. Blank lines extend multi-line containers
    /// (admonition, code, html) but terminate tables and paragraphs.
    pub(super) fn feed_blank_line(&mut self) {
        if self.in_admonition_block {
            self.admonition_content.push((String::new(), 0));
        } else if self.in_code {
            self.current_code_block.push((String::new(), 0));
        } else if self.in_html_block {
            self.current_html_block.push(String::new());
        } else if self.in_table {
            self.flush_table();
        } else {
            self.flush_paragraph();
        }
        self.had_preceding_blank = true;
        self.current_line += 1;
    }

    /// Feed a content (CommonMark text) line. Encapsulates the HTML
    /// state machine: detects block-level HTML opens/closes and routes
    /// the line into the appropriate buffer.
    pub(super) fn feed_content(&mut self, content: &str) {
        self.flush_admonition_and_table();
        if self.in_html_block {
            self.extend_html_block(content);
        } else if let Some(tag_name) = block_html_opening_tag(content) {
            self.start_html_block(content, tag_name);
        } else {
            self.append_to_paragraph(content);
        }
        self.had_preceding_blank = false;
        self.current_line += 1;
    }

    /// Feed a fenced or indented code block line.
    pub(super) fn feed_code_line(&mut self, content: &str, indent: usize) {
        self.flush_admonition_and_table();
        self.flush_html();
        if !self.in_code {
            self.flush_paragraph();
            self.in_code = true;
            self.code_block_has_preceding_blank = self.had_preceding_blank;
        }
        self.current_code_block.push((content.to_string(), indent));
        self.had_preceding_blank = false;
        self.current_line += 1;
    }

    /// Feed a standalone semantic marker (NOTE:, WARNING:, …).
    pub(super) fn feed_semantic_line(&mut self, content: &str) {
        self.flush_for_new_block();
        self.blocks.push(Block::SemanticLine(content.to_string()));
        self.had_preceding_blank = false;
        self.current_line += 1;
    }

    /// Feed an MkDocs snippet delimiter (`-8<-`).
    pub(super) fn feed_snippet_line(&mut self, content: &str) {
        self.flush_for_new_block();
        self.blocks.push(Block::SnippetLine(content.to_string()));
        self.had_preceding_blank = false;
        self.current_line += 1;
    }

    /// Feed a Quarto/Pandoc div marker (`:::` opening or closing).
    pub(super) fn feed_div_marker(&mut self, content: &str) {
        self.flush_for_new_block();
        self.blocks.push(Block::DivMarker(content.to_string()));
        self.had_preceding_blank = false;
        self.current_line += 1;
    }

    /// Feed an admonition header. Starts a fresh admonition container.
    pub(super) fn feed_admonition_header(&mut self, header_text: &str, indent: usize) {
        self.flush_for_new_block();
        self.in_admonition_block = true;
        self.admonition_header = Some((header_text.to_string(), indent));
        self.admonition_content.clear();
        self.had_preceding_blank = false;
        self.current_line += 1;
    }

    /// Feed an admonition body line. If no admonition header is in scope this
    /// degrades to appending the line to the current paragraph rather than
    /// silently dropping it.
    pub(super) fn feed_admonition_content(&mut self, content: &str, indent: usize) {
        if self.in_admonition_block {
            self.admonition_content.push((content.to_string(), indent));
        } else {
            self.append_to_paragraph(content);
        }
        self.had_preceding_blank = false;
        self.current_line += 1;
    }

    /// Feed a GFM table row. Tables flush admonition (peer container) and the
    /// mutex group, but extend themselves rather than flushing.
    pub(super) fn feed_table_line(&mut self, content: &str, indent: usize) {
        self.flush_admonition();
        self.flush_mutex_group();
        if !self.in_table {
            self.in_table = true;
            self.table_has_preceding_blank = self.had_preceding_blank;
        }
        self.current_table.push((content.to_string(), indent));
        self.had_preceding_blank = false;
        self.current_line += 1;
    }

    /// Drain remaining state and return the accumulated blocks.
    pub(super) fn finalize(mut self) -> Vec<Block> {
        self.flush_admonition();
        self.flush_table();
        if self.in_code && !self.current_code_block.is_empty() {
            self.blocks.push(Block::Code {
                lines: self.current_code_block,
                has_preceding_blank: self.code_block_has_preceding_blank,
            });
        }
        if self.in_html_block && !self.current_html_block.is_empty() {
            self.blocks.push(Block::Html {
                lines: self.current_html_block,
                has_preceding_blank: self.html_block_has_preceding_blank,
            });
        }
        if !self.current_paragraph.is_empty() {
            self.blocks.push(Block::Paragraph(self.current_paragraph));
        }
        self.blocks
    }

    // ------------------------------------------------------------------------
    // Internal flush primitives. Each `flush_*` is a no-op when the
    // corresponding buffer is empty / inactive, so callers can compose them
    // freely without explicit guards.
    // ------------------------------------------------------------------------

    fn flush_admonition(&mut self) {
        if self.in_admonition_block {
            if let Some((h, hi)) = self.admonition_header.take() {
                self.blocks.push(Block::Admonition {
                    header: h,
                    header_indent: hi,
                    content_lines: std::mem::take(&mut self.admonition_content),
                });
            }
            self.in_admonition_block = false;
        }
    }

    fn flush_table(&mut self) {
        if self.in_table {
            self.blocks.push(Block::Table {
                lines: std::mem::take(&mut self.current_table),
                has_preceding_blank: self.table_has_preceding_blank,
            });
            self.in_table = false;
        }
    }

    fn flush_admonition_and_table(&mut self) {
        self.flush_admonition();
        self.flush_table();
    }

    fn flush_code(&mut self) {
        if self.in_code {
            self.blocks.push(Block::Code {
                lines: std::mem::take(&mut self.current_code_block),
                has_preceding_blank: self.code_block_has_preceding_blank,
            });
            self.in_code = false;
        }
    }

    fn flush_html(&mut self) {
        if self.in_html_block {
            self.blocks.push(Block::Html {
                lines: std::mem::take(&mut self.current_html_block),
                has_preceding_blank: self.html_block_has_preceding_blank,
            });
            self.html_tag_stack.clear();
            self.in_html_block = false;
        }
    }

    fn flush_paragraph(&mut self) {
        if !self.current_paragraph.is_empty() {
            self.blocks
                .push(Block::Paragraph(std::mem::take(&mut self.current_paragraph)));
        }
    }

    /// Flush whichever of code / html / paragraph is currently in flight.
    /// They are mutually exclusive by construction, so the if/else chain
    /// preserves the original semantics exactly.
    fn flush_mutex_group(&mut self) {
        if self.in_code {
            self.flush_code();
        } else if self.in_html_block {
            self.flush_html();
        } else {
            self.flush_paragraph();
        }
    }

    /// Flush all in-flight state to start a fresh standalone block.
    /// Used by every LineType that introduces a new top-level block.
    fn flush_for_new_block(&mut self) {
        self.flush_admonition_and_table();
        self.flush_mutex_group();
    }

    // ------------------------------------------------------------------------
    // HTML state-machine internals. These are private to the module so the
    // tag-stack invariant ("only ever modified by these helpers") is local.
    // ------------------------------------------------------------------------

    fn extend_html_block(&mut self, content: &str) {
        self.current_html_block.push(content.to_string());

        // Track HTML block boundaries via the tag stack: a closing tag for
        // the topmost element pops the stack; nested opening tags push.
        let last_tag = match self.html_tag_stack.last() {
            Some(t) => t.clone(),
            None => return,
        };
        if is_html_closing_tag(content, &last_tag) {
            self.html_tag_stack.pop();
            if self.html_tag_stack.is_empty() {
                self.flush_html();
            }
        } else if let Some(new_tag) = block_html_opening_tag(content)
            && !is_self_closing_tag(content)
        {
            self.html_tag_stack.push(new_tag);
        }
    }

    fn start_html_block(&mut self, content: &str, tag_name: String) {
        // Starting a new HTML block: flush whichever of code / paragraph
        // is active first (admonition + table already flushed by caller).
        if self.in_code {
            self.flush_code();
        } else {
            self.flush_paragraph();
        }
        self.in_html_block = true;
        self.html_block_has_preceding_blank = self.had_preceding_blank;
        self.current_html_block.push(content.to_string());
        if is_self_closing_tag(content) {
            self.flush_html();
        } else {
            self.html_tag_stack.push(tag_name);
        }
    }

    fn append_to_paragraph(&mut self, content: &str) {
        if self.in_code {
            self.flush_code();
        }
        self.current_paragraph.push((content.to_string(), self.current_line));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraph(lines: &[&str]) -> Block {
        Block::Paragraph(lines.iter().map(|&s| (s.to_string(), 0)).collect())
    }

    fn finalize_test(builder: BlockBuilder) -> Vec<Block> {
        builder
            .finalize()
            .into_iter()
            .map(|b| match b {
                Block::Paragraph(lines) => Block::Paragraph(lines.into_iter().map(|(s, _)| (s, 0)).collect()),
                other => other,
            })
            .collect()
    }

    fn code(lines: &[(&str, usize)], has_preceding_blank: bool) -> Block {
        Block::Code {
            lines: lines.iter().map(|(c, i)| (c.to_string(), *i)).collect(),
            has_preceding_blank,
        }
    }

    fn html(lines: &[&str], has_preceding_blank: bool) -> Block {
        Block::Html {
            lines: lines.iter().map(ToString::to_string).collect(),
            has_preceding_blank,
        }
    }

    fn admonition(header: &str, header_indent: usize, content_lines: &[(&str, usize)]) -> Block {
        Block::Admonition {
            header: header.to_string(),
            header_indent,
            content_lines: content_lines.iter().map(|(c, i)| (c.to_string(), *i)).collect(),
        }
    }

    fn table(lines: &[(&str, usize)], has_preceding_blank: bool) -> Block {
        Block::Table {
            lines: lines.iter().map(|(c, i)| (c.to_string(), *i)).collect(),
            has_preceding_blank,
        }
    }

    #[test]
    fn empty_input_produces_no_blocks() {
        let blocks = finalize_test(BlockBuilder::new());
        assert!(blocks.is_empty());
    }

    #[test]
    fn paragraph_lines_collect_into_single_block() {
        let mut b = BlockBuilder::new();
        b.feed_content("first");
        b.feed_content("second");
        b.feed_content("third");
        assert_eq!(finalize_test(b), vec![paragraph(&["first", "second", "third"])]);
    }

    #[test]
    fn blank_line_terminates_paragraph() {
        let mut b = BlockBuilder::new();
        b.feed_content("para one");
        b.feed_blank_line();
        b.feed_content("para two");
        assert_eq!(
            finalize_test(b),
            vec![paragraph(&["para one"]), paragraph(&["para two"])]
        );
    }

    #[test]
    fn blank_line_extends_code_block() {
        let mut b = BlockBuilder::new();
        b.feed_code_line("fn main() {", 0);
        b.feed_blank_line();
        b.feed_code_line("}", 0);
        assert_eq!(
            finalize_test(b),
            vec![code(&[("fn main() {", 0), ("", 0), ("}", 0)], false)]
        );
    }

    #[test]
    fn blank_line_terminates_table() {
        let mut b = BlockBuilder::new();
        b.feed_table_line("| h |", 0);
        b.feed_table_line("|---|", 0);
        b.feed_blank_line();
        b.feed_content("after");
        assert_eq!(
            finalize_test(b),
            vec![table(&[("| h |", 0), ("|---|", 0)], false), paragraph(&["after"]),]
        );
    }

    #[test]
    fn blank_line_extends_admonition() {
        let mut b = BlockBuilder::new();
        b.feed_admonition_header("!!! note", 0);
        b.feed_admonition_content("first body line", 4);
        b.feed_blank_line();
        b.feed_admonition_content("second body line", 4);
        assert_eq!(
            finalize_test(b),
            vec![admonition(
                "!!! note",
                0,
                &[("first body line", 4), ("", 0), ("second body line", 4)]
            )]
        );
    }

    #[test]
    fn preceding_blank_is_recorded_on_next_block_start() {
        let mut b = BlockBuilder::new();
        b.feed_content("para");
        b.feed_blank_line();
        b.feed_code_line("code", 0);
        let blocks = finalize_test(b);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], paragraph(&["para"]));
        assert_eq!(blocks[1], code(&[("code", 0)], true));
    }

    #[test]
    fn no_preceding_blank_when_directly_adjacent() {
        let mut b = BlockBuilder::new();
        b.feed_content("para");
        b.feed_code_line("code", 0);
        let blocks = finalize_test(b);
        assert_eq!(blocks[0], paragraph(&["para"]));
        assert_eq!(blocks[1], code(&[("code", 0)], false));
    }

    #[test]
    fn standalone_lines_flush_paragraph_and_admonition() {
        let mut b = BlockBuilder::new();
        b.feed_admonition_header("!!! warn", 0);
        b.feed_admonition_content("body", 4);
        b.feed_semantic_line("NOTE:");
        b.feed_content("after");
        let blocks = finalize_test(b);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0], admonition("!!! warn", 0, &[("body", 4)]));
        assert_eq!(blocks[1], Block::SemanticLine("NOTE:".to_string()));
        assert_eq!(blocks[2], paragraph(&["after"]));
    }

    #[test]
    fn snippet_div_and_semantic_each_flush_for_new_block() {
        let mut b = BlockBuilder::new();
        b.feed_content("para");
        b.feed_snippet_line("--8<--");
        b.feed_div_marker(":::");
        b.feed_semantic_line("NOTE:");
        let blocks = finalize_test(b);
        assert_eq!(
            blocks,
            vec![
                paragraph(&["para"]),
                Block::SnippetLine("--8<--".to_string()),
                Block::DivMarker(":::".to_string()),
                Block::SemanticLine("NOTE:".to_string()),
            ]
        );
    }

    #[test]
    fn html_block_started_by_block_level_tag_collects_until_close() {
        let mut b = BlockBuilder::new();
        b.feed_content("<div>");
        b.feed_content("inside");
        b.feed_content("</div>");
        b.feed_content("after");
        assert_eq!(
            finalize_test(b),
            vec![html(&["<div>", "inside", "</div>"], false), paragraph(&["after"]),]
        );
    }

    #[test]
    fn self_closing_html_tag_emits_single_line_block() {
        let mut b = BlockBuilder::new();
        b.feed_content("<hr/>");
        b.feed_content("after");
        assert_eq!(finalize_test(b), vec![html(&["<hr/>"], false), paragraph(&["after"])]);
    }

    #[test]
    fn html_comment_collected_until_terminator() {
        let mut b = BlockBuilder::new();
        b.feed_content("<!-- start");
        b.feed_content("middle");
        b.feed_content("end -->");
        b.feed_content("after");
        assert_eq!(
            finalize_test(b),
            vec![html(&["<!-- start", "middle", "end -->"], false), paragraph(&["after"]),]
        );
    }

    #[test]
    fn nested_html_tags_track_depth_via_stack() {
        let mut b = BlockBuilder::new();
        b.feed_content("<div>");
        b.feed_content("<details>");
        b.feed_content("body");
        b.feed_content("</details>");
        b.feed_content("</div>");
        b.feed_content("after");
        assert_eq!(
            finalize_test(b),
            vec![
                html(&["<div>", "<details>", "body", "</details>", "</div>"], false),
                paragraph(&["after"]),
            ]
        );
    }

    #[test]
    fn inline_tag_in_paragraph_is_not_html_block() {
        // <strong> is not in BLOCK_LEVEL_TAGS, so the line stays in the paragraph.
        let mut b = BlockBuilder::new();
        b.feed_content("see <strong>this</strong>");
        assert_eq!(finalize_test(b), vec![paragraph(&["see <strong>this</strong>"])]);
    }

    #[test]
    fn admonition_content_without_header_falls_back_to_paragraph() {
        let mut b = BlockBuilder::new();
        b.feed_admonition_content("orphan", 4);
        assert_eq!(finalize_test(b), vec![paragraph(&["orphan"])]);
    }

    #[test]
    fn flush_for_new_block_drains_admonition_table_and_mutex_group() {
        // Admonition + table can't legally coexist (table would terminate
        // admonition body in the parser), but each transition individually
        // must flush the prior. Verify via consecutive headers.
        let mut b = BlockBuilder::new();
        b.feed_admonition_header("!!! note", 0);
        b.feed_admonition_content("body", 4);
        b.feed_admonition_header("!!! warning", 0);
        let blocks = finalize_test(b);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], admonition("!!! note", 0, &[("body", 4)]));
        assert_eq!(blocks[1], admonition("!!! warning", 0, &[]));
    }

    #[test]
    fn finalize_emits_remaining_paragraph_after_other_blocks() {
        let mut b = BlockBuilder::new();
        b.feed_code_line("code", 0);
        b.feed_blank_line();
        b.feed_content("trailing para");
        assert_eq!(
            finalize_test(b),
            vec![code(&[("code", 0), ("", 0)], false), paragraph(&["trailing para"])]
        );
    }

    #[test]
    fn finalize_emits_remaining_table() {
        let mut b = BlockBuilder::new();
        b.feed_table_line("| a |", 0);
        b.feed_table_line("|---|", 0);
        b.feed_table_line("| b |", 0);
        assert_eq!(
            finalize_test(b),
            vec![table(&[("| a |", 0), ("|---|", 0), ("| b |", 0)], false)]
        );
    }

    #[test]
    fn table_after_paragraph_carries_no_preceding_blank() {
        let mut b = BlockBuilder::new();
        b.feed_content("para");
        b.feed_table_line("| a |", 0);
        b.feed_table_line("|---|", 0);
        let blocks = finalize_test(b);
        assert_eq!(blocks[0], paragraph(&["para"]));
        assert_eq!(blocks[1], table(&[("| a |", 0), ("|---|", 0)], false));
    }

    #[test]
    fn table_with_preceding_blank_records_flag() {
        let mut b = BlockBuilder::new();
        b.feed_content("para");
        b.feed_blank_line();
        b.feed_table_line("| a |", 0);
        b.feed_table_line("|---|", 0);
        let blocks = finalize_test(b);
        assert_eq!(blocks[1], table(&[("| a |", 0), ("|---|", 0)], true));
    }

    #[test]
    fn html_block_with_preceding_blank_records_flag() {
        let mut b = BlockBuilder::new();
        b.feed_content("para");
        b.feed_blank_line();
        b.feed_content("<div>");
        b.feed_content("</div>");
        let blocks = finalize_test(b);
        assert_eq!(blocks[1], html(&["<div>", "</div>"], true));
    }

    #[test]
    fn code_after_html_flushes_html_first() {
        let mut b = BlockBuilder::new();
        b.feed_content("<div>");
        b.feed_code_line("code", 0);
        // The HTML block was never closed but feed_code_line forces a flush
        // — the renderer downstream must handle the partial-html case.
        let blocks = finalize_test(b);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], html(&["<div>"], false));
        assert_eq!(blocks[1], code(&[("code", 0)], false));
    }

    #[test]
    fn table_flushes_admonition_but_not_when_in_admonition_body() {
        // Once an admonition is open, a table line should close the
        // admonition first (admonition and table are peer containers).
        let mut b = BlockBuilder::new();
        b.feed_admonition_header("!!! note", 0);
        b.feed_admonition_content("body", 4);
        b.feed_table_line("| a |", 0);
        b.feed_table_line("|---|", 0);
        let blocks = finalize_test(b);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], admonition("!!! note", 0, &[("body", 4)]));
        assert_eq!(blocks[1], table(&[("| a |", 0), ("|---|", 0)], false));
    }

    #[test]
    fn test_line_number_tracking() {
        let mut b = BlockBuilder::new_with_start_line(10);
        b.feed_content("line 10");
        b.feed_content("line 11");
        b.feed_blank_line(); // line 12
        b.feed_content("line 13");

        let blocks = b.finalize();
        assert_eq!(blocks.len(), 2);

        // Paragraph 1: lines 10 and 11
        assert_eq!(
            blocks[0],
            Block::Paragraph(vec![("line 10".to_string(), 10), ("line 11".to_string(), 11)])
        );

        // Paragraph 2: line 13
        assert_eq!(blocks[1], Block::Paragraph(vec![("line 13".to_string(), 13)]));
    }

    // ====================================================================
    // Property tests: assert structural invariants for any feed sequence.
    //
    // These exercise the state machine against ~hundreds of randomly
    // generated input sequences per test, hardening it against transition
    // permutations the explicit unit tests don't cover.
    // ====================================================================

    use proptest::prelude::*;

    /// Mirrors the [`BlockBuilder`] feeder API as a single enum so proptest
    /// can generate arbitrary sequences of feed calls.
    #[derive(Clone, Debug)]
    enum FeedAction {
        Blank,
        Content(String),
        Code(String, usize),
        Semantic(String),
        Snippet(String),
        DivMarker(String),
        AdmonitionHeader(String, usize),
        AdmonitionContent(String, usize),
        Table(String, usize),
    }

    fn arb_action() -> impl Strategy<Value = FeedAction> {
        // Bounded strings keep shrunk failure cases readable; the upper
        // bound is large enough to exercise multi-token HTML (`<div id="x">`)
        // and admonition headers with quoted titles.
        let text = "[a-z<>/ |!=\"-]{0,16}";
        let indent = 0usize..6;
        prop_oneof![
            Just(FeedAction::Blank),
            text.prop_map(FeedAction::Content),
            (text, indent.clone()).prop_map(|(s, i)| FeedAction::Code(s, i)),
            text.prop_map(FeedAction::Semantic),
            text.prop_map(FeedAction::Snippet),
            text.prop_map(FeedAction::DivMarker),
            (text, indent.clone()).prop_map(|(s, i)| FeedAction::AdmonitionHeader(s, i)),
            (text, indent.clone()).prop_map(|(s, i)| FeedAction::AdmonitionContent(s, i)),
            (text, indent).prop_map(|(s, i)| FeedAction::Table(s, i)),
        ]
    }

    fn run(actions: &[FeedAction]) -> Vec<Block> {
        let mut b = BlockBuilder::new();
        for action in actions {
            match action {
                FeedAction::Blank => b.feed_blank_line(),
                FeedAction::Content(s) => b.feed_content(s),
                FeedAction::Code(s, i) => b.feed_code_line(s, *i),
                FeedAction::Semantic(s) => b.feed_semantic_line(s),
                FeedAction::Snippet(s) => b.feed_snippet_line(s),
                FeedAction::DivMarker(s) => b.feed_div_marker(s),
                FeedAction::AdmonitionHeader(s, i) => b.feed_admonition_header(s, *i),
                FeedAction::AdmonitionContent(s, i) => b.feed_admonition_content(s, *i),
                FeedAction::Table(s, i) => b.feed_table_line(s, *i),
            }
        }
        finalize_test(b)
    }

    /// A flushed Paragraph/Code/Html/Table block must contain at least one
    /// line; otherwise the flush primitive should have been a no-op.
    /// Admonition is excepted: a header with no body is legal.
    fn assert_blocks_well_formed(blocks: &[Block]) {
        for block in blocks {
            match block {
                Block::Paragraph(lines) => assert!(!lines.is_empty(), "Paragraph must have lines: {block:?}"),
                Block::Code { lines, .. } => assert!(!lines.is_empty(), "Code must have lines: {block:?}"),
                Block::Html { lines, .. } => assert!(!lines.is_empty(), "Html must have lines: {block:?}"),
                Block::Table { lines, .. } => assert!(!lines.is_empty(), "Table must have lines: {block:?}"),
                Block::SemanticLine(_) | Block::SnippetLine(_) | Block::DivMarker(_) | Block::Admonition { .. } => {}
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 1024,
            max_shrink_iters: 4096,
            ..ProptestConfig::default()
        })]

        /// Property: BlockBuilder never panics on arbitrary feed sequences,
        /// and every emitted block satisfies its non-emptiness invariant.
        #[test]
        fn proptest_arbitrary_feed_sequences_never_panic_and_emit_well_formed_blocks(
            actions in proptest::collection::vec(arb_action(), 0..64),
        ) {
            let blocks = run(&actions);
            assert_blocks_well_formed(&blocks);
        }

        /// Property: BlockBuilder is deterministic — running the same feed
        /// sequence twice produces identical output.
        #[test]
        fn proptest_block_builder_is_deterministic(
            actions in proptest::collection::vec(arb_action(), 0..64),
        ) {
            prop_assert_eq!(run(&actions), run(&actions));
        }

        /// Property: blank lines fed before any content are absorbed and
        /// produce no blocks. The state machine should be in its initial
        /// configuration after any number of leading blanks.
        #[test]
        fn proptest_leading_blanks_alone_emit_nothing(n_blanks in 0usize..32) {
            let actions = vec![FeedAction::Blank; n_blanks];
            prop_assert_eq!(run(&actions), Vec::<Block>::new());
        }

        /// Property: prepending blank lines to a feed sequence does not
        /// alter the *content* of emitted blocks (only flags like
        /// `has_preceding_blank` on the first content block may differ,
        /// which we strip before comparing). Encodes the invariant that
        /// leading blanks are absorbed by the builder's initial state.
        #[test]
        fn proptest_leading_blanks_do_not_alter_block_content(
            actions in proptest::collection::vec(arb_action(), 0..32),
            n_blanks in 0usize..8,
        ) {
            let mut prefixed: Vec<FeedAction> = vec![FeedAction::Blank; n_blanks];
            prefixed.extend(actions.iter().cloned());

            let baseline = run(&actions).into_iter().map(strip_preceding_blank_flag).collect::<Vec<_>>();
            let with_prefix = run(&prefixed).into_iter().map(strip_preceding_blank_flag).collect::<Vec<_>>();

            prop_assert_eq!(baseline, with_prefix);
        }
    }

    /// Helper for `proptest_leading_blanks_do_not_alter_block_content`:
    /// normalises the `has_preceding_blank` flag so two block lists are
    /// compared on content alone. (The flag legitimately differs when a
    /// blank-prefix runs precedes a Code/Html/Table opening.)
    fn strip_preceding_blank_flag(block: Block) -> Block {
        match block {
            Block::Code { lines, .. } => Block::Code {
                lines,
                has_preceding_blank: false,
            },
            Block::Html { lines, .. } => Block::Html {
                lines,
                has_preceding_blank: false,
            },
            Block::Table { lines, .. } => Block::Table {
                lines,
                has_preceding_blank: false,
            },
            other => other,
        }
    }
}
