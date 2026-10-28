//!
//! Utility functions for detecting and handling code blocks and code spans in Markdown for rumdl.
//!
//! Code block detection is delegated to pulldown-cmark, which correctly implements the
//! CommonMark specification. This handles edge cases like:
//! - Backtick fences with backticks in the info string (invalid per spec)
//! - Nested fences (longer fence contains shorter fence as content)
//! - Mixed fence types (tilde fence contains backticks as content)
//! - Indented code blocks with proper list context handling

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use super::parser_options::rumdl_parser_options_for;

/// Detailed information about a code block captured during parsing
#[derive(Debug, Clone)]
pub struct CodeBlockDetail {
    /// Byte offset where this code block starts
    pub start: usize,
    /// Byte offset where this code block ends
    pub end: usize,
    /// Whether this is a fenced code block (true) or indented (false)
    pub is_fenced: bool,
    /// The info string from fenced blocks (e.g., "rust" from ```rust), empty for indented
    pub info_string: String,
    /// For an indented block, the visual column (tabs expanded to multiples of
    /// four) where its enclosing container's content begins: the document
    /// margin, a list item's content column, or a blockquote's content. The
    /// code itself starts four columns past it. `None` for fenced blocks.
    pub content_column: Option<usize>,
    /// For a fenced block, whether a closing fence line ends it. A block the
    /// document or its container ends first is not closed, even when its last
    /// line looks like a fence (a shorter run, another character, or code
    /// indented four columns). `false` for indented blocks.
    pub closed: bool,
    /// For a fenced block, the containers holding it, outermost first. Empty
    /// for a block at the document margin and for indented blocks.
    pub enclosing: Vec<Enclosure>,
}

/// A container a fenced block sits in, with the extent the parser gave it.
///
/// What continues the container on a later line is relative to the container
/// around it: a quote's marker may be indented differently on every line, and
/// a list item's indent counts from where its parent's content starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enclosure {
    /// A blockquote: each line carries a `>` marker.
    Quote { end: usize },
    /// A list item: a non-blank line continues it when indented `width`
    /// columns past the content start of the container around it.
    Item { width: usize, end: usize },
    /// A container whose continuation rules are not modelled (definition,
    /// footnote definition, or an item whose position cannot be derived).
    Opaque { end: usize },
}

impl Enclosure {
    /// Byte offset where the container's range ends.
    pub fn end(&self) -> usize {
        match *self {
            Self::Quote { end } | Self::Item { end, .. } | Self::Opaque { end } => end,
        }
    }
}

/// A line with its container prefixes removed (see [`strip_enclosure`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct StrippedLine {
    /// Byte offset in the line where the unconsumed text starts. When a tab was
    /// only partly consumed this is past the tab and `pending` holds the rest.
    pub pos: usize,
    /// Visual column reached, counting the consumed part of a split tab.
    pub col: usize,
    /// Columns of a split tab not yet consumed.
    pub pending: usize,
}

impl StrippedLine {
    /// Columns of indentation left, and the byte offset of the first character
    /// that is not a space or tab. `None` offset: the rest of the line is blank.
    pub fn indent(&self, line: &str) -> (usize, Option<usize>) {
        let mut col = self.col + self.pending;
        let mut pos = self.pos;
        for c in line[pos..].chars() {
            match c {
                ' ' => col += 1,
                '\t' => col += 4 - col % 4,
                _ => return (col - self.col, Some(pos)),
            }
            pos += c.len_utf8();
        }
        (col - self.col, None)
    }

    fn take_whitespace(&mut self, line: &str, max: usize) -> usize {
        let mut taken = 0;
        while taken < max {
            if self.pending > 0 {
                let n = self.pending.min(max - taken);
                self.pending -= n;
                self.col += n;
                taken += n;
                continue;
            }
            match line[self.pos..].chars().next() {
                Some(' ') => {
                    self.pos += 1;
                    self.col += 1;
                    taken += 1;
                }
                Some('\t') => {
                    let width = 4 - self.col % 4;
                    let n = width.min(max - taken);
                    self.pos += 1;
                    self.col += n;
                    self.pending = width - n;
                    taken += n;
                }
                _ => break,
            }
        }
        taken
    }

    fn rest_is_blank(&self, line: &str) -> bool {
        line[self.pos..].chars().all(|c| c == ' ' || c == '\t')
    }

    /// Consume the prefix one container puts on `line`, or `false` when the
    /// line does not continue it.
    fn take_container(&mut self, line: &str, enclosure: Enclosure) -> bool {
        match enclosure {
            Enclosure::Quote { .. } => {
                self.take_whitespace(line, 3);
                if self.pending > 0 || !line[self.pos..].starts_with('>') {
                    return false;
                }
                self.pos += 1;
                self.col += 1;
                self.take_whitespace(line, 1);
                true
            }
            Enclosure::Item { width, .. } => self.take_whitespace(line, width) >= width || self.rest_is_blank(line),
            Enclosure::Opaque { .. } => false,
        }
    }
}

/// Remove the prefixes the containers in `enclosing` (outermost first) put on
/// `line`, or `None` when the line does not continue all of them.
///
/// A quote takes up to three columns of indentation, `>`, and one optional
/// column after it. An item takes its `width` columns of indentation; a blank
/// line continues an item but not a quote. A line that continues nothing (a
/// lazy line) is `None`: lazy continuation never applies to code.
pub fn strip_enclosure(line: &str, enclosing: &[Enclosure]) -> Option<StrippedLine> {
    let mut at = StrippedLine {
        pos: 0,
        col: 0,
        pending: 0,
    };
    enclosing
        .iter()
        .all(|&enclosure| at.take_container(line, enclosure))
        .then_some(at)
}

/// How many of the containers in `enclosing` (outermost first) `line`
/// continues: the longest prefix of `enclosing` that `strip_enclosure` accepts.
///
/// `item_runs` is `item_runs(enclosing)`. Once the rest of the line is blank,
/// the items up to the next quote continue it and that quote does not, so the
/// cost is bounded by the line's length rather than by the nesting depth.
pub fn continued_depth(line: &str, enclosing: &[Enclosure], item_runs: &[usize]) -> usize {
    let blank_from = line.trim_end_matches([' ', '\t']).len();
    let mut at = StrippedLine {
        pos: 0,
        col: 0,
        pending: 0,
    };
    for (i, &enclosure) in enclosing.iter().enumerate() {
        if at.pos >= blank_from {
            return i + item_runs[i];
        }
        if !at.take_container(line, enclosure) {
            return i;
        }
    }
    enclosing.len()
}

/// For each container in `enclosing`, the number of consecutive items that
/// start there.
pub fn item_runs(enclosing: &[Enclosure]) -> Vec<usize> {
    let mut runs = vec![0; enclosing.len()];
    for i in (0..enclosing.len()).rev() {
        if matches!(enclosing[i], Enclosure::Item { .. }) {
            runs[i] = 1 + runs.get(i + 1).copied().unwrap_or(0);
        }
    }
    runs
}

/// The width a list item opening at `marker` takes, in columns: the marker, the
/// whitespace after it when 1 to 4 columns of text follow, otherwise a single
/// column. Returns where its content starts on the marker line, and that line's
/// start.
fn item_content_start(content: &str, marker: usize) -> Option<(StrippedLine, usize)> {
    let line_start = content[..marker].rfind('\n').map_or(0, |i| i + 1);
    let rest = content.get(marker..)?;
    let marker_len = match rest.as_bytes().first()? {
        b'-' | b'+' | b'*' => 1,
        b'0'..=b'9' => {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            match rest.as_bytes().get(digits)? {
                b'.' | b')' => digits + 1,
                _ => return None,
            }
        }
        _ => return None,
    };
    let marker_end = marker + marker_len;
    let marker_col = visual_column(content, marker_end);
    let line_end = content[marker_end..]
        .find('\n')
        .map_or(content.len(), |i| marker_end + i);
    let after = content[marker_end..line_end].trim_end_matches('\r');
    let mut col = marker_col;
    let mut text = None;
    for (i, c) in after.char_indices() {
        match c {
            ' ' => col += 1,
            '\t' => col += 4 - col % 4,
            _ => {
                text = Some(i);
                break;
            }
        }
    }
    let mut at = StrippedLine {
        pos: marker_end - line_start,
        col: marker_col,
        pending: 0,
    };
    match text {
        Some(i) if (1..=4).contains(&(col - marker_col)) => {
            at.pos += i;
            at.col = col;
        }
        // Nothing follows the marker on an empty item, so no later container
        // reads the position; its content still starts one column on.
        _ => {
            if at.take_whitespace(&content[line_start..line_end], 1) == 0 {
                at.col += 1;
            }
        }
    }
    Some((at, line_start))
}

/// Whether the fenced block spanning `start..end` of `content` ends with a
/// closing fence line, given how many lines its literal text has.
///
/// A closing fence is the one line of a block that is not in its literal text,
/// so a closed block spans two lines more than its literal holds (opener and
/// closer) and an unclosed one a single line more. A final line holding only
/// whitespace is inside the parser's range without a text event of its own, so
/// the last line must also start with a fence character once quote markers and
/// whitespace are set aside.
fn fenced_block_is_closed(content: &str, start: usize, end: usize, literal_lines: usize) -> bool {
    let bytes = content.as_bytes();
    let end = end.min(bytes.len());
    // The range of an unclosed block ends after the newline of its last line.
    let body_end = if end > start && bytes[end - 1] == b'\n' {
        end - 1
    } else {
        end
    };
    let body = &bytes[start.min(body_end)..body_end];
    // Each newline starts another line, and an empty body is one line.
    let span_lines = body.split(|&b| b == b'\n').count();
    let line_start = body
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(start, |at| start + at + 1);
    let last_line = content.get(line_start..body_end).unwrap_or("");
    span_lines == literal_lines + 2 && last_line.trim_start_matches(['>', ' ', '\t']).starts_with(['`', '~'])
}

/// Visual column of byte `pos`, counting from the start of its line, with a
/// tab advancing to the next multiple of four.
fn visual_column(content: &str, pos: usize) -> usize {
    let line_start = content[..pos].rfind('\n').map_or(0, |i| i + 1);
    content[line_start..pos]
        .chars()
        .fold(0, |col, c| if c == '\t' { col + 4 - col % 4 } else { col + 1 })
}

/// A strong emphasis span captured during parsing
#[derive(Debug, Clone)]
pub struct StrongSpanDetail {
    /// Byte offset where the strong span starts (including **)
    pub start: usize,
    /// Byte offset where the strong span ends (including **)
    pub end: usize,
    /// Whether this uses asterisk (**) or underscore (__) markers
    pub is_asterisk: bool,
}

/// Text a definition holds directly, captured during parsing
///
/// One of the definition's paragraphs, or, for a tight definition, the run of
/// inline content the parser reports with no paragraph around it. Blocks nested
/// in the definition (lists, blockquotes, code) hold their own text and are not
/// part of one.
#[derive(Debug, Clone)]
pub struct DefinitionTextDetail {
    /// Byte offset where the text starts
    pub start: usize,
    /// Byte offset where the text ends
    pub end: usize,
    /// Byte offset where the definition holding the text starts: its `:`
    /// marker, or indentation before it inside a container
    pub definition_start: usize,
}

/// Ordered list membership: maps line number (1-indexed) to list ID
pub type LineToListMap = std::collections::HashMap<usize, usize>;
/// Ordered list start values: maps list ID to the start value
pub type ListStartValues = std::collections::HashMap<usize, u64>;

/// Structure facts from document analysis, capturing data needed by individual rules.
#[derive(Debug, Default)]
pub struct ParseResult {
    /// Code block byte ranges (start, end)
    pub code_blocks: Vec<(usize, usize)>,
    /// Inline code span byte ranges (start, end)
    pub code_spans: Vec<(usize, usize)>,
    /// Detailed code block info (fenced vs indented, info string)
    pub code_block_details: Vec<CodeBlockDetail>,
    /// Strong emphasis span details
    pub strong_spans: Vec<StrongSpanDetail>,
    /// Ordered list membership: maps line number (1-indexed) to list ID
    pub line_to_list: LineToListMap,
    /// Ordered list start values: maps list ID to start value
    pub list_start_values: ListStartValues,
    /// HTML block byte ranges (start, end)
    ///
    /// Needed to know how far an unclosed `<!--` reaches. The block ends where
    /// CommonMark says it does, which is the end of the enclosing container
    /// rather than the end of the document: an unclosed comment in a blockquote
    /// stops at the quote, and one in a list item stops at the blank line.
    pub html_blocks: Vec<(usize, usize)>,
    /// Definition list item byte ranges (start, end) in document order: a
    /// definition together with the terms before it, blank lines after it left
    /// out, items of nested lists too. Only definitions passing
    /// `opens_definition`, and their terms, count.
    pub definition_items: Vec<(usize, usize)>,
    /// Definition list term byte ranges (start, end)
    pub definition_terms: Vec<(usize, usize)>,
    /// Text held directly by a definition, in document order
    pub definition_texts: Vec<DefinitionTextDetail>,
    /// Definitions failing `opens_definition`, with their terms, in document
    /// order, where reading them as the paragraph text rumdl takes them for
    /// changes which inline constructs the text holds. The parse the other
    /// fields come from reads a definition, which ends every code span, link
    /// and emphasis running into it.
    pub colon_definitions: Vec<ColonDefinitionDetail>,
    /// Byte range of every table the parser reads
    pub table_ranges: Vec<(usize, usize)>,
    /// Byte range of every block that is not inside another block, in document
    /// order: a paragraph, list, blockquote, table, code block, and so on
    pub top_level_blocks: Vec<(usize, usize)>,
    /// Byte range (terms included) of every definition failing
    /// `opens_definition`, before `colon_definitions` drops those whose text
    /// reads the same either way
    pub rejected_definition_ranges: Vec<(usize, usize)>,
    /// Byte range of every list item together with the byte offset where its
    /// content starts, for items that have content
    pub list_items: Vec<(usize, usize, usize)>,
}

/// A definition pulldown-cmark opens on a colon touching its text, together
/// with the terms before it
#[derive(Debug, Clone)]
pub struct ColonDefinitionDetail {
    /// Byte offset where the first term starts, or the definition when it has none
    pub start: usize,
    /// Byte offset where the definition ends, blank lines after it left out
    pub end: usize,
    /// Byte offset where the definition starts
    pub definition_start: usize,
}

/// Classification of code blocks relative to list contexts
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeBlockContext {
    /// Code block that separates lists (root-level, with blank lines)
    Standalone,
    /// Code block that continues a list (properly indented)
    Indented,
    /// Code block adjacent to list content (edge case, defaults to non-breaking)
    Adjacent,
}

/// Whether a tag opens inline content rather than a block
fn is_inline_tag(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Superscript
            | Tag::Subscript
            | Tag::Link { .. }
            | Tag::Image { .. }
    )
}

/// Whether a tag end closes inline content rather than a block
fn is_inline_tag_end(tag_end: TagEnd) -> bool {
    matches!(
        tag_end,
        TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Link
            | TagEnd::Image
    )
}

/// Whether the definition starting at `start` opens with a colon followed by
/// whitespace or the end of its line.
///
/// That is the marker the definition-list extensions agree on (PHP Markdown
/// Extra, Python-Markdown, Pandoc). pulldown-cmark also opens a definition on a
/// colon touching its text, as in `:warning:` or a `:::` fence closing a div,
/// which those read as prose or as the fence it is.
fn opens_definition(content: &str, start: usize) -> bool {
    // Defensively reject offsets inside UTF-8 characters. Before 0.13.5, a
    // tab crossing a definition's container indent could produce one.
    let Some(rest) = content.get(start..) else {
        return false;
    };
    rest.trim_start_matches([' ', '\t'])
        .strip_prefix(':')
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t', '\n', '\r']))
}

/// Every indented code block a parse with `options` finds in `content`, as
/// its byte range and the content column of the container it sits in: four
/// columns short of where its code begins, less the columns a tab straddling
/// that point overshot, which the parser supplies as a leading text event
/// with an empty range.
fn indented_block_columns(content: &str, options: Options) -> Vec<(usize, usize, usize)> {
    let mut blocks = Vec::new();
    let mut open: Option<(usize, usize)> = None;
    let mut awaiting_first_event = false;
    for (event, range) in Parser::new_ext(content, options).into_offset_iter() {
        if std::mem::take(&mut awaiting_first_event)
            && let Event::Text(text) = &event
            && range.is_empty()
            && let Some((_, column)) = &mut open
        {
            *column = column.saturating_sub(text.len());
        }
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                open = Some((range.start, visual_column(content, range.start).saturating_sub(4)));
                awaiting_first_event = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((start, column)) = open.take() {
                    blocks.push((start, range.end, column));
                }
            }
            _ => {}
        }
    }
    blocks
}

/// The byte ranges of the inline constructs a parse with `options` finds in
/// `content`, in document order: code spans, math, inline HTML, and the
/// emphasis, link and image spans holding text.
fn inline_ranges(content: &str, options: Options) -> Vec<(usize, usize)> {
    Parser::new_ext(content, options)
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Start(tag) if is_inline_tag(&tag) => Some((range.start, range.end)),
            Event::Code(_) | Event::InlineHtml(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {
                Some((range.start, range.end))
            }
            _ => None,
        })
        .collect()
}

/// Read source-ordered inline ranges for source-ordered definition queries.
/// Keep only earlier ranges that still overlap the current definition, and
/// inspect later ranges only when they actually fall inside that definition.
/// This also preserves nested ranges and spans crossing several definitions.
struct InlineRangeCursor<'a> {
    ranges: &'a [(usize, usize)],
    next: usize,
    active: Vec<(usize, usize)>,
}

impl<'a> InlineRangeCursor<'a> {
    fn new(ranges: &'a [(usize, usize)]) -> Self {
        Self {
            ranges,
            next: 0,
            active: Vec::new(),
        }
    }

    fn overlapping(&mut self, detail: &ColonDefinitionDetail) -> impl Iterator<Item = (usize, usize)> + '_ {
        while self.next < self.ranges.len() && self.ranges[self.next].0 <= detail.start {
            self.active.push(self.ranges[self.next]);
            self.next += 1;
        }
        self.active.retain(|&(_, end)| end > detail.start);
        let end = detail.end;
        self.active.iter().copied().chain(
            self.ranges[self.next..]
                .iter()
                .take_while(move |&&(start, _)| start < end)
                .copied(),
        )
    }
}

/// Utility functions for detecting and handling code blocks in Markdown
pub struct CodeBlockUtils;

impl CodeBlockUtils {
    /// Detect all code blocks in the content (NOT including inline code spans)
    ///
    /// Uses pulldown-cmark for spec-compliant CommonMark parsing. This correctly handles:
    /// - Fenced code blocks (``` and ~~~)
    /// - Indented code blocks (4 spaces or tab)
    /// - Code blocks inside lists, blockquotes, and other containers
    /// - Edge cases like backticks in info strings (which invalidate the fence)
    ///
    /// Returns a sorted vector of (start, end) byte offset tuples.
    pub fn detect_code_blocks(content: &str) -> Vec<(usize, usize)> {
        Self::detect_code_blocks_and_spans(content).code_blocks
    }

    /// Returns code block ranges, inline code span ranges, and detailed code block info
    /// in a single pulldown-cmark pass.
    pub fn detect_code_blocks_and_spans(content: &str) -> ParseResult {
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        let options = rumdl_parser_options_for(content);
        Self::collect_code_blocks_and_spans(
            content,
            &line_starts,
            options,
            Parser::new_ext(content, options).into_offset_iter(),
        )
    }

    /// Collect structure from an existing parse, sharing its events and source
    /// line index with other document consumers. Recovery parses still use
    /// their own options where rumdl's reading differs from the primary parse.
    pub(crate) fn collect_code_blocks_and_spans<'a>(
        content: &str,
        line_starts: &[usize],
        options: Options,
        events: impl Iterator<Item = (Event<'a>, std::ops::Range<usize>)>,
    ) -> ParseResult {
        let mut blocks = Vec::new();
        let mut spans = Vec::new();
        let mut details = Vec::new();
        let mut strong_spans = Vec::new();
        let mut html_blocks = Vec::new();
        let mut code_block_start: Option<(usize, bool, String, Option<usize>)> = None;
        // The literal text of the code block being read, as a line count: the
        // newlines seen, and whether the text so far ends in a partial line.
        let mut literal_newlines = 0usize;
        let mut literal_partial_line = false;
        // Set while the event right after an indented block's start is unseen.
        // When a tab straddles the column where the code begins, the parser
        // starts the block after the tab and supplies the columns it overshot
        // as a leading text event with an empty range.
        let mut awaiting_first_code_event = false;
        // One entry per open definition: whether `opens_definition` rejects
        // it. The parser reads a rejected definition as a container, rumdl as
        // the text it is, so an indented block inside one takes its content
        // column from a parse without definition lists (`text_reading`).
        let mut definition_rejected: Vec<bool> = Vec::new();
        let mut text_reading = Vec::new();

        // Definition structure. `block_stack` holds one entry per open block,
        // the definition's start for a definition and `None` for anything else,
        // so the innermost entry says whether content belongs to a definition
        // directly. Inline tags never touch it.
        //
        // Only a definition passing `opens_definition` counts, and a term
        // waits in `pending_terms` until a definition following it does. Extents
        // are recorded per item rather than per list: the parser's range for a
        // list can run over the paragraph after it, and a list can hold items
        // that do not count between items that do.
        let mut definition_items: Vec<(usize, usize)> = Vec::new();
        let mut pending_terms: Vec<(usize, usize)> = Vec::new();
        let mut definition_terms = Vec::new();
        let mut definition_texts = Vec::new();
        let mut colon_definitions = Vec::new();
        let mut table_ranges: Vec<(usize, usize)> = Vec::new();
        let mut top_level_blocks: Vec<(usize, usize)> = Vec::new();
        let mut rejected_definition_ranges: Vec<(usize, usize)> = Vec::new();
        let mut list_item_spans: Vec<(usize, usize, Option<usize>)> = Vec::new();
        let mut open_items: Vec<usize> = Vec::new();
        // Containers open at the current event, outermost first, each with the
        // start of the line it opened on and, for an item, where its content
        // starts there.
        let mut open_containers: Vec<(Enclosure, usize, StrippedLine)> = Vec::new();
        let mut code_block_enclosing: Vec<Enclosure> = Vec::new();
        let mut block_stack: Vec<Option<usize>> = Vec::new();
        let mut definition_paragraph: Option<(usize, usize)> = None;
        let mut tight_run: Option<DefinitionTextDetail> = None;

        // List membership tracking for ordered lists
        let mut line_to_list = LineToListMap::new();
        let mut list_start_values = ListStartValues::new();
        let mut list_stack: Vec<(usize, bool, u64)> = Vec::new(); // (list_id, is_ordered, start_value)
        let mut next_list_id: usize = 0;

        let byte_to_line = |byte_offset: usize| -> usize { line_starts.partition_point(|&start| start <= byte_offset) };

        for (event, range) in events {
            if let Some(&item) = open_items.last()
                && list_item_spans[item].2.is_none()
                && !matches!(event, Event::End(_))
            {
                list_item_spans[item].2 = Some(range.start);
            }
            if matches!(event, Event::Start(Tag::Item)) {
                open_items.push(list_item_spans.len());
                list_item_spans.push((range.start, range.end, None));
            } else if matches!(event, Event::End(TagEnd::Item)) {
                open_items.pop();
            }
            match &event {
                Event::Start(Tag::BlockQuote(_)) => {
                    let line_start = content[..range.start].rfind('\n').map_or(0, |i| i + 1);
                    open_containers.push((Enclosure::Quote { end: range.end }, line_start, StrippedLine::default()));
                }
                Event::Start(Tag::Item) => {
                    let entry = item_content_start(content, range.start)
                        .and_then(|(content_start, line_start)| {
                            let prefix = &content[line_start..range.start];
                            // A parent item that opened on this same line has
                            // its marker in the prefix, so the prefix is read
                            // from where that item's content starts.
                            let same_line = open_containers.iter().rposition(|&(parent, parent_line, _)| {
                                matches!(parent, Enclosure::Item { .. }) && parent_line == line_start
                            });
                            let (mut at, rest) = match same_line {
                                Some(i) => (open_containers[i].2, &open_containers[i + 1..]),
                                None => (StrippedLine::default(), &open_containers[..]),
                            };
                            if !rest.iter().all(|&(parent, _, _)| at.take_container(prefix, parent)) {
                                return None;
                            }
                            let width = content_start.col.checked_sub(at.col)?;
                            Some((Enclosure::Item { width, end: range.end }, line_start, content_start))
                        })
                        .unwrap_or((Enclosure::Opaque { end: range.end }, 0, StrippedLine::default()));
                    open_containers.push(entry);
                }
                Event::Start(Tag::DefinitionListDefinition | Tag::FootnoteDefinition(_)) => {
                    open_containers.push((Enclosure::Opaque { end: range.end }, 0, StrippedLine::default()));
                }
                Event::End(
                    TagEnd::BlockQuote(_)
                    | TagEnd::Item
                    | TagEnd::DefinitionListDefinition
                    | TagEnd::FootnoteDefinition,
                ) => {
                    open_containers.pop();
                }
                _ => {}
            }
            match &event {
                Event::Start(tag) if !is_inline_tag(tag) => {
                    definition_texts.extend(tight_run.take());
                    if block_stack.is_empty() {
                        top_level_blocks.push((range.start, range.end));
                    }
                    if matches!(tag, Tag::Table(_)) {
                        table_ranges.push((range.start, range.end));
                    }
                    if let (Tag::Paragraph, Some(Some(definition_start))) = (tag, block_stack.last()) {
                        definition_paragraph = Some((range.start, *definition_start));
                    }
                    let mut definition_start = None;
                    match tag {
                        Tag::DefinitionList => pending_terms.clear(),
                        Tag::DefinitionListTitle => pending_terms.push((range.start, range.end)),
                        Tag::DefinitionListDefinition => {
                            // The definition's range takes in the blank lines after
                            // it, quoted ones too, which belong to no block.
                            let item_start = pending_terms.first().map_or(range.start, |&(start, _)| start);
                            // Only ASCII suffix bytes are removed, remaining
                            // safe even if a parser regression reports a marker
                            // start inside a UTF-8 character.
                            let item_end = content.as_bytes()[range.clone()]
                                .iter()
                                .rposition(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'>'))
                                .map_or(range.start, |index| range.start + index + 1);
                            let accepted = opens_definition(content, range.start);
                            definition_rejected.push(!accepted);
                            if accepted {
                                definition_items.push((item_start, item_end));
                                definition_terms.append(&mut pending_terms);
                                definition_start = Some(range.start);
                            } else {
                                rejected_definition_ranges.push((item_start, item_end));
                                colon_definitions.push(ColonDefinitionDetail {
                                    start: item_start,
                                    end: item_end,
                                    definition_start: range.start,
                                });
                                pending_terms.clear();
                            }
                        }
                        _ => {}
                    }
                    block_stack.push(definition_start);
                }
                Event::End(tag_end) if !is_inline_tag_end(*tag_end) => {
                    definition_texts.extend(tight_run.take());
                    if let (TagEnd::Paragraph, Some((start, definition_start))) = (tag_end, definition_paragraph.take())
                    {
                        definition_texts.push(DefinitionTextDetail {
                            start,
                            end: range.end,
                            definition_start,
                        });
                    }
                    if let TagEnd::DefinitionListDefinition = tag_end {
                        definition_rejected.pop();
                    }
                    if let TagEnd::DefinitionList = tag_end {
                        pending_terms.clear();
                    }
                    block_stack.pop();
                }
                // Inline content directly under a definition is a tight
                // definition's text, which the parser does not wrap in a paragraph.
                _ => {
                    if let Some(Some(definition_start)) = block_stack.last() {
                        match &mut tight_run {
                            Some(run) => run.end = run.end.max(range.end),
                            None => {
                                tight_run = Some(DefinitionTextDetail {
                                    start: range.start,
                                    end: range.end,
                                    definition_start: *definition_start,
                                });
                            }
                        }
                    }
                }
            }
            if std::mem::take(&mut awaiting_first_code_event)
                && let Event::Text(text) = &event
                && range.is_empty()
                && let Some((_, _, _, Some(column))) = &mut code_block_start
            {
                *column = column.saturating_sub(text.len());
            }
            if code_block_start.is_some()
                && let Event::Text(text) = &event
                && !text.is_empty()
            {
                literal_newlines += text.matches('\n').count();
                literal_partial_line = !text.ends_with('\n');
            }
            match event {
                Event::Start(Tag::CodeBlock(kind)) => {
                    literal_newlines = 0;
                    literal_partial_line = false;
                    let (is_fenced, info_string) = match &kind {
                        CodeBlockKind::Fenced(info) => (true, info.to_string()),
                        CodeBlockKind::Indented => (false, String::new()),
                    };
                    let mut content_column =
                        (!is_fenced).then(|| visual_column(content, range.start).saturating_sub(4));
                    if !is_fenced && definition_rejected.contains(&true) {
                        // Dropping definition lists would also drop an accepted
                        // definition around the block, so its column is unknown.
                        if !block_stack.iter().any(Option::is_some) {
                            text_reading.push(range.start);
                        }
                        content_column = None;
                    }
                    awaiting_first_code_event = !is_fenced;
                    code_block_enclosing = if is_fenced {
                        open_containers.iter().map(|container| container.0).collect()
                    } else {
                        Vec::new()
                    };
                    code_block_start = Some((range.start, is_fenced, info_string, content_column));
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some((start, is_fenced, info_string, content_column)) = code_block_start.take() {
                        blocks.push((start, range.end));
                        let literal_lines = literal_newlines + usize::from(literal_partial_line);
                        details.push(CodeBlockDetail {
                            start,
                            end: range.end,
                            is_fenced,
                            info_string,
                            content_column,
                            closed: is_fenced && fenced_block_is_closed(content, start, range.end, literal_lines),
                            enclosing: std::mem::take(&mut code_block_enclosing),
                        });
                    }
                }
                Event::Start(Tag::Strong) => {
                    if range.start + 2 <= content.len() {
                        let is_asterisk = &content[range.start..range.start + 2] == "**";
                        strong_spans.push(StrongSpanDetail {
                            start: range.start,
                            end: range.end,
                            is_asterisk,
                        });
                    }
                }
                Event::Start(Tag::List(start_num)) => {
                    let is_ordered = start_num.is_some();
                    let start_value = start_num.unwrap_or(1);
                    list_stack.push((next_list_id, is_ordered, start_value));
                    if is_ordered {
                        list_start_values.insert(next_list_id, start_value);
                    }
                    next_list_id += 1;
                }
                Event::End(TagEnd::List(_)) => {
                    list_stack.pop();
                }
                Event::Start(Tag::Item) => {
                    if let Some(&(list_id, is_ordered, _)) = list_stack.last()
                        && is_ordered
                    {
                        let line_num = byte_to_line(range.start);
                        line_to_list.insert(line_num, list_id);
                    }
                }
                Event::Start(Tag::HtmlBlock) => {
                    // The start event's range already spans the whole block.
                    html_blocks.push((range.start, range.end));
                }
                Event::Code(_) => {
                    spans.push((range.start, range.end));
                }
                _ => {}
            }
        }

        // Handle edge case: unclosed code block at end of content
        // pulldown-cmark should handle this, but be defensive
        if let Some((start, is_fenced, info_string, content_column)) = code_block_start {
            blocks.push((start, content.len()));
            details.push(CodeBlockDetail {
                start,
                end: content.len(),
                is_fenced,
                info_string,
                content_column,
                closed: false,
                enclosing: std::mem::take(&mut code_block_enclosing),
            });
        }

        if !text_reading.is_empty() {
            let text_blocks = indented_block_columns(content, options - Options::ENABLE_DEFINITION_LIST);
            for detail in details.iter_mut().filter(|d| text_reading.contains(&d.start)) {
                let lines = (byte_to_line(detail.start), byte_to_line(detail.end.saturating_sub(1)));
                detail.content_column = text_blocks
                    .iter()
                    .find(|&&(start, end, _)| (byte_to_line(start), byte_to_line(end.saturating_sub(1))) == lines)
                    .map(|&(_, _, column)| column);
            }
        }

        // A colon-led definition whose text holds the same inline constructs
        // either way is read the same by both, and drops out.
        if !colon_definitions.is_empty() {
            let as_definitions = inline_ranges(content, options);
            let as_text = inline_ranges(content, options - Options::ENABLE_DEFINITION_LIST);
            let mut definition_ranges = InlineRangeCursor::new(&as_definitions);
            let mut text_ranges = InlineRangeCursor::new(&as_text);
            colon_definitions.retain(|detail| {
                !definition_ranges
                    .overlapping(detail)
                    .eq(text_ranges.overlapping(detail))
            });
        }

        // Sort by start position (should already be sorted, but ensure consistency)
        blocks.sort_by_key(|&(start, _)| start);
        spans.sort_by_key(|&(start, _)| start);
        details.sort_by_key(|d| d.start);
        strong_spans.sort_by_key(|s| s.start);
        html_blocks.sort_by_key(|&(start, _)| start);
        ParseResult {
            definition_items,
            definition_terms,
            definition_texts,
            colon_definitions,
            code_blocks: blocks,
            code_spans: spans,
            code_block_details: details,
            strong_spans,
            line_to_list,
            list_start_values,
            html_blocks,
            table_ranges,
            top_level_blocks,
            rejected_definition_ranges,
            list_items: list_item_spans
                .into_iter()
                .filter_map(|(start, end, content)| content.map(|content| (start, end, content)))
                .collect(),
        }
    }

    /// Check if a position is within a code block (for compatibility)
    pub fn is_in_code_block_or_span(blocks: &[(usize, usize)], pos: usize) -> bool {
        Self::is_in_code_block(blocks, pos)
    }

    /// Check if a byte position falls within any of the given sorted, non-overlapping ranges.
    ///
    /// Uses binary search on the sorted block ranges for O(log n) lookup.
    /// The blocks slice must be sorted by start position (as returned by
    /// `detect_code_blocks` and `detect_code_blocks_and_spans`).
    pub fn is_in_code_block(blocks: &[(usize, usize)], pos: usize) -> bool {
        // Binary search: find the last block whose start <= pos
        let idx = blocks.partition_point(|&(start, _)| start <= pos);
        // partition_point returns the first index where start > pos,
        // so the candidate is at idx - 1
        idx > 0 && pos < blocks[idx - 1].1
    }

    /// Analyze code block context relative to list parsing
    /// This is the core function implementing Design #3's three-tier classification
    pub fn analyze_code_block_context(
        lines: &[crate::lint_context::LineInfo],
        line_idx: usize,
        min_continuation_indent: usize,
    ) -> CodeBlockContext {
        if let Some(line_info) = lines.get(line_idx) {
            // Rule 1: Indentation Analysis - Is it sufficiently indented for list continuation?
            if line_info.indent >= min_continuation_indent {
                return CodeBlockContext::Indented;
            }

            // Rule 2: Blank Line Context - Check for structural separation indicators
            let (prev_blanks, next_blanks) = Self::count_surrounding_blank_lines(lines, line_idx);

            // Rule 3: Standalone Detection - Insufficient indentation + blank line separation
            // This is the key fix: root-level code blocks with blank lines separate lists
            if prev_blanks > 0 || next_blanks > 0 {
                return CodeBlockContext::Standalone;
            }

            // Rule 4: Default - Adjacent (conservative, non-breaking for edge cases)
            CodeBlockContext::Adjacent
        } else {
            // Fallback for invalid line index
            CodeBlockContext::Adjacent
        }
    }

    /// Count blank lines before and after the given line index
    fn count_surrounding_blank_lines(lines: &[crate::lint_context::LineInfo], line_idx: usize) -> (usize, usize) {
        let mut prev_blanks = 0;
        let mut next_blanks = 0;

        // Count blank lines before (look backwards)
        for i in (0..line_idx).rev() {
            if let Some(line) = lines.get(i) {
                if line.is_blank {
                    prev_blanks += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Count blank lines after (look forwards)
        for i in (line_idx + 1)..lines.len() {
            if let Some(line) = lines.get(i) {
                if line.is_blank {
                    next_blanks += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        (prev_blanks, next_blanks)
    }

    /// Calculate minimum indentation required for code block to continue a list
    /// Based on the most recent list item's marker width
    pub fn calculate_min_continuation_indent(
        content: &str,
        lines: &[crate::lint_context::LineInfo],
        current_line_idx: usize,
    ) -> usize {
        // Look backwards to find the most recent list item or separator.
        for i in (0..current_line_idx).rev() {
            if let Some(line_info) = lines.get(i)
                && let Some(indent) = Self::continuation_indent_boundary(content, line_info)
            {
                return indent;
            }
        }
        0
    }

    /// A list marker sets the continuation indent; a structural separator
    /// clears it. Other lines preserve the previous value. Marker precedence
    /// matters for list lines whose content also resembles a separator.
    pub(crate) fn continuation_indent_boundary(
        content: &str,
        line_info: &crate::lint_context::LineInfo,
    ) -> Option<usize> {
        if let Some(list_item) = &line_info.list_item {
            Some(if list_item.is_ordered {
                list_item.marker_column + list_item.marker.len() + 1
            } else {
                list_item.marker_column + 2
            })
        } else if line_info.heading.is_some() || Self::is_structural_separator(line_info.content(content)) {
            Some(0)
        } else {
            None
        }
    }

    /// Check if content is a structural separator (headings, horizontal rules, etc.)
    fn is_structural_separator(content: &str) -> bool {
        let trimmed = content.trim();
        trimmed.starts_with("---")
            || trimmed.starts_with("***")
            || trimmed.starts_with("___")
            || crate::utils::skip_context::is_table_line(trimmed)
            || trimmed.starts_with('>') // Blockquotes
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_continued_depth_is_the_longest_prefix_strip_enclosure_accepts() {
        use super::{Enclosure, continued_depth, item_runs, strip_enclosure};
        let quote = Enclosure::Quote { end: 0 };
        let item = |width| Enclosure::Item { width, end: 0 };
        let stacks = [
            vec![quote, item(2), item(2), quote, item(3)],
            vec![item(2), quote, quote, item(4)],
            vec![item(2), item(2), item(2)],
            vec![quote, Enclosure::Opaque { end: 0 }, item(2)],
        ];
        let lines = [
            "",
            "   ",
            "\t",
            ">",
            "> ",
            ">\t",
            ">    ",
            ">     > x",
            "> x",
            "  > > x",
            "  >",
            "  >  ",
            "    x",
            "  x",
            "      x",
            "\tx",
            " \t x",
            "lazy",
            "> > > x",
            ">     >    x",
        ];
        for stack in &stacks {
            let runs = item_runs(stack);
            for line in lines {
                let expected = (0..=stack.len())
                    .rev()
                    .find(|&k| strip_enclosure(line, &stack[..k]).is_some())
                    .unwrap();
                assert_eq!(
                    continued_depth(line, stack, &runs),
                    expected,
                    "line {line:?} in {stack:?}"
                );
            }
        }
    }

    #[test]
    fn test_items_opening_on_one_line_through_a_quote_record_their_widths() {
        use super::Enclosure;
        let quote = Enclosure::Quote { end: 0 };
        let item = |width| Enclosure::Item { width, end: 0 };
        let cases = [
            ("- > - ```md\n  >   x\n  >   ```\n", vec![item(2), quote, item(2)]),
            (
                "> - > - ```md\n>   >   x\n>   >   ```\n",
                vec![quote, item(2), quote, item(2)],
            ),
            ("1. > - ```md\n   >   x\n   >   ```\n", vec![item(3), quote, item(2)]),
            ("- > 10. ```md\n  >     x\n  >     ```\n", vec![item(2), quote, item(4)]),
            ("- >\t- ```md\n  >\t  x\n  >\t  ```\n", vec![item(2), quote, item(2)]),
            (
                "- > - - ```md\n  >     x\n  >     ```\n",
                vec![item(2), quote, item(2), item(2)],
            ),
            (
                "- > > - ```md\n  > >   x\n  > >   ```\n",
                vec![item(2), quote, quote, item(2)],
            ),
        ];
        for (content, expected) in cases {
            let parsed = CodeBlockUtils::detect_code_blocks_and_spans(content);
            let enclosing: Vec<Enclosure> = parsed.code_block_details[0]
                .enclosing
                .iter()
                .map(|&enclosure| match enclosure {
                    Enclosure::Quote { .. } => quote,
                    Enclosure::Item { width, .. } => item(width),
                    Enclosure::Opaque { .. } => Enclosure::Opaque { end: 0 },
                })
                .collect();
            assert_eq!(enclosing, expected, "{content:?}");
        }
    }

    #[test]
    fn test_table_ranges_and_top_level_blocks_are_recorded() {
        let content = "para\n\n|a|b|\n|-|-|\n|1|2|\n\n- item\n\n  |c|d|\n  |-|-|\n";
        let parsed = CodeBlockUtils::detect_code_blocks_and_spans(content);
        let text = |&(start, end): &(usize, usize)| &content[start..end];

        assert_eq!(parsed.table_ranges.len(), 2);
        assert_eq!(text(&parsed.table_ranges[0]), "|a|b|\n|-|-|\n|1|2|\n");
        assert!(text(&parsed.table_ranges[1]).starts_with("|c|d|"));

        // The paragraph, the first table and the list are top level; the table
        // inside the list item is not.
        let top: Vec<&str> = parsed.top_level_blocks.iter().map(text).collect();
        assert_eq!(top.len(), 3, "{top:?}");
        assert_eq!(top[0], "para\n");
        assert!(top[2].starts_with("- item"));
    }

    #[test]
    fn test_list_items_record_where_their_content_starts() {
        let content = "- - x\n\n-   y\n-\n- \n  z\n";
        let parsed = CodeBlockUtils::detect_code_blocks_and_spans(content);
        let starts: Vec<(&str, &str)> = parsed
            .list_items
            .iter()
            .map(|&(start, _, content_start)| (&content[start..=start], &content[content_start..=content_start]))
            .collect();
        // Outer item holds the inner marker; the empty `-` item has no content.
        assert_eq!(starts, [("-", "-"), ("-", "x"), ("-", "y"), ("-", "z")]);
    }

    #[test]
    fn test_no_table_ranges_without_a_table() {
        let parsed = CodeBlockUtils::detect_code_blocks_and_spans("a|b\n|-|\n\n    |a|b|\n    |-|-|\n");
        assert!(parsed.table_ranges.is_empty());
    }

    use super::*;

    #[test]
    fn test_detect_fenced_code_blocks() {
        // The function detects fenced blocks and inline code spans
        // Fence markers (``` at line start) are now skipped in inline span detection

        // Basic fenced code block with backticks
        let content = "Some text\n```\ncode here\n```\nMore text";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // Should find: 1 fenced block (fences are no longer detected as inline spans)
        assert_eq!(blocks.len(), 1);

        // Check that we have the fenced block
        let fenced_block = blocks
            .iter()
            .find(|(start, end)| end - start > 10 && content[*start..*end].contains("code here"));
        assert!(fenced_block.is_some());

        // Fenced code block with tildes (no inline code detection for ~)
        let content = "Some text\n~~~\ncode here\n~~~\nMore text";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(&content[blocks[0].0..blocks[0].1], "~~~\ncode here\n~~~");

        // Multiple code blocks
        let content = "Text\n```\ncode1\n```\nMiddle\n~~~\ncode2\n~~~\nEnd";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 2 fenced blocks (fence markers no longer detected as inline spans)
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn test_detect_code_blocks_with_language() {
        // Code block with language identifier
        let content = "Text\n```rust\nfn main() {}\n```\nMore";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 1 fenced block (fence markers no longer detected as inline spans)
        assert_eq!(blocks.len(), 1);
        // Check we have the full fenced block
        let fenced = blocks.iter().find(|(s, e)| content[*s..*e].contains("fn main"));
        assert!(fenced.is_some());
    }

    #[test]
    fn test_unclosed_code_block() {
        // Unclosed code block should extend to end of content
        let content = "Text\n```\ncode here\nno closing fence";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, content.len());
    }

    #[test]
    fn test_indented_code_blocks() {
        // Basic indented code block
        let content = "Paragraph\n\n    code line 1\n    code line 2\n\nMore text";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert!(content[blocks[0].0..blocks[0].1].contains("code line 1"));
        assert!(content[blocks[0].0..blocks[0].1].contains("code line 2"));

        // Indented code with tabs
        let content = "Paragraph\n\n\tcode with tab\n\tanother line\n\nText";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn test_indented_code_requires_blank_line() {
        // Indented lines without preceding blank line are not code blocks
        let content = "Paragraph\n    indented but not code\nMore text";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0);

        // With blank line, it becomes a code block
        let content = "Paragraph\n\n    now it's code\nMore text";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn test_indented_content_with_list_markers_is_code_block() {
        // Per CommonMark spec: 4-space indented content after blank line IS a code block,
        // even if the content looks like list markers. The indentation takes precedence.
        // Verified with: echo 'List:\n\n    - Item 1' | npx commonmark
        // Output: <pre><code>- Item 1</code></pre>
        let content = "List:\n\n    - Item 1\n    - Item 2\n    * Item 3\n    + Item 4";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1); // This IS a code block per spec

        // Same for numbered list markers
        let content = "List:\n\n    1. First\n    2. Second";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1); // This IS a code block per spec
    }

    #[test]
    fn test_actual_list_items_not_code_blocks() {
        // Actual list items (no preceding blank line + 4 spaces) are NOT code blocks
        let content = "- Item 1\n- Item 2\n* Item 3";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0);

        // Nested list items
        let content = "- Item 1\n  - Nested item\n- Item 2";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0);
    }

    #[test]
    fn test_inline_code_spans_not_detected() {
        // Inline code spans should NOT be detected as code blocks
        let content = "Text with `inline code` here";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0); // No blocks, only inline spans

        // Multiple backtick code span
        let content = "Text with ``code with ` backtick`` here";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0); // No blocks, only inline spans

        // Multiple code spans
        let content = "Has `code1` and `code2` spans";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0); // No blocks, only inline spans
    }

    #[test]
    fn test_unclosed_code_span() {
        // Unclosed code span should not be detected
        let content = "Text with `unclosed code span";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0);

        // Mismatched backticks
        let content = "Text with ``one style` different close";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0);
    }

    #[test]
    fn test_mixed_code_blocks_and_spans() {
        let content = "Has `span1` text\n```\nblock\n```\nand `span2`";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // Should only detect the fenced block, NOT the inline spans
        assert_eq!(blocks.len(), 1);

        // Check we have the fenced block only
        assert!(blocks.iter().any(|(s, e)| content[*s..*e].contains("block")));
        // Should NOT detect inline spans
        assert!(!blocks.iter().any(|(s, e)| &content[*s..*e] == "`span1`"));
        assert!(!blocks.iter().any(|(s, e)| &content[*s..*e] == "`span2`"));
    }

    #[test]
    fn test_is_in_code_block_or_span() {
        let blocks = vec![(10, 20), (30, 40), (50, 60)];

        // Test positions inside blocks
        assert!(CodeBlockUtils::is_in_code_block_or_span(&blocks, 15));
        assert!(CodeBlockUtils::is_in_code_block_or_span(&blocks, 35));
        assert!(CodeBlockUtils::is_in_code_block_or_span(&blocks, 55));

        // Test positions at boundaries
        assert!(CodeBlockUtils::is_in_code_block_or_span(&blocks, 10)); // Start is inclusive
        assert!(!CodeBlockUtils::is_in_code_block_or_span(&blocks, 20)); // End is exclusive

        // Test positions outside blocks
        assert!(!CodeBlockUtils::is_in_code_block_or_span(&blocks, 5));
        assert!(!CodeBlockUtils::is_in_code_block_or_span(&blocks, 25));
        assert!(!CodeBlockUtils::is_in_code_block_or_span(&blocks, 65));
    }

    #[test]
    fn test_empty_content() {
        let blocks = CodeBlockUtils::detect_code_blocks("");
        assert_eq!(blocks.len(), 0);
    }

    #[test]
    fn test_code_block_at_start() {
        let content = "```\ncode\n```\nText after";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 1 fenced block (fence markers no longer detected as inline spans)
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, 0); // Fenced block starts at 0
    }

    #[test]
    fn test_code_block_at_end() {
        let content = "Text before\n```\ncode\n```";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 1 fenced block (fence markers no longer detected as inline spans)
        assert_eq!(blocks.len(), 1);
        // Check we have the fenced block
        let fenced = blocks.iter().find(|(s, e)| content[*s..*e].contains("code"));
        assert!(fenced.is_some());
    }

    #[test]
    fn test_nested_fence_markers() {
        // Code block containing fence markers as content
        let content = "Text\n````\n```\nnested\n```\n````\nAfter";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // Should detect: outer block, inner ```, outer ````
        assert!(!blocks.is_empty());
        // Check we have the outer block
        let outer = blocks.iter().find(|(s, e)| content[*s..*e].contains("nested"));
        assert!(outer.is_some());
    }

    #[test]
    fn test_indented_code_with_blank_lines() {
        // Indented code blocks can contain blank lines
        let content = "Text\n\n    line1\n\n    line2\n\nAfter";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // May have multiple blocks due to blank line handling
        assert!(!blocks.is_empty());
        // Check that we captured the indented code
        let all_content: String = blocks
            .iter()
            .map(|(s, e)| &content[*s..*e])
            .collect::<Vec<_>>()
            .join("");
        assert!(all_content.contains("line1") || content[blocks[0].0..blocks[0].1].contains("line1"));
    }

    #[test]
    fn test_code_span_with_spaces() {
        // Code spans should NOT be detected as code blocks
        let content = "Text ` code with spaces ` more";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0); // No blocks, only inline span
    }

    #[test]
    fn test_fenced_block_with_info_string() {
        // Fenced code blocks with complex info strings
        let content = "```rust,no_run,should_panic\ncode\n```";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 1 fenced block (fence markers no longer detected as inline spans)
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, 0);
    }

    #[test]
    fn test_indented_fences_not_code_blocks() {
        // Indented fence markers should still work as fences
        let content = "Text\n  ```\n  code\n  ```\nAfter";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // Only 1 fenced block (indented fences still work)
        assert_eq!(blocks.len(), 1);
    }

    // Issue #175: Backticks in info string invalidate the fence
    #[test]
    fn test_backticks_in_info_string_not_code_block() {
        // Per CommonMark spec: "If the info string comes after a backtick fence,
        // it may not contain any backtick characters."
        // So ```something``` is NOT a valid fence - the backticks are treated as inline code.
        // Verified with: echo '```something```' | npx commonmark
        // Output: <p><code>something</code></p>
        let content = "```something```\n\n```bash\n# comment\n```";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // Should find only the valid ```bash block, NOT the invalid ```something```
        assert_eq!(blocks.len(), 1);
        // The valid block should contain "# comment"
        assert!(content[blocks[0].0..blocks[0].1].contains("# comment"));
    }

    #[test]
    fn test_issue_175_reproduction() {
        // Full reproduction of issue #175
        let content = "```something```\n\n```bash\n# Have a parrot\necho \"🦜\"\n```";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // Only the bash block is a code block
        assert_eq!(blocks.len(), 1);
        assert!(content[blocks[0].0..blocks[0].1].contains("Have a parrot"));
    }

    #[test]
    fn test_tilde_fence_allows_tildes_in_info_string() {
        // Tilde fences CAN have tildes in info string (only backtick restriction exists)
        // ~~~abc~~~ opens an unclosed code block with info string "abc~~~"
        let content = "~~~abc~~~\ncode content\n~~~";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // This is a valid tilde fence that opens and closes
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn test_nested_longer_fence_contains_shorter() {
        // Longer fence (````) can contain shorter fence (```) as content
        let content = "````\n```\nnested content\n```\n````";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert!(content[blocks[0].0..blocks[0].1].contains("nested content"));
    }

    #[test]
    fn test_mixed_fence_types() {
        // Tilde fence contains backtick markers as content
        let content = "~~~\n```\nmixed content\n~~~";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert!(content[blocks[0].0..blocks[0].1].contains("mixed content"));
    }

    #[test]
    fn test_indented_block_content_column_is_its_containers() {
        let cases = [
            ("Text\n\n    code\n", 0),
            ("- a\n\n      code\n", 2),
            ("1. a\n\n       code\n", 3),
            // The unindented paragraph ends the list, so the block is at the margin.
            ("1. a\n\nb\n\n       code\n", 0),
            // A lazy continuation keeps the item open however long it runs.
            ("1. a\nb\nc\nd\ne\nf\ng\n\n       code\n", 3),
            (">     quoted\n", 2),
            ("- a\n\n\t  code\n", 2),
            // The second tab straddles column 7, where the code begins.
            ("1. a\n\n\t\tcode\n", 3),
            (">\t\tcode\n", 2),
        ];
        for (content, expected) in cases {
            let result = CodeBlockUtils::detect_code_blocks_and_spans(content);
            let columns: Vec<_> = result.code_block_details.iter().map(|d| d.content_column).collect();
            assert_eq!(columns, [Some(expected)], "content column for {content:?}");
        }

        let fenced = CodeBlockUtils::detect_code_blocks_and_spans("- a\n\n  ```\n  code\n  ```\n");
        let columns: Vec<_> = fenced.code_block_details.iter().map(|d| d.content_column).collect();
        assert_eq!(columns, [None]);
    }

    #[test]
    fn test_indented_block_in_rejected_definition_takes_its_columns_from_the_text_reading() {
        let cases = [
            // `:warning:` opens no definition, so the block sits at the margin.
            ("term\n:warning:\n\n      x\n", Some(0)),
            ("- a\n\n  term\n  :warning:\n\n        x\n", Some(2)),
            // A definition rumdl accepts is a container.
            ("term\n: def\n\n      x\n", Some(2)),
            // Under an accepted definition the text reading would drop that
            // container too, so the column is unknown.
            ("term\n: def\n\n  t2\n  :warning:\n\n        x\n", None),
        ];
        for (content, expected) in cases {
            let result = CodeBlockUtils::detect_code_blocks_and_spans(content);
            let columns: Vec<_> = result.code_block_details.iter().map(|d| d.content_column).collect();
            assert_eq!(columns, [expected], "content column for {content:?}");
        }
    }

    #[test]
    fn tab_indented_definition_markers_after_utf8_keep_valid_ranges() {
        for newline in ["\n", "\r", "\r\n"] {
            for term in ["", "a", "¡", "🦀"] {
                for text in ["b", "\u{e000}", "🦀"] {
                    for tail in ["\t:", "\t: next", "\t: *next*", "\t: `next`"] {
                        let source = format!("{term}{newline}:{text}{newline}{tail}");
                        let result = CodeBlockUtils::detect_code_blocks_and_spans(&source);
                        // 0.13.5 fixes the nested marker's offset. Its valid
                        // colon is now accepted, while the colon touching `text`
                        // remains rejected. No preceding term means no list.
                        assert_eq!(
                            result.definition_items.len(),
                            usize::from(!term.is_empty()),
                            "{source:?}"
                        );
                        assert_eq!(
                            result.definition_texts.len(),
                            usize::from(!term.is_empty() && tail != "\t:"),
                            "{source:?}"
                        );
                        let parent_colon = term.len() + newline.len();
                        let nested_colon = source.len() - tail.len() + 1;
                        for &(start, end) in &result.definition_items {
                            assert_eq!((start, end), (parent_colon + 1, source.len()), "{source:?}");
                            assert!(source.get(start..end).is_some(), "{source:?}: {start}..{end}");
                        }
                        for detail in &result.definition_texts {
                            assert!(source.get(detail.start..detail.end).is_some(), "{source:?}: {detail:?}");
                            assert_eq!(detail.definition_start, nested_colon, "{source:?}");
                        }
                        if !term.is_empty() {
                            assert!(
                                result
                                    .rejected_definition_ranges
                                    .iter()
                                    .any(|&(start, end)| start <= parent_colon && parent_colon < end),
                                "{source:?}"
                            );
                        }
                        for &(start, end) in &result.rejected_definition_ranges {
                            assert!(source.get(start..end).is_some(), "{source:?}: {start}..{end}");
                        }
                        let ctx = crate::lint_context::LintContext::new(
                            &source,
                            crate::config::MarkdownFlavor::Standard,
                            None,
                        );
                        assert_eq!(ctx.content, source);
                    }
                }
            }
        }
        assert!(!super::opens_definition("¡", 1));
        assert!(!super::opens_definition(": text", usize::MAX));
        assert!(super::opens_definition("\t: text", 0));
    }

    #[test]
    fn test_indented_code_in_list_issue_276() {
        // Issue #276: Indented code block inside a list should be detected by pulldown-cmark
        let content = r#"1. First item
2. Second item with code:

        # This is a code block in a list
        print("Hello, world!")

4. Third item"#;

        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // pulldown-cmark SHOULD detect this indented code block inside the list
        assert!(!blocks.is_empty(), "Should detect indented code block inside list");

        // Verify the detected block contains our code
        let all_content: String = blocks
            .iter()
            .map(|(s, e)| &content[*s..*e])
            .collect::<Vec<_>>()
            .join("");
        assert!(
            all_content.contains("code block in a list") || all_content.contains("print"),
            "Detected block should contain the code content: {all_content:?}"
        );
    }

    // ── is_in_code_block binary search tests ─────────────────────────────

    #[test]
    fn test_is_in_code_block_empty_blocks() {
        assert!(!CodeBlockUtils::is_in_code_block(&[], 0));
        assert!(!CodeBlockUtils::is_in_code_block(&[], 100));
        assert!(!CodeBlockUtils::is_in_code_block(&[], usize::MAX));
    }

    #[test]
    fn test_is_in_code_block_single_range() {
        let blocks = [(10, 20)];
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 0));
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 9));
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 10));
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 15));
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 19));
        // end is exclusive
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 20));
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 21));
    }

    #[test]
    fn test_is_in_code_block_multiple_ranges() {
        let blocks = [(5, 10), (20, 30), (50, 60)];
        // Before all
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 0));
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 4));
        // In first
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 5));
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 9));
        // Gap between first and second
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 10));
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 15));
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 19));
        // In second
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 20));
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 29));
        // Gap between second and third
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 30));
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 49));
        // In third
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 50));
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 59));
        // After all
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 60));
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 1000));
    }

    #[test]
    fn test_is_in_code_block_adjacent_ranges() {
        // Ranges that are exactly adjacent (end of one == start of next)
        let blocks = [(0, 10), (10, 20), (20, 30)];
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 0));
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 9));
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 10));
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 19));
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 20));
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 29));
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 30));
    }

    #[test]
    fn test_is_in_code_block_single_byte_range() {
        let blocks = [(5, 6)];
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 4));
        assert!(CodeBlockUtils::is_in_code_block(&blocks, 5));
        assert!(!CodeBlockUtils::is_in_code_block(&blocks, 6));
    }

    #[test]
    fn test_is_in_code_block_matches_linear_scan() {
        // Verify binary search produces identical results to linear scan
        // for a realistic document layout
        let content = "# Heading\n\n```rust\nlet x = 1;\nlet y = 2;\n```\n\nSome text\n\n```\nmore code\n```\n\nEnd\n";
        let blocks = CodeBlockUtils::detect_code_blocks(content);

        for pos in 0..content.len() {
            let binary = CodeBlockUtils::is_in_code_block(&blocks, pos);
            let linear = blocks.iter().any(|&(s, e)| pos >= s && pos < e);
            assert_eq!(
                binary, linear,
                "Mismatch at pos {pos}: binary={binary}, linear={linear}, blocks={blocks:?}"
            );
        }
    }

    #[test]
    fn test_is_in_code_block_at_range_boundaries() {
        // Exhaustive boundary testing for every block start/end
        let blocks = [(100, 200), (300, 400), (500, 600)];
        for &(start, end) in &blocks {
            assert!(
                !CodeBlockUtils::is_in_code_block(&blocks, start - 1),
                "pos={} should be outside",
                start - 1
            );
            assert!(
                CodeBlockUtils::is_in_code_block(&blocks, start),
                "pos={start} should be inside"
            );
            assert!(
                CodeBlockUtils::is_in_code_block(&blocks, end - 1),
                "pos={} should be inside",
                end - 1
            );
            assert!(
                !CodeBlockUtils::is_in_code_block(&blocks, end),
                "pos={end} should be outside"
            );
        }
    }
}

/// `CodeBlockDetail::closed` says whether a closing fence line ends the block,
/// which only the parser's reading of the fence rules settles: a last line that
/// looks like a fence can be code.
#[cfg(test)]
mod closed_fence_tests {
    use super::*;

    fn closed_of(content: &str) -> Vec<bool> {
        CodeBlockUtils::detect_code_blocks_and_spans(content)
            .code_block_details
            .iter()
            .filter(|detail| detail.is_fenced)
            .map(|detail| detail.closed)
            .collect()
    }

    #[test]
    fn a_block_ended_by_a_closing_fence_is_closed() {
        for content in [
            "```\nx\n```\n",
            "```\nx\n```",
            "```\n```\n",
            "~~~\nx\n~~~\n",
            "````\nx\n`````\n",
            "```\nx\n``` \n",
            "> ```\n> x\n> ```\nfoo\n",
            "> ```\n> ```\nfoo\n",
            "-\n  ```\n  x\n  ```\n",
            "- ```\n  x\n\n  ```\n",
            "```\n\n\n```\n",
            "   ```\nx\n   ```\n",
        ] {
            assert_eq!(closed_of(content), vec![true], "{content:?}");
        }
    }

    #[test]
    fn a_block_the_document_or_container_ends_is_not_closed() {
        for content in [
            "```\nx\n",
            "```\nx",
            "```",
            "```\n",
            "> ```\nfoo\n",
            "> ```\n> x\nfoo\n",
            // Last line looks like a fence but is code.
            "> ```\n> ~~~\nfoo\n",
            "```\n~~~\n",
            "> ````\n> ```\nfoo\n",
            "> ```\n>     ```\nfoo\n",
            "```\n    ```\n",
            "> ```\n> \u{a0}```\nfoo\n",
            "> ```\n> ```x\nfoo\n",
            "> ```\n> ```\u{a0}\nfoo\n",
            // A last line of whitespace is inside the parser's range, and has no
            // text event of its own.
            "> ```\n> ",
            "```\n  ",
            "> ```\n> ~~~\n> ",
            "- ```\nfoo\n",
        ] {
            assert_eq!(closed_of(content), vec![false], "{content:?}");
        }
    }
}
