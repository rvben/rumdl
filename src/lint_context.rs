use crate::utils::code_block_utils::CodeBlockUtils;
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
    /// The heading text (without markers)
    pub text: String,
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
}

use std::sync::{Arc, Mutex};

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

            // Detect list items
            let list_item = if !in_code_block && !is_blank {
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
                    let content = caps.get(4).map_or("", |m| m.as_str());
                    let marker_column = blockquote_prefix_len + leading_spaces.len();
                    let content_column = marker_column + marker.len() + spacing.len();

                    // Check if this is likely emphasis or not a list item
                    if spacing.is_empty() {
                        // No space after marker - check if it's likely emphasis or just text
                        if marker == "*" && content.ends_with('*') && !content[..content.len() - 1].contains('*') {
                            // Likely emphasis like *text*
                            None
                        } else if marker == "*" && content.starts_with('*') {
                            // Likely bold emphasis like **text** or horizontal rule like ***
                            None
                        } else if (marker == "*" || marker == "-")
                            && content.chars().all(|c| c == marker.chars().next().unwrap())
                            && content.len() >= 2
                        {
                            // Likely horizontal rule like *** or ---
                            None
                        } else if !content.is_empty() && content.chars().next().unwrap().is_alphabetic() {
                            // Single word starting with marker, likely not intended as list
                            // This matches markdownlint behavior
                            None
                        } else {
                            // Other cases with no space - treat as malformed list item
                            Some(ListItemInfo {
                                marker: marker.to_string(),
                                is_ordered: false,
                                number: None,
                                marker_column,
                                content_column,
                            })
                        }
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
                    let content = caps.get(5).map_or("", |m| m.as_str());
                    let marker = format!("{number_str}{delimiter}");
                    let marker_column = blockquote_prefix_len + leading_spaces.len();
                    let content_column = marker_column + marker.len() + spacing.len();

                    // Check if this is likely not a list item
                    if spacing.is_empty() && !content.is_empty() && content.chars().next().unwrap().is_alphabetic() {
                        // No space after marker and starts with alphabetic, likely not intended as list
                        // This matches markdownlint behavior
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
                list_item,
                heading: None,    // Will be populated in second pass for Setext headings
                blockquote: None, // Will be populated after line creation
            });
        }

        // Detect front matter boundaries
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
                let has_multiple_spaces = spaces_after.len() > 1;

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

                // Check for closing sequence
                let (text, has_closing, closing_seq) = {
                    // Find the start of a potential closing sequence
                    let trimmed_rest = rest.trim_end();
                    if let Some(last_hash_pos) = trimmed_rest.rfind('#') {
                        // Look for the start of the hash sequence
                        let mut start_of_hashes = last_hash_pos;
                        while start_of_hashes > 0 && trimmed_rest.chars().nth(start_of_hashes - 1) == Some('#') {
                            start_of_hashes -= 1;
                        }

                        // Check if this is a valid closing sequence (all hashes to end of line)
                        let potential_closing = &trimmed_rest[start_of_hashes..];
                        let is_all_hashes = potential_closing.chars().all(|c| c == '#');

                        if is_all_hashes {
                            // This is a closing sequence, regardless of spacing
                            let closing_hashes = potential_closing.to_string();
                            let text_part = rest[..start_of_hashes].trim_end();
                            (text_part.to_string(), true, closing_hashes)
                        } else {
                            (rest.to_string(), false, String::new())
                        }
                    } else {
                        (rest.to_string(), false, String::new())
                    }
                };

                let content_column = marker_column + hashes.len() + spaces_after.len();

                lines[i].heading = Some(HeadingInfo {
                    level,
                    style: HeadingStyle::ATX,
                    marker: hashes.to_string(),
                    marker_column,
                    content_column,
                    text: text.trim().to_string(),
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

                    lines[i].heading = Some(HeadingInfo {
                        level,
                        style,
                        marker: underline.to_string(),
                        marker_column: next_line.len() - next_line.trim_start().len(),
                        content_column: lines[i].indent,
                        text: line.trim().to_string(),
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

        for (line_idx, line_info) in lines.iter().enumerate() {
            let line_num = line_idx + 1;

            // Handle code blocks - they should continue the list if properly indented
            if line_info.in_code_block {
                if let Some(ref mut block) = current_block {
                    // For code blocks to continue a list, they need to be indented
                    // at least 2 spaces beyond the list marker
                    if line_info.indent >= current_indent_level + 2 {
                        // Code blocks at list continuation level should continue the list
                        block.end_line = line_num;
                        continue;
                    }
                }
                // If we have a current block and hit a non-indented code block, end it
                if let Some(block) = current_block.take() {
                    list_blocks.push(block);
                }
                continue;
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
                        for check_line in (last_list_item_line + 1)..line_num {
                            let check_idx = check_line - 1;
                            if check_idx < lines.len() {
                                let check_info = &lines[check_idx];
                                if !check_info.is_blank && !check_info.in_code_block && check_info.list_item.is_none() {
                                    // Found non-blank, non-list content
                                    if check_info.indent < 2 {
                                        // Not indented, so it's not list continuation
                                        found_non_list = true;
                                        break;
                                    }
                                }
                            }
                        }
                        found_non_list
                    };

                    if same_type && same_context && reasonable_distance && marker_compatible && !has_non_list_content {
                        // Extend current block
                        block.end_line = line_num;
                        block.item_lines.push(line_num);

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
                    });
                }

                last_list_item_line = line_num;
                current_indent_level = item_indent;
            } else if let Some(ref mut block) = current_block {
                // Not a list item - check if it continues the current block

                // For MD032 compatibility, we use a simple approach:
                // - Indented lines continue the list
                // - Blank lines followed by indented content continue the list
                // - Everything else ends the list

                if line_info.indent >= current_indent_level + 2 {
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
                        if !next_line.in_code_block && next_line.indent >= current_indent_level + 2 {
                            found_continuation = true;
                        }
                        // Check if followed by another list item at the same level
                        else if !next_line.in_code_block && next_line.list_item.is_some() {
                            if let Some(item) = &next_line.list_item {
                                let next_blockquote_prefix = BLOCKQUOTE_PREFIX_REGEX
                                    .find(&next_line.content)
                                    .map_or(String::new(), |m| m.as_str().to_string());
                                if item.marker_column == current_indent_level
                                    && item.is_ordered == block.is_ordered
                                    && block.blockquote_prefix.trim() == next_blockquote_prefix.trim()
                                {
                                    // Check if there was meaningful content between the list items
                                    let has_meaningful_content = (line_idx + 1..check_idx).any(|idx| {
                                        if let Some(between_line) = lines.get(idx) {
                                            let trimmed = between_line.content.trim();
                                            // Skip empty lines
                                            if trimmed.is_empty() {
                                                return false;
                                            }
                                            // Check for meaningful content
                                            let line_indent =
                                                between_line.content.len() - between_line.content.trim_start().len();
                                            // Code fences or properly indented content
                                            trimmed.starts_with("```")
                                                || trimmed.starts_with("~~~")
                                                || line_indent >= current_indent_level + 2
                                        } else {
                                            false
                                        }
                                    });

                                    if block.is_ordered {
                                        // For ordered lists: only continue if there's meaningful content
                                        found_continuation = has_meaningful_content;
                                    } else {
                                        // For unordered lists: continue regardless
                                        found_continuation = true;
                                    }
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
                    let is_lazy_continuation =
                        last_list_item_line == line_num - 1 && line_info.heading.is_none() && !line_info.is_blank;

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
}

/// Merge adjacent list blocks that should be treated as one
fn merge_adjacent_list_blocks(list_blocks: &mut Vec<ListBlock>, lines: &[LineInfo]) {
    if list_blocks.len() < 2 {
        return;
    }

    let mut merged = Vec::new();
    let mut current = list_blocks[0].clone();

    for next in list_blocks.iter().skip(1) {
        // Check if blocks should be merged
        // For MD032 purposes, consecutive unordered lists with different markers
        // should be treated as one list block only if truly consecutive
        let consecutive = next.start_line == current.end_line + 1;
        let only_blank_between = next.start_line == current.end_line + 2;

        // For ordered lists, we need to be more careful about merging
        let should_merge_condition = if current.is_ordered && next.is_ordered {
            // Only merge ordered lists if there's meaningful content between them
            // (like code blocks, continuation paragraphs) not just blank lines
            consecutive || has_meaningful_content_between(&current, next, lines)
        } else {
            // For unordered lists, use original logic
            consecutive || (only_blank_between && current.marker == next.marker)
        };

        let should_merge = next.is_ordered == current.is_ordered
            && next.blockquote_prefix == current.blockquote_prefix
            && next.nesting_level == current.nesting_level
            && should_merge_condition;

        if should_merge {
            // Merge blocks
            current.end_line = next.end_line;
            current.item_lines.extend_from_slice(&next.item_lines);

            // Update marker consistency
            if !current.is_ordered && current.marker.is_some() && next.marker.is_some() && current.marker != next.marker
            {
                current.marker = None; // Mixed markers
            }
        } else {
            // Save current and start new
            merged.push(current);
            current = next.clone();
        }
    }

    // Don't forget the last block
    merged.push(current);

    *list_blocks = merged;
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

            // Check if this line has proper indentation for list continuation
            let line_indent = line_info.content.len() - line_info.content.trim_start().len();

            // If the line is indented enough to be list continuation content, it's meaningful
            // List content should be indented at least 2 spaces beyond the marker
            if line_indent >= current.nesting_level + 2 {
                return true;
            }

            // Code fences are meaningful content
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                return true;
            }

            // Any other non-blank content at this level breaks the sequence
            return false;
        }
    }

    // Only blank lines between blocks - should not merge
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
