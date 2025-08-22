use crate::utils::code_block_utils::{CodeBlockContext, CodeBlockUtils};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Comprehensive link pattern that captures both inline and reference links
    // Use (?s) flag to make . match newlines
    static ref LINK_PATTERN: Regex = Regex::new(
        r"(?sx)
        \[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\]          # Link text in group 1 (handles nested brackets)
        (?:
            \(([^)]*)\)       # Inline URL in group 2 (can be empty)
            |
            \[([^\]]*)\]      # Reference ID in group 3
        )"
    ).unwrap();

    // Image pattern (similar to links but with ! prefix)
    // Use (?s) flag to make . match newlines
    static ref IMAGE_PATTERN: Regex = Regex::new(
        r"(?sx)
        !\[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\]         # Alt text in group 1 (handles nested brackets)
        (?:
            \(([^)]*)\)       # Inline URL in group 2 (can be empty)
            |
            \[([^\]]*)\]      # Reference ID in group 3
        )"
    ).unwrap();

    // Reference definition pattern
    static ref REF_DEF_PATTERN: Regex = Regex::new(
        r#"(?m)^[ ]{0,3}\[([^\]]+)\]:\s*([^\s]+)(?:\s+(?:"([^"]*)"|'([^']*)'))?$"#
    ).unwrap();

    // Code span pattern - matches backticks and captures content
    // This handles multi-backtick code spans correctly
    static ref CODE_SPAN_PATTERN: Regex = Regex::new(
        r"`+"
    ).unwrap();

    // Pattern for bare URLs
    static ref BARE_URL_PATTERN: Regex = Regex::new(
        r#"(https?|ftp)://[^\s<>\[\]()\\'"`]+(?:\.[^\s<>\[\]()\\'"`]+)*(?::\d+)?(?:/[^\s<>\[\]()\\'"`]*)?(?:\?[^\s<>\[\]()\\'"`]*)?(?:#[^\s<>\[\]()\\'"`]*)?"#
    ).unwrap();

    // Pattern for email addresses
    static ref BARE_EMAIL_PATTERN: Regex = Regex::new(
        r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}"
    ).unwrap();

    // Pattern for angle bracket links (to exclude from bare URL detection)
    static ref ANGLE_BRACKET_PATTERN: Regex = Regex::new(
        r"<((?:https?|ftp)://[^>]+|[^@\s]+@[^@\s]+\.[^@\s>]+)>"
    ).unwrap();

    // Pattern for blockquote prefix in parse_list_blocks
    static ref BLOCKQUOTE_PREFIX_REGEX: Regex = Regex::new(r"^(\s*>+\s*)").unwrap();
}

/// Pre-computed information about a line
#[derive(Debug, Clone)]
pub struct LineInfo {
    /// The actual line content (without newline)
    pub content: String,
    /// Byte offset where this line starts in the document
    pub byte_offset: usize,
    /// Number of leading spaces/tabs
    pub indent: usize,
    /// Whether the line is blank (empty or only whitespace)
    pub is_blank: bool,
    /// Whether this line is inside a code block
    pub in_code_block: bool,
    /// Whether this line is inside front matter
    pub in_front_matter: bool,
    /// List item information if this line starts a list item
    pub list_item: Option<ListItemInfo>,
    /// Heading information if this line is a heading
    pub heading: Option<HeadingInfo>,
    /// Blockquote information if this line is a blockquote
    pub blockquote: Option<BlockquoteInfo>,
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
pub struct ParsedLink {
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
    pub text: String,
    /// Link URL or reference
    pub url: String,
    /// Whether this is a reference link [text][ref] vs inline [text](url)
    pub is_reference: bool,
    /// Reference ID for reference links
    pub reference_id: Option<String>,
}

/// Parsed image information
#[derive(Debug, Clone)]
pub struct ParsedImage {
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
    pub alt_text: String,
    /// Image URL or reference
    pub url: String,
    /// Whether this is a reference image ![alt][ref] vs inline ![alt](url)
    pub is_reference: bool,
    /// Reference ID for reference images
    pub reference_id: Option<String>,
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
}

/// Parsed code span information
#[derive(Debug, Clone)]
pub struct CodeSpan {
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
    /// Number of backticks used (1, 2, 3, etc.)
    pub backtick_count: usize,
    /// Content inside the code span (without backticks)
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

use std::sync::{Arc, Mutex};

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
    /// Whether it's a closing tag (</tag>)
    pub is_closing: bool,
    /// Whether it's self-closing (<tag />)
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
    pub links: Vec<ParsedLink>,           // Pre-parsed links
    pub images: Vec<ParsedImage>,         // Pre-parsed images
    pub reference_defs: Vec<ReferenceDef>, // Reference definitions
    code_spans_cache: Mutex<Option<Arc<Vec<CodeSpan>>>>, // Lazy-loaded inline code spans
    pub list_blocks: Vec<ListBlock>,      // Pre-parsed list blocks
    pub char_frequency: CharFrequency,    // Character frequency analysis
    html_tags_cache: Mutex<Option<Arc<Vec<HtmlTag>>>>, // Lazy-loaded HTML tags
    emphasis_spans_cache: Mutex<Option<Arc<Vec<EmphasisSpan>>>>, // Lazy-loaded emphasis spans
    table_rows_cache: Mutex<Option<Arc<Vec<TableRow>>>>, // Lazy-loaded table rows
    bare_urls_cache: Mutex<Option<Arc<Vec<BareUrl>>>>, // Lazy-loaded bare URLs
}

impl<'a> LintContext<'a> {
    pub fn new(content: &'a str) -> Self {
        let mut line_offsets = vec![0];
        for (i, c) in content.char_indices() {
            if c == '\n' {
                line_offsets.push(i + 1);
            }
        }

        // Detect code blocks once and cache them
        let code_blocks = CodeBlockUtils::detect_code_blocks(content);

        // Pre-compute line information
        let lines = Self::compute_line_info(content, &line_offsets, &code_blocks);

        // Parse links, images, references, and list blocks
        // Skip code spans - they'll be computed lazily
        let links = Self::parse_links(content, &lines, &code_blocks);
        let images = Self::parse_images(content, &lines, &code_blocks);
        let reference_defs = Self::parse_reference_defs(content, &lines);
        let list_blocks = Self::parse_list_blocks(&lines);

        // Compute character frequency for fast content analysis
        let char_frequency = Self::compute_char_frequency(content);

        Self {
            content,
            line_offsets,
            code_blocks,
            lines,
            links,
            images,
            reference_defs,
            code_spans_cache: Mutex::new(None),
            list_blocks,
            char_frequency,
            html_tags_cache: Mutex::new(None),
            emphasis_spans_cache: Mutex::new(None),
            table_rows_cache: Mutex::new(None),
            bare_urls_cache: Mutex::new(None),
        }
    }

    /// Get code spans - computed lazily on first access
    pub fn code_spans(&self) -> Arc<Vec<CodeSpan>> {
        let mut cache = self.code_spans_cache.lock().unwrap();

        // Check if we need to compute code spans
        if cache.is_none() {
            let code_spans = Self::parse_code_spans(self.content, &self.lines);
            *cache = Some(Arc::new(code_spans));
        }

        // Return a reference to the cached code spans
        cache.as_ref().unwrap().clone()
    }

    /// Get HTML tags - computed lazily on first access
    pub fn html_tags(&self) -> Arc<Vec<HtmlTag>> {
        let mut cache = self.html_tags_cache.lock().unwrap();

        if cache.is_none() {
            let html_tags = Self::parse_html_tags(self.content, &self.lines, &self.code_blocks);
            *cache = Some(Arc::new(html_tags));
        }

        cache.as_ref().unwrap().clone()
    }

    /// Get emphasis spans - computed lazily on first access
    pub fn emphasis_spans(&self) -> Arc<Vec<EmphasisSpan>> {
        let mut cache = self.emphasis_spans_cache.lock().unwrap();

        if cache.is_none() {
            let emphasis_spans = Self::parse_emphasis_spans(self.content, &self.lines, &self.code_blocks);
            *cache = Some(Arc::new(emphasis_spans));
        }

        cache.as_ref().unwrap().clone()
    }

    /// Get table rows - computed lazily on first access
    pub fn table_rows(&self) -> Arc<Vec<TableRow>> {
        let mut cache = self.table_rows_cache.lock().unwrap();

        if cache.is_none() {
            let table_rows = Self::parse_table_rows(&self.lines);
            *cache = Some(Arc::new(table_rows));
        }

        cache.as_ref().unwrap().clone()
    }

    /// Get bare URLs - computed lazily on first access
    pub fn bare_urls(&self) -> Arc<Vec<BareUrl>> {
        let mut cache = self.bare_urls_cache.lock().unwrap();

        if cache.is_none() {
            let bare_urls = Self::parse_bare_urls(self.content, &self.lines, &self.code_blocks);
            *cache = Some(Arc::new(bare_urls));
        }

        cache.as_ref().unwrap().clone()
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

    /// Get URL for a reference link/image by its ID
    pub fn get_reference_url(&self, ref_id: &str) -> Option<&str> {
        let normalized_id = ref_id.to_lowercase();
        self.reference_defs
            .iter()
            .find(|def| def.id == normalized_id)
            .map(|def| def.url.as_str())
    }

    /// Get links on a specific line
    pub fn links_on_line(&self, line_num: usize) -> Vec<&ParsedLink> {
        self.links.iter().filter(|link| link.line == line_num).collect()
    }

    /// Get images on a specific line
    pub fn images_on_line(&self, line_num: usize) -> Vec<&ParsedImage> {
        self.images.iter().filter(|img| img.line == line_num).collect()
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

    /// Parse all links in the content
    fn parse_links(content: &str, lines: &[LineInfo], code_blocks: &[(usize, usize)]) -> Vec<ParsedLink> {
        // Pre-size based on a heuristic: most markdown files have relatively few links
        let mut links = Vec::with_capacity(content.len() / 500); // ~1 link per 500 chars

        // Parse links across the entire content, not line by line
        for cap in LINK_PATTERN.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let match_start = full_match.start();
            let match_end = full_match.end();

            // Skip if the opening bracket is escaped (preceded by \)
            if match_start > 0 && content.as_bytes().get(match_start - 1) == Some(&b'\\') {
                continue;
            }

            // Skip if this is actually an image (preceded by !)
            if match_start > 0 && content.as_bytes().get(match_start - 1) == Some(&b'!') {
                continue;
            }

            // Skip if in code block or span
            if CodeBlockUtils::is_in_code_block_or_span(code_blocks, match_start) {
                continue;
            }

            // Find which line this link starts on
            let mut line_num = 1;
            let mut col_start = match_start;
            for (idx, line_info) in lines.iter().enumerate() {
                if match_start >= line_info.byte_offset {
                    line_num = idx + 1;
                    col_start = match_start - line_info.byte_offset;
                } else {
                    break;
                }
            }

            // Find which line this link ends on (and calculate column on that line)
            let mut end_line_num = 1;
            let mut col_end = match_end;
            for (idx, line_info) in lines.iter().enumerate() {
                if match_end > line_info.byte_offset {
                    end_line_num = idx + 1;
                    col_end = match_end - line_info.byte_offset;
                } else {
                    break;
                }
            }

            // For single-line links, use the same approach as before
            if line_num == end_line_num {
                // col_end is already correct
            } else {
                // For multi-line links, col_end represents the column on the ending line
                // which is what we want
            }

            let text = cap.get(1).map_or("", |m| m.as_str()).to_string();

            if let Some(inline_url) = cap.get(2) {
                // Inline link
                links.push(ParsedLink {
                    line: line_num,
                    start_col: col_start,
                    end_col: col_end,
                    byte_offset: match_start,
                    byte_end: match_end,
                    text,
                    url: inline_url.as_str().to_string(),
                    is_reference: false,
                    reference_id: None,
                });
            } else if let Some(ref_id) = cap.get(3) {
                // Reference link
                let ref_id_str = ref_id.as_str();
                let normalized_ref = if ref_id_str.is_empty() {
                    text.to_lowercase() // Implicit reference
                } else {
                    ref_id_str.to_lowercase()
                };

                links.push(ParsedLink {
                    line: line_num,
                    start_col: col_start,
                    end_col: col_end,
                    byte_offset: match_start,
                    byte_end: match_end,
                    text,
                    url: String::new(), // Will be resolved with reference_defs
                    is_reference: true,
                    reference_id: Some(normalized_ref),
                });
            }
        }

        links
    }

    /// Parse all images in the content
    fn parse_images(content: &str, lines: &[LineInfo], code_blocks: &[(usize, usize)]) -> Vec<ParsedImage> {
        // Pre-size based on a heuristic: images are less common than links
        let mut images = Vec::with_capacity(content.len() / 1000); // ~1 image per 1000 chars

        // Parse images across the entire content, not line by line
        for cap in IMAGE_PATTERN.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let match_start = full_match.start();
            let match_end = full_match.end();

            // Skip if the ! is escaped (preceded by \)
            if match_start > 0 && content.as_bytes().get(match_start - 1) == Some(&b'\\') {
                continue;
            }

            // Skip if in code block or span
            if CodeBlockUtils::is_in_code_block_or_span(code_blocks, match_start) {
                continue;
            }

            // Find which line this image starts on
            let mut line_num = 1;
            let mut col_start = match_start;
            for (idx, line_info) in lines.iter().enumerate() {
                if match_start >= line_info.byte_offset {
                    line_num = idx + 1;
                    col_start = match_start - line_info.byte_offset;
                } else {
                    break;
                }
            }

            // Find which line this image ends on (and calculate column on that line)
            let mut end_line_num = 1;
            let mut col_end = match_end;
            for (idx, line_info) in lines.iter().enumerate() {
                if match_end > line_info.byte_offset {
                    end_line_num = idx + 1;
                    col_end = match_end - line_info.byte_offset;
                } else {
                    break;
                }
            }

            // For single-line images, use the same approach as before
            if line_num == end_line_num {
                // col_end is already correct
            } else {
                // For multi-line images, col_end represents the column on the ending line
                // which is what we want
            }

            let alt_text = cap.get(1).map_or("", |m| m.as_str()).to_string();

            if let Some(inline_url) = cap.get(2) {
                // Inline image
                images.push(ParsedImage {
                    line: line_num,
                    start_col: col_start,
                    end_col: col_end,
                    byte_offset: match_start,
                    byte_end: match_end,
                    alt_text,
                    url: inline_url.as_str().to_string(),
                    is_reference: false,
                    reference_id: None,
                });
            } else if let Some(ref_id) = cap.get(3) {
                // Reference image
                let ref_id_str = ref_id.as_str();
                let normalized_ref = if ref_id_str.is_empty() {
                    alt_text.to_lowercase() // Implicit reference
                } else {
                    ref_id_str.to_lowercase()
                };

                images.push(ParsedImage {
                    line: line_num,
                    start_col: col_start,
                    end_col: col_end,
                    byte_offset: match_start,
                    byte_end: match_end,
                    alt_text,
                    url: String::new(), // Will be resolved with reference_defs
                    is_reference: true,
                    reference_id: Some(normalized_ref),
                });
            }
        }

        images
    }

    /// Parse reference definitions
    fn parse_reference_defs(_content: &str, lines: &[LineInfo]) -> Vec<ReferenceDef> {
        // Pre-size based on lines count as reference definitions are line-based
        let mut refs = Vec::with_capacity(lines.len() / 20); // ~1 ref per 20 lines

        for (line_idx, line_info) in lines.iter().enumerate() {
            // Skip lines in code blocks
            if line_info.in_code_block {
                continue;
            }

            let line = &line_info.content;
            let line_num = line_idx + 1;

            if let Some(cap) = REF_DEF_PATTERN.captures(line) {
                let id = cap.get(1).unwrap().as_str().to_lowercase();
                let url = cap.get(2).unwrap().as_str().to_string();
                let title = cap.get(3).or_else(|| cap.get(4)).map(|m| m.as_str().to_string());

                refs.push(ReferenceDef {
                    line: line_num,
                    id,
                    url,
                    title,
                });
            }
        }

        refs
    }

    /// Pre-compute line information
    fn compute_line_info(content: &str, line_offsets: &[usize], code_blocks: &[(usize, usize)]) -> Vec<LineInfo> {
        lazy_static! {
            // Regex for list detection - allow any whitespace including no space (to catch malformed lists)
            static ref UNORDERED_REGEX: regex::Regex = regex::Regex::new(r"^(\s*)([-*+])([ \t]*)(.*)").unwrap();
            static ref ORDERED_REGEX: regex::Regex = regex::Regex::new(r"^(\s*)(\d+)([.)])([ \t]*)(.*)").unwrap();

            // Regex for blockquote prefix
            static ref BLOCKQUOTE_REGEX: regex::Regex = regex::Regex::new(r"^(\s*>\s*)(.*)").unwrap();

            // Regex for heading detection
            static ref ATX_HEADING_REGEX: regex::Regex = regex::Regex::new(r"^(\s*)(#{1,6})(\s*)(.*)$").unwrap();
            static ref SETEXT_UNDERLINE_REGEX: regex::Regex = regex::Regex::new(r"^(\s*)(=+|-+)\s*$").unwrap();

            // Regex for blockquote detection
            static ref BLOCKQUOTE_REGEX_FULL: regex::Regex = regex::Regex::new(r"^(\s*)(>+)(\s*)(.*)$").unwrap();
        }

        let content_lines: Vec<&str> = content.lines().collect();
        let mut lines = Vec::with_capacity(content_lines.len());

        // Detect front matter boundaries FIRST, before any other parsing
        let mut in_front_matter = false;
        let mut front_matter_end = 0;
        if content_lines.first().map(|l| l.trim()) == Some("---") {
            in_front_matter = true;
            for (idx, line) in content_lines.iter().enumerate().skip(1) {
                if line.trim() == "---" {
                    front_matter_end = idx;
                    break;
                }
            }
        }

        for (i, line) in content_lines.iter().enumerate() {
            let byte_offset = line_offsets.get(i).copied().unwrap_or(0);
            let indent = line.len() - line.trim_start().len();
            // For blank detection, consider blockquote context
            let is_blank = if let Some(caps) = BLOCKQUOTE_REGEX.captures(line) {
                // In blockquote context, check if content after prefix is blank
                let after_prefix = caps.get(2).map_or("", |m| m.as_str());
                after_prefix.trim().is_empty()
            } else {
                line.trim().is_empty()
            };
            // Check if this line is inside a code block (not inline code span)
            // We only want to check for fenced/indented code blocks, not inline code
            let in_code_block = code_blocks.iter().any(|&(start, end)| {
                // Only consider ranges that span multiple lines (code blocks)
                // Inline code spans are typically on a single line
                let block_content = &content[start..end];
                let is_multiline = block_content.contains('\n');
                let is_fenced = block_content.starts_with("```") || block_content.starts_with("~~~");
                let is_indented = !is_fenced
                    && block_content
                        .lines()
                        .all(|l| l.starts_with("    ") || l.starts_with("\t") || l.trim().is_empty());

                byte_offset >= start && byte_offset < end && (is_multiline || is_fenced || is_indented)
            });

            // Detect list items (skip if in frontmatter)
            let list_item = if !(in_code_block || is_blank || in_front_matter && i <= front_matter_end) {
                // Strip blockquote prefix if present for list detection
                let (line_for_list_check, blockquote_prefix_len) = if let Some(caps) = BLOCKQUOTE_REGEX.captures(line) {
                    let prefix = caps.get(1).unwrap().as_str();
                    let content = caps.get(2).unwrap().as_str();
                    (content, prefix.len())
                } else {
                    (&**line, 0)
                };

                if let Some(caps) = UNORDERED_REGEX.captures(line_for_list_check) {
                    let leading_spaces = caps.get(1).map_or("", |m| m.as_str());
                    let marker = caps.get(2).map_or("", |m| m.as_str());
                    let spacing = caps.get(3).map_or("", |m| m.as_str());
                    let _content = caps.get(4).map_or("", |m| m.as_str());
                    let marker_column = blockquote_prefix_len + leading_spaces.len();
                    let content_column = marker_column + marker.len() + spacing.len();

                    // According to CommonMark spec, unordered list items MUST have at least one space
                    // after the marker (-, *, or +). Without a space, it's not a list item.
                    // This also naturally handles cases like:
                    // - *emphasis* (not a list)
                    // - **bold** (not a list)
                    // - --- (horizontal rule, not a list)
                    if spacing.is_empty() {
                        None
                    } else {
                        Some(ListItemInfo {
                            marker: marker.to_string(),
                            is_ordered: false,
                            number: None,
                            marker_column,
                            content_column,
                        })
                    }
                } else if let Some(caps) = ORDERED_REGEX.captures(line_for_list_check) {
                    let leading_spaces = caps.get(1).map_or("", |m| m.as_str());
                    let number_str = caps.get(2).map_or("", |m| m.as_str());
                    let delimiter = caps.get(3).map_or("", |m| m.as_str());
                    let spacing = caps.get(4).map_or("", |m| m.as_str());
                    let _content = caps.get(5).map_or("", |m| m.as_str());
                    let marker = format!("{number_str}{delimiter}");
                    let marker_column = blockquote_prefix_len + leading_spaces.len();
                    let content_column = marker_column + marker.len() + spacing.len();

                    // According to CommonMark spec, ordered list items MUST have at least one space
                    // after the marker (period or parenthesis). Without a space, it's not a list item.
                    if spacing.is_empty() {
                        None
                    } else {
                        Some(ListItemInfo {
                            marker,
                            is_ordered: true,
                            number: number_str.parse().ok(),
                            marker_column,
                            content_column,
                        })
                    }
                } else {
                    None
                }
            } else {
                None
            };

            lines.push(LineInfo {
                content: line.to_string(),
                byte_offset,
                indent,
                is_blank,
                in_code_block,
                in_front_matter: in_front_matter && i <= front_matter_end,
                list_item,
                heading: None,    // Will be populated in second pass for Setext headings
                blockquote: None, // Will be populated after line creation
            });
        }

        // Second pass: detect headings (including Setext which needs look-ahead) and blockquotes
        for i in 0..content_lines.len() {
            if lines[i].in_code_block {
                continue;
            }

            // Skip lines in front matter
            if in_front_matter && i <= front_matter_end {
                continue;
            }

            let line = content_lines[i];

            // Check for blockquotes (even on blank lines within blockquotes)
            if let Some(caps) = BLOCKQUOTE_REGEX_FULL.captures(line) {
                let indent_str = caps.get(1).map_or("", |m| m.as_str());
                let markers = caps.get(2).map_or("", |m| m.as_str());
                let spaces_after = caps.get(3).map_or("", |m| m.as_str());
                let content = caps.get(4).map_or("", |m| m.as_str());

                let nesting_level = markers.chars().filter(|&c| c == '>').count();
                let marker_column = indent_str.len();

                // Build the prefix (indentation + markers + space)
                let prefix = format!("{indent_str}{markers}{spaces_after}");

                // Check for various blockquote issues
                let has_no_space = spaces_after.is_empty() && !content.is_empty();
                // Consider tabs as multiple spaces, or actual multiple spaces
                let has_multiple_spaces = spaces_after.len() > 1 || spaces_after.contains('\t');

                // Check if needs MD028 fix (empty blockquote without proper spacing)
                let needs_md028_fix = content.trim().is_empty() && spaces_after.is_empty();

                lines[i].blockquote = Some(BlockquoteInfo {
                    nesting_level,
                    indent: indent_str.to_string(),
                    marker_column,
                    prefix,
                    content: content.to_string(),
                    has_no_space_after_marker: has_no_space,
                    has_multiple_spaces_after_marker: has_multiple_spaces,
                    needs_md028_fix,
                });
            }

            // Skip heading detection for blank lines
            if lines[i].is_blank {
                continue;
            }

            // Check for ATX headings
            if let Some(caps) = ATX_HEADING_REGEX.captures(line) {
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
                    if let Some(last_hash_pos) = trimmed_rest.rfind('#') {
                        // Look for the start of the hash sequence
                        let mut start_of_hashes = last_hash_pos;
                        while start_of_hashes > 0 && trimmed_rest.chars().nth(start_of_hashes - 1) == Some('#') {
                            start_of_hashes -= 1;
                        }

                        // Check if there's at least one space before the closing hashes
                        let has_space_before = start_of_hashes == 0
                            || trimmed_rest
                                .chars()
                                .nth(start_of_hashes - 1)
                                .is_some_and(|c| c.is_whitespace());

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
                                format!("{}{}", rest_without_id[..start_of_hashes].trim_end(), custom_id_part)
                            } else {
                                rest_without_id[..start_of_hashes].trim_end().to_string()
                            };
                            (text_part, true, closing_hashes)
                        } else {
                            // Not a valid closing sequence, return the full content
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
                });
            }
            // Check for Setext headings (need to look at next line)
            else if i + 1 < content_lines.len() {
                let next_line = content_lines[i + 1];
                if !lines[i + 1].in_code_block && SETEXT_UNDERLINE_REGEX.is_match(next_line) {
                    // Skip if next line is front matter delimiter
                    if in_front_matter && i < front_matter_end {
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
                    });
                }
            }
        }

        lines
    }

    /// Parse all inline code spans in the content
    fn parse_code_spans(content: &str, lines: &[LineInfo]) -> Vec<CodeSpan> {
        // Pre-size based on content - code spans are fairly common
        let mut code_spans = Vec::with_capacity(content.matches('`').count() / 2); // Each code span has 2 backticks

        // Quick check - if no backticks, no code spans
        if !content.contains('`') {
            return code_spans;
        }

        let mut pos = 0;
        let bytes = content.as_bytes();

        while pos < bytes.len() {
            // Find the next backtick
            if let Some(backtick_start) = content[pos..].find('`') {
                let start_pos = pos + backtick_start;

                // Skip if this backtick is inside a code block
                let mut in_code_block = false;
                for (line_idx, line_info) in lines.iter().enumerate() {
                    if start_pos >= line_info.byte_offset
                        && (line_idx + 1 >= lines.len() || start_pos < lines[line_idx + 1].byte_offset)
                    {
                        in_code_block = line_info.in_code_block;
                        break;
                    }
                }

                if in_code_block {
                    pos = start_pos + 1;
                    continue;
                }

                // Count consecutive backticks
                let mut backtick_count = 0;
                let mut i = start_pos;
                while i < bytes.len() && bytes[i] == b'`' {
                    backtick_count += 1;
                    i += 1;
                }

                // Look for matching closing backticks
                let search_start = start_pos + backtick_count;
                let closing_pattern = &content[start_pos..start_pos + backtick_count];

                if let Some(rel_end) = content[search_start..].find(closing_pattern) {
                    // Check that the closing backticks are not followed by more backticks
                    let end_pos = search_start + rel_end;
                    let check_pos = end_pos + backtick_count;

                    // Make sure we have exactly the right number of backticks (not more)
                    if check_pos >= bytes.len() || bytes[check_pos] != b'`' {
                        // We found a valid code span
                        let content_start = start_pos + backtick_count;
                        let content_end = end_pos;
                        let span_content = content[content_start..content_end].to_string();

                        // Find which line this code span starts on
                        let mut line_num = 1;
                        let mut col_start = start_pos;
                        for (idx, line_info) in lines.iter().enumerate() {
                            if start_pos >= line_info.byte_offset {
                                line_num = idx + 1;
                                col_start = start_pos - line_info.byte_offset;
                            } else {
                                break;
                            }
                        }

                        // Find end column
                        let mut col_end = end_pos + backtick_count;
                        for line_info in lines.iter() {
                            if end_pos + backtick_count > line_info.byte_offset {
                                col_end = end_pos + backtick_count - line_info.byte_offset;
                            } else {
                                break;
                            }
                        }

                        code_spans.push(CodeSpan {
                            line: line_num,
                            start_col: col_start,
                            end_col: col_end,
                            byte_offset: start_pos,
                            byte_end: end_pos + backtick_count,
                            backtick_count,
                            content: span_content,
                        });

                        // Continue searching after this code span
                        pos = end_pos + backtick_count;
                        continue;
                    }
                }

                // No matching closing backticks found, move past these opening backticks
                pos = start_pos + backtick_count;
            } else {
                // No more backticks found
                break;
            }
        }

        code_spans
    }

    /// Parse all list blocks in the content
    fn parse_list_blocks(lines: &[LineInfo]) -> Vec<ListBlock> {
        // Pre-size based on lines that could be list items
        let mut list_blocks = Vec::with_capacity(lines.len() / 10); // Estimate ~10% of lines might start list blocks
        let mut current_block: Option<ListBlock> = None;
        let mut last_list_item_line = 0;
        let mut current_indent_level = 0;
        let mut last_marker_width = 0;

        for (line_idx, line_info) in lines.iter().enumerate() {
            let line_num = line_idx + 1;

            // Enhanced code block handling using Design #3's context analysis
            if line_info.in_code_block {
                if let Some(ref mut block) = current_block {
                    // Calculate minimum indentation for list continuation
                    let min_continuation_indent = CodeBlockUtils::calculate_min_continuation_indent(lines, line_idx);

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
            let blockquote_prefix = if let Some(caps) = BLOCKQUOTE_PREFIX_REGEX.captures(&line_info.content) {
                caps.get(0).unwrap().as_str().to_string()
            } else {
                String::new()
            };

            // Check if this line is a list item
            if let Some(list_item) = &line_info.list_item {
                // Calculate nesting level based on indentation
                let item_indent = list_item.marker_column;
                let nesting = item_indent / 2; // Assume 2-space indentation for nesting

                if let Some(ref mut block) = current_block {
                    // Check if this continues the current block
                    // For nested lists, we need to check if this is a nested item (higher nesting level)
                    // or a continuation at the same or lower level
                    let is_nested = nesting > block.nesting_level;
                    let same_type =
                        (block.is_ordered && list_item.is_ordered) || (!block.is_ordered && !list_item.is_ordered);
                    let same_context = block.blockquote_prefix == blockquote_prefix;
                    let reasonable_distance = line_num <= last_list_item_line + 2; // Allow one blank line

                    // For unordered lists, also check marker consistency
                    let marker_compatible =
                        block.is_ordered || block.marker.is_none() || block.marker.as_ref() == Some(&list_item.marker);

                    // Check if there's non-list content between the last item and this one
                    let has_non_list_content = {
                        let mut found_non_list = false;
                        // Use the last item from the current block, not the global last_list_item_line
                        let block_last_item_line = block.item_lines.last().copied().unwrap_or(block.end_line);
                        for check_line in (block_last_item_line + 1)..line_num {
                            let check_idx = check_line - 1;
                            if check_idx < lines.len() {
                                let check_info = &lines[check_idx];
                                // Check for content that breaks the list
                                let is_list_breaking_content = if check_info.in_code_block {
                                    // Use enhanced code block classification for list separation
                                    let last_item_marker_width =
                                        if block_last_item_line > 0 && block_last_item_line <= lines.len() {
                                            lines[block_last_item_line - 1]
                                                .list_item
                                                .as_ref()
                                                .map(|li| {
                                                    if li.is_ordered {
                                                        li.marker.len() + 1 // Add 1 for the space after ordered list markers
                                                    } else {
                                                        li.marker.len()
                                                    }
                                                })
                                                .unwrap_or(3) // fallback to 3 if no list item found
                                        } else {
                                            3 // fallback
                                        };

                                    let min_continuation = if block.is_ordered { last_item_marker_width } else { 2 };

                                    // Analyze code block context using our enhanced classification
                                    let context = CodeBlockUtils::analyze_code_block_context(
                                        lines,
                                        check_line - 1,
                                        min_continuation,
                                    );

                                    // Standalone code blocks break lists, indented ones continue them
                                    matches!(context, CodeBlockContext::Standalone)
                                } else if !check_info.is_blank && check_info.list_item.is_none() {
                                    // Check for structural separators that should break lists (from issue #42)
                                    let line_content = check_info.content.trim();

                                    // Any of these structural separators break lists
                                    if check_info.heading.is_some()
                                        || line_content.starts_with("---")
                                        || line_content.starts_with("***")
                                        || line_content.starts_with("___")
                                        || line_content.contains('|')
                                        || line_content.starts_with(">")
                                    {
                                        true
                                    }
                                    // Other non-list content - check if properly indented
                                    else {
                                        let last_item_marker_width =
                                            if block_last_item_line > 0 && block_last_item_line <= lines.len() {
                                                lines[block_last_item_line - 1]
                                                    .list_item
                                                    .as_ref()
                                                    .map(|li| {
                                                        if li.is_ordered {
                                                            li.marker.len() + 1 // Add 1 for the space after ordered list markers
                                                        } else {
                                                            li.marker.len()
                                                        }
                                                    })
                                                    .unwrap_or(3) // fallback to 3 if no list item found
                                            } else {
                                                3 // fallback
                                            };

                                        let min_continuation =
                                            if block.is_ordered { last_item_marker_width } else { 2 };
                                        check_info.indent < min_continuation
                                    }
                                } else {
                                    false
                                };

                                if is_list_breaking_content {
                                    // Not indented enough, so it breaks the list
                                    found_non_list = true;
                                    break;
                                }
                            }
                        }
                        found_non_list
                    };

                    // A list continues if:
                    // 1. It's a nested item (indented more than the parent), OR
                    // 2. It's the same type at the same level with reasonable distance
                    let continues_list = if is_nested {
                        // Nested items always continue the list if they're in the same context
                        same_context && reasonable_distance && !has_non_list_content
                    } else {
                        // Same-level items need to match type and markers
                        same_type && same_context && reasonable_distance && marker_compatible && !has_non_list_content
                    };

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
                    } else {
                        // End current block and start a new one
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

                // For MD032 compatibility, we use a simple approach:
                // - Indented lines continue the list
                // - Blank lines followed by indented content continue the list
                // - Everything else ends the list

                // Calculate minimum indentation for list continuation
                // For ordered lists, use the last marker width (e.g., 3 for "1. ", 4 for "10. ")
                // For unordered lists like "- ", content starts at column 2, so continuations need at least 2 spaces
                let min_continuation_indent = if block.is_ordered {
                    current_indent_level + last_marker_width
                } else {
                    current_indent_level + 2 // Unordered lists need at least 2 spaces (e.g., "- " = 2 chars)
                };

                if line_info.indent >= min_continuation_indent {
                    // Indented line continues the list
                    block.end_line = line_num;
                } else if line_info.is_blank {
                    // Blank line - check if it's internal to the list or ending it
                    // We only include blank lines that are followed by more list content
                    let mut check_idx = line_idx + 1;
                    let mut found_continuation = false;

                    // Skip additional blank lines
                    while check_idx < lines.len() && lines[check_idx].is_blank {
                        check_idx += 1;
                    }

                    if check_idx < lines.len() {
                        let next_line = &lines[check_idx];
                        // Check if followed by indented content (list continuation)
                        if !next_line.in_code_block && next_line.indent >= min_continuation_indent {
                            found_continuation = true;
                        }
                        // Check if followed by another list item at the same level
                        else if !next_line.in_code_block
                            && next_line.list_item.is_some()
                            && let Some(item) = &next_line.list_item
                        {
                            let next_blockquote_prefix = BLOCKQUOTE_PREFIX_REGEX
                                .find(&next_line.content)
                                .map_or(String::new(), |m| m.as_str().to_string());
                            if item.marker_column == current_indent_level
                                && item.is_ordered == block.is_ordered
                                && block.blockquote_prefix.trim() == next_blockquote_prefix.trim()
                            {
                                // Check if there was meaningful content between the list items (unused now)
                                // This variable is kept for potential future use but is currently replaced by has_structural_separators
                                let _has_meaningful_content = (line_idx + 1..check_idx).any(|idx| {
                                    if let Some(between_line) = lines.get(idx) {
                                        let trimmed = between_line.content.trim();
                                        // Skip empty lines
                                        if trimmed.is_empty() {
                                            return false;
                                        }
                                        // Check for meaningful content
                                        let line_indent =
                                            between_line.content.len() - between_line.content.trim_start().len();

                                        // Structural separators (code fences, headings, etc.) are meaningful and should BREAK lists
                                        if trimmed.starts_with("```")
                                            || trimmed.starts_with("~~~")
                                            || trimmed.starts_with("---")
                                            || trimmed.starts_with("***")
                                            || trimmed.starts_with("___")
                                            || trimmed.starts_with(">")
                                            || trimmed.contains('|') // Tables
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
                                            let trimmed = between_line.content.trim();
                                            if trimmed.is_empty() {
                                                return false;
                                            }
                                            // Check for structural separators that break lists
                                            trimmed.starts_with("```")
                                                || trimmed.starts_with("~~~")
                                                || trimmed.starts_with("---")
                                                || trimmed.starts_with("***")
                                                || trimmed.starts_with("___")
                                                || trimmed.starts_with(">")
                                                || trimmed.contains('|') // Tables
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
                                            let trimmed = between_line.content.trim();
                                            if trimmed.is_empty() {
                                                return false;
                                            }
                                            // Check for structural separators that break lists
                                            trimmed.starts_with("```")
                                                || trimmed.starts_with("~~~")
                                                || trimmed.starts_with("---")
                                                || trimmed.starts_with("***")
                                                || trimmed.starts_with("___")
                                                || trimmed.starts_with(">")
                                                || trimmed.contains('|') // Tables
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
                    let line_content = line_info.content.trim();
                    let is_structural_separator = line_info.heading.is_some()
                        || line_content.starts_with("```")
                        || line_content.starts_with("~~~")
                        || line_content.starts_with("---")
                        || line_content.starts_with("***")
                        || line_content.starts_with("___")
                        || line_content.starts_with(">")
                        || line_content.contains('|'); // Tables

                    let is_lazy_continuation = last_list_item_line == line_num - 1
                        && !is_structural_separator
                        && !line_info.is_blank
                        && (line_info.indent == 0 || line_info.indent >= min_required_indent);

                    if is_lazy_continuation {
                        // Additional check: if the line starts with uppercase and looks like a new sentence,
                        // it's probably not a continuation
                        let content_to_check = if !blockquote_prefix.is_empty() {
                            // Strip blockquote prefix to check the actual content
                            line_info
                                .content
                                .strip_prefix(&blockquote_prefix)
                                .unwrap_or(&line_info.content)
                                .trim()
                        } else {
                            line_info.content.trim()
                        };

                        let starts_with_uppercase = content_to_check.chars().next().is_some_and(|c| c.is_uppercase());

                        // If it starts with uppercase and the previous line ended with punctuation,
                        // it's likely a new paragraph, not a continuation
                        if starts_with_uppercase && last_list_item_line > 0 {
                            // This looks like a new paragraph
                            list_blocks.push(block.clone());
                            current_block = None;
                        } else {
                            // This is a lazy continuation line
                            block.end_line = line_num;
                        }
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
        merge_adjacent_list_blocks(&mut list_blocks, lines);

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
    fn parse_html_tags(content: &str, lines: &[LineInfo], code_blocks: &[(usize, usize)]) -> Vec<HtmlTag> {
        lazy_static! {
            static ref HTML_TAG_REGEX: regex::Regex =
                regex::Regex::new(r"(?i)<(/?)([a-zA-Z][a-zA-Z0-9]*)\b[^>]*(/?)>").unwrap();
        }

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
            let tag_name = cap.get(2).unwrap().as_str().to_lowercase();
            let is_self_closing = !cap.get(3).unwrap().as_str().is_empty();

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

    /// Parse emphasis spans in the content
    fn parse_emphasis_spans(content: &str, lines: &[LineInfo], code_blocks: &[(usize, usize)]) -> Vec<EmphasisSpan> {
        lazy_static! {
            static ref EMPHASIS_REGEX: regex::Regex =
                regex::Regex::new(r"(\*{1,3}|_{1,3})([^*_\s][^*_]*?)(\*{1,3}|_{1,3})").unwrap();
        }

        let mut emphasis_spans = Vec::with_capacity(content.matches('*').count() + content.matches('_').count() / 4);

        for cap in EMPHASIS_REGEX.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let match_start = full_match.start();
            let match_end = full_match.end();

            // Skip if in code block
            if CodeBlockUtils::is_in_code_block_or_span(code_blocks, match_start) {
                continue;
            }

            let opening_markers = cap.get(1).unwrap().as_str();
            let content_part = cap.get(2).unwrap().as_str();
            let closing_markers = cap.get(3).unwrap().as_str();

            // Validate matching markers
            if opening_markers.chars().next() != closing_markers.chars().next()
                || opening_markers.len() != closing_markers.len()
            {
                continue;
            }

            let marker = opening_markers.chars().next().unwrap();
            let marker_count = opening_markers.len();

            // Find which line this emphasis is on
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

        emphasis_spans
    }

    /// Parse table rows in the content
    fn parse_table_rows(lines: &[LineInfo]) -> Vec<TableRow> {
        let mut table_rows = Vec::with_capacity(lines.len() / 20);

        for (line_idx, line_info) in lines.iter().enumerate() {
            // Skip lines in code blocks or blank lines
            if line_info.in_code_block || line_info.is_blank {
                continue;
            }

            let line = &line_info.content;
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
        for cap in BARE_URL_PATTERN.captures_iter(content) {
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
}

/// Merge adjacent list blocks that should be treated as one
fn merge_adjacent_list_blocks(list_blocks: &mut Vec<ListBlock>, lines: &[LineInfo]) {
    if list_blocks.len() < 2 {
        return;
    }

    let mut merger = ListBlockMerger::new(lines);
    *list_blocks = merger.merge(list_blocks);
}

/// Helper struct to manage the complex logic of merging list blocks
struct ListBlockMerger<'a> {
    lines: &'a [LineInfo],
}

impl<'a> ListBlockMerger<'a> {
    fn new(lines: &'a [LineInfo]) -> Self {
        Self { lines }
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
        if has_meaningful_content_between(current, next, self.lines) {
            return false; // Structural separators prevent merging
        }

        // Only merge unordered lists with same marker across single blank
        !current.is_ordered && current.marker == next.marker
    }

    /// Check if ordered lists can be merged when there's content between them
    fn can_merge_with_content_between(&self, current: &ListBlock, next: &ListBlock) -> bool {
        // Do not merge lists if there are structural separators between them
        if has_meaningful_content_between(current, next, self.lines) {
            return false; // Structural separators prevent merging
        }

        // Only consider merging ordered lists if there's no structural content between
        current.is_ordered && next.is_ordered
    }

    /// Check if there are only blank lines between blocks
    fn has_only_blank_lines_between(&self, current: &ListBlock, next: &ListBlock) -> bool {
        for line_num in (current.end_line + 1)..next.start_line {
            if let Some(line_info) = self.lines.get(line_num - 1)
                && !line_info.content.trim().is_empty()
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
fn has_meaningful_content_between(current: &ListBlock, next: &ListBlock, lines: &[LineInfo]) -> bool {
    // Check lines between current.end_line and next.start_line
    for line_num in (current.end_line + 1)..next.start_line {
        if let Some(line_info) = lines.get(line_num - 1) {
            // Convert to 0-indexed
            let trimmed = line_info.content.trim();

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

            // Tables separate lists (lines containing |)
            if trimmed.contains('|') && trimmed.len() > 1 {
                return true; // Has meaningful content - tables separate lists
            }

            // Blockquotes separate lists
            if trimmed.starts_with('>') {
                return true; // Has meaningful content - blockquotes separate lists
            }

            // Code block fences separate lists (unless properly indented as list content)
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let line_indent = line_info.content.len() - line_info.content.trim_start().len();

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
            let line_indent = line_info.content.len() - line_info.content.trim_start().len();

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

/// Check if a line is a horizontal rule (---, ***, ___)
fn is_horizontal_rule(trimmed: &str) -> bool {
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

/// Check if content contains patterns that cause the markdown crate to panic
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_content() {
        let ctx = LintContext::new("");
        assert_eq!(ctx.content, "");
        assert_eq!(ctx.line_offsets, vec![0]);
        assert_eq!(ctx.offset_to_line_col(0), (1, 1));
        assert_eq!(ctx.lines.len(), 0);
    }

    #[test]
    fn test_single_line() {
        let ctx = LintContext::new("# Hello");
        assert_eq!(ctx.content, "# Hello");
        assert_eq!(ctx.line_offsets, vec![0]);
        assert_eq!(ctx.offset_to_line_col(0), (1, 1));
        assert_eq!(ctx.offset_to_line_col(3), (1, 4));
    }

    #[test]
    fn test_multi_line() {
        let content = "# Title\n\nSecond line\nThird line";
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);

        // Test line info
        assert_eq!(ctx.lines.len(), 7);

        // Line 1: "# Title"
        let line1 = &ctx.lines[0];
        assert_eq!(line1.content, "# Title");
        assert_eq!(line1.byte_offset, 0);
        assert_eq!(line1.indent, 0);
        assert!(!line1.is_blank);
        assert!(!line1.in_code_block);
        assert!(line1.list_item.is_none());

        // Line 2: "    indented"
        let line2 = &ctx.lines[1];
        assert_eq!(line2.content, "    indented");
        assert_eq!(line2.byte_offset, 8);
        assert_eq!(line2.indent, 4);
        assert!(!line2.is_blank);

        // Line 3: "" (blank)
        let line3 = &ctx.lines[2];
        assert_eq!(line3.content, "");
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
        let ctx = LintContext::new(content);

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
        let ctx = LintContext::new(content);
        // line_offsets: [0, 2, 4]
        assert_eq!(ctx.offset_to_line_col(0), (1, 1)); // 'a'
        assert_eq!(ctx.offset_to_line_col(1), (1, 2)); // after 'a'
        assert_eq!(ctx.offset_to_line_col(2), (2, 1)); // 'b'
        assert_eq!(ctx.offset_to_line_col(3), (2, 2)); // after 'b'
        assert_eq!(ctx.offset_to_line_col(4), (3, 1)); // 'c'
        assert_eq!(ctx.offset_to_line_col(5), (3, 2)); // after 'c'
    }
}
