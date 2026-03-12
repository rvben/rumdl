use crate::config::MarkdownFlavor;
use crate::utils::code_block_utils::CodeBlockUtils;
use crate::utils::mkdocs_admonitions;
use crate::utils::mkdocs_tabs;
use crate::utils::regex_cache::URL_SIMPLE_REGEX;
use pulldown_cmark::{Event, Options, Parser};
use regex::Regex;
use std::sync::LazyLock;

use super::types::*;

/// Pattern for email addresses
static BARE_EMAIL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap());

/// Parse all inline code spans in the content using pulldown-cmark streaming parser.
///
/// Note: For MkDocs content, `scan_mkdocs_container_code_spans()` must be called separately
/// to detect code spans that pulldown-cmark misses inside 4-space-indented containers.
/// This is done during LintContext construction in mod.rs.
pub(super) fn parse_code_spans(content: &str, lines: &[LineInfo]) -> Vec<CodeSpan> {
    // Quick check - if no backticks, no code spans
    if !content.contains('`') {
        return Vec::new();
    }

    // Use pulldown-cmark's streaming parser with byte offsets
    let parser = Parser::new(content).into_offset_iter();
    let mut ranges = Vec::new();

    for (event, range) in parser {
        if let Event::Code(_) = event {
            ranges.push((range.start, range.end));
        }
    }

    build_code_spans_from_ranges(content, lines, &ranges)
}

/// Scan MkDocs container lines for code spans that pulldown-cmark missed.
///
/// pulldown-cmark treats 4-space-indented MkDocs content (admonitions, content tabs,
/// markdown HTML blocks) as indented code blocks, so it never emits `Event::Code` for
/// backtick spans within those regions. This function dedents contiguous runs of container
/// lines and reparses them with pulldown-cmark, which correctly handles both single-line
/// and multi-line code spans including all CommonMark edge cases.
pub(super) fn scan_mkdocs_container_code_spans(
    content: &str,
    lines: &[LineInfo],
    existing_ranges: &[(usize, usize)],
) -> Vec<CodeSpan> {
    let mut extra_ranges: Vec<(usize, usize)> = Vec::new();

    // Process contiguous runs of MkDocs container lines
    let mut i = 0;
    while i < lines.len() {
        // Find start of a container run
        if !lines[i].in_mkdocs_container() || lines[i].in_code_block {
            i += 1;
            continue;
        }

        // Collect the contiguous run
        let run_start = i;
        while i < lines.len() && lines[i].in_mkdocs_container() && !lines[i].in_code_block {
            i += 1;
        }
        let run_end = i;

        // Quick check: any backticks in this run?
        let has_backticks = lines[run_start..run_end]
            .iter()
            .any(|li| li.content(content).contains('`'));
        if !has_backticks {
            continue;
        }

        // Compute minimum indentation across content lines only.
        // Container openers (e.g., `=== "Tab"`, `!!! note`) are structural markers
        // that should be excluded from the min_indent calculation. For nested
        // containers (admonition inside content tab), including openers would
        // prevent stripping enough indent from deeply nested content, causing
        // pulldown-cmark to misinterpret it as indented code blocks.
        let min_indent = lines[run_start..run_end]
            .iter()
            .filter(|li| {
                if li.is_blank || li.indent == 0 {
                    return false;
                }
                let line_text = li.content(content);
                // Exclude container openers from min_indent calculation
                if mkdocs_admonitions::is_admonition_start(line_text) || mkdocs_tabs::is_tab_marker(line_text) {
                    return false;
                }
                true
            })
            .map(|li| li.indent)
            .min()
            .unwrap_or(0);

        // Build dedented string and line map for offset translation.
        // Each entry: (byte offset in dedented string, byte offset in original document)
        let mut dedented = String::new();
        let mut line_map: Vec<(usize, usize)> = Vec::new();

        for li in &lines[run_start..run_end] {
            let dedented_line_start = dedented.len();
            let line_content = li.content(content);
            let bytes_to_strip = min_indent.min(li.indent);
            let stripped = &line_content[bytes_to_strip..];
            let original_start = li.byte_offset + bytes_to_strip;
            line_map.push((dedented_line_start, original_start));
            dedented.push_str(stripped);
            dedented.push('\n');
        }

        // Parse the dedented string with pulldown-cmark
        let parser = Parser::new(&dedented).into_offset_iter();
        for (event, range) in parser {
            if let Event::Code(_) = event {
                let orig_start = dedented_to_original(range.start, &line_map);
                let orig_end = dedented_to_original(range.end, &line_map);

                // Skip ranges already detected by the initial pulldown-cmark pass
                let overlaps = existing_ranges.iter().any(|&(s, e)| s < orig_end && e > orig_start);
                if !overlaps {
                    extra_ranges.push((orig_start, orig_end));
                }
            }
        }
    }

    if extra_ranges.is_empty() {
        return Vec::new();
    }

    extra_ranges.sort_unstable_by_key(|&(start, _)| start);
    build_code_spans_from_ranges(content, lines, &extra_ranges)
}

/// Convert a byte offset in the dedented string back to the original document offset.
///
/// `line_map` entries are `(dedented_line_start, original_line_start)`.
fn dedented_to_original(dedented_offset: usize, line_map: &[(usize, usize)]) -> usize {
    // Find the rightmost entry whose dedented_line_start <= dedented_offset
    let idx = line_map
        .partition_point(|&(ds, _)| ds <= dedented_offset)
        .saturating_sub(1);
    let (dedented_line_start, original_line_start) = line_map[idx];
    original_line_start + (dedented_offset - dedented_line_start)
}

pub(super) fn build_code_spans_from_ranges(
    content: &str,
    lines: &[LineInfo],
    ranges: &[(usize, usize)],
) -> Vec<CodeSpan> {
    let mut code_spans = Vec::new();
    if ranges.is_empty() {
        return code_spans;
    }

    for &(start_pos, end_pos) in ranges {
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

    // Sort by position to ensure consistent ordering
    code_spans.sort_by_key(|span| span.byte_offset);

    code_spans
}

/// Parse all math spans (inline $...$ and display $$...$$) using pulldown-cmark
pub(super) fn parse_math_spans(content: &str, lines: &[LineInfo]) -> Vec<MathSpan> {
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

/// Parse HTML tags in the content
pub(super) fn parse_html_tags(
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

        // Find which line this tag is on using binary search
        let line_idx = lines.partition_point(|info| info.byte_offset <= match_start);
        let line_idx = line_idx.saturating_sub(1);
        let line_num = line_idx + 1;
        let col_start = match_start - lines[line_idx].byte_offset;
        let col_end = match_end - lines[line_idx].byte_offset;

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
pub(super) fn parse_table_rows(content: &str, lines: &[LineInfo]) -> Vec<TableRow> {
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

        // Count columns by splitting on pipes, masking escaped and code-span pipes
        let escaped = crate::utils::table_utils::TableUtils::mask_pipes_for_table_parsing(line);
        let masked = crate::utils::table_utils::TableUtils::mask_pipes_in_inline_code(&escaped);
        let parts: Vec<&str> = masked.split('|').collect();
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
pub(super) fn parse_bare_urls(content: &str, lines: &[LineInfo], code_blocks: &[(usize, usize)]) -> Vec<BareUrl> {
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
        // All delimiter characters checked here are ASCII, so byte indexing is safe
        let preceding_byte = if match_start > 0 {
            Some(content.as_bytes()[match_start - 1])
        } else {
            None
        };
        let following_byte = content.as_bytes().get(match_end).copied();

        if preceding_byte == Some(b'<') || preceding_byte == Some(b'(') || preceding_byte == Some(b'[') {
            continue;
        }
        if following_byte == Some(b'>') || following_byte == Some(b')') || following_byte == Some(b']') {
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

        // Find which line this URL is on using binary search
        let line_idx = lines
            .partition_point(|info| info.byte_offset <= match_start)
            .saturating_sub(1);
        let line_num = line_idx + 1;
        let col_start = match_start - lines[line_idx].byte_offset;
        let col_end = match_end - lines[line_idx].byte_offset;

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
        // All delimiter characters checked here are ASCII, so byte indexing is safe
        let preceding_byte = if match_start > 0 {
            Some(content.as_bytes()[match_start - 1])
        } else {
            None
        };
        let following_byte = content.as_bytes().get(match_end).copied();

        if preceding_byte == Some(b'<') || preceding_byte == Some(b'(') || preceding_byte == Some(b'[') {
            continue;
        }
        if following_byte == Some(b'>') || following_byte == Some(b')') || following_byte == Some(b']') {
            continue;
        }

        let email = full_match.as_str();

        // Find which line this email is on using binary search
        let line_idx = lines
            .partition_point(|info| info.byte_offset <= match_start)
            .saturating_sub(1);
        let line_num = line_idx + 1;
        let col_start = match_start - lines[line_idx].byte_offset;
        let col_end = match_end - lines[line_idx].byte_offset;

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
