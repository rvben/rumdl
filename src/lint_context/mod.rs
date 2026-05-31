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
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::utils::code_block_utils::{CodeBlockDetail, CodeBlockUtils};
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
    pub(super) pandoc_div_ranges: &'a [crate::utils::skip_context::ByteRange],
    pub(super) pymdown_block_ranges: &'a [crate::utils::skip_context::ByteRange],
}

use std::sync::{Arc, OnceLock};

/// Map from line byte offset to list item data: (is_ordered, marker, marker_column, content_column, number)
pub(super) type ListItemMap = std::collections::HashMap<usize, (bool, String, usize, usize, Option<usize>)>;

/// Type alias for byte ranges used in JSX expression and MDX comment detection
pub(super) type ByteRanges = Vec<(usize, usize)>;

pub struct LintContext<'a> {
    pub content: &'a str,
    content_lines: Vec<&'a str>, // Pre-split lines from content (avoids repeated allocations)
    pub line_offsets: Vec<usize>,
    pub code_blocks: Vec<(usize, usize)>, // Cached code block ranges (not including inline code spans)
    pub code_block_details: Vec<CodeBlockDetail>, // Per-block metadata (fenced/indented, info string)
    pub strong_spans: Vec<crate::utils::code_block_utils::StrongSpanDetail>, // Pre-computed strong emphasis spans
    pub line_to_list: crate::utils::code_block_utils::LineToListMap, // Ordered list membership by line
    pub list_start_values: crate::utils::code_block_utils::ListStartValues, // Start values per list ID
    pub lines: Vec<LineInfo>,             // Pre-computed line information
    pub links: Vec<ParsedLink<'a>>,       // Pre-parsed links
    pub images: Vec<ParsedImage<'a>>,     // Pre-parsed images
    pub broken_links: Vec<BrokenLinkInfo>, // Broken/undefined references
    pub footnote_refs: Vec<FootnoteRef>,  // Pre-parsed footnote references
    pub reference_defs: Vec<ReferenceDef>, // Reference definitions
    reference_defs_map: HashMap<String, usize>, // O(1) lookup by lowercase ID -> index in reference_defs
    code_spans_cache: OnceLock<Arc<Vec<CodeSpan>>>, // Lazy-loaded inline code spans
    math_spans_cache: OnceLock<Arc<Vec<MathSpan>>>, // Lazy-loaded math spans ($...$ and $$...$$)
    math_byte_ranges_cache: OnceLock<Vec<(usize, usize)>>, // Lazy-loaded math byte ranges for is_in_math_context
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
    citation_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed Pandoc/Quarto citation ranges (@key, [@key])
    pandoc_div_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed Pandoc/Quarto div block ranges (::: ... :::)
    colon_fence_ranges: Vec<(usize, usize)>, // Pre-computed Azure DevOps colon code fence ranges (:::lang ... :::)
    inline_footnote_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed Pandoc inline footnote ranges (^[...])
    pandoc_header_slugs: std::collections::HashSet<String>, // Pre-computed Pandoc implicit header reference slugs
    example_list_marker_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed Pandoc example-list marker ranges (@) / (@label)
    example_reference_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed Pandoc example reference ranges (@label) inline
    sub_super_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed Pandoc subscript (~x~) and superscript (^x^) ranges
    inline_code_attr_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed Pandoc inline code attribute ranges (`code`{.lang})
    bracketed_span_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed Pandoc bracketed span ranges ([text]{attrs})
    line_block_ranges: Vec<crate::utils::skip_context::ByteRange>,     // Pre-computed Pandoc line block ranges (| text)
    pipe_table_caption_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed Pandoc pipe-table caption ranges (: caption)
    pandoc_metadata_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed Pandoc YAML metadata block ranges (--- ... --- or ...)
    grid_table_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed Pandoc grid-table ranges (+---+---+)
    multi_line_table_ranges: Vec<crate::utils::skip_context::ByteRange>, // Pre-computed Pandoc multi-line table ranges
    shortcode_ranges: Vec<(usize, usize)>, // Pre-computed Hugo/Quarto shortcode ranges ({{< ... >}} and {{% ... %}})
    link_title_ranges: Vec<(usize, usize)>, // Pre-computed sorted link title byte ranges
    code_span_byte_ranges: Vec<(usize, usize)>, // Pre-computed code span byte ranges from pulldown-cmark
    inline_config: InlineConfig,           // Parsed inline configuration comments for rule disabling
    obsidian_comment_ranges: Vec<(usize, usize)>, // Pre-computed Obsidian comment ranges (%%...%%)
    lazy_cont_lines_cache: OnceLock<Arc<Vec<LazyContLine>>>, // Lazy-loaded lazy continuation lines
    myst_directive_ranges: Vec<(usize, usize)>, // Pre-computed MyST colon directive byte ranges (:::{name} ... :::)
    myst_comment_ranges: Vec<(usize, usize)>, // Pre-computed MyST comment byte ranges (% comment)
    myst_role_ranges: Vec<(usize, usize)>, // Pre-computed MyST role byte ranges ({role}`content`)
}

impl<'a> LintContext<'a> {
    pub fn new(content: &'a str, flavor: MarkdownFlavor, source_file: Option<PathBuf>) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let profile = std::env::var("RUMDL_PROFILE_QUADRATIC").is_ok();

        let line_offsets = profile_section!("Line offsets", profile, {
            let mut offsets = vec![0];
            for (i, c) in content.char_indices() {
                if c == '\n' {
                    offsets.push(i + 1);
                }
            }
            offsets
        });

        // Compute content_lines once for all functions that need it
        let content_lines: Vec<&str> = content.lines().collect();

        // Detect front matter boundaries once for all functions that need it
        let front_matter_end = FrontMatterUtils::get_front_matter_end_line(content);

        // Detect code blocks and code spans once and cache them
        let parse_result = profile_section!(
            "Code blocks",
            profile,
            CodeBlockUtils::detect_code_blocks_and_spans(content)
        );
        let mut code_blocks = parse_result.code_blocks;
        let code_span_ranges = parse_result.code_spans;
        let code_block_details = parse_result.code_block_details;
        let strong_spans = parse_result.strong_spans;
        let line_to_list = parse_result.line_to_list;
        let list_start_values = parse_result.list_start_values;

        // Pre-compute HTML comment ranges ONCE for all operations
        let html_comment_ranges = profile_section!(
            "HTML comment ranges",
            profile,
            crate::utils::skip_context::compute_html_comment_ranges(content)
        );

        // Pre-compute autodoc block ranges (avoids O(n^2) scaling)
        // Detected for all flavors except AzureDevOps, where `:::` denotes code fences
        // rather than autodoc directives.
        let autodoc_ranges = profile_section!("Autodoc block ranges", profile, {
            if flavor.supports_colon_code_fences() || flavor.supports_myst_directives() {
                Vec::new()
            } else {
                crate::utils::mkdocstrings_refs::detect_autodoc_block_ranges(content)
            }
        });

        // Pre-compute Pandoc/Quarto div block ranges for Pandoc-compatible flavors
        let pandoc_div_ranges = profile_section!("Pandoc div ranges", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::detect_div_block_ranges(content)
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
            pandoc_div_ranges: &pandoc_div_ranges,
            pymdown_block_ranges: &pymdown_block_ranges,
        };
        let (mut lines, emphasis_spans) = profile_section!(
            "Basic line info",
            profile,
            line_computation::compute_basic_line_info(
                content,
                &content_lines,
                &line_offsets,
                &code_blocks,
                flavor,
                &skip_ranges,
                front_matter_end,
            )
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

        // Detect JSX component blocks in MDX files (e.g. <Tabs>...</Tabs>)
        profile_section!(
            "JSX block detection",
            profile,
            flavor_detection::detect_jsx_blocks(content, &mut lines, flavor)
        );

        // Detect JSX expressions and MDX comments in MDX files
        let (jsx_expression_ranges, mdx_comment_ranges) = profile_section!(
            "JSX/MDX detection",
            profile,
            flavor_detection::detect_jsx_and_mdx_comments(content, &mut lines, flavor, &code_blocks)
        );

        // Detect `<div markdown>`-style HTML blocks (grid cards, etc.) regardless of flavor.
        // The `markdown` attribute is an explicit, author-supplied signal; recognizing it
        // in all flavors keeps `rumdl fmt` from mangling Material grid cards when the
        // MkDocs flavor isn't active.
        profile_section!(
            "Markdown-in-HTML blocks",
            profile,
            flavor_detection::detect_markdown_html_blocks(&content_lines, &mut lines)
        );

        // Detect MkDocs-specific constructs (admonitions, tabs, definition lists)
        profile_section!(
            "MkDocs constructs",
            profile,
            flavor_detection::detect_mkdocs_line_info(&content_lines, &mut lines, flavor)
        );

        // Detect footnote definitions and correct false code block detection.
        // With ENABLE_FOOTNOTES, pulldown-cmark correctly parses multi-line
        // footnotes, but the code block detector may still mark 4-space-indented
        // footnote continuation lines as indented code blocks.
        profile_section!(
            "Footnote definitions",
            profile,
            detect_footnote_definitions(content, &mut lines, &line_offsets)
        );

        // Filter code_blocks to remove false positives from footnote continuation content.
        // Same pattern as MkDocs/JSX corrections below.
        {
            let mut new_code_blocks = Vec::with_capacity(code_blocks.len());
            for &(start, end) in &code_blocks {
                let start_line = line_offsets
                    .partition_point(|&offset| offset <= start)
                    .saturating_sub(1);
                let end_line = line_offsets.partition_point(|&offset| offset < end).min(lines.len());

                let mut sub_start: Option<usize> = None;
                for (i, &offset) in line_offsets[start_line..end_line]
                    .iter()
                    .enumerate()
                    .map(|(j, o)| (j + start_line, o))
                {
                    let is_real_code = lines.get(i).is_some_and(|info| info.in_code_block);
                    if is_real_code && sub_start.is_none() {
                        let byte_start = if i == start_line { start } else { offset };
                        sub_start = Some(byte_start);
                    } else if !is_real_code && sub_start.is_some() {
                        new_code_blocks.push((sub_start.unwrap(), offset));
                        sub_start = None;
                    }
                }
                if let Some(s) = sub_start {
                    new_code_blocks.push((s, end));
                }
            }
            code_blocks = new_code_blocks;
        }

        // Filter code_blocks to remove false positives from MkDocs admonition/tab content
        // and `<div markdown>` HTML blocks (grid cards).
        // pulldown-cmark treats 4-space-indented content as indented code blocks, but inside
        // these containers this is regular markdown content. detect_mkdocs_line_info and
        // detect_markdown_html_blocks already corrected LineInfo.in_code_block for these lines,
        // but the code_blocks byte ranges are still stale. We split ranges rather than using
        // all-or-nothing removal, so fenced code blocks within the containers are preserved.
        let has_markdown_html = lines.iter().any(|l| l.in_mkdocs_html_markdown);
        if flavor == MarkdownFlavor::MkDocs || has_markdown_html {
            let mut new_code_blocks = Vec::with_capacity(code_blocks.len());
            for &(start, end) in &code_blocks {
                let start_line = line_offsets
                    .partition_point(|&offset| offset <= start)
                    .saturating_sub(1);
                let end_line = line_offsets.partition_point(|&offset| offset < end).min(lines.len());

                // Walk lines in this range, collecting sub-ranges where in_code_block is true
                let mut sub_start: Option<usize> = None;
                for (i, &offset) in line_offsets[start_line..end_line]
                    .iter()
                    .enumerate()
                    .map(|(j, o)| (j + start_line, o))
                {
                    let is_real_code = lines.get(i).is_some_and(|info| info.in_code_block);
                    if is_real_code && sub_start.is_none() {
                        let byte_start = if i == start_line { start } else { offset };
                        sub_start = Some(byte_start);
                    } else if !is_real_code && sub_start.is_some() {
                        new_code_blocks.push((sub_start.unwrap(), offset));
                        sub_start = None;
                    }
                }
                if let Some(s) = sub_start {
                    new_code_blocks.push((s, end));
                }
            }
            code_blocks = new_code_blocks;
        }

        // Filter code_blocks for MDX JSX blocks (same pattern as MkDocs above).
        // detect_jsx_blocks already corrected LineInfo.in_code_block for indented content
        // inside JSX component blocks, but code_blocks byte ranges need updating too.
        if flavor.supports_jsx() {
            let mut new_code_blocks = Vec::with_capacity(code_blocks.len());
            for &(start, end) in &code_blocks {
                let start_line = line_offsets
                    .partition_point(|&offset| offset <= start)
                    .saturating_sub(1);
                let end_line = line_offsets.partition_point(|&offset| offset < end).min(lines.len());

                let mut sub_start: Option<usize> = None;
                for (i, &offset) in line_offsets[start_line..end_line]
                    .iter()
                    .enumerate()
                    .map(|(j, o)| (j + start_line, o))
                {
                    let is_real_code = lines.get(i).is_some_and(|info| info.in_code_block);
                    if is_real_code && sub_start.is_none() {
                        let byte_start = if i == start_line { start } else { offset };
                        sub_start = Some(byte_start);
                    } else if !is_real_code && sub_start.is_some() {
                        new_code_blocks.push((sub_start.unwrap(), offset));
                        sub_start = None;
                    }
                }
                if let Some(s) = sub_start {
                    new_code_blocks.push((s, end));
                }
            }
            code_blocks = new_code_blocks;
        }

        // Detect Azure DevOps colon code fences and extend code_blocks so that
        // all byte-range consumers correctly skip their content.
        let colon_fence_ranges = profile_section!(
            "Azure colon fence detection",
            profile,
            flavor_detection::detect_azure_colon_fences(content, &mut lines, flavor)
        );
        if !colon_fence_ranges.is_empty() {
            code_blocks.extend(colon_fence_ranges.iter().copied());
            code_blocks.sort_by_key(|&(start, _)| start);
        }

        // Detect MyST colon directives (:::{name} ... :::) — these are structural
        // containers, NOT code blocks. Content inside is linted as markdown.
        let myst_directive_ranges = profile_section!(
            "MyST colon directives",
            profile,
            flavor_detection::detect_myst_colon_directives(content, &mut lines, flavor)
        );

        // Detect MyST % comments
        let myst_comment_ranges = profile_section!(
            "MyST comments",
            profile,
            flavor_detection::detect_myst_comments(content, &mut lines, flavor)
        );

        // Detect MyST backtick directives (```{name}) and clear in_code_block for
        // content-bearing directives so their body is linted as markdown.
        profile_section!(
            "MyST backtick directives",
            profile,
            flavor_detection::detect_myst_backtick_directives(
                content,
                &mut lines,
                flavor,
                &code_block_details,
                &line_offsets
            )
        );

        // Filter code_blocks to remove false positives from MyST content-bearing directives.
        // Same pattern as MkDocs admonition filtering.
        if flavor.supports_myst_directives() {
            let mut new_code_blocks = Vec::with_capacity(code_blocks.len());
            for &(start, end) in &code_blocks {
                let start_line = line_offsets
                    .partition_point(|&offset| offset <= start)
                    .saturating_sub(1);
                let end_line = line_offsets.partition_point(|&offset| offset < end).min(lines.len());

                let mut sub_start: Option<usize> = None;
                for (i, &offset) in line_offsets[start_line..end_line]
                    .iter()
                    .enumerate()
                    .map(|(j, o)| (j + start_line, o))
                {
                    let is_real_code = lines.get(i).is_some_and(|info| info.in_code_block);
                    if is_real_code && sub_start.is_none() {
                        let byte_start = if i == start_line { start } else { offset };
                        sub_start = Some(byte_start);
                    } else if !is_real_code && sub_start.is_some() {
                        new_code_blocks.push((sub_start.unwrap(), offset));
                        sub_start = None;
                    }
                }
                if let Some(s) = sub_start {
                    new_code_blocks.push((s, end));
                }
            }
            code_blocks = new_code_blocks;
        }

        // Detect kramdown constructs (extension blocks, IALs, ALDs) in kramdown flavor
        profile_section!(
            "Kramdown constructs",
            profile,
            flavor_detection::detect_kramdown_line_info(content, &mut lines, flavor)
        );

        // Layer 1: Sanitize content-derived fields inside kramdown extension blocks
        // so downstream heading detection and collection builders never see them.
        // This must run BEFORE detect_headings_and_blockquotes to prevent headings
        // from being populated inside extension blocks.
        for line in &mut lines {
            if line.in_kramdown_extension_block {
                line.list_item = None;
                line.is_horizontal_rule = false;
                line.blockquote = None;
                line.is_kramdown_block_ial = false;
            }
        }

        // Detect Obsidian comments (%%...%%) in Obsidian flavor
        let obsidian_comment_ranges = profile_section!(
            "Obsidian comments",
            profile,
            flavor_detection::detect_obsidian_comments(content, &mut lines, flavor, &code_span_ranges)
        );

        // Detect MyST role syntax ({role}`content`)
        let myst_role_ranges = profile_section!(
            "MyST roles",
            profile,
            flavor_detection::detect_myst_role_ranges(content, &lines, flavor, &code_blocks)
        );

        // Run pulldown-cmark parse for links, images, and link byte ranges in a single pass.
        // Link byte ranges are needed for heading detection; links/images are finalized later
        // after code_spans are available.
        let pulldown_result = profile_section!(
            "Links, images & link ranges",
            profile,
            link_parser::parse_links_images_pulldown(content, &lines, &code_blocks, flavor, &html_comment_ranges)
        );

        // Now detect headings and blockquotes
        profile_section!(
            "Headings & blockquotes",
            profile,
            heading_detection::detect_headings_and_blockquotes(
                &content_lines,
                &mut lines,
                flavor,
                &html_comment_ranges,
                &pulldown_result.link_byte_ranges,
                front_matter_end,
            )
        );

        // Clear headings that were detected inside kramdown extension blocks
        for line in &mut lines {
            if line.in_kramdown_extension_block {
                line.heading = None;
            }
        }

        // Parse code spans early so we can exclude them from link/image parsing
        let mut code_spans = profile_section!(
            "Code spans",
            profile,
            element_parsers::build_code_spans_from_ranges(content, &lines, &code_span_ranges)
        );

        // Supplement code spans for MkDocs container content that pulldown-cmark missed.
        // pulldown-cmark treats 4-space-indented MkDocs content as indented code blocks,
        // so backtick code spans within admonitions/tabs/markdown HTML are invisible to it.
        if flavor == MarkdownFlavor::MkDocs {
            let extra = profile_section!(
                "MkDocs code spans",
                profile,
                element_parsers::scan_mkdocs_container_code_spans(content, &lines, &code_span_ranges,)
            );
            if !extra.is_empty() {
                code_spans.extend(extra);
                code_spans.sort_by_key(|span| span.byte_offset);
            }
        }

        // Supplement code spans for MDX JSX component body content that pulldown-cmark missed.
        // pulldown-cmark treats JSX component opening tags (e.g. `<ParamField>`) as HTML block
        // starters, so backtick code spans within component bodies are invisible to the initial
        // parse.
        if flavor == MarkdownFlavor::MDX {
            let extra = profile_section!(
                "MDX JSX code spans",
                profile,
                element_parsers::scan_jsx_block_code_spans(content, &lines, &code_span_ranges)
            );
            if !extra.is_empty() {
                code_spans.extend(extra);
                code_spans.sort_by_key(|span| span.byte_offset);
            }
        }

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

        // Finalize links and images: filter by code_spans and run regex fallbacks
        let (links, images, broken_links, footnote_refs) = profile_section!(
            "Links & images finalize",
            profile,
            link_parser::finalize_links_and_images(
                content,
                &lines,
                &code_blocks,
                &code_spans,
                flavor,
                &html_comment_ranges,
                pulldown_result
            )
        );

        let reference_defs = profile_section!(
            "Reference defs",
            profile,
            link_parser::parse_reference_defs(content, &lines)
        );

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

        // Layer 2: Filter pre-computed collections to exclude items inside kramdown extension blocks.
        // Rules that iterate these collections automatically skip kramdown content.
        let links = links
            .into_iter()
            .filter(|link| !lines.get(link.line - 1).is_some_and(|l| l.in_kramdown_extension_block))
            .collect::<Vec<_>>();
        let images = images
            .into_iter()
            .filter(|img| !lines.get(img.line - 1).is_some_and(|l| l.in_kramdown_extension_block))
            .collect::<Vec<_>>();
        let broken_links = broken_links
            .into_iter()
            .filter(|bl| {
                // BrokenLinkInfo has span but no line field; find line from byte offset
                let line_idx = line_offsets
                    .partition_point(|&offset| offset <= bl.span.start)
                    .saturating_sub(1);
                !lines.get(line_idx).is_some_and(|l| l.in_kramdown_extension_block)
            })
            .collect::<Vec<_>>();
        let footnote_refs = footnote_refs
            .into_iter()
            .filter(|fr| !lines.get(fr.line - 1).is_some_and(|l| l.in_kramdown_extension_block))
            .collect::<Vec<_>>();
        let reference_defs = reference_defs
            .into_iter()
            .filter(|def| !lines.get(def.line - 1).is_some_and(|l| l.in_kramdown_extension_block))
            .collect::<Vec<_>>();
        let list_blocks = list_blocks
            .into_iter()
            .filter(|block| {
                !lines
                    .get(block.start_line - 1)
                    .is_some_and(|l| l.in_kramdown_extension_block)
            })
            .collect::<Vec<_>>();
        let table_blocks = table_blocks
            .into_iter()
            .filter(|block| {
                // TableBlock.start_line is 0-indexed
                !lines
                    .get(block.start_line)
                    .is_some_and(|l| l.in_kramdown_extension_block)
            })
            .collect::<Vec<_>>();
        let emphasis_spans = emphasis_spans
            .into_iter()
            .filter(|span| !lines.get(span.line - 1).is_some_and(|l| l.in_kramdown_extension_block))
            .collect::<Vec<_>>();

        // Mark lines covered by a list or table block so is_in_list_block /
        // is_in_table_block are O(1) reads (mirrors in_html_block) instead of
        // scanning the whole block vector on every call.
        for block in &list_blocks {
            // ListBlock line numbers are 1-indexed.
            for line_num in block.start_line..=block.end_line {
                if let Some(li) = lines.get_mut(line_num - 1) {
                    li.in_list_block = true;
                }
            }
        }
        for block in &table_blocks {
            // TableBlock line numbers are 0-indexed.
            for idx in block.start_line..=block.end_line {
                if let Some(li) = lines.get_mut(idx) {
                    li.in_table_block = true;
                }
            }
        }

        // Rebuild reference_defs_map after filtering
        let reference_defs_map: HashMap<String, usize> = reference_defs
            .iter()
            .enumerate()
            .map(|(idx, def)| (def.id.to_lowercase(), idx))
            .collect();

        // Pre-compute sorted link title byte ranges for binary search
        let link_title_ranges: Vec<(usize, usize)> = reference_defs
            .iter()
            .filter_map(|def| match (def.title_byte_start, def.title_byte_end) {
                (Some(start), Some(end)) => Some((start, end)),
                _ => None,
            })
            .collect();

        // Reuse already-computed line_offsets and code_blocks instead of re-detecting
        let line_index = profile_section!(
            "Line index",
            profile,
            crate::utils::range_utils::LineIndex::with_line_starts_and_code_blocks(
                content,
                line_offsets.clone(),
                &code_blocks,
            )
        );

        // Pre-compute Jinja template ranges once for all rules (eliminates O(n*m) in MD011)
        let jinja_ranges = profile_section!(
            "Jinja ranges",
            profile,
            crate::utils::jinja_utils::find_jinja_ranges(content)
        );

        // Pre-compute Pandoc/Quarto citation ranges for Pandoc-compatible flavors
        let citation_ranges = profile_section!("Citation ranges", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::find_citation_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Pandoc inline footnote ranges for Pandoc-compatible flavors
        let inline_footnote_ranges = profile_section!("Inline footnote ranges", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::detect_inline_footnote_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Pandoc implicit header reference slugs for Pandoc-compatible flavors
        let pandoc_header_slugs = profile_section!("Pandoc header slugs", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::collect_pandoc_header_slugs(content)
            } else {
                std::collections::HashSet::new()
            }
        });

        // Pre-compute Pandoc example-list marker ranges for Pandoc-compatible flavors
        let example_list_marker_ranges = profile_section!("Example list markers", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::detect_example_list_marker_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Pandoc example reference ranges for Pandoc-compatible flavors
        let example_reference_ranges = profile_section!("Example references", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::detect_example_reference_ranges(content, &example_list_marker_ranges)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Pandoc subscript (~x~) and superscript (^x^) ranges
        let sub_super_ranges = profile_section!("Subscript/superscript ranges", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::detect_subscript_superscript_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Pandoc inline code attribute ranges (`code`{.lang}) for Pandoc-compatible flavors
        let inline_code_attr_ranges = profile_section!("Inline code attribute ranges", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::detect_inline_code_attr_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Pandoc bracketed span ranges ([text]{attrs}) for Pandoc-compatible flavors
        let bracketed_span_ranges = profile_section!("Bracketed span ranges", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::detect_bracketed_span_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Pandoc line block ranges (| text) for Pandoc-compatible flavors
        let line_block_ranges = profile_section!("Line block ranges", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::detect_line_block_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Pandoc pipe-table caption ranges (: caption) for Pandoc-compatible flavors
        let pipe_table_caption_ranges = profile_section!("Pipe-table caption ranges", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::detect_pipe_table_caption_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Pandoc YAML metadata block ranges (--- ... --- or ...) for Pandoc-compatible flavors
        let pandoc_metadata_ranges = profile_section!("Pandoc metadata ranges", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::detect_yaml_metadata_block_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Pandoc grid-table ranges (+---+---+) for Pandoc-compatible flavors
        let grid_table_ranges = profile_section!("Grid table ranges", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::detect_grid_table_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Pandoc multi-line table ranges for Pandoc-compatible flavors
        let multi_line_table_ranges = profile_section!("Multi-line table ranges", profile, {
            if flavor.is_pandoc_compatible() {
                crate::utils::pandoc::detect_multi_line_table_ranges(content)
            } else {
                Vec::new()
            }
        });

        // Pre-compute Hugo/Quarto shortcode ranges ({{< ... >}} and {{% ... %}})
        let shortcode_ranges = profile_section!("Shortcode ranges", profile, {
            use crate::utils::regex_cache::HUGO_SHORTCODE_REGEX;
            let mut ranges = Vec::new();
            for mat in HUGO_SHORTCODE_REGEX.find_iter(content) {
                ranges.push((mat.start(), mat.end()));
            }
            ranges
        });

        let inline_config = InlineConfig::from_content_with_code_blocks(content, &code_blocks);

        Self {
            content,
            content_lines,
            line_offsets,
            code_blocks,
            code_block_details,
            strong_spans,
            line_to_list,
            list_start_values,
            lines,
            links,
            images,
            broken_links,
            footnote_refs,
            reference_defs,
            reference_defs_map,
            code_spans_cache: OnceLock::from(Arc::new(code_spans)),
            math_spans_cache: OnceLock::new(),       // Lazy-loaded on first access
            math_byte_ranges_cache: OnceLock::new(), // Lazy-loaded on first access
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
            pandoc_div_ranges,
            colon_fence_ranges,
            inline_footnote_ranges,
            pandoc_header_slugs,
            example_list_marker_ranges,
            example_reference_ranges,
            sub_super_ranges,
            inline_code_attr_ranges,
            bracketed_span_ranges,
            line_block_ranges,
            pipe_table_caption_ranges,
            pandoc_metadata_ranges,
            grid_table_ranges,
            multi_line_table_ranges,
            shortcode_ranges,
            link_title_ranges,
            code_span_byte_ranges: code_span_ranges,
            inline_config,
            obsidian_comment_ranges,
            lazy_cont_lines_cache: OnceLock::new(),
            myst_directive_ranges,
            myst_comment_ranges,
            myst_role_ranges,
        }
    }

    /// Binary search for whether `pos` falls inside any range in a sorted, non-overlapping
    /// slice of `(start, end)` byte ranges. O(log n) instead of O(n).
    #[inline]
    fn binary_search_ranges(ranges: &[(usize, usize)], pos: usize) -> bool {
        // Find the rightmost range whose start <= pos
        let idx = ranges.partition_point(|&(start, _)| start <= pos);
        // If idx == 0, no range starts at or before pos
        idx > 0 && pos < ranges[idx - 1].1
    }

    /// Check if a byte position is within a code span. O(log n).
    pub fn is_in_code_span_byte(&self, pos: usize) -> bool {
        Self::binary_search_ranges(&self.code_span_byte_ranges, pos)
    }

    /// Check if `pos` is inside any link byte range. O(log n).
    pub fn is_in_link(&self, pos: usize) -> bool {
        let idx = self.links.partition_point(|link| link.byte_offset <= pos);
        if idx > 0 && pos < self.links[idx - 1].byte_end {
            return true;
        }
        let idx = self.images.partition_point(|img| img.byte_offset <= pos);
        if idx > 0 && pos < self.images[idx - 1].byte_end {
            return true;
        }
        self.is_in_reference_def(pos)
    }

    /// Get parsed inline configuration state.
    pub fn inline_config(&self) -> &InlineConfig {
        &self.inline_config
    }

    /// Byte ranges of Azure DevOps colon code fences (`:::lang … :::`).
    /// Empty for all other flavors.
    pub fn colon_fence_ranges(&self) -> &[(usize, usize)] {
        &self.colon_fence_ranges
    }

    /// Get pre-split content lines, avoiding repeated `content.lines().collect()` allocations.
    ///
    /// Lines are 0-indexed (line 0 corresponds to line number 1 in the document).
    pub fn raw_lines(&self) -> &[&'a str] {
        &self.content_lines
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

    /// Math byte ranges (`$...$` inline and `$$...$$` display), computed once and
    /// cached. Used by `is_in_math_context`; without the cache that helper
    /// rescanned the whole document on every call.
    pub fn math_byte_ranges(&self) -> &[(usize, usize)] {
        self.math_byte_ranges_cache
            .get_or_init(|| crate::utils::skip_context::math_byte_ranges(self.content))
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
        // Binary search: find the last span whose byte_offset <= byte_pos
        let idx = math_spans.partition_point(|span| span.byte_offset <= byte_pos);
        idx > 0 && byte_pos < math_spans[idx - 1].byte_end
    }

    /// Get HTML comment ranges - pre-computed during LintContext construction
    pub fn html_comment_ranges(&self) -> &[crate::utils::skip_context::ByteRange] {
        &self.html_comment_ranges
    }

    /// Check if a byte position is inside an Obsidian comment
    ///
    /// Returns false for non-Obsidian flavors.
    pub fn is_in_obsidian_comment(&self, byte_pos: usize) -> bool {
        Self::binary_search_ranges(&self.obsidian_comment_ranges, byte_pos)
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

    /// Get byte ranges of MyST colon directive blocks
    pub fn myst_directive_ranges(&self) -> &[(usize, usize)] {
        &self.myst_directive_ranges
    }

    /// Check if a byte position is inside a MyST role (`{role}`content``)
    pub fn is_in_myst_role(&self, byte_pos: usize) -> bool {
        Self::binary_search_ranges(&self.myst_role_ranges, byte_pos)
    }

    /// Check if a byte position is inside a MyST comment (`% comment`)
    pub fn is_in_myst_comment(&self, byte_pos: usize) -> bool {
        Self::binary_search_ranges(&self.myst_comment_ranges, byte_pos)
    }

    /// Check if a line (1-indexed) is a MyST colon-fence directive opener (`:::{name} ...`).
    ///
    /// The text after `{name}` on an opener is the directive's argument (an opaque
    /// path, URL, or label), not markdown prose. Rules that reformat prose should
    /// skip these lines. Returns false for non-MyST flavors and for directive body
    /// or closer lines.
    pub fn is_myst_colon_directive_opener_line(&self, line_num: usize) -> bool {
        if !self.flavor.supports_myst_directives() {
            return false;
        }
        self.lines.get(line_num.wrapping_sub(1)).is_some_and(|info| {
            info.in_myst_directive
                && flavor_detection::myst_colon_directive_opener(info.content(self.content)).is_some()
        })
    }

    /// Get HTML tags - computed lazily on first access
    pub fn html_tags(&self) -> Arc<Vec<HtmlTag>> {
        Arc::clone(self.html_tags_cache.get_or_init(|| {
            let tags = element_parsers::parse_html_tags(self.content, &self.lines, &self.code_blocks, self.flavor);
            // Filter out HTML tags inside kramdown extension blocks
            Arc::new(
                tags.into_iter()
                    .filter(|tag| {
                        !self
                            .lines
                            .get(tag.line - 1)
                            .is_some_and(|l| l.in_kramdown_extension_block)
                    })
                    .collect(),
            )
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

    /// Get lazy continuation lines - computed lazily on first access
    pub fn lazy_continuation_lines(&self) -> Arc<Vec<LazyContLine>> {
        Arc::clone(self.lazy_cont_lines_cache.get_or_init(|| {
            Arc::new(element_parsers::detect_lazy_continuation_lines(
                self.content,
                &self.lines,
                &self.line_offsets,
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
                || line_info.in_mdx_comment
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

    /// Check if a position is within a code block or code span. O(log n).
    pub fn is_in_code_block_or_span(&self, pos: usize) -> bool {
        // Check code blocks first (already uses binary search internally)
        if CodeBlockUtils::is_in_code_block_or_span(&self.code_blocks, pos) {
            return true;
        }

        // Check inline code spans via binary search
        self.is_byte_offset_in_code_span(pos)
    }

    /// Get line information by line number (1-indexed)
    pub fn line_info(&self, line_num: usize) -> Option<&LineInfo> {
        if line_num > 0 {
            self.lines.get(line_num - 1)
        } else {
            None
        }
    }

    /// Get URL for a reference link/image by its ID (O(1) lookup via HashMap)
    pub fn get_reference_url(&self, ref_id: &str) -> Option<&str> {
        let normalized_id = ref_id.to_lowercase();
        self.reference_defs_map
            .get(&normalized_id)
            .map(|&idx| self.reference_defs[idx].url.as_str())
    }

    /// Check if a line is part of a list block
    pub fn is_in_list_block(&self, line_num: usize) -> bool {
        if line_num == 0 || line_num > self.lines.len() {
            return false;
        }
        self.lines[line_num - 1].in_list_block
    }

    /// Check if a line is within an HTML block
    pub fn is_in_html_block(&self, line_num: usize) -> bool {
        if line_num == 0 || line_num > self.lines.len() {
            return false;
        }
        self.lines[line_num - 1].in_html_block
    }

    /// Check if a 1-indexed line number is inside a GFM table block.
    ///
    /// Returns `true` for the header line, delimiter line, and all body rows.
    /// `TableBlock` spans are stored 0-indexed; this helper accepts the
    /// 1-indexed line numbers used elsewhere in the rule API.
    pub fn is_in_table_block(&self, line_num: usize) -> bool {
        if line_num == 0 || line_num > self.lines.len() {
            return false;
        }
        self.lines[line_num - 1].in_table_block
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

    /// Check if a byte offset is within a code span. O(log n).
    #[inline]
    pub fn is_byte_offset_in_code_span(&self, byte_offset: usize) -> bool {
        let code_spans = self.code_spans();
        let idx = code_spans.partition_point(|span| span.byte_offset <= byte_offset);
        idx > 0 && byte_offset < code_spans[idx - 1].byte_end
    }

    /// Check if a byte position is within a reference definition. O(log n).
    #[inline]
    pub fn is_in_reference_def(&self, byte_pos: usize) -> bool {
        let idx = self.reference_defs.partition_point(|rd| rd.byte_offset <= byte_pos);
        idx > 0 && byte_pos < self.reference_defs[idx - 1].byte_end
    }

    /// Check if a byte position is within an HTML comment. O(log n).
    #[inline]
    pub fn is_in_html_comment(&self, byte_pos: usize) -> bool {
        let idx = self.html_comment_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.html_comment_ranges[idx - 1].end
    }

    /// Check if a byte position is within an HTML tag (including multiline tags).
    /// Uses the pre-parsed html_tags which correctly handles tags spanning multiple lines. O(log n).
    #[inline]
    pub fn is_in_html_tag(&self, byte_pos: usize) -> bool {
        let tags = self.html_tags();
        let idx = tags.partition_point(|tag| tag.byte_offset <= byte_pos);
        idx > 0 && byte_pos < tags[idx - 1].byte_end
    }

    /// Check if a byte position is within a Jinja template ({{ }} or {% %}). O(log n).
    pub fn is_in_jinja_range(&self, byte_pos: usize) -> bool {
        Self::binary_search_ranges(&self.jinja_ranges, byte_pos)
    }

    /// Check if a byte position is within a JSX expression (MDX: {expression}). O(log n).
    #[inline]
    pub fn is_in_jsx_expression(&self, byte_pos: usize) -> bool {
        Self::binary_search_ranges(&self.jsx_expression_ranges, byte_pos)
    }

    /// Check if a byte position is within an MDX comment ({/* ... */}). O(log n).
    #[inline]
    pub fn is_in_mdx_comment(&self, byte_pos: usize) -> bool {
        Self::binary_search_ranges(&self.mdx_comment_ranges, byte_pos)
    }

    /// Check if a byte position is within a Pandoc/Quarto citation (`@key` or `[@key]`).
    /// Active for Pandoc-compatible flavors. O(log n).
    #[inline]
    pub fn is_in_citation(&self, byte_pos: usize) -> bool {
        let idx = self.citation_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.citation_ranges[idx - 1].end
    }

    /// Pre-computed Pandoc/Quarto citation ranges.
    #[inline]
    pub fn citation_ranges(&self) -> &[crate::utils::skip_context::ByteRange] {
        &self.citation_ranges
    }

    /// Check if a byte position is within a Pandoc/Quarto div block (`::: ... :::`).
    /// Active for Pandoc-compatible flavors. O(log n) via binary search over sorted ranges.
    #[inline]
    pub fn is_in_div_block(&self, byte_pos: usize) -> bool {
        let idx = self.pandoc_div_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.pandoc_div_ranges[idx - 1].end
    }

    /// Check if a byte position is within a Pandoc inline footnote (`^[note text]`).
    /// Active for Pandoc-compatible flavors. O(log n).
    #[inline]
    pub fn is_in_inline_footnote(&self, byte_pos: usize) -> bool {
        let idx = self.inline_footnote_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.inline_footnote_ranges[idx - 1].end
    }

    /// Check if a byte position is within a Pandoc example-list marker (`(@)` /
    /// `(@label)` at line start). Active for Pandoc-compatible flavors. O(log n).
    #[inline]
    pub fn is_in_example_list_marker(&self, byte_pos: usize) -> bool {
        let idx = self.example_list_marker_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.example_list_marker_ranges[idx - 1].end
    }

    /// Check if a byte position is within a Pandoc example reference (`(@label)`
    /// inline). Active for Pandoc-compatible flavors. O(log n).
    #[inline]
    pub fn is_in_example_reference(&self, byte_pos: usize) -> bool {
        let idx = self.example_reference_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.example_reference_ranges[idx - 1].end
    }

    /// Check if a byte position is within a Pandoc subscript (`~x~`) or
    /// superscript (`^x^`) span. Active for Pandoc-compatible flavors. O(log n).
    #[inline]
    pub fn is_in_subscript_or_superscript(&self, byte_pos: usize) -> bool {
        let idx = self.sub_super_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.sub_super_ranges[idx - 1].end
    }

    /// Check if a byte position is within a Pandoc inline-code attribute block
    /// (`{.lang}` immediately following `` `code` ``). Active for Pandoc-compatible
    /// flavors. O(log n).
    #[inline]
    pub fn is_in_inline_code_attr(&self, byte_pos: usize) -> bool {
        let idx = self.inline_code_attr_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.inline_code_attr_ranges[idx - 1].end
    }

    /// Check if a byte position is within a Pandoc bracketed span (`[text]{attrs}`).
    /// Active for Pandoc-compatible flavors. O(log n).
    #[inline]
    pub fn is_in_bracketed_span(&self, byte_pos: usize) -> bool {
        let idx = self.bracketed_span_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.bracketed_span_ranges[idx - 1].end
    }

    /// Returns true if `byte_pos` falls inside a Pandoc line block (`| text`).
    /// Active for Pandoc-compatible flavors. O(log n).
    #[inline]
    pub fn is_in_line_block(&self, byte_pos: usize) -> bool {
        let idx = self.line_block_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.line_block_ranges[idx - 1].end
    }

    /// Returns true if `byte_pos` falls inside a Pandoc pipe-table caption
    /// (`: caption` adjacent to a pipe table). Active for Pandoc-compatible
    /// flavors. O(log n).
    #[inline]
    pub fn is_in_pipe_table_caption(&self, byte_pos: usize) -> bool {
        let idx = self.pipe_table_caption_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.pipe_table_caption_ranges[idx - 1].end
    }

    /// Returns true if `byte_pos` falls inside a Pandoc YAML metadata block.
    /// Active for Pandoc-compatible flavors. O(log n).
    #[inline]
    pub fn is_in_pandoc_metadata(&self, byte_pos: usize) -> bool {
        let idx = self.pandoc_metadata_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.pandoc_metadata_ranges[idx - 1].end
    }

    /// Returns true if `byte_pos` falls inside a Pandoc grid table.
    /// Active for Pandoc-compatible flavors. O(log n).
    #[inline]
    pub fn is_in_grid_table(&self, byte_pos: usize) -> bool {
        let idx = self.grid_table_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.grid_table_ranges[idx - 1].end
    }

    /// Returns true if `byte_pos` falls inside a Pandoc multi-line table.
    /// Active for Pandoc-compatible flavors. O(log n).
    #[inline]
    pub fn is_in_multi_line_table(&self, byte_pos: usize) -> bool {
        let idx = self.multi_line_table_ranges.partition_point(|r| r.start <= byte_pos);
        idx > 0 && byte_pos < self.multi_line_table_ranges[idx - 1].end
    }

    /// Returns true if `link_text`, after Pandoc slugification, matches a heading
    /// in the document. Returns false for non-Pandoc-compatible flavors because
    /// the `pandoc_header_slugs` set is empty when the pre-pass detector is gated
    /// off. Use this when the caller has raw bracketed text (`[Section name]`).
    pub fn matches_implicit_header_reference(&self, link_text: &str) -> bool {
        let slug = crate::utils::pandoc::pandoc_header_slug(link_text);
        self.pandoc_header_slugs.contains(&slug)
    }

    /// Returns true if `slug` (already in Pandoc-slug form) matches a heading
    /// in the document. Returns false for non-Pandoc-compatible flavors because
    /// the `pandoc_header_slugs` set is empty when the pre-pass detector is gated
    /// off. Use this when the caller already has a slug (e.g. the fragment of a
    /// URL after `#`). O(1).
    #[inline]
    pub fn has_pandoc_slug(&self, slug: &str) -> bool {
        self.pandoc_header_slugs.contains(slug)
    }

    /// Check if a byte position is within a Hugo/Quarto shortcode ({{< ... >}} or {{% ... %}}). O(log n).
    #[inline]
    pub fn is_in_shortcode(&self, byte_pos: usize) -> bool {
        Self::binary_search_ranges(&self.shortcode_ranges, byte_pos)
    }

    /// Pre-computed Hugo/Quarto shortcode ranges.
    #[inline]
    pub fn shortcode_ranges(&self) -> &[(usize, usize)] {
        &self.shortcode_ranges
    }

    /// Check if a byte position is within a link reference definition title. O(log n).
    pub fn is_in_link_title(&self, byte_pos: usize) -> bool {
        Self::binary_search_ranges(&self.link_title_ranges, byte_pos)
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
        self.char_frequency.hash_count > 0 || self.char_frequency.hyphen_count > 2 || self.content.contains('=') // Setext H1 underlines use '='
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

/// Detect footnote definitions and mark their continuation lines.
///
/// Uses pulldown-cmark to find footnote definition ranges and fenced code
/// blocks within them, then:
/// 1. Sets `in_footnote_definition = true` on all lines within
/// 2. Clears `in_code_block = false` on continuation lines that were
///    misidentified as indented code blocks (but preserves real fenced
///    code blocks within footnotes)
fn detect_footnote_definitions(content: &str, lines: &mut [types::LineInfo], line_offsets: &[usize]) {
    use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};

    let options = crate::utils::rumdl_parser_options();
    let parser = Parser::new_ext(content, options).into_offset_iter();

    // Collect footnote ranges and fenced code block ranges within them
    let mut footnote_ranges: Vec<(usize, usize)> = Vec::new();
    let mut fenced_code_ranges: Vec<(usize, usize)> = Vec::new();
    let mut in_footnote = false;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::FootnoteDefinition(_)) => {
                in_footnote = true;
                footnote_ranges.push((range.start, range.end));
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                in_footnote = false;
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_))) if in_footnote => {
                fenced_code_ranges.push((range.start, range.end));
            }
            _ => {}
        }
    }

    let byte_to_line = |byte_offset: usize| -> usize {
        line_offsets
            .partition_point(|&offset| offset <= byte_offset)
            .saturating_sub(1)
    };

    // Mark footnote definition lines
    for &(start, end) in &footnote_ranges {
        let start_line = byte_to_line(start);
        let end_line = line_offsets.partition_point(|&offset| offset < end).min(lines.len());

        for line in &mut lines[start_line..end_line] {
            line.in_footnote_definition = true;
            line.in_code_block = false;
        }
    }

    // Restore in_code_block for fenced code blocks within footnotes
    for &(start, end) in &fenced_code_ranges {
        let start_line = byte_to_line(start);
        let end_line = line_offsets.partition_point(|&offset| offset < end).min(lines.len());

        for line in &mut lines[start_line..end_line] {
            line.in_code_block = true;
        }
    }
}
