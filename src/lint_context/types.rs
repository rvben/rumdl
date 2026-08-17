use pulldown_cmark::LinkType;
use std::borrow::Cow;

/// Pre-computed information about a line
#[derive(Debug, Clone)]
pub struct LineInfo {
    /// Byte offset where this line starts in the document
    pub byte_offset: usize,
    /// Length of the line in bytes (without newline)
    pub byte_len: usize,
    /// Number of bytes of leading whitespace (for substring extraction)
    pub indent: usize,
    /// Visual column width of leading whitespace (with proper tab expansion)
    /// Per CommonMark, tabs expand to the next column that is a multiple of 4.
    /// Use this for numeric comparisons like checking for indented code blocks (>= 4).
    pub visual_indent: usize,
    /// Whether the line is blank (empty or only whitespace)
    pub is_blank: bool,
    /// Whether this line is inside a code block
    pub in_code_block: bool,
    /// Whether this line is inside front matter
    pub in_front_matter: bool,
    /// Whether this line is inside an HTML block
    pub in_html_block: bool,
    /// Whether this line is part of a list block (precomputed for O(1) lookup)
    pub in_list_block: bool,
    /// Whether this line is part of a table block (precomputed for O(1) lookup)
    pub in_table_block: bool,
    /// Whether this line is inside an HTML comment
    pub in_html_comment: bool,
    /// List item information if this line starts a list item
    /// Boxed to reduce LineInfo size: most lines are not list items
    pub list_item: Option<Box<ListItemInfo>>,
    /// Heading information if this line is a heading
    /// Boxed to reduce LineInfo size: most lines are not headings
    pub heading: Option<Box<HeadingInfo>>,
    /// Blockquote information if this line is a blockquote
    /// Boxed to reduce LineInfo size: most lines are not blockquotes
    pub blockquote: Option<Box<BlockquoteInfo>>,
    /// Whether this line is inside a mkdocstrings autodoc block
    pub in_mkdocstrings: bool,
    /// Whether this line is part of an ESM import/export block (MDX only)
    pub in_esm_block: bool,
    /// Whether this line is a continuation of a multi-line code span from a previous line
    pub in_code_span_continuation: bool,
    /// Whether this line is a horizontal rule (---, ***, ___, etc.)
    /// Pre-computed for consistent detection across all rules
    pub is_horizontal_rule: bool,
    /// Whether this line is inside a math block ($$ ... $$)
    pub in_math_block: bool,
    /// Whether this line is inside a Pandoc/Quarto div block (::: ... :::)
    pub in_pandoc_div: bool,
    /// Whether this line is a Quarto/Pandoc div marker (opening ::: {.class} or closing :::)
    /// Analogous to `is_horizontal_rule` — marks structural delimiters that are not paragraph text
    pub is_div_marker: bool,
    /// Whether this line contains or is inside a JSX expression (MDX only)
    pub in_jsx_expression: bool,
    /// Whether this line is inside an MDX comment {/* ... */} (MDX only)
    pub in_mdx_comment: bool,
    /// Whether this line is inside an MkDocs admonition block (!!! or ???)
    pub in_admonition: bool,
    /// Whether this line is inside an MkDocs content tab block (===)
    pub in_content_tab: bool,
    /// Whether this line is inside an HTML block with markdown attribute (MkDocs grid cards, etc.)
    pub in_mkdocs_html_markdown: bool,
    /// Whether this line is a definition list item (: definition)
    pub in_definition_list: bool,
    /// Whether this line is inside an Obsidian comment (%%...%% syntax, Obsidian flavor only)
    pub in_obsidian_comment: bool,
    /// Whether this line is inside a PyMdown Blocks region (/// ... ///, MkDocs flavor only)
    pub in_pymdown_block: bool,
    /// Whether this line is inside a kramdown extension block ({::comment}...{:/comment}, {::nomarkdown}...{:/nomarkdown})
    pub in_kramdown_extension_block: bool,
    /// Whether this line is a kramdown block IAL ({:.class #id}) or ALD ({:ref: .class})
    pub is_kramdown_block_ial: bool,
    /// Whether this line is inside a JSX component block (MDX only, e.g. `<Tabs>...</Tabs>`)
    pub in_jsx_block: bool,
    /// Whether this line is inside a footnote definition body (continuation lines)
    pub in_footnote_definition: bool,
    /// Whether this line is inside a MyST directive block (colon or backtick fence with `{name}`)
    pub in_myst_directive: bool,
    /// Whether this line is a MyST comment (`% comment`)
    pub is_myst_comment: bool,
}

impl LineInfo {
    /// Get the line content as a string slice from the source document
    pub fn content<'a>(&self, source: &'a str) -> &'a str {
        &source[self.byte_offset..self.byte_offset + self.byte_len]
    }

    /// Check if this line is inside MkDocs-specific indented content (admonitions, tabs, or markdown HTML).
    /// This content uses 4-space indentation which pulldown-cmark would interpret as code blocks,
    /// but in MkDocs flavor it's actually container content that should be preserved.
    #[inline]
    pub fn in_mkdocs_container(&self) -> bool {
        self.in_admonition || self.in_content_tab || self.in_mkdocs_html_markdown
    }

    /// Whether this line is a heading in the document's structure.
    ///
    /// An ATX line without a space after its `#`s is recorded as a heading so
    /// MD018 can report it, with `is_valid` carrying heading detection's verdict
    /// on whether a heading was meant: `#2, #3` and `#hashtag` read as paragraph
    /// text (`is_valid == false`), `##hashtag` and `#Hashtag` as headings missing
    /// their space. Structurally an invalid one is paragraph text: it continues a
    /// list item and does not separate two lists. Structural code asks this
    /// instead of `heading.is_some()`.
    #[inline]
    pub fn is_valid_heading(&self) -> bool {
        self.heading.as_ref().is_some_and(|h| h.is_valid)
    }

    /// Whether this line could be part of a paragraph block (CommonMark `paragraph` token).
    ///
    /// Returns true for ordinary prose lines, including those inside blockquotes and list items.
    /// Returns false for lines that belong to non-paragraph blocks: headings, code blocks,
    /// HTML blocks, math blocks, horizontal rules, front matter, structural div markers, and
    /// flavor-specific extension blocks. This is the per-line view; cross-line constructs like
    /// setext underlines aren't visible here and need additional context to detect.
    ///
    /// Used by rules (e.g. MD009 strict mode) that need to distinguish "trailing whitespace
    /// could produce a meaningful `<br>`" from "trailing whitespace is on a structural boundary."
    #[inline]
    pub fn is_paragraph_context(&self) -> bool {
        !self.in_code_block
            && !self.in_front_matter
            && !self.in_html_block
            && !self.in_html_comment
            && !self.in_math_block
            && !self.is_horizontal_rule
            && !self.is_div_marker
            && !self.in_pymdown_block
            && !self.in_kramdown_extension_block
            && !self.is_kramdown_block_ial
            && !self.is_myst_comment
            && self.heading.is_none()
    }
}

/// Information about a list item
#[derive(Debug, Clone)]
pub struct ListItemInfo {
    /// The marker used (*, -, +, or number with . or ))
    pub marker: String,
    /// Whether it's ordered (true) or unordered (false)
    pub is_ordered: bool,
    /// The number for ordered lists
    pub number: Option<usize>,
    /// Column where the marker starts (0-based)
    pub marker_column: usize,
    /// Column where content after marker starts
    pub content_column: usize,
}

/// Heading style type
#[derive(Debug, Clone, PartialEq)]
pub enum HeadingStyle {
    /// ATX style heading (# Heading)
    ATX,
    /// Setext style heading with = underline
    Setext1,
    /// Setext style heading with - underline
    Setext2,
}

/// Parsed link information
#[derive(Debug, Clone)]
pub struct ParsedLink<'a> {
    /// Line number (1-indexed)
    pub line: usize,
    /// Line the link ends on (1-indexed). A link can span lines, so `end_col` is
    /// a column of *this* line, not of `line`.
    pub end_line: usize,
    /// Start column (0-indexed) in the line
    pub start_col: usize,
    /// End column (0-indexed) in `end_line`
    pub end_col: usize,
    /// Byte offset in document
    pub byte_offset: usize,
    /// End byte offset in document
    pub byte_end: usize,
    /// Link text
    pub text: Cow<'a, str>,
    /// Link URL or reference
    pub url: Cow<'a, str>,
    /// Inline title (without surrounding delimiters), as produced by pulldown-cmark
    /// after backslash-escape handling. `None` when the link has no title or is a
    /// reference style without a matched definition.
    pub title: Option<Cow<'a, str>>,
    /// Whether this is a reference link `[text][ref]` vs inline `[text](url)`
    pub is_reference: bool,
    /// Reference ID for reference links
    pub reference_id: Option<Cow<'a, str>>,
    /// Link type from pulldown-cmark
    pub link_type: LinkType,
}

/// Information about a broken link reported by pulldown-cmark
#[derive(Debug, Clone)]
pub struct BrokenLinkInfo {
    /// The reference text that couldn't be resolved
    pub reference: String,
    /// Byte span in the source document
    pub span: std::ops::Range<usize>,
    /// The type of the broken link
    pub link_type: LinkType,
}

/// Parsed footnote reference (e.g., `[^1]`, `[^note]`)
#[derive(Debug, Clone)]
pub struct FootnoteRef {
    /// The footnote ID (without the ^ prefix)
    pub id: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Start byte offset in document
    pub byte_offset: usize,
}

/// Parsed image information
#[derive(Debug, Clone)]
pub struct ParsedImage<'a> {
    /// Line number (1-indexed)
    pub line: usize,
    /// Line the image ends on (1-indexed). An image can span lines, so `end_col`
    /// is a column of *this* line, not of `line`.
    pub end_line: usize,
    /// Start column (0-indexed) in the line
    pub start_col: usize,
    /// End column (0-indexed) in `end_line`
    pub end_col: usize,
    /// Byte offset in document
    pub byte_offset: usize,
    /// End byte offset in document
    pub byte_end: usize,
    /// Alt text
    pub alt_text: Cow<'a, str>,
    /// Image URL or reference
    pub url: Cow<'a, str>,
    /// Inline title (without surrounding delimiters), as produced by pulldown-cmark
    /// after backslash-escape handling. `None` when the image has no title or is a
    /// reference style without a matched definition.
    pub title: Option<Cow<'a, str>>,
    /// Whether this is a reference image ![alt][ref] vs inline ![alt](url)
    pub is_reference: bool,
    /// Reference ID for reference images
    pub reference_id: Option<Cow<'a, str>>,
    /// Link type from pulldown-cmark
    pub link_type: LinkType,
}

/// Reference definition `[ref]: url "title"`
#[derive(Debug, Clone)]
pub struct ReferenceDef {
    /// Line number (1-indexed)
    pub line: usize,
    /// Reference ID (normalized to lowercase)
    pub id: String,
    /// URL
    pub url: String,
    /// Optional title
    pub title: Option<String>,
    /// Byte offset where the reference definition starts
    pub byte_offset: usize,
    /// Byte offset where the reference definition ends
    pub byte_end: usize,
    /// Byte offset where the title starts (if present, includes quote)
    pub title_byte_start: Option<usize>,
    /// Byte offset where the title ends (if present, includes quote)
    pub title_byte_end: Option<usize>,
}

/// Parsed code span information
#[derive(Debug, Clone)]
pub struct CodeSpan {
    /// Line number where the code span starts (1-indexed)
    pub line: usize,
    /// Line number where the code span ends (1-indexed)
    pub end_line: usize,
    /// Start column (0-indexed) in the line
    pub start_col: usize,
    /// End column (0-indexed) in the line
    pub end_col: usize,
    /// Byte offset in document
    pub byte_offset: usize,
    /// End byte offset in document
    pub byte_end: usize,
    /// Number of backticks used (1, 2, 3, etc.)
    pub backtick_count: usize,
    /// Content inside the code span (without backticks)
    pub content: String,
}

/// Parsed math span information (inline $...$ or display $$...$$)
#[derive(Debug, Clone)]
pub struct MathSpan {
    /// Line number where the math span starts (1-indexed)
    pub line: usize,
    /// Line number where the math span ends (1-indexed)
    pub end_line: usize,
    /// Start column (0-indexed) in the line
    pub start_col: usize,
    /// End column (0-indexed) in the line
    pub end_col: usize,
    /// Byte offset in document
    pub byte_offset: usize,
    /// End byte offset in document
    pub byte_end: usize,
    /// Whether this is display math ($$...$$) vs inline ($...$)
    pub is_display: bool,
    /// Content inside the math delimiters
    pub content: String,
}

/// Information about a heading
#[derive(Debug, Clone)]
pub struct HeadingInfo {
    /// Heading level (1-6 for ATX, 1-2 for Setext)
    pub level: u8,
    /// Style of heading
    pub style: HeadingStyle,
    /// The heading marker (# characters or underline)
    pub marker: String,
    /// Column where the marker starts (0-based)
    pub marker_column: usize,
    /// Column where heading text starts
    pub content_column: usize,
    /// The heading text (without markers and without custom ID syntax)
    pub text: String,
    /// Custom header ID if present (e.g., from {#custom-id} syntax)
    pub custom_id: Option<String>,
    /// Original heading text including custom ID syntax
    pub raw_text: String,
    /// Whether it has a closing sequence (for ATX)
    pub has_closing_sequence: bool,
    /// The closing sequence if present
    pub closing_sequence: String,
    /// Whether this is a valid CommonMark heading (ATX headings require space after #)
    /// False for malformed headings like `#NoSpace` that MD018 should flag
    pub is_valid: bool,
}

/// A heading recognized in the rendered Markdown document.
///
/// Unlike [`ValidHeading`], this view includes headings inside blockquotes and
/// malformed ATX headings retained for diagnostics such as MD018. Consumers
/// can select the semantics they need without reparsing source lines.
#[derive(Debug, Clone, Copy)]
pub struct ParsedHeading<'a> {
    /// The 1-indexed line number in the document.
    pub line_num: usize,
    /// Parsed heading metadata.
    pub heading: &'a HeadingInfo,
    /// Full source-line metadata.
    pub line_info: &'a LineInfo,
    /// Blockquote nesting depth, or zero for a top-level heading.
    pub blockquote_depth: usize,
}

impl ParsedHeading<'_> {
    /// Whether this heading is inside a blockquote.
    #[inline]
    pub fn is_blockquote(&self) -> bool {
        self.blockquote_depth > 0
    }

    /// Whether this is a Setext-style heading.
    #[inline]
    pub fn is_setext(&self) -> bool {
        matches!(self.heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2)
    }

    /// Byte offsets `(start, end)` of the heading text within its source line.
    ///
    /// Markers, closing ATX sequences, and custom-ID syntax are excluded. The
    /// range is line-relative so callers can convert it to their own position
    /// representation without rescanning Markdown syntax.
    #[must_use]
    pub fn text_byte_range(&self, source: &str) -> (usize, usize) {
        let line = self.line_info.content(source);
        let content_start = self.heading.content_column.min(line.len());
        let relative_start = line[content_start..].find(&self.heading.text).unwrap_or(0);
        let start = content_start + relative_start;
        (start, (start + self.heading.text.len()).min(line.len()))
    }
}

/// Iterator over all headings recognized in the rendered document.
pub struct ParsedHeadingsIter<'a> {
    lines: &'a [LineInfo],
    blockquote_headings: &'a [Option<Box<HeadingInfo>>],
    current_index: usize,
}

impl<'a> ParsedHeadingsIter<'a> {
    pub(super) fn new(lines: &'a [LineInfo], blockquote_headings: &'a [Option<Box<HeadingInfo>>]) -> Self {
        debug_assert_eq!(lines.len(), blockquote_headings.len());
        Self {
            lines,
            blockquote_headings,
            current_index: 0,
        }
    }
}

impl<'a> Iterator for ParsedHeadingsIter<'a> {
    type Item = ParsedHeading<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_index < self.lines.len() {
            let idx = self.current_index;
            self.current_index += 1;

            let line_info = &self.lines[idx];
            let (heading, blockquote_depth) = if let Some(heading) = line_info.heading.as_deref() {
                (heading, 0)
            } else if let Some(heading) = self.blockquote_headings[idx].as_deref() {
                (heading, line_info.blockquote.as_ref().map_or(0, |bq| bq.nesting_level))
            } else {
                continue;
            };
            return Some(ParsedHeading {
                line_num: idx + 1,
                heading,
                line_info,
                blockquote_depth,
            });
        }
        None
    }
}

/// A valid heading from a filtered iteration
///
/// Only includes headings that are CommonMark-compliant (have space after #).
/// Hashtag-like patterns (`#tag`, `#123`) are excluded.
#[derive(Debug, Clone)]
pub struct ValidHeading<'a> {
    /// The 1-indexed line number in the document
    pub line_num: usize,
    /// Reference to the heading information
    pub heading: &'a HeadingInfo,
    /// Reference to the full line info (for rules that need additional context)
    pub line_info: &'a LineInfo,
}

/// Iterator over valid CommonMark headings in a document
///
/// Filters out malformed headings like `#NoSpace` that should be flagged by MD018
/// but should not be processed by other heading rules.
pub struct ValidHeadingsIter<'a> {
    lines: &'a [LineInfo],
    current_index: usize,
}

impl<'a> ValidHeadingsIter<'a> {
    pub(super) fn new(lines: &'a [LineInfo]) -> Self {
        Self {
            lines,
            current_index: 0,
        }
    }
}

impl<'a> Iterator for ValidHeadingsIter<'a> {
    type Item = ValidHeading<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_index < self.lines.len() {
            let idx = self.current_index;
            self.current_index += 1;

            let line_info = &self.lines[idx];
            if let Some(heading) = line_info.heading.as_deref()
                && heading.is_valid
            {
                return Some(ValidHeading {
                    line_num: idx + 1, // Convert 0-indexed to 1-indexed
                    heading,
                    line_info,
                });
            }
        }
        None
    }
}

/// Information about a blockquote line
#[derive(Debug, Clone)]
pub struct BlockquoteInfo {
    /// Nesting level (1 for >, 2 for >>, etc.)
    pub nesting_level: usize,
    /// Column where the first > starts (0-based)
    pub marker_column: usize,
    /// The blockquote prefix (e.g., "> ", ">> ", etc.)
    pub prefix: String,
    /// Content after the blockquote marker(s)
    pub content: String,
    /// Whether the line has multiple spaces after the marker
    pub has_multiple_spaces_after_marker: bool,
}

/// Information about a list block
#[derive(Debug, Clone)]
pub struct ListBlock {
    /// Line number where the list starts (1-indexed)
    pub start_line: usize,
    /// Line number where the list ends (1-indexed)
    pub end_line: usize,
    /// Whether it's ordered or unordered
    pub is_ordered: bool,
    /// The consistent marker for unordered lists (if any)
    pub marker: Option<String>,
    /// Blockquote prefix for this list (empty if not in blockquote)
    pub blockquote_prefix: String,
    /// Lines that are list items within this block
    pub item_lines: Vec<usize>,
    /// Nesting level (0 for top-level lists)
    pub nesting_level: usize,
    /// Maximum marker width seen in this block (e.g., 3 for "1. ", 4 for "10. ")
    pub max_marker_width: usize,
}

/// A borrowed list item recognized in the parsed document.
///
/// This view gives rules stable access to list syntax and its source line
/// without exposing how list items are stored inside [`LineInfo`]. Columns are
/// the parser's existing source columns; rules that need visual columns must
/// continue to apply their established tab and container policy.
#[derive(Debug, Clone, Copy)]
pub struct ParsedListItem<'a> {
    line_num: usize,
    item: &'a ListItemInfo,
    line_info: &'a LineInfo,
}

impl<'a> ParsedListItem<'a> {
    pub(super) fn new(line_num: usize, item: &'a ListItemInfo, line_info: &'a LineInfo) -> Self {
        Self {
            line_num,
            item,
            line_info,
        }
    }

    /// The 1-indexed source line containing this item.
    #[inline]
    pub fn line_num(self) -> usize {
        self.line_num
    }

    /// Full metadata for the source line containing this item.
    #[inline]
    pub fn line_info(self) -> &'a LineInfo {
        self.line_info
    }

    /// The marker as parsed (`*`, `-`, `+`, or an ordered-list marker).
    #[inline]
    pub fn marker(self) -> &'a str {
        &self.item.marker
    }

    /// The first character of the marker, if present.
    #[inline]
    pub fn marker_char(self) -> Option<char> {
        self.item.marker.chars().next()
    }

    /// Whether this is an ordered-list item.
    #[inline]
    pub fn is_ordered(self) -> bool {
        self.item.is_ordered
    }

    /// The parsed ordered-list number, when applicable.
    #[inline]
    pub fn number(self) -> Option<usize> {
        self.item.number
    }

    /// Source column where the marker starts.
    #[inline]
    pub fn marker_column(self) -> usize {
        self.item.marker_column
    }

    /// Source column where content after the marker starts.
    #[inline]
    pub fn content_column(self) -> usize {
        self.item.content_column
    }

    /// Absolute byte offset where the marker starts.
    #[inline]
    pub fn marker_byte_offset(self) -> usize {
        self.line_info.byte_offset + self.item.marker_column
    }

    /// Blockquote nesting depth, or zero outside a blockquote.
    #[inline]
    pub fn blockquote_depth(self) -> usize {
        self.line_info.blockquote.as_ref().map_or(0, |bq| bq.nesting_level)
    }

    /// Length in bytes of the normalized blockquote prefix, or zero outside a blockquote.
    #[inline]
    pub fn blockquote_prefix_len(self) -> usize {
        self.line_info.blockquote.as_ref().map_or(0, |bq| bq.prefix.len())
    }
}

/// A borrowed parsed list block and its items.
#[derive(Debug, Clone, Copy)]
pub struct ParsedListBlock<'a> {
    block: &'a ListBlock,
    lines: &'a [LineInfo],
}

impl<'a> ParsedListBlock<'a> {
    pub(super) fn new(block: &'a ListBlock, lines: &'a [LineInfo]) -> Self {
        Self { block, lines }
    }

    /// First source line in the block (1-indexed).
    #[inline]
    pub fn start_line(self) -> usize {
        self.block.start_line
    }

    /// Last source line in the block (1-indexed, inclusive).
    #[inline]
    pub fn end_line(self) -> usize {
        self.block.end_line
    }

    /// Whether the block's primary list type is ordered.
    #[inline]
    pub fn is_ordered(self) -> bool {
        self.block.is_ordered
    }

    /// Consistent unordered marker for the block, when one exists.
    #[inline]
    pub fn marker(self) -> Option<&'a str> {
        self.block.marker.as_deref()
    }

    /// Blockquote prefix shared by the block.
    #[inline]
    pub fn blockquote_prefix(self) -> &'a str {
        &self.block.blockquote_prefix
    }

    /// Parser-computed nesting level for the block.
    #[inline]
    pub fn nesting_level(self) -> usize {
        self.block.nesting_level
    }

    /// Maximum marker width in the block.
    #[inline]
    pub fn max_marker_width(self) -> usize {
        self.block.max_marker_width
    }

    /// Iterate over parsed items belonging to this block, in source order.
    pub fn items(self) -> ParsedListBlockItemsIter<'a> {
        ParsedListBlockItemsIter {
            item_lines: &self.block.item_lines,
            lines: self.lines,
            current_index: 0,
        }
    }
}

/// Borrowed collection of parsed list blocks.
#[derive(Debug, Clone, Copy)]
pub struct ParsedListBlocks<'a> {
    blocks: &'a [ListBlock],
    lines: &'a [LineInfo],
}

impl<'a> ParsedListBlocks<'a> {
    pub(super) fn new(blocks: &'a [ListBlock], lines: &'a [LineInfo]) -> Self {
        Self { blocks, lines }
    }

    #[inline]
    pub fn is_empty(self) -> bool {
        self.blocks.is_empty()
    }

    #[inline]
    pub fn len(self) -> usize {
        self.blocks.len()
    }

    pub fn get(self, index: usize) -> Option<ParsedListBlock<'a>> {
        self.blocks
            .get(index)
            .map(|block| ParsedListBlock::new(block, self.lines))
    }

    pub fn iter(self) -> ParsedListBlocksIter<'a> {
        ParsedListBlocksIter {
            blocks: self.blocks.iter(),
            lines: self.lines,
        }
    }
}

impl<'a> IntoIterator for ParsedListBlocks<'a> {
    type Item = ParsedListBlock<'a>;
    type IntoIter = ParsedListBlocksIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct ParsedListBlocksIter<'a> {
    blocks: std::slice::Iter<'a, ListBlock>,
    lines: &'a [LineInfo],
}

impl<'a> Iterator for ParsedListBlocksIter<'a> {
    type Item = ParsedListBlock<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.blocks.next().map(|block| ParsedListBlock::new(block, self.lines))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.blocks.size_hint()
    }
}

impl ExactSizeIterator for ParsedListBlocksIter<'_> {}

pub struct ParsedListBlockItemsIter<'a> {
    item_lines: &'a [usize],
    lines: &'a [LineInfo],
    current_index: usize,
}

impl<'a> Iterator for ParsedListBlockItemsIter<'a> {
    type Item = ParsedListItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(&line_num) = self.item_lines.get(self.current_index) {
            self.current_index += 1;
            let Some(line_index) = line_num.checked_sub(1) else {
                continue;
            };
            let Some(line_info) = self.lines.get(line_index) else {
                continue;
            };
            if let Some(item) = line_info.list_item.as_deref() {
                return Some(ParsedListItem::new(line_num, item, line_info));
            }
        }
        None
    }
}

pub struct ParsedListItemsIter<'a> {
    lines: &'a [LineInfo],
    current_index: usize,
}

impl<'a> ParsedListItemsIter<'a> {
    pub(super) fn new(lines: &'a [LineInfo]) -> Self {
        Self {
            lines,
            current_index: 0,
        }
    }
}

impl<'a> Iterator for ParsedListItemsIter<'a> {
    type Item = ParsedListItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_index < self.lines.len() {
            let idx = self.current_index;
            self.current_index += 1;
            let line_info = &self.lines[idx];
            if let Some(item) = line_info.list_item.as_deref() {
                return Some(ParsedListItem::new(idx + 1, item, line_info));
            }
        }
        None
    }
}

/// Cached CommonMark membership for one ordered list.
#[derive(Debug, Clone)]
pub(super) struct CommonMarkOrderedListInfo {
    pub(super) start_value: u64,
    pub(super) item_lines: Vec<usize>,
}

/// A borrowed ordered list as grouped by the CommonMark parser.
///
/// This grouping is independent of visual list blocks: nested ordered lists
/// have their own membership and start value even when their source lines are
/// interleaved with the parent list.
#[derive(Debug, Clone, Copy)]
pub struct CommonMarkOrderedList<'a> {
    list: &'a CommonMarkOrderedListInfo,
    lines: &'a [LineInfo],
}

impl<'a> CommonMarkOrderedList<'a> {
    pub(super) fn new(list: &'a CommonMarkOrderedListInfo, lines: &'a [LineInfo]) -> Self {
        Self { list, lines }
    }

    /// The number on the first item, as interpreted by CommonMark.
    #[inline]
    pub fn start_value(self) -> u64 {
        self.list.start_value
    }

    /// Iterate over this list's ordered items in source order.
    pub fn items(self) -> CommonMarkOrderedListItemsIter<'a> {
        CommonMarkOrderedListItemsIter {
            item_lines: &self.list.item_lines,
            lines: self.lines,
            current_index: 0,
        }
    }
}

/// Borrowed collection of CommonMark-grouped ordered lists in source order.
#[derive(Debug, Clone, Copy)]
pub struct CommonMarkOrderedLists<'a> {
    lists: &'a [CommonMarkOrderedListInfo],
    lines: &'a [LineInfo],
}

impl<'a> CommonMarkOrderedLists<'a> {
    pub(super) fn new(lists: &'a [CommonMarkOrderedListInfo], lines: &'a [LineInfo]) -> Self {
        Self { lists, lines }
    }

    /// Whether the document has no CommonMark-grouped ordered lists.
    #[inline]
    pub fn is_empty(self) -> bool {
        self.lists.is_empty()
    }

    /// Number of CommonMark-grouped ordered lists in the document.
    #[inline]
    pub fn len(self) -> usize {
        self.lists.len()
    }

    /// Return a list by source-order index.
    pub fn get(self, index: usize) -> Option<CommonMarkOrderedList<'a>> {
        self.lists
            .get(index)
            .map(|list| CommonMarkOrderedList::new(list, self.lines))
    }

    /// Iterate over ordered lists in the order of their first source item.
    pub fn iter(self) -> CommonMarkOrderedListsIter<'a> {
        CommonMarkOrderedListsIter {
            lists: self.lists.iter(),
            lines: self.lines,
        }
    }
}

impl<'a> IntoIterator for CommonMarkOrderedLists<'a> {
    type Item = CommonMarkOrderedList<'a>;
    type IntoIter = CommonMarkOrderedListsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over CommonMark-grouped ordered lists.
pub struct CommonMarkOrderedListsIter<'a> {
    lists: std::slice::Iter<'a, CommonMarkOrderedListInfo>,
    lines: &'a [LineInfo],
}

impl<'a> Iterator for CommonMarkOrderedListsIter<'a> {
    type Item = CommonMarkOrderedList<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.lists
            .next()
            .map(|list| CommonMarkOrderedList::new(list, self.lines))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.lists.size_hint()
    }
}

impl ExactSizeIterator for CommonMarkOrderedListsIter<'_> {}

/// Iterator over the parsed items in one CommonMark ordered list.
pub struct CommonMarkOrderedListItemsIter<'a> {
    item_lines: &'a [usize],
    lines: &'a [LineInfo],
    current_index: usize,
}

impl<'a> Iterator for CommonMarkOrderedListItemsIter<'a> {
    type Item = ParsedListItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(&line_num) = self.item_lines.get(self.current_index) {
            self.current_index += 1;
            let Some(line_index) = line_num.checked_sub(1) else {
                continue;
            };
            let Some(line_info) = self.lines.get(line_index) else {
                continue;
            };
            let Some(item) = line_info.list_item.as_deref() else {
                continue;
            };
            if item.is_ordered {
                return Some(ParsedListItem::new(line_num, item, line_info));
            }
        }
        None
    }
}

/// Character frequency data for fast content analysis
#[derive(Debug, Clone, Default)]
pub struct CharFrequency {
    /// Count of # characters (headings)
    pub hash_count: usize,
    /// Count of * characters (emphasis, lists, horizontal rules)
    pub asterisk_count: usize,
    /// Count of _ characters (emphasis, horizontal rules)
    pub underscore_count: usize,
    /// Count of - characters (lists, horizontal rules, setext headings)
    pub hyphen_count: usize,
    /// Count of + characters (lists)
    pub plus_count: usize,
    /// Count of > characters (blockquotes)
    pub gt_count: usize,
    /// Count of | characters (tables)
    pub pipe_count: usize,
    /// Count of [ characters (links, images)
    pub bracket_count: usize,
    /// Count of ` characters (code spans, code blocks)
    pub backtick_count: usize,
    /// Count of < characters (HTML tags, autolinks)
    pub lt_count: usize,
    /// Count of ! characters (images)
    pub exclamation_count: usize,
    /// Count of newline characters
    pub newline_count: usize,
}

/// Pre-parsed HTML tag information
#[derive(Debug, Clone)]
pub struct HtmlTag {
    /// Line number (1-indexed)
    pub line: usize,
    /// Start column (0-indexed) in the line
    pub start_col: usize,
    /// End column (0-indexed) in the line
    pub end_col: usize,
    /// Byte offset in document
    pub byte_offset: usize,
    /// End byte offset in document
    pub byte_end: usize,
    /// Tag name (e.g., "div", "img", "br")
    pub tag_name: String,
    /// Whether it's a closing tag (`</tag>`)
    pub is_closing: bool,
    /// Whether it's self-closing (`<tag />`)
    pub is_self_closing: bool,
}

/// Pre-parsed emphasis span information
#[derive(Debug, Clone)]
pub struct EmphasisSpan {
    /// Line number (1-indexed)
    pub line: usize,
    /// Start column (0-indexed) in the line
    pub start_col: usize,
    /// End column (0-indexed) in the line
    pub end_col: usize,
    /// Byte offset in document
    pub byte_offset: usize,
    /// End byte offset in document
    pub byte_end: usize,
    /// Type of emphasis ('*' or '_')
    pub marker: char,
    /// Whether this span is strong emphasis (`**`/`__`) rather than ordinary emphasis (`*`/`_`)
    pub is_strong: bool,
    /// Content inside the emphasis
    pub content: String,
}

/// Pre-parsed bare URL information (not in links)
#[derive(Debug, Clone)]
pub struct BareUrl {
    /// Line number (1-indexed)
    pub line: usize,
    /// Start column (0-indexed) in the line
    pub start_col: usize,
    /// End column (0-indexed) in the line
    pub end_col: usize,
    /// Byte offset in document
    pub byte_offset: usize,
    /// End byte offset in document
    pub byte_end: usize,
    /// The URL string
    pub url: String,
}

/// A lazy continuation line detected by pulldown-cmark.
///
/// Lazy continuation occurs when text continues a list item paragraph but with less
/// indentation than expected.
#[derive(Debug, Clone)]
pub struct LazyContLine {
    /// 1-indexed line number
    pub line_num: usize,
    /// Expected indentation
    pub expected_indent: usize,
    /// Current indentation
    pub current_indent: usize,
    /// Blockquote nesting level
    pub blockquote_level: usize,
}

/// Check if a line is a horizontal rule (---, ***, ___) per CommonMark spec.
/// CommonMark rules for thematic breaks (horizontal rules):
/// - May have 0-3 spaces of leading indentation (but NOT tabs)
/// - Must have 3+ of the same character (-, *, or _)
/// - May have spaces between characters
/// - No other characters allowed
pub fn is_horizontal_rule_line(line: &str) -> bool {
    // CommonMark: HRs can have 0-3 spaces of leading indentation, not tabs
    let leading_spaces = line.len() - line.trim_start_matches(' ').len();
    if leading_spaces > 3 || line.starts_with('\t') {
        return false;
    }

    is_horizontal_rule_content(line.trim())
}

/// Check if trimmed content matches horizontal rule pattern.
/// Use `is_horizontal_rule_line` for full CommonMark compliance including indentation check.
pub fn is_horizontal_rule_content(trimmed: &str) -> bool {
    if trimmed.len() < 3 {
        return false;
    }

    let mut chars = trimmed.chars();
    let Some(first_char @ ('-' | '*' | '_')) = chars.next() else {
        return false;
    };

    // Count occurrences of the rule character, rejecting non-whitespace interlopers
    let mut count = 1; // Already matched the first character
    for ch in chars {
        if ch == first_char {
            count += 1;
        } else if ch != ' ' && ch != '\t' {
            return false;
        }
    }
    count >= 3
}
