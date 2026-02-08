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
    /// Whether this line is inside a Quarto div block (::: ... :::)
    pub in_quarto_div: bool,
    /// Whether this line is a Quarto/Pandoc div marker (opening ::: {.class} or closing :::)
    /// Analogous to `is_horizontal_rule` — marks structural delimiters that are not paragraph text
    pub is_div_marker: bool,
    /// Whether this line contains or is inside a JSX expression (MDX only)
    pub in_jsx_expression: bool,
    /// Whether this line is inside an MDX comment {/* ... */} (MDX only)
    pub in_mdx_comment: bool,
    /// Whether this line is inside a JSX component (MDX only)
    pub in_jsx_component: bool,
    /// Whether this line is inside a JSX fragment (MDX only)
    pub in_jsx_fragment: bool,
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
    /// Start column (0-indexed) in the line
    pub start_col: usize,
    /// End column (0-indexed) in the line
    pub end_col: usize,
    /// Byte offset in document
    pub byte_offset: usize,
    /// End byte offset in document
    pub byte_end: usize,
    /// Link text
    pub text: Cow<'a, str>,
    /// Link URL or reference
    pub url: Cow<'a, str>,
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
    /// End byte offset in document
    pub byte_end: usize,
}

/// Parsed image information
#[derive(Debug, Clone)]
pub struct ParsedImage<'a> {
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
    /// Alt text
    pub alt_text: Cow<'a, str>,
    /// Image URL or reference
    pub url: Cow<'a, str>,
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
    /// The indentation before the blockquote marker
    pub indent: String,
    /// Column where the first > starts (0-based)
    pub marker_column: usize,
    /// The blockquote prefix (e.g., "> ", ">> ", etc.)
    pub prefix: String,
    /// Content after the blockquote marker(s)
    pub content: String,
    /// Whether the line has no space after the marker
    pub has_no_space_after_marker: bool,
    /// Whether the line has multiple spaces after the marker
    pub has_multiple_spaces_after_marker: bool,
    /// Whether this is an empty blockquote line needing MD028 fix
    pub needs_md028_fix: bool,
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
    /// Raw tag content
    pub raw_content: String,
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
    /// Number of markers (1 for italic, 2 for bold, 3+ for bold+italic)
    pub marker_count: usize,
    /// Content inside the emphasis
    pub content: String,
}

/// Pre-parsed table row information
#[derive(Debug, Clone)]
pub struct TableRow {
    /// Line number (1-indexed)
    pub line: usize,
    /// Whether this is a separator row (contains only |, -, :, and spaces)
    pub is_separator: bool,
    /// Number of columns (pipe-separated cells)
    pub column_count: usize,
    /// Alignment info from separator row
    pub column_alignments: Vec<String>, // "left", "center", "right", "none"
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
    /// Type of URL ("http", "https", "ftp", "email")
    pub url_type: String,
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
    let first_char = match chars.next() {
        Some(c @ ('-' | '*' | '_')) => c,
        _ => return false,
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

/// Backwards-compatible alias for `is_horizontal_rule_content`
pub fn is_horizontal_rule(trimmed: &str) -> bool {
    is_horizontal_rule_content(trimmed)
}
