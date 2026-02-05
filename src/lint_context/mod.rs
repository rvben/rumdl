pub mod types;
pub use types::*;

mod element_parsers;
mod flavor_detection;
mod heading_detection;
mod line_computation;
mod link_parser;
mod list_blocks;
#[cfg(test)]
mod tests;

use crate::config::MarkdownFlavor;
use crate::inline_config::InlineConfig;
use crate::utils::code_block_utils::CodeBlockUtils;
use std::collections::HashMap;
use std::path::PathBuf;

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

/// Grouped byte ranges for skip context detection
/// Used to reduce parameter count in internal functions
pub(super) struct SkipByteRanges<'a> {
    pub(super) html_comment_ranges: &'a [crate::utils::skip_context::ByteRange],
    pub(super) autodoc_ranges: &'a [crate::utils::skip_context::ByteRange],
    pub(super) quarto_div_ranges: &'a [crate::utils::skip_context::ByteRange],
    pub(super) pymdown_block_ranges: &'a [crate::utils::skip_context::ByteRange],
}

use std::sync::{Arc, OnceLock};

/// Map from line byte offset to list item data: (is_ordered, marker, marker_column, content_column, number)
pub(super) type ListItemMap = std::collections::HashMap<usize, (bool, String, usize, usize, Option<usize>)>;

/// Type alias for byte ranges used in JSX expression and MDX comment detection
pub(super) type ByteRanges = Vec<(usize, usize)>;

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
    citation_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed Pandoc/Quarto citation ranges (Quarto: @key, [@key])
    shortcode_ranges: Vec<(usize, usize)>, // Pre-computed Hugo/Quarto shortcode ranges ({{< ... >}} and {{% ... %}})
    inline_config: InlineConfig,           // Parsed inline configuration comments for rule disabling
    obsidian_comment_ranges: Vec<(usize, usize)>, // Pre-computed Obsidian comment ranges (%%...%%)
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

        // Detect code blocks and code spans once and cache them
        let (code_blocks, code_span_ranges) = profile_section!(
            "Code blocks",
            profile,
            CodeBlockUtils::detect_code_blocks_and_spans(content)
        );

        // Pre-compute HTML comment ranges ONCE for all operations
        let html_comment_ranges = profile_section!(
            "HTML comment ranges",
            profile,
            crate::utils::skip_context::compute_html_comment_ranges(content)
        );

        // Pre-compute autodoc block ranges for MkDocs flavor (avoids O(n^2) scaling)
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

        // Pre-compute PyMdown Blocks ranges for MkDocs flavor (/// ... ///)
        let pymdown_block_ranges = profile_section!("PyMdown block ranges", profile, {
            if flavor == MarkdownFlavor::MkDocs {
                crate::utils::pymdown_blocks::detect_block_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute line information AND emphasis spans (without headings/blockquotes yet)
        // Emphasis spans are captured during the same pulldown-cmark parse as list detection
        let skip_ranges = SkipByteRanges {
            html_comment_ranges: &html_comment_ranges,
            autodoc_ranges: &autodoc_ranges,
            quarto_div_ranges: &quarto_div_ranges,
            pymdown_block_ranges: &pymdown_block_ranges,
        };
        let (mut lines, emphasis_spans) = profile_section!(
            "Basic line info",
            profile,
            line_computation::compute_basic_line_info(content, &line_offsets, &code_blocks, flavor, &skip_ranges,)
        );

        // Detect HTML blocks BEFORE heading detection
        profile_section!(
            "HTML blocks",
            profile,
            heading_detection::detect_html_blocks(content, &mut lines)
        );

        // Detect ESM import/export blocks in MDX files BEFORE heading detection
        profile_section!(
            "ESM blocks",
            profile,
            flavor_detection::detect_esm_blocks(content, &mut lines, flavor)
        );

        // Detect JSX expressions and MDX comments in MDX files
        let (jsx_expression_ranges, mdx_comment_ranges) = profile_section!(
            "JSX/MDX detection",
            profile,
            flavor_detection::detect_jsx_and_mdx_comments(content, &mut lines, flavor, &code_blocks)
        );

        // Detect MkDocs-specific constructs (admonitions, tabs, definition lists)
        profile_section!(
            "MkDocs constructs",
            profile,
            flavor_detection::detect_mkdocs_line_info(content, &mut lines, flavor)
        );

        // Detect Obsidian comments (%%...%%) in Obsidian flavor
        let obsidian_comment_ranges = profile_section!(
            "Obsidian comments",
            profile,
            flavor_detection::detect_obsidian_comments(content, &mut lines, flavor, &code_span_ranges)
        );

        // Collect link byte ranges early for heading detection (to skip lines inside link syntax)
        let link_byte_ranges = profile_section!(
            "Link byte ranges",
            profile,
            link_parser::collect_link_byte_ranges(content)
        );

        // Now detect headings and blockquotes
        profile_section!(
            "Headings & blockquotes",
            profile,
            heading_detection::detect_headings_and_blockquotes(
                content,
                &mut lines,
                flavor,
                &html_comment_ranges,
                &link_byte_ranges
            )
        );

        // Parse code spans early so we can exclude them from link/image parsing
        let code_spans = profile_section!(
            "Code spans",
            profile,
            element_parsers::build_code_spans_from_ranges(content, &lines, &code_span_ranges)
        );

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
            link_parser::parse_links(content, &lines, &code_blocks, &code_spans, flavor, &html_comment_ranges)
        );

        let images = profile_section!(
            "Images",
            profile,
            link_parser::parse_images(content, &lines, &code_blocks, &code_spans, &html_comment_ranges)
        );

        let reference_defs = profile_section!(
            "Reference defs",
            profile,
            link_parser::parse_reference_defs(content, &lines)
        );

        // Build O(1) lookup map for reference definitions by lowercase ID
        let reference_defs_map: HashMap<String, usize> = reference_defs
            .iter()
            .enumerate()
            .map(|(idx, def)| (def.id.to_lowercase(), idx))
            .collect();

        let list_blocks = profile_section!("List blocks", profile, list_blocks::parse_list_blocks(content, &lines));

        // Compute character frequency for fast content analysis
        let char_frequency = profile_section!(
            "Char frequency",
            profile,
            line_computation::compute_char_frequency(content)
        );

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

        // Pre-compute Jinja template ranges once for all rules (eliminates O(n*m) in MD011)
        let jinja_ranges = profile_section!(
            "Jinja ranges",
            profile,
            crate::utils::jinja_utils::find_jinja_ranges(content)
        );

        // Pre-compute Pandoc/Quarto citation ranges for Quarto flavor
        let citation_ranges = profile_section!("Citation ranges", profile, {
            if flavor == MarkdownFlavor::Quarto {
                crate::utils::quarto_divs::find_citation_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Hugo/Quarto shortcode ranges ({{< ... >}} and {{% ... %}})
        let shortcode_ranges = profile_section!("Shortcode ranges", profile, {
            use crate::utils::regex_cache::HUGO_SHORTCODE_REGEX;
            let mut ranges = Vec::new();
            for mat in HUGO_SHORTCODE_REGEX.find_iter(content).flatten() {
                ranges.push((mat.start(), mat.end()));
            }
            ranges
        });

        let inline_config = InlineConfig::from_content_with_code_blocks(content, &code_blocks);

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
            citation_ranges,
            shortcode_ranges,
            inline_config,
            obsidian_comment_ranges,
        }
    }

    /// Check if a rule is disabled at a specific line number (1-indexed)
    ///
    /// This method checks both persistent disable comments (<!-- rumdl-disable -->)
    /// and line-specific comments (<!-- rumdl-disable-line -->, <!-- rumdl-disable-next-line -->).
    pub fn is_rule_disabled(&self, rule_name: &str, line_number: usize) -> bool {
        self.inline_config.is_rule_disabled(rule_name, line_number)
    }

    /// Get code spans - computed lazily on first access
    pub fn code_spans(&self) -> Arc<Vec<CodeSpan>> {
        Arc::clone(
            self.code_spans_cache
                .get_or_init(|| Arc::new(element_parsers::parse_code_spans(self.content, &self.lines))),
        )
    }

    /// Get math spans - computed lazily on first access
    pub fn math_spans(&self) -> Arc<Vec<MathSpan>> {
        Arc::clone(
            self.math_spans_cache
                .get_or_init(|| Arc::new(element_parsers::parse_math_spans(self.content, &self.lines))),
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

    /// Get Obsidian comment ranges - pre-computed during LintContext construction
    /// Returns empty slice for non-Obsidian flavors
    pub fn obsidian_comment_ranges(&self) -> &[(usize, usize)] {
        &self.obsidian_comment_ranges
    }

    /// Check if a byte position is inside an Obsidian comment
    ///
    /// Returns false for non-Obsidian flavors.
    pub fn is_in_obsidian_comment(&self, byte_pos: usize) -> bool {
        self.obsidian_comment_ranges
            .iter()
            .any(|(start, end)| byte_pos >= *start && byte_pos < *end)
    }

    /// Check if a line/column position is inside an Obsidian comment
    ///
    /// Line number is 1-indexed, column is 1-indexed.
    /// Returns false for non-Obsidian flavors.
    pub fn is_position_in_obsidian_comment(&self, line_num: usize, col: usize) -> bool {
        if self.obsidian_comment_ranges.is_empty() {
            return false;
        }

        // Convert line/column (1-indexed, char-based) to byte position
        let byte_pos = self.line_index.line_col_to_byte_range(line_num, col).start;
        self.is_in_obsidian_comment(byte_pos)
    }

    /// Get HTML tags - computed lazily on first access
    pub fn html_tags(&self) -> Arc<Vec<HtmlTag>> {
        Arc::clone(self.html_tags_cache.get_or_init(|| {
            Arc::new(element_parsers::parse_html_tags(
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
                .get_or_init(|| Arc::new(element_parsers::parse_table_rows(self.content, &self.lines))),
        )
    }

    /// Get bare URLs - computed lazily on first access
    pub fn bare_urls(&self) -> Arc<Vec<BareUrl>> {
        Arc::clone(self.bare_urls_cache.get_or_init(|| {
            Arc::new(element_parsers::parse_bare_urls(
                self.content,
                &self.lines,
                &self.code_blocks,
            ))
        }))
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
    #[inline]
    pub fn is_in_reference_def(&self, byte_pos: usize) -> bool {
        self.reference_defs
            .iter()
            .any(|ref_def| byte_pos >= ref_def.byte_offset && byte_pos < ref_def.byte_end)
    }

    /// Check if a byte position is within an HTML comment
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

    /// Check if a byte position is within a Pandoc/Quarto citation (`@key` or `[@key]`)
    /// Only active in Quarto flavor
    #[inline]
    pub fn is_in_citation(&self, byte_pos: usize) -> bool {
        self.citation_ranges
            .iter()
            .any(|range| byte_pos >= range.start && byte_pos < range.end)
    }

    /// Get all citation byte ranges (Quarto flavor only)
    pub fn citation_ranges(&self) -> &[crate::utils::skip_context::ByteRange] {
        &self.citation_ranges
    }

    /// Check if a byte position is within a Hugo/Quarto shortcode ({{< ... >}} or {{% ... %}})
    #[inline]
    pub fn is_in_shortcode(&self, byte_pos: usize) -> bool {
        self.shortcode_ranges
            .iter()
            .any(|(start, end)| byte_pos >= *start && byte_pos < *end)
    }

    /// Get all shortcode byte ranges
    pub fn shortcode_ranges(&self) -> &[(usize, usize)] {
        &self.shortcode_ranges
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

    /// Get an iterator over valid headings (skipping invalid ones like `#NoSpace`)
    ///
    /// Valid headings have proper spacing after the `#` markers (or are level > 1).
    /// This is the standard iterator for rules that need to process headings.
    ///
    /// # Examples
    ///
    /// ```
    /// use rumdl::lint_context::LintContext;
    /// use rumdl::config::MarkdownFlavor;
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
