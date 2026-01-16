use crate::config::MarkdownFlavor;
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::utils::code_block_utils::{CodeBlockContext, CodeBlockUtils};
use crate::utils::element_cache::ElementCache;
use crate::utils::regex_cache::URL_SIMPLE_REGEX;
use pulldown_cmark::{BrokenLink, Event, LinkType, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;

/// Macro for profiling sections - only active in non-WASM builds
#[cfg(not(target_arch = "wasm32"))]
macro_rules! profile_section {
    ($name:expr, $profile:expr, $code:expr) => {{
        let start = std::time::Instant::now();
        let result = $code;
        if $profile {
            eprintln!("[PROFILE] {}: {:?}", $name, start.elapsed());
        }
        result
    }};
}

#[cfg(target_arch = "wasm32")]
macro_rules! profile_section {
    ($name:expr, $profile:expr, $code:expr) => {{ $code }};
}

// Comprehensive link pattern that captures both inline and reference links
// Use (?s) flag to make . match newlines
static LINK_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?sx)
        \[((?:[^\[\]\\]|\\.)*)\]          # Link text in group 1 (optimized - no nested brackets to prevent catastrophic backtracking)
        (?:
            \((?:<([^<>\n]*)>|([^)"']*))(?:\s+(?:"([^"]*)"|'([^']*)'))?\)  # URL in group 2 (angle) or 3 (bare), title in 4/5
            |
            \[([^\]]*)\]      # Reference ID in group 6
        )"#
    ).unwrap()
});

// Image pattern (similar to links but with ! prefix)
// Use (?s) flag to make . match newlines
static IMAGE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?sx)
        !\[((?:[^\[\]\\]|\\.)*)\]         # Alt text in group 1 (optimized - no nested brackets to prevent catastrophic backtracking)
        (?:
            \((?:<([^<>\n]*)>|([^)"']*))(?:\s+(?:"([^"]*)"|'([^']*)'))?\)  # URL in group 2 (angle) or 3 (bare), title in 4/5
            |
            \[([^\]]*)\]      # Reference ID in group 6
        )"#
    ).unwrap()
});

// Reference definition pattern
static REF_DEF_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^[ ]{0,3}\[([^\]]+)\]:\s*([^\s]+)(?:\s+(?:"([^"]*)"|'([^']*)'))?$"#).unwrap());

// Pattern for bare URLs - uses centralized URL pattern from regex_cache

// Pattern for email addresses
static BARE_EMAIL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap());

// Pattern for blockquote prefix in parse_list_blocks
static BLOCKQUOTE_PREFIX_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*>+\s*)").unwrap());

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
    pub list_item: Option<ListItemInfo>,
    /// Heading information if this line is a heading
    pub heading: Option<HeadingInfo>,
    /// Blockquote information if this line is a blockquote
    pub blockquote: Option<BlockquoteInfo>,
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
    /// Whether this line contains or is inside a JSX expression (MDX only)
    pub in_jsx_expression: bool,
    /// Whether this line is inside an MDX comment {/* ... */} (MDX only)
    pub in_mdx_comment: bool,
}

impl LineInfo {
    /// Get the line content as a string slice from the source document
    pub fn content<'a>(&self, source: &'a str) -> &'a str {
        &source[self.byte_offset..self.byte_offset + self.byte_len]
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
    /// Whether this is a reference link [text][ref] vs inline [text](url)
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

/// Reference definition [ref]: url "title"
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
    fn new(lines: &'a [LineInfo]) -> Self {
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
            if let Some(heading) = &line_info.heading
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

use std::sync::{Arc, OnceLock};

/// Map from line byte offset to list item data: (is_ordered, marker, marker_column, content_column, number)
type ListItemMap = std::collections::HashMap<usize, (bool, String, usize, usize, Option<usize>)>;

/// Type alias for byte ranges used in JSX expression and MDX comment detection
type ByteRanges = Vec<(usize, usize)>;

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

pub struct LintContext<'a> {
    pub content: &'a str,
    pub line_offsets: Vec<usize>,
    pub code_blocks: Vec<(usize, usize)>, // Cached code block ranges (not including inline code spans)
    pub lines: Vec<LineInfo>,             // Pre-computed line information
    pub links: Vec<ParsedLink<'a>>,       // Pre-parsed links
    pub images: Vec<ParsedImage<'a>>,     // Pre-parsed images
    pub broken_links: Vec<BrokenLinkInfo>, // Broken/undefined references
    pub footnote_refs: Vec<FootnoteRef>,  // Pre-parsed footnote references
    pub reference_defs: Vec<ReferenceDef>, // Reference definitions
    reference_defs_map: HashMap<String, usize>, // O(1) lookup by lowercase ID -> index in reference_defs
    code_spans_cache: OnceLock<Arc<Vec<CodeSpan>>>, // Lazy-loaded inline code spans
    math_spans_cache: OnceLock<Arc<Vec<MathSpan>>>, // Lazy-loaded math spans ($...$ and $$...$$)
    pub list_blocks: Vec<ListBlock>,      // Pre-parsed list blocks
    pub char_frequency: CharFrequency,    // Character frequency analysis
    html_tags_cache: OnceLock<Arc<Vec<HtmlTag>>>, // Lazy-loaded HTML tags
    emphasis_spans_cache: OnceLock<Arc<Vec<EmphasisSpan>>>, // Lazy-loaded emphasis spans
    table_rows_cache: OnceLock<Arc<Vec<TableRow>>>, // Lazy-loaded table rows
    bare_urls_cache: OnceLock<Arc<Vec<BareUrl>>>, // Lazy-loaded bare URLs
    has_mixed_list_nesting_cache: OnceLock<bool>, // Cached result for mixed ordered/unordered list nesting detection
    html_comment_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed HTML comment ranges
    pub table_blocks: Vec<crate::utils::table_utils::TableBlock>, // Pre-computed table blocks
    pub line_index: crate::utils::range_utils::LineIndex<'a>, // Pre-computed line index for byte position calculations
    jinja_ranges: Vec<(usize, usize)>,    // Pre-computed Jinja template ranges ({{ }}, {% %})
    pub flavor: MarkdownFlavor,           // Markdown flavor being used
    pub source_file: Option<PathBuf>,     // Source file path (for rules that need file context)
    jsx_expression_ranges: Vec<(usize, usize)>, // Pre-computed JSX expression ranges (MDX: {expression})
    mdx_comment_ranges: Vec<(usize, usize)>, // Pre-computed MDX comment ranges ({/* ... */})
}

/// Detailed blockquote parse result with all components
struct BlockquoteComponents<'a> {
    indent: &'a str,
    markers: &'a str,
    spaces_after: &'a str,
    content: &'a str,
}

/// Parse blockquote prefix with detailed components using manual parsing
#[inline]
fn parse_blockquote_detailed(line: &str) -> Option<BlockquoteComponents<'_>> {
    let bytes = line.as_bytes();
    let mut pos = 0;

    // Parse leading whitespace (indent)
    while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
        pos += 1;
    }
    let indent_end = pos;

    // Must have at least one '>' marker
    if pos >= bytes.len() || bytes[pos] != b'>' {
        return None;
    }

    // Parse '>' markers
    while pos < bytes.len() && bytes[pos] == b'>' {
        pos += 1;
    }
    let markers_end = pos;

    // Parse spaces after markers
    while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
        pos += 1;
    }
    let spaces_end = pos;

    Some(BlockquoteComponents {
        indent: &line[0..indent_end],
        markers: &line[indent_end..markers_end],
        spaces_after: &line[markers_end..spaces_end],
        content: &line[spaces_end..],
    })
}

impl<'a> LintContext<'a> {
    pub fn new(content: &'a str, flavor: MarkdownFlavor, source_file: Option<PathBuf>) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let profile = std::env::var("RUMDL_PROFILE_QUADRATIC").is_ok();
        #[cfg(target_arch = "wasm32")]
        let profile = false;

        let line_offsets = profile_section!("Line offsets", profile, {
            let mut offsets = vec![0];
            for (i, c) in content.char_indices() {
                if c == '\n' {
                    offsets.push(i + 1);
                }
            }
            offsets
        });

        // Detect code blocks once and cache them
        let code_blocks = profile_section!("Code blocks", profile, CodeBlockUtils::detect_code_blocks(content));

        // Pre-compute HTML comment ranges ONCE for all operations
        let html_comment_ranges = profile_section!(
            "HTML comment ranges",
            profile,
            crate::utils::skip_context::compute_html_comment_ranges(content)
        );

        // Pre-compute autodoc block ranges for MkDocs flavor (avoids O(n²) scaling)
        let autodoc_ranges = profile_section!("Autodoc block ranges", profile, {
            if flavor == MarkdownFlavor::MkDocs {
                crate::utils::mkdocstrings_refs::detect_autodoc_block_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Quarto div block ranges for Quarto flavor
        let quarto_div_ranges = profile_section!("Quarto div ranges", profile, {
            if flavor == MarkdownFlavor::Quarto {
                crate::utils::quarto_divs::detect_div_block_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute line information AND emphasis spans (without headings/blockquotes yet)
        // Emphasis spans are captured during the same pulldown-cmark parse as list detection
        let (mut lines, emphasis_spans) = profile_section!(
            "Basic line info",
            profile,
            Self::compute_basic_line_info(
                content,
                &line_offsets,
                &code_blocks,
                flavor,
                &html_comment_ranges,
                &autodoc_ranges,
                &quarto_div_ranges,
            )
        );

        // Detect HTML blocks BEFORE heading detection
        profile_section!("HTML blocks", profile, Self::detect_html_blocks(content, &mut lines));

        // Detect ESM import/export blocks in MDX files BEFORE heading detection
        profile_section!(
            "ESM blocks",
            profile,
            Self::detect_esm_blocks(content, &mut lines, flavor)
        );

        // Detect JSX expressions and MDX comments in MDX files
        let (jsx_expression_ranges, mdx_comment_ranges) = profile_section!(
            "JSX/MDX detection",
            profile,
            Self::detect_jsx_and_mdx_comments(content, &mut lines, flavor, &code_blocks)
        );

        // Collect link byte ranges early for heading detection (to skip lines inside link syntax)
        let link_byte_ranges = profile_section!("Link byte ranges", profile, Self::collect_link_byte_ranges(content));

        // Now detect headings and blockquotes
        profile_section!(
            "Headings & blockquotes",
            profile,
            Self::detect_headings_and_blockquotes(content, &mut lines, flavor, &html_comment_ranges, &link_byte_ranges)
        );

        // Parse code spans early so we can exclude them from link/image parsing
        let code_spans = profile_section!("Code spans", profile, Self::parse_code_spans(content, &lines));

        // Mark lines that are continuations of multi-line code spans
        // This is needed for parse_list_blocks to correctly handle list items with multi-line code spans
        for span in &code_spans {
            if span.end_line > span.line {
                // Mark lines after the first line as continuations
                for line_num in (span.line + 1)..=span.end_line {
                    if let Some(line_info) = lines.get_mut(line_num - 1) {
                        line_info.in_code_span_continuation = true;
                    }
                }
            }
        }

        // Parse links, images, references, and list blocks
        let (links, broken_links, footnote_refs) = profile_section!(
            "Links",
            profile,
            Self::parse_links(content, &lines, &code_blocks, &code_spans, flavor, &html_comment_ranges)
        );

        let images = profile_section!(
            "Images",
            profile,
            Self::parse_images(content, &lines, &code_blocks, &code_spans, &html_comment_ranges)
        );

        let reference_defs = profile_section!("Reference defs", profile, Self::parse_reference_defs(content, &lines));

        // Build O(1) lookup map for reference definitions by lowercase ID
        let reference_defs_map: HashMap<String, usize> = reference_defs
            .iter()
            .enumerate()
            .map(|(idx, def)| (def.id.to_lowercase(), idx))
            .collect();

        let list_blocks = profile_section!("List blocks", profile, Self::parse_list_blocks(content, &lines));

        // Compute character frequency for fast content analysis
        let char_frequency = profile_section!("Char frequency", profile, Self::compute_char_frequency(content));

        // Pre-compute table blocks for rules that need them (MD013, MD055, MD056, MD058, MD060)
        let table_blocks = profile_section!(
            "Table blocks",
            profile,
            crate::utils::table_utils::TableUtils::find_table_blocks_with_code_info(
                content,
                &code_blocks,
                &code_spans,
                &html_comment_ranges,
            )
        );

        // Pre-compute LineIndex once for all rules (eliminates 46x content cloning)
        let line_index = profile_section!(
            "Line index",
            profile,
            crate::utils::range_utils::LineIndex::new(content)
        );

        // Pre-compute Jinja template ranges once for all rules (eliminates O(n×m) in MD011)
        let jinja_ranges = profile_section!(
            "Jinja ranges",
            profile,
            crate::utils::jinja_utils::find_jinja_ranges(content)
        );

        Self {
            content,
            line_offsets,
            code_blocks,
            lines,
            links,
            images,
            broken_links,
            footnote_refs,
            reference_defs,
            reference_defs_map,
            code_spans_cache: OnceLock::from(Arc::new(code_spans)),
            math_spans_cache: OnceLock::new(), // Lazy-loaded on first access
            list_blocks,
            char_frequency,
            html_tags_cache: OnceLock::new(),
            emphasis_spans_cache: OnceLock::from(Arc::new(emphasis_spans)),
            table_rows_cache: OnceLock::new(),
            bare_urls_cache: OnceLock::new(),
            has_mixed_list_nesting_cache: OnceLock::new(),
            html_comment_ranges,
            table_blocks,
            line_index,
            jinja_ranges,
            flavor,
            source_file,
            jsx_expression_ranges,
            mdx_comment_ranges,
        }
    }

    /// Get code spans - computed lazily on first access
    pub fn code_spans(&self) -> Arc<Vec<CodeSpan>> {
        Arc::clone(
            self.code_spans_cache
                .get_or_init(|| Arc::new(Self::parse_code_spans(self.content, &self.lines))),
        )
    }

    /// Get math spans - computed lazily on first access
    pub fn math_spans(&self) -> Arc<Vec<MathSpan>> {
        Arc::clone(
            self.math_spans_cache
                .get_or_init(|| Arc::new(Self::parse_math_spans(self.content, &self.lines))),
        )
    }

    /// Check if a byte position is within a math span (inline $...$ or display $$...$$)
    pub fn is_in_math_span(&self, byte_pos: usize) -> bool {
        let math_spans = self.math_spans();
        math_spans
            .iter()
            .any(|span| byte_pos >= span.byte_offset && byte_pos < span.byte_end)
    }

    /// Get HTML comment ranges - pre-computed during LintContext construction
    pub fn html_comment_ranges(&self) -> &[crate::utils::skip_context::ByteRange] {
        &self.html_comment_ranges
    }

    /// Get HTML tags - computed lazily on first access
    pub fn html_tags(&self) -> Arc<Vec<HtmlTag>> {
        Arc::clone(self.html_tags_cache.get_or_init(|| {
            Arc::new(Self::parse_html_tags(
                self.content,
                &self.lines,
                &self.code_blocks,
                self.flavor,
            ))
        }))
    }

    /// Get emphasis spans - pre-computed during construction
    pub fn emphasis_spans(&self) -> Arc<Vec<EmphasisSpan>> {
        Arc::clone(
            self.emphasis_spans_cache
                .get()
                .expect("emphasis_spans_cache initialized during construction"),
        )
    }

    /// Get table rows - computed lazily on first access
    pub fn table_rows(&self) -> Arc<Vec<TableRow>> {
        Arc::clone(
            self.table_rows_cache
                .get_or_init(|| Arc::new(Self::parse_table_rows(self.content, &self.lines))),
        )
    }

    /// Get bare URLs - computed lazily on first access
    pub fn bare_urls(&self) -> Arc<Vec<BareUrl>> {
        Arc::clone(
            self.bare_urls_cache
                .get_or_init(|| Arc::new(Self::parse_bare_urls(self.content, &self.lines, &self.code_blocks))),
        )
    }

    /// Check if document has mixed ordered/unordered list nesting.
    /// Result is cached after first computation (document-level invariant).
    /// This is used by MD007 for smart style auto-detection.
    pub fn has_mixed_list_nesting(&self) -> bool {
        *self
            .has_mixed_list_nesting_cache
            .get_or_init(|| self.compute_mixed_list_nesting())
    }

    /// Internal computation for mixed list nesting (only called once per LintContext).
    fn compute_mixed_list_nesting(&self) -> bool {
        // Track parent list items by their marker position and type
        // Using marker_column instead of indent because it works correctly
        // for blockquoted content where indent doesn't account for the prefix
        // Stack stores: (marker_column, is_ordered)
        let mut stack: Vec<(usize, bool)> = Vec::new();
        let mut last_was_blank = false;

        for line_info in &self.lines {
            // Skip non-content lines (code blocks, frontmatter, HTML comments, etc.)
            if line_info.in_code_block
                || line_info.in_front_matter
                || line_info.in_mkdocstrings
                || line_info.in_html_comment
                || line_info.in_esm_block
            {
                continue;
            }

            // OPTIMIZATION: Use pre-computed is_blank instead of content().trim()
            if line_info.is_blank {
                last_was_blank = true;
                continue;
            }

            if let Some(list_item) = &line_info.list_item {
                // Normalize column 1 to column 0 (consistent with MD007 check function)
                let current_pos = if list_item.marker_column == 1 {
                    0
                } else {
                    list_item.marker_column
                };

                // If there was a blank line and this item is at root level, reset stack
                if last_was_blank && current_pos == 0 {
                    stack.clear();
                }
                last_was_blank = false;

                // Pop items at same or greater position (they're siblings or deeper, not parents)
                while let Some(&(pos, _)) = stack.last() {
                    if pos >= current_pos {
                        stack.pop();
                    } else {
                        break;
                    }
                }

                // Check if immediate parent has different type - this is mixed nesting
                if let Some(&(_, parent_is_ordered)) = stack.last()
                    && parent_is_ordered != list_item.is_ordered
                {
                    return true; // Found mixed nesting - early exit
                }

                stack.push((current_pos, list_item.is_ordered));
            } else {
                // Non-list line (but not blank) - could be paragraph or other content
                last_was_blank = false;
            }
        }

        false
    }

    /// Map a byte offset to (line, column)
    pub fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        match self.line_offsets.binary_search(&offset) {
            Ok(line) => (line + 1, 1),
            Err(line) => {
                let line_start = self.line_offsets.get(line.wrapping_sub(1)).copied().unwrap_or(0);
                (line, offset - line_start + 1)
            }
        }
    }

    /// Check if a position is within a code block or code span
    pub fn is_in_code_block_or_span(&self, pos: usize) -> bool {
        // Check code blocks first
        if CodeBlockUtils::is_in_code_block_or_span(&self.code_blocks, pos) {
            return true;
        }

        // Check inline code spans (lazy load if needed)
        self.code_spans()
            .iter()
            .any(|span| pos >= span.byte_offset && pos < span.byte_end)
    }

    /// Get line information by line number (1-indexed)
    pub fn line_info(&self, line_num: usize) -> Option<&LineInfo> {
        if line_num > 0 {
            self.lines.get(line_num - 1)
        } else {
            None
        }
    }

    /// Get byte offset for a line number (1-indexed)
    pub fn line_to_byte_offset(&self, line_num: usize) -> Option<usize> {
        self.line_info(line_num).map(|info| info.byte_offset)
    }

    /// Get URL for a reference link/image by its ID (O(1) lookup via HashMap)
    pub fn get_reference_url(&self, ref_id: &str) -> Option<&str> {
        let normalized_id = ref_id.to_lowercase();
        self.reference_defs_map
            .get(&normalized_id)
            .map(|&idx| self.reference_defs[idx].url.as_str())
    }

    /// Get a reference definition by its ID (O(1) lookup via HashMap)
    pub fn get_reference_def(&self, ref_id: &str) -> Option<&ReferenceDef> {
        let normalized_id = ref_id.to_lowercase();
        self.reference_defs_map
            .get(&normalized_id)
            .map(|&idx| &self.reference_defs[idx])
    }

    /// Check if a reference definition exists by ID (O(1) lookup via HashMap)
    pub fn has_reference_def(&self, ref_id: &str) -> bool {
        let normalized_id = ref_id.to_lowercase();
        self.reference_defs_map.contains_key(&normalized_id)
    }

    /// Check if a line is part of a list block
    pub fn is_in_list_block(&self, line_num: usize) -> bool {
        self.list_blocks
            .iter()
            .any(|block| line_num >= block.start_line && line_num <= block.end_line)
    }

    /// Get the list block containing a specific line
    pub fn list_block_for_line(&self, line_num: usize) -> Option<&ListBlock> {
        self.list_blocks
            .iter()
            .find(|block| line_num >= block.start_line && line_num <= block.end_line)
    }

    // Compatibility methods for DocumentStructure migration

    /// Check if a line is within a code block
    pub fn is_in_code_block(&self, line_num: usize) -> bool {
        if line_num == 0 || line_num > self.lines.len() {
            return false;
        }
        self.lines[line_num - 1].in_code_block
    }

    /// Check if a line is within front matter
    pub fn is_in_front_matter(&self, line_num: usize) -> bool {
        if line_num == 0 || line_num > self.lines.len() {
            return false;
        }
        self.lines[line_num - 1].in_front_matter
    }

    /// Check if a line is within an HTML block
    pub fn is_in_html_block(&self, line_num: usize) -> bool {
        if line_num == 0 || line_num > self.lines.len() {
            return false;
        }
        self.lines[line_num - 1].in_html_block
    }

    /// Check if a line and column is within a code span
    pub fn is_in_code_span(&self, line_num: usize, col: usize) -> bool {
        if line_num == 0 || line_num > self.lines.len() {
            return false;
        }

        // Use the code spans cache to check
        // Note: col is 1-indexed from caller, but span.start_col and span.end_col are 0-indexed
        // Convert col to 0-indexed for comparison
        let col_0indexed = if col > 0 { col - 1 } else { 0 };
        let code_spans = self.code_spans();
        code_spans.iter().any(|span| {
            // Check if line is within the span's line range
            if line_num < span.line || line_num > span.end_line {
                return false;
            }

            if span.line == span.end_line {
                // Single-line span: check column bounds
                col_0indexed >= span.start_col && col_0indexed < span.end_col
            } else if line_num == span.line {
                // First line of multi-line span: anything after start_col is in span
                col_0indexed >= span.start_col
            } else if line_num == span.end_line {
                // Last line of multi-line span: anything before end_col is in span
                col_0indexed < span.end_col
            } else {
                // Middle line of multi-line span: entire line is in span
                true
            }
        })
    }

    /// Check if a byte offset is within a code span
    #[inline]
    pub fn is_byte_offset_in_code_span(&self, byte_offset: usize) -> bool {
        let code_spans = self.code_spans();
        code_spans
            .iter()
            .any(|span| byte_offset >= span.byte_offset && byte_offset < span.byte_end)
    }

    /// Check if a byte position is within a reference definition
    /// This is much faster than scanning the content with regex for each check (O(1) vs O(n))
    #[inline]
    pub fn is_in_reference_def(&self, byte_pos: usize) -> bool {
        self.reference_defs
            .iter()
            .any(|ref_def| byte_pos >= ref_def.byte_offset && byte_pos < ref_def.byte_end)
    }

    /// Check if a byte position is within an HTML comment
    /// This is much faster than scanning the content with regex for each check (O(k) vs O(n))
    /// where k is the number of HTML comments (typically very small)
    #[inline]
    pub fn is_in_html_comment(&self, byte_pos: usize) -> bool {
        self.html_comment_ranges
            .iter()
            .any(|range| byte_pos >= range.start && byte_pos < range.end)
    }

    /// Check if a byte position is within an HTML tag (including multiline tags)
    /// Uses the pre-parsed html_tags which correctly handles tags spanning multiple lines
    #[inline]
    pub fn is_in_html_tag(&self, byte_pos: usize) -> bool {
        self.html_tags()
            .iter()
            .any(|tag| byte_pos >= tag.byte_offset && byte_pos < tag.byte_end)
    }

    /// Check if a byte position is within a Jinja template ({{ }} or {% %})
    pub fn is_in_jinja_range(&self, byte_pos: usize) -> bool {
        self.jinja_ranges
            .iter()
            .any(|(start, end)| byte_pos >= *start && byte_pos < *end)
    }

    /// Check if a byte position is within a JSX expression (MDX: {expression})
    #[inline]
    pub fn is_in_jsx_expression(&self, byte_pos: usize) -> bool {
        self.jsx_expression_ranges
            .iter()
            .any(|(start, end)| byte_pos >= *start && byte_pos < *end)
    }

    /// Check if a byte position is within an MDX comment ({/* ... */})
    #[inline]
    pub fn is_in_mdx_comment(&self, byte_pos: usize) -> bool {
        self.mdx_comment_ranges
            .iter()
            .any(|(start, end)| byte_pos >= *start && byte_pos < *end)
    }

    /// Get all JSX expression byte ranges
    pub fn jsx_expression_ranges(&self) -> &[(usize, usize)] {
        &self.jsx_expression_ranges
    }

    /// Get all MDX comment byte ranges
    pub fn mdx_comment_ranges(&self) -> &[(usize, usize)] {
        &self.mdx_comment_ranges
    }

    /// Check if a byte position is within a link reference definition title
    pub fn is_in_link_title(&self, byte_pos: usize) -> bool {
        self.reference_defs.iter().any(|def| {
            if let (Some(start), Some(end)) = (def.title_byte_start, def.title_byte_end) {
                byte_pos >= start && byte_pos < end
            } else {
                false
            }
        })
    }

    /// Check if content has any instances of a specific character (fast)
    pub fn has_char(&self, ch: char) -> bool {
        match ch {
            '#' => self.char_frequency.hash_count > 0,
            '*' => self.char_frequency.asterisk_count > 0,
            '_' => self.char_frequency.underscore_count > 0,
            '-' => self.char_frequency.hyphen_count > 0,
            '+' => self.char_frequency.plus_count > 0,
            '>' => self.char_frequency.gt_count > 0,
            '|' => self.char_frequency.pipe_count > 0,
            '[' => self.char_frequency.bracket_count > 0,
            '`' => self.char_frequency.backtick_count > 0,
            '<' => self.char_frequency.lt_count > 0,
            '!' => self.char_frequency.exclamation_count > 0,
            '\n' => self.char_frequency.newline_count > 0,
            _ => self.content.contains(ch), // Fallback for other characters
        }
    }

    /// Get count of a specific character (fast)
    pub fn char_count(&self, ch: char) -> usize {
        match ch {
            '#' => self.char_frequency.hash_count,
            '*' => self.char_frequency.asterisk_count,
            '_' => self.char_frequency.underscore_count,
            '-' => self.char_frequency.hyphen_count,
            '+' => self.char_frequency.plus_count,
            '>' => self.char_frequency.gt_count,
            '|' => self.char_frequency.pipe_count,
            '[' => self.char_frequency.bracket_count,
            '`' => self.char_frequency.backtick_count,
            '<' => self.char_frequency.lt_count,
            '!' => self.char_frequency.exclamation_count,
            '\n' => self.char_frequency.newline_count,
            _ => self.content.matches(ch).count(), // Fallback for other characters
        }
    }

    /// Check if content likely contains headings (fast)
    pub fn likely_has_headings(&self) -> bool {
        self.char_frequency.hash_count > 0 || self.char_frequency.hyphen_count > 2 // Potential setext underlines
    }

    /// Check if content likely contains lists (fast)
    pub fn likely_has_lists(&self) -> bool {
        self.char_frequency.asterisk_count > 0
            || self.char_frequency.hyphen_count > 0
            || self.char_frequency.plus_count > 0
    }

    /// Check if content likely contains emphasis (fast)
    pub fn likely_has_emphasis(&self) -> bool {
        self.char_frequency.asterisk_count > 1 || self.char_frequency.underscore_count > 1
    }

    /// Check if content likely contains tables (fast)
    pub fn likely_has_tables(&self) -> bool {
        self.char_frequency.pipe_count > 2
    }

    /// Check if content likely contains blockquotes (fast)
    pub fn likely_has_blockquotes(&self) -> bool {
        self.char_frequency.gt_count > 0
    }

    /// Check if content likely contains code (fast)
    pub fn likely_has_code(&self) -> bool {
        self.char_frequency.backtick_count > 0
    }

    /// Check if content likely contains links or images (fast)
    pub fn likely_has_links_or_images(&self) -> bool {
        self.char_frequency.bracket_count > 0 || self.char_frequency.exclamation_count > 0
    }

    /// Check if content likely contains HTML (fast)
    pub fn likely_has_html(&self) -> bool {
        self.char_frequency.lt_count > 0
    }

    /// Get the blockquote prefix for inserting a blank line at the given line index.
    /// Returns the prefix without trailing content (e.g., ">" or ">>").
    /// This is needed because blank lines inside blockquotes must preserve the blockquote structure.
    /// Returns an empty string if the line is not inside a blockquote.
    pub fn blockquote_prefix_for_blank_line(&self, line_idx: usize) -> String {
        if let Some(line_info) = self.lines.get(line_idx)
            && let Some(ref bq) = line_info.blockquote
        {
            bq.prefix.trim_end().to_string()
        } else {
            String::new()
        }
    }

    /// Get HTML tags on a specific line
    pub fn html_tags_on_line(&self, line_num: usize) -> Vec<HtmlTag> {
        self.html_tags()
            .iter()
            .filter(|tag| tag.line == line_num)
            .cloned()
            .collect()
    }

    /// Get emphasis spans on a specific line
    pub fn emphasis_spans_on_line(&self, line_num: usize) -> Vec<EmphasisSpan> {
        self.emphasis_spans()
            .iter()
            .filter(|span| span.line == line_num)
            .cloned()
            .collect()
    }

    /// Get table rows on a specific line
    pub fn table_rows_on_line(&self, line_num: usize) -> Vec<TableRow> {
        self.table_rows()
            .iter()
            .filter(|row| row.line == line_num)
            .cloned()
            .collect()
    }

    /// Get bare URLs on a specific line
    pub fn bare_urls_on_line(&self, line_num: usize) -> Vec<BareUrl> {
        self.bare_urls()
            .iter()
            .filter(|url| url.line == line_num)
            .cloned()
            .collect()
    }

    /// Find the line index for a given byte offset using binary search.
    /// Returns (line_index, line_number, column) where:
    /// - line_index is the 0-based index in the lines array
    /// - line_number is the 1-based line number
    /// - column is the byte offset within that line
    #[inline]
    fn find_line_for_offset(lines: &[LineInfo], byte_offset: usize) -> (usize, usize, usize) {
        // Binary search to find the line containing this byte offset
        let idx = match lines.binary_search_by(|line| {
            if byte_offset < line.byte_offset {
                std::cmp::Ordering::Greater
            } else if byte_offset > line.byte_offset + line.byte_len {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        let line = &lines[idx];
        let line_num = idx + 1;
        let col = byte_offset.saturating_sub(line.byte_offset);

        (idx, line_num, col)
    }

    /// Check if a byte offset is within a code span using binary search
    #[inline]
    fn is_offset_in_code_span(code_spans: &[CodeSpan], offset: usize) -> bool {
        // Since spans are sorted by byte_offset, use partition_point for binary search
        let idx = code_spans.partition_point(|span| span.byte_offset <= offset);

        // Check the span that starts at or before our offset
        if idx > 0 {
            let span = &code_spans[idx - 1];
            if offset >= span.byte_offset && offset < span.byte_end {
                return true;
            }
        }

        false
    }

    /// Collect byte ranges of all links using pulldown-cmark
    /// This is used to skip heading detection for lines that fall within link syntax
    /// (e.g., multiline links like `[text](url\n#fragment)`)
    fn collect_link_byte_ranges(content: &str) -> Vec<(usize, usize)> {
        use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

        let mut link_ranges = Vec::new();
        let mut options = Options::empty();
        options.insert(Options::ENABLE_WIKILINKS);
        options.insert(Options::ENABLE_FOOTNOTES);

        let parser = Parser::new_ext(content, options).into_offset_iter();
        let mut link_stack: Vec<usize> = Vec::new();

        for (event, range) in parser {
            match event {
                Event::Start(Tag::Link { .. }) => {
                    link_stack.push(range.start);
                }
                Event::End(TagEnd::Link) => {
                    if let Some(start_pos) = link_stack.pop() {
                        link_ranges.push((start_pos, range.end));
                    }
                }
                _ => {}
            }
        }

        link_ranges
    }

    /// Parse all links in the content
    fn parse_links(
        content: &'a str,
        lines: &[LineInfo],
        code_blocks: &[(usize, usize)],
        code_spans: &[CodeSpan],
        flavor: MarkdownFlavor,
        html_comment_ranges: &[crate::utils::skip_context::ByteRange],
    ) -> (Vec<ParsedLink<'a>>, Vec<BrokenLinkInfo>, Vec<FootnoteRef>) {
        use crate::utils::skip_context::{is_in_html_comment_ranges, is_mkdocs_snippet_line};
        use std::collections::HashSet;

        let mut links = Vec::with_capacity(content.len() / 500);
        let mut broken_links = Vec::new();
        let mut footnote_refs = Vec::new();

        // Track byte positions of links found by pulldown-cmark
        let mut found_positions = HashSet::new();

        // Use pulldown-cmark's streaming parser with BrokenLink callback
        // The callback captures undefined references: [text][undefined], [shortcut], [text][]
        // This automatically handles:
        // - Escaped links (won't generate events)
        // - Links in code blocks/spans (won't generate Link events)
        // - Images (generates Tag::Image instead)
        // - Reference resolution (dest_url is already resolved!)
        // - Broken references (callback is invoked)
        // - Wiki-links (enabled via ENABLE_WIKILINKS)
        let mut options = Options::empty();
        options.insert(Options::ENABLE_WIKILINKS);
        options.insert(Options::ENABLE_FOOTNOTES);

        let parser = Parser::new_with_broken_link_callback(
            content,
            options,
            Some(|link: BrokenLink<'_>| {
                broken_links.push(BrokenLinkInfo {
                    reference: link.reference.to_string(),
                    span: link.span.clone(),
                });
                None
            }),
        )
        .into_offset_iter();

        let mut link_stack: Vec<(
            usize,
            usize,
            pulldown_cmark::CowStr<'a>,
            LinkType,
            pulldown_cmark::CowStr<'a>,
        )> = Vec::new();
        let mut text_chunks: Vec<(String, usize, usize)> = Vec::new(); // (text, start, end)

        for (event, range) in parser {
            match event {
                Event::Start(Tag::Link {
                    link_type,
                    dest_url,
                    id,
                    ..
                }) => {
                    // Link start - record position, URL, and reference ID
                    link_stack.push((range.start, range.end, dest_url, link_type, id));
                    text_chunks.clear();
                }
                Event::Text(text) if !link_stack.is_empty() => {
                    // Track text content with its byte range
                    text_chunks.push((text.to_string(), range.start, range.end));
                }
                Event::Code(code) if !link_stack.is_empty() => {
                    // Include inline code in link text (with backticks)
                    let code_text = format!("`{code}`");
                    text_chunks.push((code_text, range.start, range.end));
                }
                Event::End(TagEnd::Link) => {
                    if let Some((start_pos, _link_start_end, url, link_type, ref_id)) = link_stack.pop() {
                        // Skip if in HTML comment
                        if is_in_html_comment_ranges(html_comment_ranges, start_pos) {
                            text_chunks.clear();
                            continue;
                        }

                        // Find line and column information
                        let (line_idx, line_num, col_start) = Self::find_line_for_offset(lines, start_pos);

                        // Skip if this link is on a MkDocs snippet line
                        if is_mkdocs_snippet_line(lines[line_idx].content(content), flavor) {
                            text_chunks.clear();
                            continue;
                        }

                        let (_, _end_line_num, col_end) = Self::find_line_for_offset(lines, range.end);

                        let is_reference = matches!(
                            link_type,
                            LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
                        );

                        // Extract link text directly from source bytes to preserve escaping
                        // Text events from pulldown-cmark unescape \] → ], which breaks MD039
                        let link_text = if start_pos < content.len() {
                            let link_bytes = &content.as_bytes()[start_pos..range.end.min(content.len())];

                            // Find MATCHING ] by tracking bracket depth for nested brackets
                            // An unescaped bracket is one NOT preceded by an odd number of backslashes
                            // Brackets inside code spans (between backticks) should be ignored
                            let mut close_pos = None;
                            let mut depth = 0;
                            let mut in_code_span = false;

                            for (i, &byte) in link_bytes.iter().enumerate().skip(1) {
                                // Count preceding backslashes
                                let mut backslash_count = 0;
                                let mut j = i;
                                while j > 0 && link_bytes[j - 1] == b'\\' {
                                    backslash_count += 1;
                                    j -= 1;
                                }
                                let is_escaped = backslash_count % 2 != 0;

                                // Track code spans - backticks toggle in/out of code
                                if byte == b'`' && !is_escaped {
                                    in_code_span = !in_code_span;
                                }

                                // Only count brackets when NOT in a code span
                                if !is_escaped && !in_code_span {
                                    if byte == b'[' {
                                        depth += 1;
                                    } else if byte == b']' {
                                        if depth == 0 {
                                            // Found the matching closing bracket
                                            close_pos = Some(i);
                                            break;
                                        } else {
                                            depth -= 1;
                                        }
                                    }
                                }
                            }

                            if let Some(pos) = close_pos {
                                Cow::Borrowed(std::str::from_utf8(&link_bytes[1..pos]).unwrap_or(""))
                            } else {
                                Cow::Borrowed("")
                            }
                        } else {
                            Cow::Borrowed("")
                        };

                        // For reference links, use the actual reference ID from pulldown-cmark
                        let reference_id = if is_reference && !ref_id.is_empty() {
                            Some(Cow::Owned(ref_id.to_lowercase()))
                        } else if is_reference {
                            // For collapsed/shortcut references without explicit ID, use the link text
                            Some(Cow::Owned(link_text.to_lowercase()))
                        } else {
                            None
                        };

                        // Track this position as found
                        found_positions.insert(start_pos);

                        links.push(ParsedLink {
                            line: line_num,
                            start_col: col_start,
                            end_col: col_end,
                            byte_offset: start_pos,
                            byte_end: range.end,
                            text: link_text,
                            url: Cow::Owned(url.to_string()),
                            is_reference,
                            reference_id,
                            link_type,
                        });

                        text_chunks.clear();
                    }
                }
                Event::FootnoteReference(footnote_id) => {
                    // Capture footnote references like [^1], [^note]
                    // Skip if in HTML comment
                    if is_in_html_comment_ranges(html_comment_ranges, range.start) {
                        continue;
                    }

                    let (_, line_num, _) = Self::find_line_for_offset(lines, range.start);
                    footnote_refs.push(FootnoteRef {
                        id: footnote_id.to_string(),
                        line: line_num,
                        byte_offset: range.start,
                        byte_end: range.end,
                    });
                }
                _ => {}
            }
        }

        // Also find undefined references using regex
        // These are patterns like [text][ref] that pulldown-cmark didn't parse as links
        // because the reference is undefined
        for cap in LINK_PATTERN.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let match_start = full_match.start();
            let match_end = full_match.end();

            // Skip if this was already found by pulldown-cmark (it's a valid link)
            if found_positions.contains(&match_start) {
                continue;
            }

            // Skip if escaped
            if match_start > 0 && content.as_bytes().get(match_start - 1) == Some(&b'\\') {
                continue;
            }

            // Skip if it's an image
            if match_start > 0 && content.as_bytes().get(match_start - 1) == Some(&b'!') {
                continue;
            }

            // Skip if in code block
            if CodeBlockUtils::is_in_code_block(code_blocks, match_start) {
                continue;
            }

            // Skip if in code span
            if Self::is_offset_in_code_span(code_spans, match_start) {
                continue;
            }

            // Skip if in HTML comment
            if is_in_html_comment_ranges(html_comment_ranges, match_start) {
                continue;
            }

            // Find line and column information
            let (line_idx, line_num, col_start) = Self::find_line_for_offset(lines, match_start);

            // Skip if this link is on a MkDocs snippet line
            if is_mkdocs_snippet_line(lines[line_idx].content(content), flavor) {
                continue;
            }

            let (_, _end_line_num, col_end) = Self::find_line_for_offset(lines, match_end);

            let text = cap.get(1).map_or("", |m| m.as_str());

            // Only process reference links (group 6)
            if let Some(ref_id) = cap.get(6) {
                let ref_id_str = ref_id.as_str();
                let normalized_ref = if ref_id_str.is_empty() {
                    Cow::Owned(text.to_lowercase()) // Implicit reference
                } else {
                    Cow::Owned(ref_id_str.to_lowercase())
                };

                // This is an undefined reference (pulldown-cmark didn't parse it)
                links.push(ParsedLink {
                    line: line_num,
                    start_col: col_start,
                    end_col: col_end,
                    byte_offset: match_start,
                    byte_end: match_end,
                    text: Cow::Borrowed(text),
                    url: Cow::Borrowed(""), // Empty URL indicates undefined reference
                    is_reference: true,
                    reference_id: Some(normalized_ref),
                    link_type: LinkType::Reference, // Undefined references are reference-style
                });
            }
        }

        (links, broken_links, footnote_refs)
    }

    /// Parse all images in the content
    fn parse_images(
        content: &'a str,
        lines: &[LineInfo],
        code_blocks: &[(usize, usize)],
        code_spans: &[CodeSpan],
        html_comment_ranges: &[crate::utils::skip_context::ByteRange],
    ) -> Vec<ParsedImage<'a>> {
        use crate::utils::skip_context::is_in_html_comment_ranges;
        use std::collections::HashSet;

        // Pre-size based on a heuristic: images are less common than links
        let mut images = Vec::with_capacity(content.len() / 1000);
        let mut found_positions = HashSet::new();

        // Use pulldown-cmark for parsing - more accurate and faster
        let parser = Parser::new(content).into_offset_iter();
        let mut image_stack: Vec<(usize, pulldown_cmark::CowStr<'a>, LinkType, pulldown_cmark::CowStr<'a>)> =
            Vec::new();
        let mut text_chunks: Vec<(String, usize, usize)> = Vec::new(); // (text, start, end)

        for (event, range) in parser {
            match event {
                Event::Start(Tag::Image {
                    link_type,
                    dest_url,
                    id,
                    ..
                }) => {
                    image_stack.push((range.start, dest_url, link_type, id));
                    text_chunks.clear();
                }
                Event::Text(text) if !image_stack.is_empty() => {
                    text_chunks.push((text.to_string(), range.start, range.end));
                }
                Event::Code(code) if !image_stack.is_empty() => {
                    let code_text = format!("`{code}`");
                    text_chunks.push((code_text, range.start, range.end));
                }
                Event::End(TagEnd::Image) => {
                    if let Some((start_pos, url, link_type, ref_id)) = image_stack.pop() {
                        // Skip if in code block
                        if CodeBlockUtils::is_in_code_block(code_blocks, start_pos) {
                            continue;
                        }

                        // Skip if in code span
                        if Self::is_offset_in_code_span(code_spans, start_pos) {
                            continue;
                        }

                        // Skip if in HTML comment
                        if is_in_html_comment_ranges(html_comment_ranges, start_pos) {
                            continue;
                        }

                        // Find line and column using binary search
                        let (_, line_num, col_start) = Self::find_line_for_offset(lines, start_pos);
                        let (_, _end_line_num, col_end) = Self::find_line_for_offset(lines, range.end);

                        let is_reference = matches!(
                            link_type,
                            LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
                        );

                        // Extract alt text directly from source bytes to preserve escaping
                        // Text events from pulldown-cmark unescape \] → ], which breaks rules that need escaping
                        let alt_text = if start_pos < content.len() {
                            let image_bytes = &content.as_bytes()[start_pos..range.end.min(content.len())];

                            // Find MATCHING ] by tracking bracket depth for nested brackets
                            // An unescaped bracket is one NOT preceded by an odd number of backslashes
                            let mut close_pos = None;
                            let mut depth = 0;

                            if image_bytes.len() > 2 {
                                for (i, &byte) in image_bytes.iter().enumerate().skip(2) {
                                    // Count preceding backslashes
                                    let mut backslash_count = 0;
                                    let mut j = i;
                                    while j > 0 && image_bytes[j - 1] == b'\\' {
                                        backslash_count += 1;
                                        j -= 1;
                                    }
                                    let is_escaped = backslash_count % 2 != 0;

                                    if !is_escaped {
                                        if byte == b'[' {
                                            depth += 1;
                                        } else if byte == b']' {
                                            if depth == 0 {
                                                // Found the matching closing bracket
                                                close_pos = Some(i);
                                                break;
                                            } else {
                                                depth -= 1;
                                            }
                                        }
                                    }
                                }
                            }

                            if let Some(pos) = close_pos {
                                Cow::Borrowed(std::str::from_utf8(&image_bytes[2..pos]).unwrap_or(""))
                            } else {
                                Cow::Borrowed("")
                            }
                        } else {
                            Cow::Borrowed("")
                        };

                        let reference_id = if is_reference && !ref_id.is_empty() {
                            Some(Cow::Owned(ref_id.to_lowercase()))
                        } else if is_reference {
                            Some(Cow::Owned(alt_text.to_lowercase())) // Collapsed/shortcut references
                        } else {
                            None
                        };

                        found_positions.insert(start_pos);
                        images.push(ParsedImage {
                            line: line_num,
                            start_col: col_start,
                            end_col: col_end,
                            byte_offset: start_pos,
                            byte_end: range.end,
                            alt_text,
                            url: Cow::Owned(url.to_string()),
                            is_reference,
                            reference_id,
                            link_type,
                        });
                    }
                }
                _ => {}
            }
        }

        // Regex fallback for undefined references that pulldown-cmark treats as plain text
        for cap in IMAGE_PATTERN.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let match_start = full_match.start();
            let match_end = full_match.end();

            // Skip if already found by pulldown-cmark
            if found_positions.contains(&match_start) {
                continue;
            }

            // Skip if the ! is escaped
            if match_start > 0 && content.as_bytes().get(match_start - 1) == Some(&b'\\') {
                continue;
            }

            // Skip if in code block, code span, or HTML comment
            if CodeBlockUtils::is_in_code_block(code_blocks, match_start)
                || Self::is_offset_in_code_span(code_spans, match_start)
                || is_in_html_comment_ranges(html_comment_ranges, match_start)
            {
                continue;
            }

            // Only process reference images (undefined references not found by pulldown-cmark)
            if let Some(ref_id) = cap.get(6) {
                let (_, line_num, col_start) = Self::find_line_for_offset(lines, match_start);
                let (_, _end_line_num, col_end) = Self::find_line_for_offset(lines, match_end);
                let alt_text = cap.get(1).map_or("", |m| m.as_str());
                let ref_id_str = ref_id.as_str();
                let normalized_ref = if ref_id_str.is_empty() {
                    Cow::Owned(alt_text.to_lowercase())
                } else {
                    Cow::Owned(ref_id_str.to_lowercase())
                };

                images.push(ParsedImage {
                    line: line_num,
                    start_col: col_start,
                    end_col: col_end,
                    byte_offset: match_start,
                    byte_end: match_end,
                    alt_text: Cow::Borrowed(alt_text),
                    url: Cow::Borrowed(""),
                    is_reference: true,
                    reference_id: Some(normalized_ref),
                    link_type: LinkType::Reference, // Undefined references are reference-style
                });
            }
        }

        images
    }

    /// Parse reference definitions
    fn parse_reference_defs(content: &str, lines: &[LineInfo]) -> Vec<ReferenceDef> {
        // Pre-size based on lines count as reference definitions are line-based
        let mut refs = Vec::with_capacity(lines.len() / 20); // ~1 ref per 20 lines

        for (line_idx, line_info) in lines.iter().enumerate() {
            // Skip lines in code blocks
            if line_info.in_code_block {
                continue;
            }

            let line = line_info.content(content);
            let line_num = line_idx + 1;

            if let Some(cap) = REF_DEF_PATTERN.captures(line) {
                let id_raw = cap.get(1).unwrap().as_str();

                // Skip footnote definitions - they use [^id]: syntax and are semantically
                // different from reference link definitions
                if id_raw.starts_with('^') {
                    continue;
                }

                let id = id_raw.to_lowercase();
                let url = cap.get(2).unwrap().as_str().to_string();
                let title_match = cap.get(3).or_else(|| cap.get(4));
                let title = title_match.map(|m| m.as_str().to_string());

                // Calculate byte positions
                // The match starts at the beginning of the line (0) and extends to the end
                let match_obj = cap.get(0).unwrap();
                let byte_offset = line_info.byte_offset + match_obj.start();
                let byte_end = line_info.byte_offset + match_obj.end();

                // Calculate title byte positions (includes the quote character before content)
                let (title_byte_start, title_byte_end) = if let Some(m) = title_match {
                    // The match is the content inside quotes, so we include the quote before
                    let start = line_info.byte_offset + m.start().saturating_sub(1);
                    let end = line_info.byte_offset + m.end() + 1; // Include closing quote
                    (Some(start), Some(end))
                } else {
                    (None, None)
                };

                refs.push(ReferenceDef {
                    line: line_num,
                    id,
                    url,
                    title,
                    byte_offset,
                    byte_end,
                    title_byte_start,
                    title_byte_end,
                });
            }
        }

        refs
    }

    /// Fast blockquote prefix parser - replaces regex for 5-10x speedup
    /// Handles nested blockquotes like `> > > content`
    /// Returns: Some((prefix_with_ws, content_after_prefix)) or None
    #[inline]
    fn parse_blockquote_prefix(line: &str) -> Option<(&str, &str)> {
        let trimmed_start = line.trim_start();
        if !trimmed_start.starts_with('>') {
            return None;
        }

        // Track total prefix length to handle nested blockquotes
        let mut remaining = line;
        let mut total_prefix_len = 0;

        loop {
            let trimmed = remaining.trim_start();
            if !trimmed.starts_with('>') {
                break;
            }

            // Add leading whitespace + '>' to prefix
            let leading_ws_len = remaining.len() - trimmed.len();
            total_prefix_len += leading_ws_len + 1;

            let after_gt = &trimmed[1..];

            // Handle optional whitespace after '>' (space or tab)
            if let Some(stripped) = after_gt.strip_prefix(' ') {
                total_prefix_len += 1;
                remaining = stripped;
            } else if let Some(stripped) = after_gt.strip_prefix('\t') {
                total_prefix_len += 1;
                remaining = stripped;
            } else {
                remaining = after_gt;
            }
        }

        Some((&line[..total_prefix_len], remaining))
    }

    /// Detect list items using pulldown-cmark for CommonMark-compliant parsing.
    ///
    /// Returns a HashMap keyed by line byte offset, containing:
    /// `(is_ordered, marker, marker_column, content_column, number)`
    ///
    /// ## Why pulldown-cmark?
    /// Using pulldown-cmark instead of regex ensures we only detect actual list items,
    /// not lines that merely look like lists (e.g., continuation paragraphs, code blocks).
    /// This fixes issue #253 where continuation lines were falsely detected.
    ///
    /// ## Tab indentation quirk
    /// Pulldown-cmark reports nested list items at the newline character position
    /// when tab indentation is used. For example, in `"* Item\n\t- Nested"`,
    /// the nested item is reported at byte 7 (the `\n`), not byte 8 (the `\t`).
    /// We detect this and advance to the correct line.
    ///
    /// ## HashMap key strategy
    /// We use `entry().or_insert()` because pulldown-cmark may emit multiple events
    /// that resolve to the same line (after newline adjustment). The first event
    /// for each line is authoritative.
    /// Detect list items and emphasis spans in a single pulldown-cmark pass.
    /// Returns both list items (for LineInfo) and emphasis spans (for MD030).
    /// This avoids a separate parse for emphasis detection.
    fn detect_list_items_and_emphasis_with_pulldown(
        content: &str,
        line_offsets: &[usize],
        flavor: MarkdownFlavor,
        front_matter_end: usize,
        code_blocks: &[(usize, usize)],
    ) -> (ListItemMap, Vec<EmphasisSpan>) {
        use std::collections::HashMap;

        let mut list_items = HashMap::new();
        let mut emphasis_spans = Vec::with_capacity(content.matches('*').count() + content.matches('_').count() / 4);

        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
        // Always enable GFM features for consistency with existing behavior
        options.insert(Options::ENABLE_GFM);

        // Suppress unused variable warning
        let _ = flavor;

        let parser = Parser::new_ext(content, options).into_offset_iter();
        let mut list_depth: usize = 0;
        let mut list_stack: Vec<bool> = Vec::new();

        for (event, range) in parser {
            match event {
                // Capture emphasis spans (for MD030's emphasis detection)
                Event::Start(Tag::Emphasis) | Event::Start(Tag::Strong) => {
                    let marker_count = if matches!(event, Event::Start(Tag::Strong)) {
                        2
                    } else {
                        1
                    };
                    let match_start = range.start;
                    let match_end = range.end;

                    // Skip if in code block
                    if !CodeBlockUtils::is_in_code_block_or_span(code_blocks, match_start) {
                        // Determine marker character by looking at the content at the start
                        let marker = content[match_start..].chars().next().unwrap_or('*');
                        if marker == '*' || marker == '_' {
                            // Extract content between markers
                            let content_start = match_start + marker_count;
                            let content_end = if match_end >= marker_count {
                                match_end - marker_count
                            } else {
                                match_end
                            };
                            let content_part = if content_start < content_end && content_end <= content.len() {
                                &content[content_start..content_end]
                            } else {
                                ""
                            };

                            // Find which line this emphasis is on using line_offsets
                            let line_idx = match line_offsets.binary_search(&match_start) {
                                Ok(idx) => idx,
                                Err(idx) => idx.saturating_sub(1),
                            };
                            let line_num = line_idx + 1;
                            let line_start = line_offsets.get(line_idx).copied().unwrap_or(0);
                            let col_start = match_start - line_start;
                            let col_end = match_end - line_start;

                            emphasis_spans.push(EmphasisSpan {
                                line: line_num,
                                start_col: col_start,
                                end_col: col_end,
                                byte_offset: match_start,
                                byte_end: match_end,
                                marker,
                                marker_count,
                                content: content_part.to_string(),
                            });
                        }
                    }
                }
                Event::Start(Tag::List(start_number)) => {
                    list_depth += 1;
                    list_stack.push(start_number.is_some());
                }
                Event::End(TagEnd::List(_)) => {
                    list_depth = list_depth.saturating_sub(1);
                    list_stack.pop();
                }
                Event::Start(Tag::Item) if list_depth > 0 => {
                    // Get the ordered state for the CURRENT (innermost) list
                    let current_list_is_ordered = list_stack.last().copied().unwrap_or(false);
                    // Find which line this byte offset corresponds to
                    let item_start = range.start;

                    // Binary search to find the line number
                    let mut line_idx = match line_offsets.binary_search(&item_start) {
                        Ok(idx) => idx,
                        Err(idx) => idx.saturating_sub(1),
                    };

                    // Pulldown-cmark reports nested list items at the newline before the item
                    // when using tab indentation (e.g., "* Item\n\t- Nested").
                    // Advance to the actual content line in this case.
                    if item_start < content.len() && content.as_bytes()[item_start] == b'\n' {
                        line_idx += 1;
                    }

                    // Skip list items in frontmatter (they are YAML/TOML syntax, not Markdown)
                    if front_matter_end > 0 && line_idx < front_matter_end {
                        continue;
                    }

                    if line_idx < line_offsets.len() {
                        let line_start_byte = line_offsets[line_idx];
                        let line_end = line_offsets.get(line_idx + 1).copied().unwrap_or(content.len());
                        let line = &content[line_start_byte..line_end.min(content.len())];

                        // Strip trailing newline
                        let line = line
                            .strip_suffix('\n')
                            .or_else(|| line.strip_suffix("\r\n"))
                            .unwrap_or(line);

                        // Strip blockquote prefix if present
                        let blockquote_parse = Self::parse_blockquote_prefix(line);
                        let (blockquote_prefix_len, line_to_parse) = if let Some((prefix, content)) = blockquote_parse {
                            (prefix.len(), content)
                        } else {
                            (0, line)
                        };

                        // Parse the list marker from the actual line
                        if current_list_is_ordered {
                            if let Some((leading_spaces, number_str, delimiter, spacing, _content)) =
                                Self::parse_ordered_list(line_to_parse)
                            {
                                let marker = format!("{number_str}{delimiter}");
                                let marker_column = blockquote_prefix_len + leading_spaces.len();
                                let content_column = marker_column + marker.len() + spacing.len();
                                let number = number_str.parse().ok();

                                list_items.entry(line_start_byte).or_insert((
                                    true,
                                    marker,
                                    marker_column,
                                    content_column,
                                    number,
                                ));
                            }
                        } else if let Some((leading_spaces, marker, spacing, _content)) =
                            Self::parse_unordered_list(line_to_parse)
                        {
                            let marker_column = blockquote_prefix_len + leading_spaces.len();
                            let content_column = marker_column + 1 + spacing.len();

                            list_items.entry(line_start_byte).or_insert((
                                false,
                                marker.to_string(),
                                marker_column,
                                content_column,
                                None,
                            ));
                        }
                    }
                }
                _ => {}
            }
        }

        (list_items, emphasis_spans)
    }

    /// Fast unordered list parser - replaces regex for 5-10x speedup
    /// Matches: ^(\s*)([-*+])([ \t]*)(.*)
    /// Returns: Some((leading_ws, marker, spacing, content)) or None
    #[inline]
    fn parse_unordered_list(line: &str) -> Option<(&str, char, &str, &str)> {
        let bytes = line.as_bytes();
        let mut i = 0;

        // Skip leading whitespace
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }

        // Check for marker
        if i >= bytes.len() {
            return None;
        }
        let marker = bytes[i] as char;
        if marker != '-' && marker != '*' && marker != '+' {
            return None;
        }
        let marker_pos = i;
        i += 1;

        // Collect spacing after marker (space or tab only)
        let spacing_start = i;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }

        Some((&line[..marker_pos], marker, &line[spacing_start..i], &line[i..]))
    }

    /// Fast ordered list parser - replaces regex for 5-10x speedup
    /// Matches: ^(\s*)(\d+)([.)])([ \t]*)(.*)
    /// Returns: Some((leading_ws, number_str, delimiter, spacing, content)) or None
    #[inline]
    fn parse_ordered_list(line: &str) -> Option<(&str, &str, char, &str, &str)> {
        let bytes = line.as_bytes();
        let mut i = 0;

        // Skip leading whitespace
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }

        // Collect digits
        let number_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == number_start {
            return None; // No digits found
        }

        // Check for delimiter
        if i >= bytes.len() {
            return None;
        }
        let delimiter = bytes[i] as char;
        if delimiter != '.' && delimiter != ')' {
            return None;
        }
        let delimiter_pos = i;
        i += 1;

        // Collect spacing after delimiter (space or tab only)
        let spacing_start = i;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }

        Some((
            &line[..number_start],
            &line[number_start..delimiter_pos],
            delimiter,
            &line[spacing_start..i],
            &line[i..],
        ))
    }

    /// Pre-compute which lines are in code blocks - O(m*n) where m=code_blocks, n=lines
    /// Returns a Vec<bool> where index i indicates if line i is in a code block
    fn compute_code_block_line_map(content: &str, line_offsets: &[usize], code_blocks: &[(usize, usize)]) -> Vec<bool> {
        let num_lines = line_offsets.len();
        let mut in_code_block = vec![false; num_lines];

        // For each code block, mark all lines within it
        for &(start, end) in code_blocks {
            // Ensure we're at valid UTF-8 boundaries
            let safe_start = if start > 0 && !content.is_char_boundary(start) {
                let mut boundary = start;
                while boundary > 0 && !content.is_char_boundary(boundary) {
                    boundary -= 1;
                }
                boundary
            } else {
                start
            };

            let safe_end = if end < content.len() && !content.is_char_boundary(end) {
                let mut boundary = end;
                while boundary < content.len() && !content.is_char_boundary(boundary) {
                    boundary += 1;
                }
                boundary
            } else {
                end.min(content.len())
            };

            // Trust the code blocks detected by CodeBlockUtils::detect_code_blocks()
            // That function now has proper list context awareness (see code_block_utils.rs)
            // and correctly distinguishes between:
            // - Fenced code blocks (``` or ~~~)
            // - Indented code blocks at document level (4 spaces + blank line before)
            // - List continuation paragraphs (NOT code blocks, even with 4 spaces)
            //
            // We no longer need to re-validate here. The original validation logic
            // was causing false positives by marking list continuation paragraphs as
            // code blocks when they have 4 spaces of indentation.

            // Use binary search to find the first and last line indices
            // line_offsets is sorted, so we can use partition_point for O(log n) lookup
            // Use safe_start/safe_end (UTF-8 boundaries) for consistent line mapping
            //
            // Find the line that CONTAINS safe_start: the line with the largest
            // start offset that is <= safe_start. partition_point gives us the
            // first line that starts AFTER safe_start, so we subtract 1.
            let first_line_after = line_offsets.partition_point(|&offset| offset <= safe_start);
            let first_line = first_line_after.saturating_sub(1);
            let last_line = line_offsets.partition_point(|&offset| offset < safe_end);

            // Mark all lines in the range at once
            for flag in in_code_block.iter_mut().take(last_line).skip(first_line) {
                *flag = true;
            }
        }

        in_code_block
    }

    /// Pre-compute which lines are inside math blocks ($$ ... $$) - O(n) single pass
    /// Returns a Vec<bool> where index i indicates if line i is in a math block
    fn compute_math_block_line_map(content: &str, code_block_map: &[bool]) -> Vec<bool> {
        let content_lines: Vec<&str> = content.lines().collect();
        let num_lines = content_lines.len();
        let mut in_math_block = vec![false; num_lines];

        let mut inside_math = false;

        for (i, line) in content_lines.iter().enumerate() {
            // Skip lines that are in code blocks - math delimiters inside code are literal
            if code_block_map.get(i).copied().unwrap_or(false) {
                continue;
            }

            let trimmed = line.trim();

            // Check for math block delimiter ($$)
            // A line with just $$ toggles the math block state
            if trimmed == "$$" {
                if inside_math {
                    // Closing delimiter - this line is still part of the math block
                    in_math_block[i] = true;
                    inside_math = false;
                } else {
                    // Opening delimiter - this line starts the math block
                    in_math_block[i] = true;
                    inside_math = true;
                }
            } else if inside_math {
                // Content inside math block
                in_math_block[i] = true;
            }
        }

        in_math_block
    }

    /// Pre-compute basic line information (without headings/blockquotes)
    /// Also returns emphasis spans detected during the pulldown-cmark parse
    fn compute_basic_line_info(
        content: &str,
        line_offsets: &[usize],
        code_blocks: &[(usize, usize)],
        flavor: MarkdownFlavor,
        html_comment_ranges: &[crate::utils::skip_context::ByteRange],
        autodoc_ranges: &[crate::utils::skip_context::ByteRange],
        quarto_div_ranges: &[crate::utils::skip_context::ByteRange],
    ) -> (Vec<LineInfo>, Vec<EmphasisSpan>) {
        let content_lines: Vec<&str> = content.lines().collect();
        let mut lines = Vec::with_capacity(content_lines.len());

        // Pre-compute which lines are in code blocks
        let code_block_map = Self::compute_code_block_line_map(content, line_offsets, code_blocks);

        // Pre-compute which lines are in math blocks ($$ ... $$)
        let math_block_map = Self::compute_math_block_line_map(content, &code_block_map);

        // Detect front matter boundaries FIRST, before any other parsing
        // Use FrontMatterUtils to detect all types of front matter (YAML, TOML, JSON, malformed)
        let front_matter_end = FrontMatterUtils::get_front_matter_end_line(content);

        // Use pulldown-cmark to detect list items AND emphasis spans in a single pass
        // (context-aware, eliminates false positives)
        let (list_item_map, emphasis_spans) = Self::detect_list_items_and_emphasis_with_pulldown(
            content,
            line_offsets,
            flavor,
            front_matter_end,
            code_blocks,
        );

        for (i, line) in content_lines.iter().enumerate() {
            let byte_offset = line_offsets.get(i).copied().unwrap_or(0);
            let indent = line.len() - line.trim_start().len();
            // Compute visual indent with proper CommonMark tab expansion
            let visual_indent = ElementCache::calculate_indentation_width_default(line);

            // Parse blockquote prefix once and reuse it (avoid redundant parsing)
            let blockquote_parse = Self::parse_blockquote_prefix(line);

            // For blank detection, consider blockquote context
            let is_blank = if let Some((_, content)) = blockquote_parse {
                // In blockquote context, check if content after prefix is blank
                content.trim().is_empty()
            } else {
                line.trim().is_empty()
            };

            // Use pre-computed map for O(1) lookup instead of O(m) iteration
            let in_code_block = code_block_map.get(i).copied().unwrap_or(false);

            // Detect list items (skip if in frontmatter, in mkdocstrings block, or in HTML comment)
            let in_mkdocstrings = flavor == MarkdownFlavor::MkDocs
                && crate::utils::mkdocstrings_refs::is_within_autodoc_block_ranges(autodoc_ranges, byte_offset);
            // Check if the ENTIRE line is within an HTML comment (not just the line start)
            // This ensures content after `-->` on the same line is not incorrectly skipped
            let line_end_offset = byte_offset + line.len();
            let in_html_comment = crate::utils::skip_context::is_line_entirely_in_html_comment(
                html_comment_ranges,
                byte_offset,
                line_end_offset,
            );
            // Use pulldown-cmark's list detection for context-aware parsing
            // This eliminates false positives on continuation lines (issue #253)
            let list_item =
                list_item_map
                    .get(&byte_offset)
                    .map(
                        |(is_ordered, marker, marker_column, content_column, number)| ListItemInfo {
                            marker: marker.clone(),
                            is_ordered: *is_ordered,
                            number: *number,
                            marker_column: *marker_column,
                            content_column: *content_column,
                        },
                    );

            // Detect horizontal rules (only outside code blocks and frontmatter)
            // Uses CommonMark-compliant check including leading indentation validation
            let in_front_matter = front_matter_end > 0 && i < front_matter_end;
            let is_hr = !in_code_block && !in_front_matter && is_horizontal_rule_line(line);

            // Get math block status for this line
            let in_math_block = math_block_map.get(i).copied().unwrap_or(false);

            // Check if line is inside a Quarto div block
            let in_quarto_div = flavor == MarkdownFlavor::Quarto
                && crate::utils::quarto_divs::is_within_div_block_ranges(quarto_div_ranges, byte_offset);

            lines.push(LineInfo {
                byte_offset,
                byte_len: line.len(),
                indent,
                visual_indent,
                is_blank,
                in_code_block,
                in_front_matter,
                in_html_block: false, // Will be populated after line creation
                in_html_comment,
                list_item,
                heading: None,    // Will be populated in second pass for Setext headings
                blockquote: None, // Will be populated after line creation
                in_mkdocstrings,
                in_esm_block: false, // Will be populated after line creation for MDX files
                in_code_span_continuation: false, // Will be populated after code spans are parsed
                is_horizontal_rule: is_hr,
                in_math_block,
                in_quarto_div,
                in_jsx_expression: false, // Will be populated for MDX files
                in_mdx_comment: false,    // Will be populated for MDX files
            });
        }

        (lines, emphasis_spans)
    }

    /// Detect headings and blockquotes (called after HTML block detection)
    fn detect_headings_and_blockquotes(
        content: &str,
        lines: &mut [LineInfo],
        flavor: MarkdownFlavor,
        html_comment_ranges: &[crate::utils::skip_context::ByteRange],
        link_byte_ranges: &[(usize, usize)],
    ) {
        // Regex for heading detection
        static ATX_HEADING_REGEX: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"^(\s*)(#{1,6})(\s*)(.*)$").unwrap());
        static SETEXT_UNDERLINE_REGEX: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"^(\s*)(=+|-+)\s*$").unwrap());

        let content_lines: Vec<&str> = content.lines().collect();

        // Detect front matter boundaries to skip those lines
        let front_matter_end = FrontMatterUtils::get_front_matter_end_line(content);

        // Detect headings (including Setext which needs look-ahead) and blockquotes
        for i in 0..lines.len() {
            let line = content_lines[i];

            // Detect blockquotes FIRST, before any skip conditions.
            // A line can be both a blockquote AND contain a code block inside it.
            // We need to know about the blockquote marker regardless of code block status.
            // Skip only frontmatter lines - those are never blockquotes.
            if !(front_matter_end > 0 && i < front_matter_end)
                && let Some(bq) = parse_blockquote_detailed(line)
            {
                let nesting_level = bq.markers.len();
                let marker_column = bq.indent.len();
                let prefix = format!("{}{}{}", bq.indent, bq.markers, bq.spaces_after);
                let has_no_space = bq.spaces_after.is_empty() && !bq.content.is_empty();
                let has_multiple_spaces = bq.spaces_after.chars().filter(|&c| c == ' ').count() > 1;
                let needs_md028_fix = bq.content.is_empty() && bq.spaces_after.is_empty();

                lines[i].blockquote = Some(BlockquoteInfo {
                    nesting_level,
                    indent: bq.indent.to_string(),
                    marker_column,
                    prefix,
                    content: bq.content.to_string(),
                    has_no_space_after_marker: has_no_space,
                    has_multiple_spaces_after_marker: has_multiple_spaces,
                    needs_md028_fix,
                });

                // Update is_horizontal_rule for blockquote content
                // The original detection doesn't strip blockquote prefix, so we need to check here
                if !lines[i].in_code_block && is_horizontal_rule_content(bq.content.trim()) {
                    lines[i].is_horizontal_rule = true;
                }
            }

            // Now apply skip conditions for heading detection
            if lines[i].in_code_block {
                continue;
            }

            // Skip lines in front matter
            if front_matter_end > 0 && i < front_matter_end {
                continue;
            }

            // Skip lines in HTML blocks - HTML content should not be parsed as markdown
            if lines[i].in_html_block {
                continue;
            }

            // Skip heading detection for blank lines
            if lines[i].is_blank {
                continue;
            }

            // Check for ATX headings (but skip MkDocs snippet lines)
            // In MkDocs flavor, lines like "# -8<- [start:name]" are snippet markers, not headings
            let is_snippet_line = if flavor == MarkdownFlavor::MkDocs {
                crate::utils::mkdocs_snippets::is_snippet_section_start(line)
                    || crate::utils::mkdocs_snippets::is_snippet_section_end(line)
            } else {
                false
            };

            if !is_snippet_line && let Some(caps) = ATX_HEADING_REGEX.captures(line) {
                // Skip headings inside HTML comments (using pre-computed ranges for efficiency)
                if crate::utils::skip_context::is_in_html_comment_ranges(html_comment_ranges, lines[i].byte_offset) {
                    continue;
                }
                // Skip lines that fall within link syntax (e.g., multiline links like `[text](url\n#fragment)`)
                // This prevents false positives where `#fragment` is detected as a heading
                let line_offset = lines[i].byte_offset;
                if link_byte_ranges
                    .iter()
                    .any(|&(start, end)| line_offset > start && line_offset < end)
                {
                    continue;
                }
                let leading_spaces = caps.get(1).map_or("", |m| m.as_str());
                let hashes = caps.get(2).map_or("", |m| m.as_str());
                let spaces_after = caps.get(3).map_or("", |m| m.as_str());
                let rest = caps.get(4).map_or("", |m| m.as_str());

                let level = hashes.len() as u8;
                let marker_column = leading_spaces.len();

                // Check for closing sequence, but handle custom IDs that might come after
                let (text, has_closing, closing_seq) = {
                    // First check if there's a custom ID at the end
                    let (rest_without_id, custom_id_part) = if let Some(id_start) = rest.rfind(" {#") {
                        // Check if this looks like a valid custom ID (ends with })
                        if rest[id_start..].trim_end().ends_with('}') {
                            // Split off the custom ID
                            (&rest[..id_start], &rest[id_start..])
                        } else {
                            (rest, "")
                        }
                    } else {
                        (rest, "")
                    };

                    // Now look for closing hashes in the part before the custom ID
                    let trimmed_rest = rest_without_id.trim_end();
                    if let Some(last_hash_byte_pos) = trimmed_rest.rfind('#') {
                        // Find the start of the hash sequence by walking backwards
                        // Use char_indices to get byte positions at char boundaries
                        let char_positions: Vec<(usize, char)> = trimmed_rest.char_indices().collect();

                        // Find which char index corresponds to last_hash_byte_pos
                        let last_hash_char_idx = char_positions
                            .iter()
                            .position(|(byte_pos, _)| *byte_pos == last_hash_byte_pos);

                        if let Some(mut char_idx) = last_hash_char_idx {
                            // Walk backwards to find start of hash sequence
                            while char_idx > 0 && char_positions[char_idx - 1].1 == '#' {
                                char_idx -= 1;
                            }

                            // Get the byte position of the start of hashes
                            let start_of_hashes = char_positions[char_idx].0;

                            // Check if there's at least one space before the closing hashes
                            let has_space_before = char_idx == 0 || char_positions[char_idx - 1].1.is_whitespace();

                            // Check if this is a valid closing sequence (all hashes to end of trimmed part)
                            let potential_closing = &trimmed_rest[start_of_hashes..];
                            let is_all_hashes = potential_closing.chars().all(|c| c == '#');

                            if is_all_hashes && has_space_before {
                                // This is a closing sequence
                                let closing_hashes = potential_closing.to_string();
                                // The text is everything before the closing hashes
                                // Don't include the custom ID here - it will be extracted later
                                let text_part = if !custom_id_part.is_empty() {
                                    // If we have a custom ID, append it back to get the full rest
                                    // This allows the extract_header_id function to handle it properly
                                    format!("{}{}", trimmed_rest[..start_of_hashes].trim_end(), custom_id_part)
                                } else {
                                    trimmed_rest[..start_of_hashes].trim_end().to_string()
                                };
                                (text_part, true, closing_hashes)
                            } else {
                                // Not a valid closing sequence, return the full content
                                (rest.to_string(), false, String::new())
                            }
                        } else {
                            // Couldn't find char boundary, return the full content
                            (rest.to_string(), false, String::new())
                        }
                    } else {
                        // No hashes found, return the full content
                        (rest.to_string(), false, String::new())
                    }
                };

                let content_column = marker_column + hashes.len() + spaces_after.len();

                // Extract custom header ID if present
                let raw_text = text.trim().to_string();
                let (clean_text, mut custom_id) = crate::utils::header_id_utils::extract_header_id(&raw_text);

                // If no custom ID was found on the header line, check the next line for standalone attr-list
                if custom_id.is_none() && i + 1 < content_lines.len() && i + 1 < lines.len() {
                    let next_line = content_lines[i + 1];
                    if !lines[i + 1].in_code_block
                        && crate::utils::header_id_utils::is_standalone_attr_list(next_line)
                        && let Some(next_line_id) =
                            crate::utils::header_id_utils::extract_standalone_attr_list_id(next_line)
                    {
                        custom_id = Some(next_line_id);
                    }
                }

                // ATX heading is "valid" for processing by heading rules if:
                // 1. Has space after # (CommonMark compliant): `# Heading`
                // 2. Is empty (just hashes): `#`
                // 3. Has multiple hashes (##intro is likely intended heading, not hashtag)
                // 4. Content starts with uppercase (likely intended heading, not social hashtag)
                //
                // Invalid patterns (hashtag-like) are skipped by most heading rules:
                // - `#tag` - single # with lowercase (social hashtag)
                // - `#123` - single # with number (GitHub issue ref)
                let is_valid = !spaces_after.is_empty()
                    || rest.is_empty()
                    || level > 1
                    || rest.trim().chars().next().is_some_and(|c| c.is_uppercase());

                lines[i].heading = Some(HeadingInfo {
                    level,
                    style: HeadingStyle::ATX,
                    marker: hashes.to_string(),
                    marker_column,
                    content_column,
                    text: clean_text,
                    custom_id,
                    raw_text,
                    has_closing_sequence: has_closing,
                    closing_sequence: closing_seq,
                    is_valid,
                });
            }
            // Check for Setext headings (need to look at next line)
            else if i + 1 < content_lines.len() && i + 1 < lines.len() {
                let next_line = content_lines[i + 1];
                if !lines[i + 1].in_code_block && SETEXT_UNDERLINE_REGEX.is_match(next_line) {
                    // Skip if next line is front matter delimiter
                    if front_matter_end > 0 && i < front_matter_end {
                        continue;
                    }

                    // Skip Setext headings inside HTML comments (using pre-computed ranges for efficiency)
                    if crate::utils::skip_context::is_in_html_comment_ranges(html_comment_ranges, lines[i].byte_offset)
                    {
                        continue;
                    }

                    // Per CommonMark spec 4.3, setext heading content cannot be interpretable as:
                    // list item, ATX heading, block quote, thematic break, code fence, or HTML block
                    let content_line = line.trim();

                    // Skip list items (-, *, +) and thematic breaks (---, ***, etc.)
                    if content_line.starts_with('-') || content_line.starts_with('*') || content_line.starts_with('+') {
                        continue;
                    }

                    // Skip underscore thematic breaks (___)
                    if content_line.starts_with('_') {
                        let non_ws: String = content_line.chars().filter(|c| !c.is_whitespace()).collect();
                        if non_ws.len() >= 3 && non_ws.chars().all(|c| c == '_') {
                            continue;
                        }
                    }

                    // Skip numbered lists (1. Item, 2. Item, etc.)
                    if let Some(first_char) = content_line.chars().next()
                        && first_char.is_ascii_digit()
                    {
                        let num_end = content_line.chars().take_while(|c| c.is_ascii_digit()).count();
                        if num_end < content_line.len() {
                            let next = content_line.chars().nth(num_end);
                            if next == Some('.') || next == Some(')') {
                                continue;
                            }
                        }
                    }

                    // Skip ATX headings
                    if ATX_HEADING_REGEX.is_match(line) {
                        continue;
                    }

                    // Skip blockquotes
                    if content_line.starts_with('>') {
                        continue;
                    }

                    // Skip code fences
                    let trimmed_start = line.trim_start();
                    if trimmed_start.len() >= 3 {
                        let first_three: String = trimmed_start.chars().take(3).collect();
                        if first_three == "```" || first_three == "~~~" {
                            continue;
                        }
                    }

                    // Skip HTML blocks
                    if content_line.starts_with('<') {
                        continue;
                    }

                    let underline = next_line.trim();

                    let level = if underline.starts_with('=') { 1 } else { 2 };
                    let style = if level == 1 {
                        HeadingStyle::Setext1
                    } else {
                        HeadingStyle::Setext2
                    };

                    // Extract custom header ID if present
                    let raw_text = line.trim().to_string();
                    let (clean_text, mut custom_id) = crate::utils::header_id_utils::extract_header_id(&raw_text);

                    // If no custom ID was found on the header line, check the line after underline for standalone attr-list
                    if custom_id.is_none() && i + 2 < content_lines.len() && i + 2 < lines.len() {
                        let attr_line = content_lines[i + 2];
                        if !lines[i + 2].in_code_block
                            && crate::utils::header_id_utils::is_standalone_attr_list(attr_line)
                            && let Some(attr_line_id) =
                                crate::utils::header_id_utils::extract_standalone_attr_list_id(attr_line)
                        {
                            custom_id = Some(attr_line_id);
                        }
                    }

                    lines[i].heading = Some(HeadingInfo {
                        level,
                        style,
                        marker: underline.to_string(),
                        marker_column: next_line.len() - next_line.trim_start().len(),
                        content_column: lines[i].indent,
                        text: clean_text,
                        custom_id,
                        raw_text,
                        has_closing_sequence: false,
                        closing_sequence: String::new(),
                        is_valid: true, // Setext headings are always valid
                    });
                }
            }
        }
    }

    /// Detect HTML blocks in the content
    fn detect_html_blocks(content: &str, lines: &mut [LineInfo]) {
        // HTML block elements that trigger block context
        // Includes HTML5 media, embedded content, and interactive elements
        const BLOCK_ELEMENTS: &[&str] = &[
            "address",
            "article",
            "aside",
            "audio",
            "blockquote",
            "canvas",
            "details",
            "dialog",
            "dd",
            "div",
            "dl",
            "dt",
            "embed",
            "fieldset",
            "figcaption",
            "figure",
            "footer",
            "form",
            "h1",
            "h2",
            "h3",
            "h4",
            "h5",
            "h6",
            "header",
            "hr",
            "iframe",
            "li",
            "main",
            "menu",
            "nav",
            "noscript",
            "object",
            "ol",
            "p",
            "picture",
            "pre",
            "script",
            "search",
            "section",
            "source",
            "style",
            "summary",
            "svg",
            "table",
            "tbody",
            "td",
            "template",
            "textarea",
            "tfoot",
            "th",
            "thead",
            "tr",
            "track",
            "ul",
            "video",
        ];

        let mut i = 0;
        while i < lines.len() {
            // Skip if already in code block or front matter
            if lines[i].in_code_block || lines[i].in_front_matter {
                i += 1;
                continue;
            }

            let trimmed = lines[i].content(content).trim_start();

            // Check if line starts with an HTML tag
            if trimmed.starts_with('<') && trimmed.len() > 1 {
                // Extract tag name safely
                let after_bracket = &trimmed[1..];
                let is_closing = after_bracket.starts_with('/');
                let tag_start = if is_closing { &after_bracket[1..] } else { after_bracket };

                // Extract tag name (stop at space, >, /, or end of string)
                let tag_name = tag_start
                    .chars()
                    .take_while(|c| c.is_ascii_alphabetic() || *c == '-' || c.is_ascii_digit())
                    .collect::<String>()
                    .to_lowercase();

                // Check if it's a block element
                if !tag_name.is_empty() && BLOCK_ELEMENTS.contains(&tag_name.as_str()) {
                    // Mark this line as in HTML block
                    lines[i].in_html_block = true;

                    // For simplicity, just mark lines until we find a closing tag or reach a blank line
                    // This avoids complex nesting logic that might cause infinite loops
                    if !is_closing {
                        let closing_tag = format!("</{tag_name}>");
                        // style and script tags can contain blank lines (CSS/JS formatting)
                        let allow_blank_lines = tag_name == "style" || tag_name == "script";
                        let mut j = i + 1;
                        let mut found_closing_tag = false;
                        while j < lines.len() && j < i + 100 {
                            // Limit search to 100 lines
                            // Stop at blank lines (except for style/script tags)
                            if !allow_blank_lines && lines[j].is_blank {
                                break;
                            }

                            lines[j].in_html_block = true;

                            // Check if this line contains the closing tag
                            if lines[j].content(content).contains(&closing_tag) {
                                found_closing_tag = true;
                            }

                            // After finding closing tag, continue marking lines as
                            // in_html_block until blank line (per CommonMark spec)
                            if found_closing_tag {
                                j += 1;
                                // Continue marking subsequent lines until blank
                                while j < lines.len() && j < i + 100 {
                                    if lines[j].is_blank {
                                        break;
                                    }
                                    lines[j].in_html_block = true;
                                    j += 1;
                                }
                                break;
                            }
                            j += 1;
                        }
                    }
                }
            }

            i += 1;
        }
    }

    /// Detect ESM import/export blocks anywhere in MDX files
    /// MDX 2.0+ allows imports/exports anywhere in the document, not just at the top
    fn detect_esm_blocks(content: &str, lines: &mut [LineInfo], flavor: MarkdownFlavor) {
        // Only process MDX files
        if !flavor.supports_esm_blocks() {
            return;
        }

        let mut in_multiline_import = false;

        for line in lines.iter_mut() {
            // Skip code blocks, front matter, and HTML comments
            if line.in_code_block || line.in_front_matter || line.in_html_comment {
                in_multiline_import = false;
                continue;
            }

            let line_content = line.content(content);
            let trimmed = line_content.trim();

            // Handle continuation of multi-line import/export
            if in_multiline_import {
                line.in_esm_block = true;
                // Check if this line completes the statement
                // Multi-line import ends when we see the closing quote + optional semicolon
                if trimmed.ends_with('\'')
                    || trimmed.ends_with('"')
                    || trimmed.ends_with("';")
                    || trimmed.ends_with("\";")
                    || line_content.contains(';')
                {
                    in_multiline_import = false;
                }
                continue;
            }

            // Skip blank lines
            if line.is_blank {
                continue;
            }

            // Check if line starts with import or export
            if trimmed.starts_with("import ") || trimmed.starts_with("export ") {
                line.in_esm_block = true;

                // Determine if this is a complete single-line statement or starts a multi-line one
                // Multi-line imports look like:
                //   import {
                //     Foo,
                //     Bar
                //   } from 'module'
                // Single-line imports/exports end with a quote, semicolon, or are simple exports
                let is_import = trimmed.starts_with("import ");

                // Check for simple complete statements
                let is_complete =
                    // Ends with semicolon
                    trimmed.ends_with(';')
                    // import/export with from clause that ends with quote
                    || (trimmed.contains(" from ") && (trimmed.ends_with('\'') || trimmed.ends_with('"')))
                    // Simple export (export const/let/var/function/class without from)
                    || (!is_import && !trimmed.contains(" from ") && (
                        trimmed.starts_with("export const ")
                        || trimmed.starts_with("export let ")
                        || trimmed.starts_with("export var ")
                        || trimmed.starts_with("export function ")
                        || trimmed.starts_with("export class ")
                        || trimmed.starts_with("export default ")
                    ));

                if !is_complete && is_import {
                    // Only imports can span multiple lines in the typical case
                    // Check if it looks like the start of a multi-line import
                    // e.g., "import {" or "import type {"
                    if trimmed.contains('{') && !trimmed.contains('}') {
                        in_multiline_import = true;
                    }
                }
            }
        }
    }

    /// Detect JSX expressions {expression} and MDX comments {/* comment */} in MDX files
    /// Returns (jsx_expression_ranges, mdx_comment_ranges)
    fn detect_jsx_and_mdx_comments(
        content: &str,
        lines: &mut [LineInfo],
        flavor: MarkdownFlavor,
        code_blocks: &[(usize, usize)],
    ) -> (ByteRanges, ByteRanges) {
        // Only process MDX files
        if !flavor.supports_jsx() {
            return (Vec::new(), Vec::new());
        }

        let mut jsx_expression_ranges: Vec<(usize, usize)> = Vec::new();
        let mut mdx_comment_ranges: Vec<(usize, usize)> = Vec::new();

        // Quick check - if no braces, no JSX expressions or MDX comments
        if !content.contains('{') {
            return (jsx_expression_ranges, mdx_comment_ranges);
        }

        let bytes = content.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            if bytes[i] == b'{' {
                // Check if we're in a code block
                if code_blocks.iter().any(|(start, end)| i >= *start && i < *end) {
                    i += 1;
                    continue;
                }

                let start = i;

                // Check if it's an MDX comment: {/* ... */}
                if i + 2 < bytes.len() && &bytes[i + 1..i + 3] == b"/*" {
                    // Find the closing */}
                    let mut j = i + 3;
                    while j + 2 < bytes.len() {
                        if &bytes[j..j + 2] == b"*/" && j + 2 < bytes.len() && bytes[j + 2] == b'}' {
                            let end = j + 3;
                            mdx_comment_ranges.push((start, end));

                            // Mark lines as in MDX comment
                            Self::mark_lines_in_range(lines, content, start, end, |line| {
                                line.in_mdx_comment = true;
                            });

                            i = end;
                            break;
                        }
                        j += 1;
                    }
                    if j + 2 >= bytes.len() {
                        // Unclosed MDX comment - mark rest as comment
                        mdx_comment_ranges.push((start, bytes.len()));
                        Self::mark_lines_in_range(lines, content, start, bytes.len(), |line| {
                            line.in_mdx_comment = true;
                        });
                        break;
                    }
                } else {
                    // Regular JSX expression: { ... }
                    // Need to handle nested braces
                    let mut brace_depth = 1;
                    let mut j = i + 1;
                    let mut in_string = false;
                    let mut string_char = b'"';

                    while j < bytes.len() && brace_depth > 0 {
                        let c = bytes[j];

                        // Handle strings to avoid counting braces inside them
                        if !in_string && (c == b'"' || c == b'\'' || c == b'`') {
                            in_string = true;
                            string_char = c;
                        } else if in_string && c == string_char && (j == 0 || bytes[j - 1] != b'\\') {
                            in_string = false;
                        } else if !in_string {
                            if c == b'{' {
                                brace_depth += 1;
                            } else if c == b'}' {
                                brace_depth -= 1;
                            }
                        }
                        j += 1;
                    }

                    if brace_depth == 0 {
                        let end = j;
                        jsx_expression_ranges.push((start, end));

                        // Mark lines as in JSX expression
                        Self::mark_lines_in_range(lines, content, start, end, |line| {
                            line.in_jsx_expression = true;
                        });

                        i = end;
                    } else {
                        i += 1;
                    }
                }
            } else {
                i += 1;
            }
        }

        (jsx_expression_ranges, mdx_comment_ranges)
    }

    /// Helper to mark lines within a byte range
    fn mark_lines_in_range<F>(lines: &mut [LineInfo], content: &str, start: usize, end: usize, mut f: F)
    where
        F: FnMut(&mut LineInfo),
    {
        // Find lines that overlap with the range
        for line in lines.iter_mut() {
            let line_start = line.byte_offset;
            let line_end = line.byte_offset + line.byte_len;

            // Check if this line overlaps with the range
            if line_start < end && line_end > start {
                f(line);
            }
        }

        // Silence unused warning for content (needed for signature consistency)
        let _ = content;
    }

    /// Parse all inline code spans in the content using pulldown-cmark streaming parser
    fn parse_code_spans(content: &str, lines: &[LineInfo]) -> Vec<CodeSpan> {
        let mut code_spans = Vec::new();

        // Quick check - if no backticks, no code spans
        if !content.contains('`') {
            return code_spans;
        }

        // Use pulldown-cmark's streaming parser with byte offsets
        let parser = Parser::new(content).into_offset_iter();

        for (event, range) in parser {
            if let Event::Code(_) = event {
                let start_pos = range.start;
                let end_pos = range.end;

                // The range includes the backticks, extract the actual content
                let full_span = &content[start_pos..end_pos];
                let backtick_count = full_span.chars().take_while(|&c| c == '`').count();

                // Extract content between backticks, preserving spaces
                let content_start = start_pos + backtick_count;
                let content_end = end_pos - backtick_count;
                let span_content = if content_start < content_end {
                    content[content_start..content_end].to_string()
                } else {
                    String::new()
                };

                // Use binary search to find line number - O(log n) instead of O(n)
                // Find the rightmost line whose byte_offset <= start_pos
                let line_idx = lines
                    .partition_point(|line| line.byte_offset <= start_pos)
                    .saturating_sub(1);
                let line_num = line_idx + 1;
                let byte_col_start = start_pos - lines[line_idx].byte_offset;

                // Find end column using binary search
                let end_line_idx = lines
                    .partition_point(|line| line.byte_offset <= end_pos)
                    .saturating_sub(1);
                let byte_col_end = end_pos - lines[end_line_idx].byte_offset;

                // Convert byte offsets to character positions for correct Unicode handling
                // This ensures consistency with warning.column which uses character positions
                let line_content = lines[line_idx].content(content);
                let col_start = if byte_col_start <= line_content.len() {
                    line_content[..byte_col_start].chars().count()
                } else {
                    line_content.chars().count()
                };

                let end_line_content = lines[end_line_idx].content(content);
                let col_end = if byte_col_end <= end_line_content.len() {
                    end_line_content[..byte_col_end].chars().count()
                } else {
                    end_line_content.chars().count()
                };

                code_spans.push(CodeSpan {
                    line: line_num,
                    end_line: end_line_idx + 1,
                    start_col: col_start,
                    end_col: col_end,
                    byte_offset: start_pos,
                    byte_end: end_pos,
                    backtick_count,
                    content: span_content,
                });
            }
        }

        // Sort by position to ensure consistent ordering
        code_spans.sort_by_key(|span| span.byte_offset);

        code_spans
    }

    /// Parse all math spans (inline $...$ and display $$...$$) using pulldown-cmark
    fn parse_math_spans(content: &str, lines: &[LineInfo]) -> Vec<MathSpan> {
        let mut math_spans = Vec::new();

        // Quick check - if no $ signs, no math spans
        if !content.contains('$') {
            return math_spans;
        }

        // Use pulldown-cmark with ENABLE_MATH option
        let mut options = Options::empty();
        options.insert(Options::ENABLE_MATH);
        let parser = Parser::new_ext(content, options).into_offset_iter();

        for (event, range) in parser {
            let (is_display, math_content) = match &event {
                Event::InlineMath(text) => (false, text.as_ref()),
                Event::DisplayMath(text) => (true, text.as_ref()),
                _ => continue,
            };

            let start_pos = range.start;
            let end_pos = range.end;

            // Use binary search to find line number - O(log n) instead of O(n)
            let line_idx = lines
                .partition_point(|line| line.byte_offset <= start_pos)
                .saturating_sub(1);
            let line_num = line_idx + 1;
            let byte_col_start = start_pos - lines[line_idx].byte_offset;

            // Find end column using binary search
            let end_line_idx = lines
                .partition_point(|line| line.byte_offset <= end_pos)
                .saturating_sub(1);
            let byte_col_end = end_pos - lines[end_line_idx].byte_offset;

            // Convert byte offsets to character positions for correct Unicode handling
            let line_content = lines[line_idx].content(content);
            let col_start = if byte_col_start <= line_content.len() {
                line_content[..byte_col_start].chars().count()
            } else {
                line_content.chars().count()
            };

            let end_line_content = lines[end_line_idx].content(content);
            let col_end = if byte_col_end <= end_line_content.len() {
                end_line_content[..byte_col_end].chars().count()
            } else {
                end_line_content.chars().count()
            };

            math_spans.push(MathSpan {
                line: line_num,
                end_line: end_line_idx + 1,
                start_col: col_start,
                end_col: col_end,
                byte_offset: start_pos,
                byte_end: end_pos,
                is_display,
                content: math_content.to_string(),
            });
        }

        // Sort by position to ensure consistent ordering
        math_spans.sort_by_key(|span| span.byte_offset);

        math_spans
    }

    /// Parse all list blocks in the content (legacy line-by-line approach)
    ///
    /// Uses a forward-scanning O(n) algorithm that tracks two variables during iteration:
    /// - `has_list_breaking_content_since_last_item`: Set when encountering content that
    ///   terminates a list (headings, horizontal rules, tables, insufficiently indented content)
    /// - `min_continuation_for_tracking`: Minimum indentation required for content to be
    ///   treated as list continuation (based on the list marker width)
    ///
    /// When a new list item is encountered, we check if list-breaking content was seen
    /// since the last item. If so, we start a new list block.
    fn parse_list_blocks(content: &str, lines: &[LineInfo]) -> Vec<ListBlock> {
        // Minimum indentation for unordered list continuation per CommonMark spec
        const UNORDERED_LIST_MIN_CONTINUATION_INDENT: usize = 2;

        /// Initialize or reset the forward-scanning tracking state.
        /// This helper eliminates code duplication across three initialization sites.
        #[inline]
        fn reset_tracking_state(
            list_item: &ListItemInfo,
            has_list_breaking_content: &mut bool,
            min_continuation: &mut usize,
        ) {
            *has_list_breaking_content = false;
            let marker_width = if list_item.is_ordered {
                list_item.marker.len() + 1 // Ordered markers need space after period/paren
            } else {
                list_item.marker.len()
            };
            *min_continuation = if list_item.is_ordered {
                marker_width
            } else {
                UNORDERED_LIST_MIN_CONTINUATION_INDENT
            };
        }

        // Pre-size based on lines that could be list items
        let mut list_blocks = Vec::with_capacity(lines.len() / 10); // Estimate ~10% of lines might start list blocks
        let mut current_block: Option<ListBlock> = None;
        let mut last_list_item_line = 0;
        let mut current_indent_level = 0;
        let mut last_marker_width = 0;

        // Track list-breaking content since last item (fixes O(n²) bottleneck from issue #148)
        let mut has_list_breaking_content_since_last_item = false;
        let mut min_continuation_for_tracking = 0;

        for (line_idx, line_info) in lines.iter().enumerate() {
            let line_num = line_idx + 1;

            // Enhanced code block handling using Design #3's context analysis
            if line_info.in_code_block {
                if let Some(ref mut block) = current_block {
                    // Calculate minimum indentation for list continuation
                    let min_continuation_indent =
                        CodeBlockUtils::calculate_min_continuation_indent(content, lines, line_idx);

                    // Analyze code block context using the three-tier classification
                    let context = CodeBlockUtils::analyze_code_block_context(lines, line_idx, min_continuation_indent);

                    match context {
                        CodeBlockContext::Indented => {
                            // Code block is properly indented - continues the list
                            block.end_line = line_num;
                            continue;
                        }
                        CodeBlockContext::Standalone => {
                            // Code block separates lists - end current block
                            let completed_block = current_block.take().unwrap();
                            list_blocks.push(completed_block);
                            continue;
                        }
                        CodeBlockContext::Adjacent => {
                            // Edge case - use conservative behavior (continue list)
                            block.end_line = line_num;
                            continue;
                        }
                    }
                } else {
                    // No current list block - skip code block lines
                    continue;
                }
            }

            // Extract blockquote prefix if any
            let blockquote_prefix = if let Some(caps) = BLOCKQUOTE_PREFIX_REGEX.captures(line_info.content(content)) {
                caps.get(0).unwrap().as_str().to_string()
            } else {
                String::new()
            };

            // Track list-breaking content for non-list, non-blank lines (O(n) replacement for nested loop)
            // Skip lines that are continuations of multi-line code spans - they're part of the previous list item
            if let Some(ref block) = current_block
                && line_info.list_item.is_none()
                && !line_info.is_blank
                && !line_info.in_code_span_continuation
            {
                let line_content = line_info.content(content).trim();

                // Check for structural separators that break lists
                // Note: Lazy continuation (indent=0) is valid in CommonMark and should NOT break lists.
                // Only lines with indent between 1 and min_continuation_for_tracking-1 break lists,
                // as they indicate improper indentation rather than lazy continuation.
                let is_lazy_continuation = line_info.indent == 0 && !line_info.is_blank;

                // Check if blockquote context changes (different prefix than current block)
                // Lines within the SAME blockquote context don't break lists
                let blockquote_prefix_changes = blockquote_prefix.trim() != block.blockquote_prefix.trim();

                let breaks_list = line_info.heading.is_some()
                    || line_content.starts_with("---")
                    || line_content.starts_with("***")
                    || line_content.starts_with("___")
                    || crate::utils::skip_context::is_table_line(line_content)
                    || blockquote_prefix_changes
                    || (line_info.indent > 0
                        && line_info.indent < min_continuation_for_tracking
                        && !is_lazy_continuation);

                if breaks_list {
                    has_list_breaking_content_since_last_item = true;
                }
            }

            // If this line is a code span continuation within an active list block,
            // extend the block's end_line to include this line (maintains list continuity)
            if line_info.in_code_span_continuation
                && line_info.list_item.is_none()
                && let Some(ref mut block) = current_block
            {
                block.end_line = line_num;
            }

            // Extend block.end_line for regular continuation lines (non-list-item, non-blank,
            // properly indented lines within the list). This ensures the workaround at line 2448
            // works correctly when there are multiple continuation lines before a nested list item.
            // Also include lazy continuation lines (indent=0) per CommonMark spec.
            // For blockquote lines, compute effective indent after stripping the prefix
            let effective_continuation_indent = if let Some(ref block) = current_block {
                let block_bq_level = block.blockquote_prefix.chars().filter(|&c| c == '>').count();
                let line_content = line_info.content(content);
                let line_bq_level = line_content
                    .chars()
                    .take_while(|c| *c == '>' || c.is_whitespace())
                    .filter(|&c| c == '>')
                    .count();
                if line_bq_level > 0 && line_bq_level == block_bq_level {
                    // Compute indent after blockquote markers
                    let mut pos = 0;
                    let mut found_markers = 0;
                    for c in line_content.chars() {
                        pos += c.len_utf8();
                        if c == '>' {
                            found_markers += 1;
                            if found_markers == line_bq_level {
                                if line_content.get(pos..pos + 1) == Some(" ") {
                                    pos += 1;
                                }
                                break;
                            }
                        }
                    }
                    let after_bq = &line_content[pos..];
                    after_bq.len() - after_bq.trim_start().len()
                } else {
                    line_info.indent
                }
            } else {
                line_info.indent
            };
            let adjusted_min_continuation_for_tracking = if let Some(ref block) = current_block {
                let block_bq_level = block.blockquote_prefix.chars().filter(|&c| c == '>').count();
                if block_bq_level > 0 {
                    if block.is_ordered { last_marker_width } else { 2 }
                } else {
                    min_continuation_for_tracking
                }
            } else {
                min_continuation_for_tracking
            };
            let is_valid_continuation = effective_continuation_indent >= adjusted_min_continuation_for_tracking
                || (line_info.indent == 0 && !line_info.is_blank); // Lazy continuation

            if std::env::var("RUMDL_DEBUG_LIST").is_ok() && line_info.list_item.is_none() && !line_info.is_blank {
                eprintln!(
                    "[DEBUG] Line {}: checking continuation - indent={}, min_cont={}, is_valid={}, in_code_span={}, in_code_block={}, has_block={}",
                    line_num,
                    effective_continuation_indent,
                    adjusted_min_continuation_for_tracking,
                    is_valid_continuation,
                    line_info.in_code_span_continuation,
                    line_info.in_code_block,
                    current_block.is_some()
                );
            }

            if !line_info.in_code_span_continuation
                && line_info.list_item.is_none()
                && !line_info.is_blank
                && !line_info.in_code_block
                && is_valid_continuation
                && let Some(ref mut block) = current_block
            {
                if std::env::var("RUMDL_DEBUG_LIST").is_ok() {
                    eprintln!(
                        "[DEBUG] Line {}: extending block.end_line from {} to {}",
                        line_num, block.end_line, line_num
                    );
                }
                block.end_line = line_num;
            }

            // Check if this line is a list item
            if let Some(list_item) = &line_info.list_item {
                // Calculate nesting level based on indentation
                let item_indent = list_item.marker_column;
                let nesting = item_indent / 2; // Assume 2-space indentation for nesting

                if std::env::var("RUMDL_DEBUG_LIST").is_ok() {
                    eprintln!(
                        "[DEBUG] Line {}: list item found, marker={:?}, indent={}",
                        line_num, list_item.marker, item_indent
                    );
                }

                if let Some(ref mut block) = current_block {
                    // Check if this continues the current block
                    // For nested lists, we need to check if this is a nested item (higher nesting level)
                    // or a continuation at the same or lower level
                    let is_nested = nesting > block.nesting_level;
                    let same_type =
                        (block.is_ordered && list_item.is_ordered) || (!block.is_ordered && !list_item.is_ordered);
                    let same_context = block.blockquote_prefix == blockquote_prefix;
                    // Allow one blank line after last item, or lines immediately after block content
                    let reasonable_distance = line_num <= last_list_item_line + 2 || line_num == block.end_line + 1;

                    // For unordered lists, also check marker consistency
                    let marker_compatible =
                        block.is_ordered || block.marker.is_none() || block.marker.as_ref() == Some(&list_item.marker);

                    // O(1) check: Use the tracked variable instead of O(n) nested loop
                    // This eliminates the quadratic bottleneck from issue #148
                    let has_non_list_content = has_list_breaking_content_since_last_item;

                    // A list continues if:
                    // 1. It's a nested item (indented more than the parent), OR
                    // 2. It's the same type at the same level with reasonable distance
                    let mut continues_list = if is_nested {
                        // Nested items always continue the list if they're in the same context
                        same_context && reasonable_distance && !has_non_list_content
                    } else {
                        // Same-level items need to match type and markers
                        same_type && same_context && reasonable_distance && marker_compatible && !has_non_list_content
                    };

                    if std::env::var("RUMDL_DEBUG_LIST").is_ok() {
                        eprintln!(
                            "[DEBUG] Line {}: continues_list={}, is_nested={}, same_type={}, same_context={}, reasonable_distance={}, marker_compatible={}, has_non_list_content={}, last_item={}, block.end_line={}",
                            line_num,
                            continues_list,
                            is_nested,
                            same_type,
                            same_context,
                            reasonable_distance,
                            marker_compatible,
                            has_non_list_content,
                            last_list_item_line,
                            block.end_line
                        );
                    }

                    // WORKAROUND: If items are truly consecutive (no blank lines), they MUST be in the same list
                    // This handles edge cases where content patterns might otherwise split lists incorrectly
                    // Apply for: nested items (different types OK), OR same-level same-type items
                    if !continues_list
                        && (is_nested || same_type)
                        && reasonable_distance
                        && line_num > 0
                        && block.end_line == line_num - 1
                    {
                        // Check if the previous line was a list item or a continuation of a list item
                        // (including lazy continuation lines)
                        if block.item_lines.contains(&(line_num - 1)) {
                            // They're consecutive list items - force them to be in the same list
                            continues_list = true;
                        } else {
                            // Previous line is a continuation line within this block
                            // (e.g., lazy continuation with indent=0)
                            // Since block.end_line == line_num - 1, we know line_num - 1 is part of this block
                            continues_list = true;
                        }
                    }

                    if continues_list {
                        // Extend current block
                        block.end_line = line_num;
                        block.item_lines.push(line_num);

                        // Update max marker width
                        block.max_marker_width = block.max_marker_width.max(if list_item.is_ordered {
                            list_item.marker.len() + 1
                        } else {
                            list_item.marker.len()
                        });

                        // Update marker consistency for unordered lists
                        if !block.is_ordered
                            && block.marker.is_some()
                            && block.marker.as_ref() != Some(&list_item.marker)
                        {
                            // Mixed markers, clear the marker field
                            block.marker = None;
                        }

                        // Reset tracked state for issue #148 optimization
                        reset_tracking_state(
                            list_item,
                            &mut has_list_breaking_content_since_last_item,
                            &mut min_continuation_for_tracking,
                        );
                    } else {
                        // End current block and start a new one
                        // When a different list type starts AT THE SAME LEVEL (not nested),
                        // trim back lazy continuation lines (they become part of the gap, not the list)
                        // For nested items, different types are fine - they're sub-lists
                        if !same_type
                            && !is_nested
                            && let Some(&last_item) = block.item_lines.last()
                        {
                            block.end_line = last_item;
                        }

                        list_blocks.push(block.clone());

                        *block = ListBlock {
                            start_line: line_num,
                            end_line: line_num,
                            is_ordered: list_item.is_ordered,
                            marker: if list_item.is_ordered {
                                None
                            } else {
                                Some(list_item.marker.clone())
                            },
                            blockquote_prefix: blockquote_prefix.clone(),
                            item_lines: vec![line_num],
                            nesting_level: nesting,
                            max_marker_width: if list_item.is_ordered {
                                list_item.marker.len() + 1
                            } else {
                                list_item.marker.len()
                            },
                        };

                        // Initialize tracked state for new block (issue #148 optimization)
                        reset_tracking_state(
                            list_item,
                            &mut has_list_breaking_content_since_last_item,
                            &mut min_continuation_for_tracking,
                        );
                    }
                } else {
                    // Start a new block
                    current_block = Some(ListBlock {
                        start_line: line_num,
                        end_line: line_num,
                        is_ordered: list_item.is_ordered,
                        marker: if list_item.is_ordered {
                            None
                        } else {
                            Some(list_item.marker.clone())
                        },
                        blockquote_prefix,
                        item_lines: vec![line_num],
                        nesting_level: nesting,
                        max_marker_width: list_item.marker.len(),
                    });

                    // Initialize tracked state for new block (issue #148 optimization)
                    reset_tracking_state(
                        list_item,
                        &mut has_list_breaking_content_since_last_item,
                        &mut min_continuation_for_tracking,
                    );
                }

                last_list_item_line = line_num;
                current_indent_level = item_indent;
                last_marker_width = if list_item.is_ordered {
                    list_item.marker.len() + 1 // Add 1 for the space after ordered list markers
                } else {
                    list_item.marker.len()
                };
            } else if let Some(ref mut block) = current_block {
                // Not a list item - check if it continues the current block
                if std::env::var("RUMDL_DEBUG_LIST").is_ok() {
                    eprintln!(
                        "[DEBUG] Line {}: non-list-item, is_blank={}, block exists",
                        line_num, line_info.is_blank
                    );
                }

                // For MD032 compatibility, we use a simple approach:
                // - Indented lines continue the list
                // - Blank lines followed by indented content continue the list
                // - Everything else ends the list

                // Check if the last line in the list block ended with a backslash (hard line break)
                // This handles cases where list items use backslash for hard line breaks
                let prev_line_ends_with_backslash = if block.end_line > 0 && block.end_line - 1 < lines.len() {
                    lines[block.end_line - 1].content(content).trim_end().ends_with('\\')
                } else {
                    false
                };

                // Calculate minimum indentation for list continuation
                // For ordered lists, use the last marker width (e.g., 3 for "1. ", 4 for "10. ")
                // For unordered lists like "- ", content starts at column 2, so continuations need at least 2 spaces
                let min_continuation_indent = if block.is_ordered {
                    current_indent_level + last_marker_width
                } else {
                    current_indent_level + 2 // Unordered lists need at least 2 spaces (e.g., "- " = 2 chars)
                };

                if prev_line_ends_with_backslash || line_info.indent >= min_continuation_indent {
                    // Indented line or backslash continuation continues the list
                    if std::env::var("RUMDL_DEBUG_LIST").is_ok() {
                        eprintln!(
                            "[DEBUG] Line {}: indented continuation (indent={}, min={})",
                            line_num, line_info.indent, min_continuation_indent
                        );
                    }
                    block.end_line = line_num;
                } else if line_info.is_blank {
                    // Blank line - check if it's internal to the list or ending it
                    // We only include blank lines that are followed by more list content
                    if std::env::var("RUMDL_DEBUG_LIST").is_ok() {
                        eprintln!("[DEBUG] Line {line_num}: entering blank line handling");
                    }
                    let mut check_idx = line_idx + 1;
                    let mut found_continuation = false;

                    // Skip additional blank lines
                    while check_idx < lines.len() && lines[check_idx].is_blank {
                        check_idx += 1;
                    }

                    if check_idx < lines.len() {
                        let next_line = &lines[check_idx];
                        // For blockquote lines, compute indent AFTER stripping the blockquote prefix
                        let next_content = next_line.content(content);
                        // Use blockquote level (count of >) to compare, not the full prefix
                        // This avoids issues where the regex captures extra whitespace
                        let block_bq_level_for_indent = block.blockquote_prefix.chars().filter(|&c| c == '>').count();
                        let next_bq_level_for_indent = next_content
                            .chars()
                            .take_while(|c| *c == '>' || c.is_whitespace())
                            .filter(|&c| c == '>')
                            .count();
                        let effective_indent =
                            if next_bq_level_for_indent > 0 && next_bq_level_for_indent == block_bq_level_for_indent {
                                // For lines in the same blockquote context, compute indent after the blockquote marker(s)
                                // Find position after ">" and one space
                                let mut pos = 0;
                                let mut found_markers = 0;
                                for c in next_content.chars() {
                                    pos += c.len_utf8();
                                    if c == '>' {
                                        found_markers += 1;
                                        if found_markers == next_bq_level_for_indent {
                                            // Skip optional space after last >
                                            if next_content.get(pos..pos + 1) == Some(" ") {
                                                pos += 1;
                                            }
                                            break;
                                        }
                                    }
                                }
                                let after_blockquote_marker = &next_content[pos..];
                                after_blockquote_marker.len() - after_blockquote_marker.trim_start().len()
                            } else {
                                next_line.indent
                            };
                        // Also adjust min_continuation_indent for blockquote lists
                        // The marker_column includes blockquote prefix, so subtract it
                        let adjusted_min_continuation = if block_bq_level_for_indent > 0 {
                            // For blockquote lists, the continuation is relative to blockquote content
                            // current_indent_level includes blockquote prefix (2 for "> "), so use just 2 for unordered
                            if block.is_ordered { last_marker_width } else { 2 }
                        } else {
                            min_continuation_indent
                        };
                        // Check if followed by indented content (list continuation)
                        if std::env::var("RUMDL_DEBUG_LIST").is_ok() {
                            eprintln!(
                                "[DEBUG] Blank line {} checking next line {}: effective_indent={}, adjusted_min={}, next_is_list={}, in_code_block={}",
                                line_num,
                                check_idx + 1,
                                effective_indent,
                                adjusted_min_continuation,
                                next_line.list_item.is_some(),
                                next_line.in_code_block
                            );
                        }
                        if !next_line.in_code_block && effective_indent >= adjusted_min_continuation {
                            found_continuation = true;
                        }
                        // Check if followed by another list item at the same level
                        else if !next_line.in_code_block
                            && next_line.list_item.is_some()
                            && let Some(item) = &next_line.list_item
                        {
                            let next_blockquote_prefix = BLOCKQUOTE_PREFIX_REGEX
                                .find(next_line.content(content))
                                .map_or(String::new(), |m| m.as_str().to_string());
                            if item.marker_column == current_indent_level
                                && item.is_ordered == block.is_ordered
                                && block.blockquote_prefix.trim() == next_blockquote_prefix.trim()
                            {
                                // Check if there was meaningful content between the list items (unused now)
                                // This variable is kept for potential future use but is currently replaced by has_structural_separators
                                // Pre-compute block's blockquote level for use in closures
                                let block_bq_level = block.blockquote_prefix.chars().filter(|&c| c == '>').count();
                                let _has_meaningful_content = (line_idx + 1..check_idx).any(|idx| {
                                    if let Some(between_line) = lines.get(idx) {
                                        let between_content = between_line.content(content);
                                        let trimmed = between_content.trim();
                                        // Skip empty lines
                                        if trimmed.is_empty() {
                                            return false;
                                        }
                                        // Check for meaningful content
                                        let line_indent = between_content.len() - between_content.trim_start().len();

                                        // Check if blockquote level changed (not just if line starts with ">")
                                        let between_bq_prefix = BLOCKQUOTE_PREFIX_REGEX
                                            .find(between_content)
                                            .map_or(String::new(), |m| m.as_str().to_string());
                                        let between_bq_level = between_bq_prefix.chars().filter(|&c| c == '>').count();
                                        let blockquote_level_changed =
                                            trimmed.starts_with(">") && between_bq_level != block_bq_level;

                                        // Structural separators (code fences, headings, etc.) are meaningful and should BREAK lists
                                        if trimmed.starts_with("```")
                                            || trimmed.starts_with("~~~")
                                            || trimmed.starts_with("---")
                                            || trimmed.starts_with("***")
                                            || trimmed.starts_with("___")
                                            || blockquote_level_changed
                                            || crate::utils::skip_context::is_table_line(trimmed)
                                            || between_line.heading.is_some()
                                        {
                                            return true; // These are structural separators - meaningful content that breaks lists
                                        }

                                        // Only properly indented content continues the list
                                        line_indent >= min_continuation_indent
                                    } else {
                                        false
                                    }
                                });

                                if block.is_ordered {
                                    // For ordered lists: don't continue if there are structural separators
                                    // Check if there are structural separators between the list items
                                    let has_structural_separators = (line_idx + 1..check_idx).any(|idx| {
                                        if let Some(between_line) = lines.get(idx) {
                                            let between_content = between_line.content(content);
                                            let trimmed = between_content.trim();
                                            if trimmed.is_empty() {
                                                return false;
                                            }
                                            // Check if blockquote level changed (not just if line starts with ">")
                                            let between_bq_prefix = BLOCKQUOTE_PREFIX_REGEX
                                                .find(between_content)
                                                .map_or(String::new(), |m| m.as_str().to_string());
                                            let between_bq_level =
                                                between_bq_prefix.chars().filter(|&c| c == '>').count();
                                            let blockquote_level_changed =
                                                trimmed.starts_with(">") && between_bq_level != block_bq_level;
                                            // Check for structural separators that break lists
                                            trimmed.starts_with("```")
                                                || trimmed.starts_with("~~~")
                                                || trimmed.starts_with("---")
                                                || trimmed.starts_with("***")
                                                || trimmed.starts_with("___")
                                                || blockquote_level_changed
                                                || crate::utils::skip_context::is_table_line(trimmed)
                                                || between_line.heading.is_some()
                                        } else {
                                            false
                                        }
                                    });
                                    found_continuation = !has_structural_separators;
                                } else {
                                    // For unordered lists: also check for structural separators
                                    let has_structural_separators = (line_idx + 1..check_idx).any(|idx| {
                                        if let Some(between_line) = lines.get(idx) {
                                            let between_content = between_line.content(content);
                                            let trimmed = between_content.trim();
                                            if trimmed.is_empty() {
                                                return false;
                                            }
                                            // Check if blockquote level changed (not just if line starts with ">")
                                            let between_bq_prefix = BLOCKQUOTE_PREFIX_REGEX
                                                .find(between_content)
                                                .map_or(String::new(), |m| m.as_str().to_string());
                                            let between_bq_level =
                                                between_bq_prefix.chars().filter(|&c| c == '>').count();
                                            let blockquote_level_changed =
                                                trimmed.starts_with(">") && between_bq_level != block_bq_level;
                                            // Check for structural separators that break lists
                                            trimmed.starts_with("```")
                                                || trimmed.starts_with("~~~")
                                                || trimmed.starts_with("---")
                                                || trimmed.starts_with("***")
                                                || trimmed.starts_with("___")
                                                || blockquote_level_changed
                                                || crate::utils::skip_context::is_table_line(trimmed)
                                                || between_line.heading.is_some()
                                        } else {
                                            false
                                        }
                                    });
                                    found_continuation = !has_structural_separators;
                                }
                            }
                        }
                    }

                    if std::env::var("RUMDL_DEBUG_LIST").is_ok() {
                        eprintln!("[DEBUG] Blank line {line_num} final: found_continuation={found_continuation}");
                    }
                    if found_continuation {
                        // Include the blank line in the block
                        block.end_line = line_num;
                    } else {
                        // Blank line ends the list - don't include it
                        list_blocks.push(block.clone());
                        current_block = None;
                    }
                } else {
                    // Check for lazy continuation - non-indented line immediately after a list item
                    // But only if the line has sufficient indentation for the list type
                    let min_required_indent = if block.is_ordered {
                        current_indent_level + last_marker_width
                    } else {
                        current_indent_level + 2
                    };

                    // For lazy continuation to apply, the line must either:
                    // 1. Have no indentation (true lazy continuation)
                    // 2. Have sufficient indentation for the list type
                    // BUT structural separators (headings, code blocks, etc.) should never be lazy continuations
                    let line_content = line_info.content(content).trim();

                    // Check for table-like patterns
                    let looks_like_table = crate::utils::skip_context::is_table_line(line_content);

                    // Check if blockquote level changed (not just if line starts with ">")
                    // Lines within the same blockquote level are NOT structural separators
                    let block_bq_level = block.blockquote_prefix.chars().filter(|&c| c == '>').count();
                    let current_bq_level = blockquote_prefix.chars().filter(|&c| c == '>').count();
                    let blockquote_level_changed = line_content.starts_with(">") && current_bq_level != block_bq_level;

                    let is_structural_separator = line_info.heading.is_some()
                        || line_content.starts_with("```")
                        || line_content.starts_with("~~~")
                        || line_content.starts_with("---")
                        || line_content.starts_with("***")
                        || line_content.starts_with("___")
                        || blockquote_level_changed
                        || looks_like_table;

                    // Allow lazy continuation if we're still within the same list block
                    // (not just immediately after a list item)
                    // Also treat code span continuations as valid continuations regardless of indent
                    let is_lazy_continuation = !is_structural_separator
                        && !line_info.is_blank
                        && (line_info.indent == 0
                            || line_info.indent >= min_required_indent
                            || line_info.in_code_span_continuation);

                    if is_lazy_continuation {
                        // Per CommonMark, lazy continuation continues until a blank line
                        // or structural element, regardless of uppercase at line start
                        block.end_line = line_num;
                    } else {
                        // Non-indented, non-blank line that's not a lazy continuation - end the block
                        list_blocks.push(block.clone());
                        current_block = None;
                    }
                }
            }
        }

        // Don't forget the last block
        if let Some(block) = current_block {
            list_blocks.push(block);
        }

        // Merge adjacent blocks that should be one
        merge_adjacent_list_blocks(content, &mut list_blocks, lines);

        list_blocks
    }

    /// Compute character frequency for fast content analysis
    fn compute_char_frequency(content: &str) -> CharFrequency {
        let mut frequency = CharFrequency::default();

        for ch in content.chars() {
            match ch {
                '#' => frequency.hash_count += 1,
                '*' => frequency.asterisk_count += 1,
                '_' => frequency.underscore_count += 1,
                '-' => frequency.hyphen_count += 1,
                '+' => frequency.plus_count += 1,
                '>' => frequency.gt_count += 1,
                '|' => frequency.pipe_count += 1,
                '[' => frequency.bracket_count += 1,
                '`' => frequency.backtick_count += 1,
                '<' => frequency.lt_count += 1,
                '!' => frequency.exclamation_count += 1,
                '\n' => frequency.newline_count += 1,
                _ => {}
            }
        }

        frequency
    }

    /// Parse HTML tags in the content
    fn parse_html_tags(
        content: &str,
        lines: &[LineInfo],
        code_blocks: &[(usize, usize)],
        flavor: MarkdownFlavor,
    ) -> Vec<HtmlTag> {
        static HTML_TAG_REGEX: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"(?i)<(/?)([a-zA-Z][a-zA-Z0-9-]*)(?:\s+[^>]*?)?\s*(/?)>").unwrap());

        let mut html_tags = Vec::with_capacity(content.matches('<').count());

        for cap in HTML_TAG_REGEX.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let match_start = full_match.start();
            let match_end = full_match.end();

            // Skip if in code block
            if CodeBlockUtils::is_in_code_block_or_span(code_blocks, match_start) {
                continue;
            }

            let is_closing = !cap.get(1).unwrap().as_str().is_empty();
            let tag_name_original = cap.get(2).unwrap().as_str();
            let tag_name = tag_name_original.to_lowercase();
            let is_self_closing = !cap.get(3).unwrap().as_str().is_empty();

            // Skip JSX components in MDX files (tags starting with uppercase letter)
            // JSX components like <Chart />, <MyComponent> should not be treated as HTML
            if flavor.supports_jsx() && tag_name_original.chars().next().is_some_and(|c| c.is_uppercase()) {
                continue;
            }

            // Find which line this tag is on
            let mut line_num = 1;
            let mut col_start = match_start;
            let mut col_end = match_end;
            for (idx, line_info) in lines.iter().enumerate() {
                if match_start >= line_info.byte_offset {
                    line_num = idx + 1;
                    col_start = match_start - line_info.byte_offset;
                    col_end = match_end - line_info.byte_offset;
                } else {
                    break;
                }
            }

            html_tags.push(HtmlTag {
                line: line_num,
                start_col: col_start,
                end_col: col_end,
                byte_offset: match_start,
                byte_end: match_end,
                tag_name,
                is_closing,
                is_self_closing,
                raw_content: full_match.as_str().to_string(),
            });
        }

        html_tags
    }

    /// Parse table rows in the content
    fn parse_table_rows(content: &str, lines: &[LineInfo]) -> Vec<TableRow> {
        let mut table_rows = Vec::with_capacity(lines.len() / 20);

        for (line_idx, line_info) in lines.iter().enumerate() {
            // Skip lines in code blocks or blank lines
            if line_info.in_code_block || line_info.is_blank {
                continue;
            }

            let line = line_info.content(content);
            let line_num = line_idx + 1;

            // Check if this line contains pipes (potential table row)
            if !line.contains('|') {
                continue;
            }

            // Count columns by splitting on pipes
            let parts: Vec<&str> = line.split('|').collect();
            let column_count = if parts.len() > 2 { parts.len() - 2 } else { parts.len() };

            // Check if this is a separator row
            let is_separator = line.chars().all(|c| "|:-+ \t".contains(c));
            let mut column_alignments = Vec::new();

            if is_separator {
                for part in &parts[1..parts.len() - 1] {
                    // Skip first and last empty parts
                    let trimmed = part.trim();
                    let alignment = if trimmed.starts_with(':') && trimmed.ends_with(':') {
                        "center".to_string()
                    } else if trimmed.ends_with(':') {
                        "right".to_string()
                    } else if trimmed.starts_with(':') {
                        "left".to_string()
                    } else {
                        "none".to_string()
                    };
                    column_alignments.push(alignment);
                }
            }

            table_rows.push(TableRow {
                line: line_num,
                is_separator,
                column_count,
                column_alignments,
            });
        }

        table_rows
    }

    /// Parse bare URLs and emails in the content
    fn parse_bare_urls(content: &str, lines: &[LineInfo], code_blocks: &[(usize, usize)]) -> Vec<BareUrl> {
        let mut bare_urls = Vec::with_capacity(content.matches("http").count() + content.matches('@').count());

        // Check for bare URLs (not in angle brackets or markdown links)
        for cap in URL_SIMPLE_REGEX.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let match_start = full_match.start();
            let match_end = full_match.end();

            // Skip if in code block
            if CodeBlockUtils::is_in_code_block_or_span(code_blocks, match_start) {
                continue;
            }

            // Skip if already in angle brackets or markdown links
            let preceding_char = if match_start > 0 {
                content.chars().nth(match_start - 1)
            } else {
                None
            };
            let following_char = content.chars().nth(match_end);

            if preceding_char == Some('<') || preceding_char == Some('(') || preceding_char == Some('[') {
                continue;
            }
            if following_char == Some('>') || following_char == Some(')') || following_char == Some(']') {
                continue;
            }

            let url = full_match.as_str();
            let url_type = if url.starts_with("https://") {
                "https"
            } else if url.starts_with("http://") {
                "http"
            } else if url.starts_with("ftp://") {
                "ftp"
            } else {
                "other"
            };

            // Find which line this URL is on
            let mut line_num = 1;
            let mut col_start = match_start;
            let mut col_end = match_end;
            for (idx, line_info) in lines.iter().enumerate() {
                if match_start >= line_info.byte_offset {
                    line_num = idx + 1;
                    col_start = match_start - line_info.byte_offset;
                    col_end = match_end - line_info.byte_offset;
                } else {
                    break;
                }
            }

            bare_urls.push(BareUrl {
                line: line_num,
                start_col: col_start,
                end_col: col_end,
                byte_offset: match_start,
                byte_end: match_end,
                url: url.to_string(),
                url_type: url_type.to_string(),
            });
        }

        // Check for bare email addresses
        for cap in BARE_EMAIL_PATTERN.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let match_start = full_match.start();
            let match_end = full_match.end();

            // Skip if in code block
            if CodeBlockUtils::is_in_code_block_or_span(code_blocks, match_start) {
                continue;
            }

            // Skip if already in angle brackets or markdown links
            let preceding_char = if match_start > 0 {
                content.chars().nth(match_start - 1)
            } else {
                None
            };
            let following_char = content.chars().nth(match_end);

            if preceding_char == Some('<') || preceding_char == Some('(') || preceding_char == Some('[') {
                continue;
            }
            if following_char == Some('>') || following_char == Some(')') || following_char == Some(']') {
                continue;
            }

            let email = full_match.as_str();

            // Find which line this email is on
            let mut line_num = 1;
            let mut col_start = match_start;
            let mut col_end = match_end;
            for (idx, line_info) in lines.iter().enumerate() {
                if match_start >= line_info.byte_offset {
                    line_num = idx + 1;
                    col_start = match_start - line_info.byte_offset;
                    col_end = match_end - line_info.byte_offset;
                } else {
                    break;
                }
            }

            bare_urls.push(BareUrl {
                line: line_num,
                start_col: col_start,
                end_col: col_end,
                byte_offset: match_start,
                byte_end: match_end,
                url: email.to_string(),
                url_type: "email".to_string(),
            });
        }

        bare_urls
    }

    /// Get an iterator over valid CommonMark headings
    ///
    /// This iterator filters out malformed headings like `#NoSpace` (hashtag-like patterns)
    /// that should be flagged by MD018 but should not be processed by other heading rules.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rumdl_lib::lint_context::LintContext;
    /// use rumdl_lib::config::MarkdownFlavor;
    ///
    /// let content = "# Valid Heading\n#NoSpace\n## Another Valid";
    /// let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    ///
    /// for heading in ctx.valid_headings() {
    ///     println!("Line {}: {} (level {})", heading.line_num, heading.heading.text, heading.heading.level);
    /// }
    /// // Only prints valid headings, skips `#NoSpace`
    /// ```
    #[must_use]
    pub fn valid_headings(&self) -> ValidHeadingsIter<'_> {
        ValidHeadingsIter::new(&self.lines)
    }

    /// Check if the document contains any valid CommonMark headings
    ///
    /// Returns `true` if there is at least one heading with proper space after `#`.
    #[must_use]
    pub fn has_valid_headings(&self) -> bool {
        self.lines
            .iter()
            .any(|line| line.heading.as_ref().is_some_and(|h| h.is_valid))
    }
}

/// Merge adjacent list blocks that should be treated as one
fn merge_adjacent_list_blocks(content: &str, list_blocks: &mut Vec<ListBlock>, lines: &[LineInfo]) {
    if list_blocks.len() < 2 {
        return;
    }

    let mut merger = ListBlockMerger::new(content, lines);
    *list_blocks = merger.merge(list_blocks);
}

/// Helper struct to manage the complex logic of merging list blocks
struct ListBlockMerger<'a> {
    content: &'a str,
    lines: &'a [LineInfo],
}

impl<'a> ListBlockMerger<'a> {
    fn new(content: &'a str, lines: &'a [LineInfo]) -> Self {
        Self { content, lines }
    }

    fn merge(&mut self, list_blocks: &[ListBlock]) -> Vec<ListBlock> {
        let mut merged = Vec::with_capacity(list_blocks.len());
        let mut current = list_blocks[0].clone();

        for next in list_blocks.iter().skip(1) {
            if self.should_merge_blocks(&current, next) {
                current = self.merge_two_blocks(current, next);
            } else {
                merged.push(current);
                current = next.clone();
            }
        }

        merged.push(current);
        merged
    }

    /// Determine if two adjacent list blocks should be merged
    fn should_merge_blocks(&self, current: &ListBlock, next: &ListBlock) -> bool {
        // Basic compatibility checks
        if !self.blocks_are_compatible(current, next) {
            return false;
        }

        // Check spacing and content between blocks
        let spacing = self.analyze_spacing_between(current, next);
        match spacing {
            BlockSpacing::Consecutive => true,
            BlockSpacing::SingleBlank => self.can_merge_with_blank_between(current, next),
            BlockSpacing::MultipleBlanks | BlockSpacing::ContentBetween => {
                self.can_merge_with_content_between(current, next)
            }
        }
    }

    /// Check if blocks have compatible structure for merging
    fn blocks_are_compatible(&self, current: &ListBlock, next: &ListBlock) -> bool {
        current.is_ordered == next.is_ordered
            && current.blockquote_prefix == next.blockquote_prefix
            && current.nesting_level == next.nesting_level
    }

    /// Analyze the spacing between two list blocks
    fn analyze_spacing_between(&self, current: &ListBlock, next: &ListBlock) -> BlockSpacing {
        let gap = next.start_line - current.end_line;

        match gap {
            1 => BlockSpacing::Consecutive,
            2 => BlockSpacing::SingleBlank,
            _ if gap > 2 => {
                if self.has_only_blank_lines_between(current, next) {
                    BlockSpacing::MultipleBlanks
                } else {
                    BlockSpacing::ContentBetween
                }
            }
            _ => BlockSpacing::Consecutive, // gap == 0, overlapping (shouldn't happen)
        }
    }

    /// Check if unordered lists can be merged with a single blank line between
    fn can_merge_with_blank_between(&self, current: &ListBlock, next: &ListBlock) -> bool {
        // Check if there are structural separators between the blocks
        // If has_meaningful_content_between returns true, it means there are structural separators
        if has_meaningful_content_between(self.content, current, next, self.lines) {
            return false; // Structural separators prevent merging
        }

        // Only merge unordered lists with same marker across single blank
        !current.is_ordered && current.marker == next.marker
    }

    /// Check if ordered lists can be merged when there's content between them
    fn can_merge_with_content_between(&self, current: &ListBlock, next: &ListBlock) -> bool {
        // Do not merge lists if there are structural separators between them
        if has_meaningful_content_between(self.content, current, next, self.lines) {
            return false; // Structural separators prevent merging
        }

        // Only consider merging ordered lists if there's no structural content between
        current.is_ordered && next.is_ordered
    }

    /// Check if there are only blank lines between blocks
    fn has_only_blank_lines_between(&self, current: &ListBlock, next: &ListBlock) -> bool {
        for line_num in (current.end_line + 1)..next.start_line {
            if let Some(line_info) = self.lines.get(line_num - 1)
                && !line_info.content(self.content).trim().is_empty()
            {
                return false;
            }
        }
        true
    }

    /// Merge two compatible list blocks into one
    fn merge_two_blocks(&self, mut current: ListBlock, next: &ListBlock) -> ListBlock {
        current.end_line = next.end_line;
        current.item_lines.extend_from_slice(&next.item_lines);

        // Update max marker width
        current.max_marker_width = current.max_marker_width.max(next.max_marker_width);

        // Handle marker consistency for unordered lists
        if !current.is_ordered && self.markers_differ(&current, next) {
            current.marker = None; // Mixed markers
        }

        current
    }

    /// Check if two blocks have different markers
    fn markers_differ(&self, current: &ListBlock, next: &ListBlock) -> bool {
        current.marker.is_some() && next.marker.is_some() && current.marker != next.marker
    }
}

/// Types of spacing between list blocks
#[derive(Debug, PartialEq)]
enum BlockSpacing {
    Consecutive,    // No gap between blocks
    SingleBlank,    // One blank line between blocks
    MultipleBlanks, // Multiple blank lines but no content
    ContentBetween, // Content exists between blocks
}

/// Check if there's meaningful content (not just blank lines) between two list blocks
fn has_meaningful_content_between(content: &str, current: &ListBlock, next: &ListBlock, lines: &[LineInfo]) -> bool {
    // Check lines between current.end_line and next.start_line
    for line_num in (current.end_line + 1)..next.start_line {
        if let Some(line_info) = lines.get(line_num - 1) {
            // Convert to 0-indexed
            let trimmed = line_info.content(content).trim();

            // Skip empty lines
            if trimmed.is_empty() {
                continue;
            }

            // Check for structural separators that should separate lists (CommonMark compliant)

            // Headings separate lists
            if line_info.heading.is_some() {
                return true; // Has meaningful content - headings separate lists
            }

            // Horizontal rules separate lists (---, ***, ___)
            if is_horizontal_rule(trimmed) {
                return true; // Has meaningful content - horizontal rules separate lists
            }

            // Tables separate lists
            if crate::utils::skip_context::is_table_line(trimmed) {
                return true; // Has meaningful content - tables separate lists
            }

            // Blockquotes separate lists
            if trimmed.starts_with('>') {
                return true; // Has meaningful content - blockquotes separate lists
            }

            // Code block fences separate lists (unless properly indented as list content)
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let line_indent = line_info.byte_len - line_info.content(content).trim_start().len();

                // Check if this code block is properly indented as list continuation
                let min_continuation_indent = if current.is_ordered {
                    current.nesting_level + current.max_marker_width + 1 // +1 for space after marker
                } else {
                    current.nesting_level + 2
                };

                if line_indent < min_continuation_indent {
                    // This is a standalone code block that separates lists
                    return true; // Has meaningful content - standalone code blocks separate lists
                }
            }

            // Check if this line has proper indentation for list continuation
            let line_indent = line_info.byte_len - line_info.content(content).trim_start().len();

            // Calculate minimum indentation needed to be list continuation
            let min_indent = if current.is_ordered {
                current.nesting_level + current.max_marker_width
            } else {
                current.nesting_level + 2
            };

            // If the line is not indented enough to be list continuation, it's meaningful content
            if line_indent < min_indent {
                return true; // Has meaningful content - content not indented as list continuation
            }

            // If we reach here, the line is properly indented as list continuation
            // Continue checking other lines
        }
    }

    // Only blank lines or properly indented list continuation content between blocks
    false
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

    // Check for three or more consecutive -, *, or _ characters (with optional spaces)
    let chars: Vec<char> = trimmed.chars().collect();
    if let Some(&first_char) = chars.first()
        && (first_char == '-' || first_char == '*' || first_char == '_')
    {
        let mut count = 0;
        for &ch in &chars {
            if ch == first_char {
                count += 1;
            } else if ch != ' ' && ch != '\t' {
                return false; // Non-matching, non-whitespace character
            }
        }
        return count >= 3;
    }
    false
}

/// Backwards-compatible alias for `is_horizontal_rule_content`
pub fn is_horizontal_rule(trimmed: &str) -> bool {
    is_horizontal_rule_content(trimmed)
}

/// Check if content contains patterns that cause the markdown crate to panic
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_content() {
        let ctx = LintContext::new("", MarkdownFlavor::Standard, None);
        assert_eq!(ctx.content, "");
        assert_eq!(ctx.line_offsets, vec![0]);
        assert_eq!(ctx.offset_to_line_col(0), (1, 1));
        assert_eq!(ctx.lines.len(), 0);
    }

    #[test]
    fn test_single_line() {
        let ctx = LintContext::new("# Hello", MarkdownFlavor::Standard, None);
        assert_eq!(ctx.content, "# Hello");
        assert_eq!(ctx.line_offsets, vec![0]);
        assert_eq!(ctx.offset_to_line_col(0), (1, 1));
        assert_eq!(ctx.offset_to_line_col(3), (1, 4));
    }

    #[test]
    fn test_multi_line() {
        let content = "# Title\n\nSecond line\nThird line";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert_eq!(ctx.line_offsets, vec![0, 8, 9, 21]);
        // Test offset to line/col
        assert_eq!(ctx.offset_to_line_col(0), (1, 1)); // start
        assert_eq!(ctx.offset_to_line_col(8), (2, 1)); // start of blank line
        assert_eq!(ctx.offset_to_line_col(9), (3, 1)); // start of 'Second line'
        assert_eq!(ctx.offset_to_line_col(15), (3, 7)); // middle of 'Second line'
        assert_eq!(ctx.offset_to_line_col(21), (4, 1)); // start of 'Third line'
    }

    #[test]
    fn test_line_info() {
        let content = "# Title\n    indented\n\ncode:\n```rust\nfn main() {}\n```";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Test line info
        assert_eq!(ctx.lines.len(), 7);

        // Line 1: "# Title"
        let line1 = &ctx.lines[0];
        assert_eq!(line1.content(ctx.content), "# Title");
        assert_eq!(line1.byte_offset, 0);
        assert_eq!(line1.indent, 0);
        assert!(!line1.is_blank);
        assert!(!line1.in_code_block);
        assert!(line1.list_item.is_none());

        // Line 2: "    indented"
        let line2 = &ctx.lines[1];
        assert_eq!(line2.content(ctx.content), "    indented");
        assert_eq!(line2.byte_offset, 8);
        assert_eq!(line2.indent, 4);
        assert!(!line2.is_blank);

        // Line 3: "" (blank)
        let line3 = &ctx.lines[2];
        assert_eq!(line3.content(ctx.content), "");
        assert!(line3.is_blank);

        // Test helper methods
        assert_eq!(ctx.line_to_byte_offset(1), Some(0));
        assert_eq!(ctx.line_to_byte_offset(2), Some(8));
        assert_eq!(ctx.line_info(1).map(|l| l.indent), Some(0));
        assert_eq!(ctx.line_info(2).map(|l| l.indent), Some(4));
    }

    #[test]
    fn test_list_item_detection() {
        let content = "- Unordered item\n  * Nested item\n1. Ordered item\n   2) Nested ordered\n\nNot a list";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Line 1: "- Unordered item"
        let line1 = &ctx.lines[0];
        assert!(line1.list_item.is_some());
        let list1 = line1.list_item.as_ref().unwrap();
        assert_eq!(list1.marker, "-");
        assert!(!list1.is_ordered);
        assert_eq!(list1.marker_column, 0);
        assert_eq!(list1.content_column, 2);

        // Line 2: "  * Nested item"
        let line2 = &ctx.lines[1];
        assert!(line2.list_item.is_some());
        let list2 = line2.list_item.as_ref().unwrap();
        assert_eq!(list2.marker, "*");
        assert_eq!(list2.marker_column, 2);

        // Line 3: "1. Ordered item"
        let line3 = &ctx.lines[2];
        assert!(line3.list_item.is_some());
        let list3 = line3.list_item.as_ref().unwrap();
        assert_eq!(list3.marker, "1.");
        assert!(list3.is_ordered);
        assert_eq!(list3.number, Some(1));

        // Line 6: "Not a list"
        let line6 = &ctx.lines[5];
        assert!(line6.list_item.is_none());
    }

    #[test]
    fn test_offset_to_line_col_edge_cases() {
        let content = "a\nb\nc";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        // line_offsets: [0, 2, 4]
        assert_eq!(ctx.offset_to_line_col(0), (1, 1)); // 'a'
        assert_eq!(ctx.offset_to_line_col(1), (1, 2)); // after 'a'
        assert_eq!(ctx.offset_to_line_col(2), (2, 1)); // 'b'
        assert_eq!(ctx.offset_to_line_col(3), (2, 2)); // after 'b'
        assert_eq!(ctx.offset_to_line_col(4), (3, 1)); // 'c'
        assert_eq!(ctx.offset_to_line_col(5), (3, 2)); // after 'c'
    }

    #[test]
    fn test_mdx_esm_blocks() {
        let content = r##"import {Chart} from './snowfall.js'
export const year = 2023

# Last year's snowfall

In {year}, the snowfall was above average.
It was followed by a warm spring which caused
flood conditions in many of the nearby rivers.

<Chart color="#fcb32c" year={year} />
"##;

        let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

        // Check that lines 1 and 2 are marked as ESM blocks
        assert_eq!(ctx.lines.len(), 10);
        assert!(ctx.lines[0].in_esm_block, "Line 1 (import) should be in_esm_block");
        assert!(ctx.lines[1].in_esm_block, "Line 2 (export) should be in_esm_block");
        assert!(!ctx.lines[2].in_esm_block, "Line 3 (blank) should NOT be in_esm_block");
        assert!(
            !ctx.lines[3].in_esm_block,
            "Line 4 (heading) should NOT be in_esm_block"
        );
        assert!(!ctx.lines[4].in_esm_block, "Line 5 (blank) should NOT be in_esm_block");
        assert!(!ctx.lines[5].in_esm_block, "Line 6 (text) should NOT be in_esm_block");
    }

    #[test]
    fn test_mdx_esm_blocks_not_detected_in_standard_flavor() {
        let content = r#"import {Chart} from './snowfall.js'
export const year = 2023

# Last year's snowfall
"#;

        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // ESM blocks should NOT be detected in Standard flavor
        assert!(
            !ctx.lines[0].in_esm_block,
            "Line 1 should NOT be in_esm_block in Standard flavor"
        );
        assert!(
            !ctx.lines[1].in_esm_block,
            "Line 2 should NOT be in_esm_block in Standard flavor"
        );
    }

    #[test]
    fn test_blockquote_with_indented_content() {
        // Lines with `>` followed by heavily-indented content should be detected as blockquotes.
        // The content inside the blockquote may also be detected as a code block (which is correct),
        // but for MD046 purposes, we need to know the line is inside a blockquote.
        let content = r#"# Heading

>      -S socket-path
>                    More text
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Line 3 (index 2) should be detected as blockquote
        assert!(
            ctx.lines.get(2).is_some_and(|l| l.blockquote.is_some()),
            "Line 3 should be a blockquote"
        );
        // Line 4 (index 3) should also be blockquote
        assert!(
            ctx.lines.get(3).is_some_and(|l| l.blockquote.is_some()),
            "Line 4 should be a blockquote"
        );

        // Verify blockquote content is correctly parsed
        // Note: spaces_after includes the spaces between `>` and content
        let bq3 = ctx.lines.get(2).unwrap().blockquote.as_ref().unwrap();
        assert_eq!(bq3.content, "-S socket-path");
        assert_eq!(bq3.nesting_level, 1);
        // 6 spaces after the `>` marker
        assert!(bq3.has_multiple_spaces_after_marker);

        let bq4 = ctx.lines.get(3).unwrap().blockquote.as_ref().unwrap();
        assert_eq!(bq4.content, "More text");
        assert_eq!(bq4.nesting_level, 1);
    }

    #[test]
    fn test_footnote_definitions_not_parsed_as_reference_defs() {
        // Footnote definitions use [^id]: syntax and should NOT be parsed as reference definitions
        let content = r#"# Title

A footnote[^1].

[^1]: This is the footnote content.

[^note]: Another footnote with [link](https://example.com).

[regular]: ./path.md "A real reference definition"
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Should only have one reference definition (the regular one)
        assert_eq!(
            ctx.reference_defs.len(),
            1,
            "Footnotes should not be parsed as reference definitions"
        );

        // The only reference def should be the regular one
        assert_eq!(ctx.reference_defs[0].id, "regular");
        assert_eq!(ctx.reference_defs[0].url, "./path.md");
        assert_eq!(
            ctx.reference_defs[0].title,
            Some("A real reference definition".to_string())
        );
    }

    #[test]
    fn test_footnote_with_inline_link_not_misidentified() {
        // Regression test for issue #286: footnote containing an inline link
        // was incorrectly parsed as a reference definition with URL "[link](url)"
        let content = r#"# Title

A footnote[^1].

[^1]: [link](https://www.google.com).
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Should have no reference definitions
        assert!(
            ctx.reference_defs.is_empty(),
            "Footnote with inline link should not create a reference definition"
        );
    }

    #[test]
    fn test_various_footnote_formats_excluded() {
        // Test various footnote ID formats are all excluded
        let content = r#"[^1]: Numeric footnote
[^note]: Named footnote
[^a]: Single char footnote
[^long-footnote-name]: Long named footnote
[^123abc]: Mixed alphanumeric

[ref1]: ./file1.md
[ref2]: ./file2.md
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Should only have the two regular reference definitions
        assert_eq!(
            ctx.reference_defs.len(),
            2,
            "Only regular reference definitions should be parsed"
        );

        let ids: Vec<&str> = ctx.reference_defs.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"ref1"));
        assert!(ids.contains(&"ref2"));
        assert!(!ids.iter().any(|id| id.starts_with('^')));
    }

    // =========================================================================
    // Tests for has_char and char_count methods
    // =========================================================================

    #[test]
    fn test_has_char_tracked_characters() {
        // Test all 12 tracked characters
        let content = "# Heading\n* list item\n_emphasis_ and -hyphen-\n+ plus\n> quote\n| table |\n[link]\n`code`\n<html>\n!image";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // All tracked characters should be detected
        assert!(ctx.has_char('#'), "Should detect hash");
        assert!(ctx.has_char('*'), "Should detect asterisk");
        assert!(ctx.has_char('_'), "Should detect underscore");
        assert!(ctx.has_char('-'), "Should detect hyphen");
        assert!(ctx.has_char('+'), "Should detect plus");
        assert!(ctx.has_char('>'), "Should detect gt");
        assert!(ctx.has_char('|'), "Should detect pipe");
        assert!(ctx.has_char('['), "Should detect bracket");
        assert!(ctx.has_char('`'), "Should detect backtick");
        assert!(ctx.has_char('<'), "Should detect lt");
        assert!(ctx.has_char('!'), "Should detect exclamation");
        assert!(ctx.has_char('\n'), "Should detect newline");
    }

    #[test]
    fn test_has_char_absent_characters() {
        let content = "Simple text without special chars";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // None of the tracked characters should be present
        assert!(!ctx.has_char('#'), "Should not detect hash");
        assert!(!ctx.has_char('*'), "Should not detect asterisk");
        assert!(!ctx.has_char('_'), "Should not detect underscore");
        assert!(!ctx.has_char('-'), "Should not detect hyphen");
        assert!(!ctx.has_char('+'), "Should not detect plus");
        assert!(!ctx.has_char('>'), "Should not detect gt");
        assert!(!ctx.has_char('|'), "Should not detect pipe");
        assert!(!ctx.has_char('['), "Should not detect bracket");
        assert!(!ctx.has_char('`'), "Should not detect backtick");
        assert!(!ctx.has_char('<'), "Should not detect lt");
        assert!(!ctx.has_char('!'), "Should not detect exclamation");
        // Note: single line content has no newlines
        assert!(!ctx.has_char('\n'), "Should not detect newline in single line");
    }

    #[test]
    fn test_has_char_fallback_for_untracked() {
        let content = "Text with @mention and $dollar and %percent";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Untracked characters should fall back to content.contains()
        assert!(ctx.has_char('@'), "Should detect @ via fallback");
        assert!(ctx.has_char('$'), "Should detect $ via fallback");
        assert!(ctx.has_char('%'), "Should detect % via fallback");
        assert!(!ctx.has_char('^'), "Should not detect absent ^ via fallback");
    }

    #[test]
    fn test_char_count_tracked_characters() {
        let content = "## Heading ##\n***bold***\n__emphasis__\n---\n+++\n>> nested\n|| table ||\n[[link]]\n``code``\n<<html>>\n!!";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Count each tracked character
        assert_eq!(ctx.char_count('#'), 4, "Should count 4 hashes");
        assert_eq!(ctx.char_count('*'), 6, "Should count 6 asterisks");
        assert_eq!(ctx.char_count('_'), 4, "Should count 4 underscores");
        assert_eq!(ctx.char_count('-'), 3, "Should count 3 hyphens");
        assert_eq!(ctx.char_count('+'), 3, "Should count 3 pluses");
        assert_eq!(ctx.char_count('>'), 4, "Should count 4 gt (2 nested + 2 in <<html>>)");
        assert_eq!(ctx.char_count('|'), 4, "Should count 4 pipes");
        assert_eq!(ctx.char_count('['), 2, "Should count 2 brackets");
        assert_eq!(ctx.char_count('`'), 4, "Should count 4 backticks");
        assert_eq!(ctx.char_count('<'), 2, "Should count 2 lt");
        assert_eq!(ctx.char_count('!'), 2, "Should count 2 exclamations");
        assert_eq!(ctx.char_count('\n'), 10, "Should count 10 newlines");
    }

    #[test]
    fn test_char_count_zero_for_absent() {
        let content = "Plain text";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        assert_eq!(ctx.char_count('#'), 0);
        assert_eq!(ctx.char_count('*'), 0);
        assert_eq!(ctx.char_count('_'), 0);
        assert_eq!(ctx.char_count('\n'), 0);
    }

    #[test]
    fn test_char_count_fallback_for_untracked() {
        let content = "@@@ $$ %%%";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        assert_eq!(ctx.char_count('@'), 3, "Should count 3 @ via fallback");
        assert_eq!(ctx.char_count('$'), 2, "Should count 2 $ via fallback");
        assert_eq!(ctx.char_count('%'), 3, "Should count 3 % via fallback");
        assert_eq!(ctx.char_count('^'), 0, "Should count 0 for absent char");
    }

    #[test]
    fn test_char_count_empty_content() {
        let ctx = LintContext::new("", MarkdownFlavor::Standard, None);

        assert_eq!(ctx.char_count('#'), 0);
        assert_eq!(ctx.char_count('*'), 0);
        assert_eq!(ctx.char_count('@'), 0);
        assert!(!ctx.has_char('#'));
        assert!(!ctx.has_char('@'));
    }

    // =========================================================================
    // Tests for is_in_html_tag method
    // =========================================================================

    #[test]
    fn test_is_in_html_tag_simple() {
        let content = "<div>content</div>";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Inside opening tag
        assert!(ctx.is_in_html_tag(0), "Position 0 (<) should be in tag");
        assert!(ctx.is_in_html_tag(1), "Position 1 (d) should be in tag");
        assert!(ctx.is_in_html_tag(4), "Position 4 (>) should be in tag");

        // Outside tag (in content)
        assert!(!ctx.is_in_html_tag(5), "Position 5 (c) should not be in tag");
        assert!(!ctx.is_in_html_tag(10), "Position 10 (t) should not be in tag");

        // Inside closing tag
        assert!(ctx.is_in_html_tag(12), "Position 12 (<) should be in tag");
        assert!(ctx.is_in_html_tag(17), "Position 17 (>) should be in tag");
    }

    #[test]
    fn test_is_in_html_tag_self_closing() {
        let content = "Text <br/> more text";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Before tag
        assert!(!ctx.is_in_html_tag(0), "Position 0 should not be in tag");
        assert!(!ctx.is_in_html_tag(4), "Position 4 (space) should not be in tag");

        // Inside self-closing tag
        assert!(ctx.is_in_html_tag(5), "Position 5 (<) should be in tag");
        assert!(ctx.is_in_html_tag(8), "Position 8 (/) should be in tag");
        assert!(ctx.is_in_html_tag(9), "Position 9 (>) should be in tag");

        // After tag
        assert!(!ctx.is_in_html_tag(10), "Position 10 (space) should not be in tag");
    }

    #[test]
    fn test_is_in_html_tag_with_attributes() {
        let content = r#"<a href="url" class="link">text</a>"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // All positions inside opening tag with attributes
        assert!(ctx.is_in_html_tag(0), "Start of tag");
        assert!(ctx.is_in_html_tag(10), "Inside href attribute");
        assert!(ctx.is_in_html_tag(20), "Inside class attribute");
        assert!(ctx.is_in_html_tag(26), "End of opening tag");

        // Content between tags
        assert!(!ctx.is_in_html_tag(27), "Start of content");
        assert!(!ctx.is_in_html_tag(30), "End of content");

        // Closing tag
        assert!(ctx.is_in_html_tag(31), "Start of closing tag");
    }

    #[test]
    fn test_is_in_html_tag_multiline() {
        let content = "<div\n  class=\"test\"\n>\ncontent\n</div>";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Opening tag spans multiple lines
        assert!(ctx.is_in_html_tag(0), "Start of multiline tag");
        assert!(ctx.is_in_html_tag(5), "After first newline in tag");
        assert!(ctx.is_in_html_tag(15), "Inside attribute");

        // After closing > of opening tag
        let closing_bracket_pos = content.find(">\n").unwrap();
        assert!(!ctx.is_in_html_tag(closing_bracket_pos + 2), "Content after tag");
    }

    #[test]
    fn test_is_in_html_tag_no_tags() {
        let content = "Plain text without any HTML";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // No position should be in an HTML tag
        for i in 0..content.len() {
            assert!(!ctx.is_in_html_tag(i), "Position {i} should not be in tag");
        }
    }

    // =========================================================================
    // Tests for is_in_jinja_range method
    // =========================================================================

    #[test]
    fn test_is_in_jinja_range_expression() {
        let content = "Hello {{ name }}!";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Before Jinja
        assert!(!ctx.is_in_jinja_range(0), "H should not be in Jinja");
        assert!(!ctx.is_in_jinja_range(5), "Space before Jinja should not be in Jinja");

        // Inside Jinja expression (positions 6-15 for "{{ name }}")
        assert!(ctx.is_in_jinja_range(6), "First brace should be in Jinja");
        assert!(ctx.is_in_jinja_range(7), "Second brace should be in Jinja");
        assert!(ctx.is_in_jinja_range(10), "name should be in Jinja");
        assert!(ctx.is_in_jinja_range(14), "Closing brace should be in Jinja");
        assert!(ctx.is_in_jinja_range(15), "Second closing brace should be in Jinja");

        // After Jinja
        assert!(!ctx.is_in_jinja_range(16), "! should not be in Jinja");
    }

    #[test]
    fn test_is_in_jinja_range_statement() {
        let content = "{% if condition %}content{% endif %}";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Inside opening statement
        assert!(ctx.is_in_jinja_range(0), "Start of Jinja statement");
        assert!(ctx.is_in_jinja_range(5), "condition should be in Jinja");
        assert!(ctx.is_in_jinja_range(17), "End of opening statement");

        // Content between
        assert!(!ctx.is_in_jinja_range(18), "content should not be in Jinja");

        // Inside closing statement
        assert!(ctx.is_in_jinja_range(25), "Start of endif");
        assert!(ctx.is_in_jinja_range(32), "endif should be in Jinja");
    }

    #[test]
    fn test_is_in_jinja_range_multiple() {
        let content = "{{ a }} and {{ b }}";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // First Jinja expression
        assert!(ctx.is_in_jinja_range(0));
        assert!(ctx.is_in_jinja_range(3));
        assert!(ctx.is_in_jinja_range(6));

        // Between expressions
        assert!(!ctx.is_in_jinja_range(8));
        assert!(!ctx.is_in_jinja_range(11));

        // Second Jinja expression
        assert!(ctx.is_in_jinja_range(12));
        assert!(ctx.is_in_jinja_range(15));
        assert!(ctx.is_in_jinja_range(18));
    }

    #[test]
    fn test_is_in_jinja_range_no_jinja() {
        let content = "Plain text with single braces but not Jinja";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // No position should be in Jinja
        for i in 0..content.len() {
            assert!(!ctx.is_in_jinja_range(i), "Position {i} should not be in Jinja");
        }
    }

    // =========================================================================
    // Tests for is_in_link_title method
    // =========================================================================

    #[test]
    fn test_is_in_link_title_with_title() {
        let content = r#"[ref]: https://example.com "Title text"

Some content."#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Verify we have a reference def with title
        assert_eq!(ctx.reference_defs.len(), 1);
        let def = &ctx.reference_defs[0];
        assert!(def.title_byte_start.is_some());
        assert!(def.title_byte_end.is_some());

        let title_start = def.title_byte_start.unwrap();
        let title_end = def.title_byte_end.unwrap();

        // Before title (in URL)
        assert!(!ctx.is_in_link_title(10), "URL should not be in title");

        // Inside title
        assert!(ctx.is_in_link_title(title_start), "Title start should be in title");
        assert!(
            ctx.is_in_link_title(title_start + 5),
            "Middle of title should be in title"
        );
        assert!(ctx.is_in_link_title(title_end - 1), "End of title should be in title");

        // After title
        assert!(
            !ctx.is_in_link_title(title_end),
            "After title end should not be in title"
        );
    }

    #[test]
    fn test_is_in_link_title_without_title() {
        let content = "[ref]: https://example.com\n\nSome content.";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Reference def without title
        assert_eq!(ctx.reference_defs.len(), 1);
        let def = &ctx.reference_defs[0];
        assert!(def.title_byte_start.is_none());
        assert!(def.title_byte_end.is_none());

        // No position should be in a title
        for i in 0..content.len() {
            assert!(!ctx.is_in_link_title(i), "Position {i} should not be in title");
        }
    }

    #[test]
    fn test_is_in_link_title_multiple_refs() {
        let content = r#"[ref1]: /url1 "Title One"
[ref2]: /url2
[ref3]: /url3 "Title Three"
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Should have 3 reference defs
        assert_eq!(ctx.reference_defs.len(), 3);

        // ref1 has title
        let ref1 = ctx.reference_defs.iter().find(|r| r.id == "ref1").unwrap();
        assert!(ref1.title_byte_start.is_some());

        // ref2 has no title
        let ref2 = ctx.reference_defs.iter().find(|r| r.id == "ref2").unwrap();
        assert!(ref2.title_byte_start.is_none());

        // ref3 has title
        let ref3 = ctx.reference_defs.iter().find(|r| r.id == "ref3").unwrap();
        assert!(ref3.title_byte_start.is_some());

        // Check positions in ref1's title
        if let (Some(start), Some(end)) = (ref1.title_byte_start, ref1.title_byte_end) {
            assert!(ctx.is_in_link_title(start + 1));
            assert!(!ctx.is_in_link_title(end + 5));
        }

        // Check positions in ref3's title
        if let (Some(start), Some(_end)) = (ref3.title_byte_start, ref3.title_byte_end) {
            assert!(ctx.is_in_link_title(start + 1));
        }
    }

    #[test]
    fn test_is_in_link_title_single_quotes() {
        let content = "[ref]: /url 'Single quoted title'\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        assert_eq!(ctx.reference_defs.len(), 1);
        let def = &ctx.reference_defs[0];

        if let (Some(start), Some(end)) = (def.title_byte_start, def.title_byte_end) {
            assert!(ctx.is_in_link_title(start));
            assert!(ctx.is_in_link_title(start + 5));
            assert!(!ctx.is_in_link_title(end));
        }
    }

    #[test]
    fn test_is_in_link_title_parentheses() {
        // Note: The reference def parser may not support parenthesized titles
        // This test verifies the is_in_link_title method works when titles exist
        let content = "[ref]: /url (Parenthesized title)\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Parser behavior: may or may not parse parenthesized titles
        // We test that is_in_link_title correctly reflects whatever was parsed
        if ctx.reference_defs.is_empty() {
            // Parser didn't recognize this as a reference def
            for i in 0..content.len() {
                assert!(!ctx.is_in_link_title(i));
            }
        } else {
            let def = &ctx.reference_defs[0];
            if let (Some(start), Some(end)) = (def.title_byte_start, def.title_byte_end) {
                assert!(ctx.is_in_link_title(start));
                assert!(ctx.is_in_link_title(start + 5));
                assert!(!ctx.is_in_link_title(end));
            } else {
                // Title wasn't parsed, so no position should be in title
                for i in 0..content.len() {
                    assert!(!ctx.is_in_link_title(i));
                }
            }
        }
    }

    #[test]
    fn test_is_in_link_title_no_refs() {
        let content = "Just plain text without any reference definitions.";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        assert!(ctx.reference_defs.is_empty());

        for i in 0..content.len() {
            assert!(!ctx.is_in_link_title(i));
        }
    }

    // =========================================================================
    // Math span tests (Issue #289)
    // =========================================================================

    #[test]
    fn test_math_spans_inline() {
        let content = "Text with inline math $[f](x)$ in it.";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let math_spans = ctx.math_spans();
        assert_eq!(math_spans.len(), 1, "Should detect one inline math span");

        let span = &math_spans[0];
        assert!(!span.is_display, "Should be inline math, not display");
        assert_eq!(span.content, "[f](x)", "Content should be extracted correctly");
    }

    #[test]
    fn test_math_spans_display_single_line() {
        let content = "$$X(\\zeta) = \\mathcal Z [x](\\zeta)$$";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let math_spans = ctx.math_spans();
        assert_eq!(math_spans.len(), 1, "Should detect one display math span");

        let span = &math_spans[0];
        assert!(span.is_display, "Should be display math");
        assert!(
            span.content.contains("[x](\\zeta)"),
            "Content should contain the link-like pattern"
        );
    }

    #[test]
    fn test_math_spans_display_multiline() {
        let content = "Before\n\n$$\n[x](\\zeta) = \\sum_k x(k)\n$$\n\nAfter";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let math_spans = ctx.math_spans();
        assert_eq!(math_spans.len(), 1, "Should detect one display math span");

        let span = &math_spans[0];
        assert!(span.is_display, "Should be display math");
    }

    #[test]
    fn test_is_in_math_span() {
        let content = "Text $[f](x)$ more text";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Position inside the math span
        let math_start = content.find('$').unwrap();
        let math_end = content.rfind('$').unwrap() + 1;

        assert!(
            ctx.is_in_math_span(math_start + 1),
            "Position inside math span should return true"
        );
        assert!(
            ctx.is_in_math_span(math_start + 3),
            "Position inside math span should return true"
        );

        // Position outside the math span
        assert!(!ctx.is_in_math_span(0), "Position before math span should return false");
        assert!(
            !ctx.is_in_math_span(math_end + 1),
            "Position after math span should return false"
        );
    }

    #[test]
    fn test_math_spans_mixed_with_code() {
        let content = "Math $[f](x)$ and code `[g](y)` mixed";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let math_spans = ctx.math_spans();
        let code_spans = ctx.code_spans();

        assert_eq!(math_spans.len(), 1, "Should have one math span");
        assert_eq!(code_spans.len(), 1, "Should have one code span");

        // Verify math span content
        assert_eq!(math_spans[0].content, "[f](x)");
        // Verify code span content
        assert_eq!(code_spans[0].content, "[g](y)");
    }

    #[test]
    fn test_math_spans_no_math() {
        let content = "Regular text without any math at all.";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let math_spans = ctx.math_spans();
        assert!(math_spans.is_empty(), "Should have no math spans");
    }

    #[test]
    fn test_math_spans_multiple() {
        let content = "First $a$ and second $b$ and display $$c$$";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let math_spans = ctx.math_spans();
        assert_eq!(math_spans.len(), 3, "Should detect three math spans");

        // Two inline, one display
        let inline_count = math_spans.iter().filter(|s| !s.is_display).count();
        let display_count = math_spans.iter().filter(|s| s.is_display).count();

        assert_eq!(inline_count, 2, "Should have two inline math spans");
        assert_eq!(display_count, 1, "Should have one display math span");
    }

    #[test]
    fn test_is_in_math_span_boundary_positions() {
        // Test exact boundary positions: $[f](x)$
        // Byte positions:                0123456789
        let content = "$[f](x)$";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let math_spans = ctx.math_spans();
        assert_eq!(math_spans.len(), 1, "Should have one math span");

        let span = &math_spans[0];

        // Position at opening $ should be in span (byte 0)
        assert!(
            ctx.is_in_math_span(span.byte_offset),
            "Start position should be in span"
        );

        // Position just inside should be in span
        assert!(
            ctx.is_in_math_span(span.byte_offset + 1),
            "Position after start should be in span"
        );

        // Position at closing $ should be in span (exclusive end means we check byte_end - 1)
        assert!(
            ctx.is_in_math_span(span.byte_end - 1),
            "Position at end-1 should be in span"
        );

        // Position at byte_end should NOT be in span (exclusive end)
        assert!(
            !ctx.is_in_math_span(span.byte_end),
            "Position at byte_end should NOT be in span (exclusive)"
        );
    }

    #[test]
    fn test_math_spans_at_document_start() {
        let content = "$x$ text";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let math_spans = ctx.math_spans();
        assert_eq!(math_spans.len(), 1);
        assert_eq!(math_spans[0].byte_offset, 0, "Math should start at byte 0");
    }

    #[test]
    fn test_math_spans_at_document_end() {
        let content = "text $x$";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let math_spans = ctx.math_spans();
        assert_eq!(math_spans.len(), 1);
        assert_eq!(math_spans[0].byte_end, content.len(), "Math should end at document end");
    }

    #[test]
    fn test_math_spans_consecutive() {
        let content = "$a$$b$";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let math_spans = ctx.math_spans();
        // pulldown-cmark should parse these as separate spans
        assert!(!math_spans.is_empty(), "Should detect at least one math span");

        // All positions should be in some math span
        for i in 0..content.len() {
            assert!(ctx.is_in_math_span(i), "Position {i} should be in a math span");
        }
    }

    #[test]
    fn test_math_spans_currency_not_math() {
        // Unbalanced $ should not create math spans
        let content = "Price is $100";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let math_spans = ctx.math_spans();
        // pulldown-cmark requires balanced delimiters for math
        // $100 alone is not math
        assert!(
            math_spans.is_empty() || !math_spans.iter().any(|s| s.content.contains("100")),
            "Unbalanced $ should not create math span containing 100"
        );
    }

    // =========================================================================
    // Tests for O(1) reference definition lookups via HashMap
    // =========================================================================

    #[test]
    fn test_reference_lookup_o1_basic() {
        let content = r#"[ref1]: /url1
[REF2]: /url2 "Title"
[Ref3]: /url3

Use [link][ref1] and [link][REF2]."#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Verify we have 3 reference defs
        assert_eq!(ctx.reference_defs.len(), 3);

        // Test get_reference_url with various cases
        assert_eq!(ctx.get_reference_url("ref1"), Some("/url1"));
        assert_eq!(ctx.get_reference_url("REF1"), Some("/url1")); // case insensitive
        assert_eq!(ctx.get_reference_url("Ref1"), Some("/url1")); // case insensitive
        assert_eq!(ctx.get_reference_url("ref2"), Some("/url2"));
        assert_eq!(ctx.get_reference_url("REF2"), Some("/url2"));
        assert_eq!(ctx.get_reference_url("ref3"), Some("/url3"));
        assert_eq!(ctx.get_reference_url("nonexistent"), None);
    }

    #[test]
    fn test_reference_lookup_o1_get_reference_def() {
        let content = r#"[myref]: https://example.com "My Title"
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Test get_reference_def
        let def = ctx.get_reference_def("myref").expect("Should find myref");
        assert_eq!(def.url, "https://example.com");
        assert_eq!(def.title.as_deref(), Some("My Title"));

        // Case insensitive
        let def2 = ctx.get_reference_def("MYREF").expect("Should find MYREF");
        assert_eq!(def2.url, "https://example.com");

        // Non-existent
        assert!(ctx.get_reference_def("nonexistent").is_none());
    }

    #[test]
    fn test_reference_lookup_o1_has_reference_def() {
        let content = r#"[foo]: /foo
[BAR]: /bar
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Test has_reference_def
        assert!(ctx.has_reference_def("foo"));
        assert!(ctx.has_reference_def("FOO")); // case insensitive
        assert!(ctx.has_reference_def("bar"));
        assert!(ctx.has_reference_def("Bar")); // case insensitive
        assert!(!ctx.has_reference_def("baz")); // doesn't exist
    }

    #[test]
    fn test_reference_lookup_o1_empty_content() {
        let content = "No references here.";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        assert!(ctx.reference_defs.is_empty());
        assert_eq!(ctx.get_reference_url("anything"), None);
        assert!(ctx.get_reference_def("anything").is_none());
        assert!(!ctx.has_reference_def("anything"));
    }

    #[test]
    fn test_reference_lookup_o1_special_characters_in_id() {
        let content = r#"[ref-with-dash]: /url1
[ref_with_underscore]: /url2
[ref.with.dots]: /url3
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        assert_eq!(ctx.get_reference_url("ref-with-dash"), Some("/url1"));
        assert_eq!(ctx.get_reference_url("ref_with_underscore"), Some("/url2"));
        assert_eq!(ctx.get_reference_url("ref.with.dots"), Some("/url3"));
    }

    #[test]
    fn test_reference_lookup_o1_unicode_id() {
        let content = r#"[日本語]: /japanese
[émoji]: /emoji
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        assert_eq!(ctx.get_reference_url("日本語"), Some("/japanese"));
        assert_eq!(ctx.get_reference_url("émoji"), Some("/emoji"));
        assert_eq!(ctx.get_reference_url("ÉMOJI"), Some("/emoji")); // uppercase
    }
}
