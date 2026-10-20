use super::source_positions::SourcePositions;
use crate::config::MarkdownFlavor;
use crate::utils::code_block_utils::CodeBlockUtils;
use crate::utils::mkdocs_admonitions;
use crate::utils::mkdocs_tabs;
use crate::utils::parser_options::rumdl_parser_options_for;
use crate::utils::regex_cache::URL_SIMPLE_REGEX;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::sync::LazyLock;

use super::types::*;

/// Pattern for email addresses
static BARE_EMAIL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap());

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
) -> Vec<(usize, usize)> {
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
    extra_ranges
}

/// Scan JSX block content lines for code spans that pulldown-cmark missed.
///
/// When pulldown-cmark processes MDX content, it sees `<ComponentName ...>` and starts
/// an HTML block. Inside HTML blocks pulldown-cmark does not parse inline content, so
/// backtick code spans within JSX component bodies are invisible to the initial parse.
///
/// This function extracts the content lines of each `in_jsx_block` run (skipping the
/// opening and closing tag lines that would re-trigger HTML block parsing), re-parses
/// each run with pulldown-cmark in isolation, and translates the resulting code-span
/// byte offsets back to the original document.
pub(super) fn scan_jsx_block_code_spans(
    content: &str,
    lines: &[LineInfo],
    existing_ranges: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    let mut extra_ranges: Vec<(usize, usize)> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        if !lines[i].in_jsx_block || lines[i].in_code_block {
            i += 1;
            continue;
        }

        // Collect the contiguous in_jsx_block run.
        let run_start = i;
        while i < lines.len() && lines[i].in_jsx_block {
            i += 1;
        }
        let run_end = i;

        // Quick exit: no backticks anywhere in this run.
        let has_backticks = lines[run_start..run_end]
            .iter()
            .any(|li| li.content(content).contains('`'));
        if !has_backticks {
            continue;
        }

        // Build an extracted string from non-tag content lines with an offset line map.
        // Tag lines (those whose trimmed content starts with `<`) are excluded because
        // including them would cause pulldown-cmark to start an HTML block again.
        // Each entry: (byte offset in extracted string, byte offset in original document)
        let mut extracted = String::new();
        let mut line_map: Vec<(usize, usize)> = Vec::new();

        for li in &lines[run_start..run_end] {
            if li.in_code_block {
                continue;
            }
            let line_content = li.content(content);
            let trimmed = line_content.trim_start();

            // Skip opening/closing JSX component tag lines — they would trigger HTML
            // block parsing. Only JSX components have uppercase-initial names, so we
            // match `<Component...>` and `</Component>` but leave lowercase HTML like
            // `<br>` in the extracted string where it belongs.
            {
                let after_bracket = trimmed.strip_prefix('<').unwrap_or("");
                let tag_start = after_bracket.strip_prefix('/').unwrap_or(after_bracket);
                if tag_start.as_bytes().first().is_some_and(u8::is_ascii_uppercase) {
                    continue;
                }
            }

            // If the content is indented 4+ spaces, strip down to 3 to stay below
            // pulldown-cmark's indented-code-block threshold (≥ 4 spaces).
            let bytes_to_strip = if li.indent >= 4 { li.indent - 3 } else { 0 };
            let stripped = &line_content[bytes_to_strip..];
            let original_start = li.byte_offset + bytes_to_strip;

            line_map.push((extracted.len(), original_start));
            extracted.push_str(stripped);
            extracted.push('\n');
        }

        if extracted.is_empty() || !extracted.contains('`') {
            continue;
        }

        // Parse the isolated content — no preceding JSX tag means no HTML block context.
        let parser = Parser::new(&extracted).into_offset_iter();
        for (event, range) in parser {
            if let Event::Code(_) = event {
                let orig_start = dedented_to_original(range.start, &line_map);
                let orig_end = dedented_to_original(range.end, &line_map);

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
    extra_ranges
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
) -> Vec<SourceCodeSpan> {
    let mut code_spans = Vec::new();
    if ranges.is_empty() {
        return code_spans;
    }

    let mut positions = SourcePositions::new(content, lines);
    for &(start_pos, end_pos) in ranges {
        // The range includes the backticks, extract the actual content
        let full_span = &content[start_pos..end_pos];
        let backtick_count = full_span.chars().take_while(|&c| c == '`').count();

        // Extract content between backticks, preserving spaces
        let content_start = start_pos + backtick_count;
        let content_end = end_pos - backtick_count;
        let span_content = if content_start < content_end {
            content_start..content_end
        } else {
            content_start..content_start
        };

        // Use binary search to find line number - O(log n) instead of O(n)
        // Find the rightmost line whose byte_offset <= start_pos
        let line_idx = lines
            .partition_point(|line| line.byte_offset <= start_pos)
            .saturating_sub(1);
        let line_num = line_idx + 1;

        // Find end column using binary search
        let end_line_idx = lines
            .partition_point(|line| line.byte_offset <= end_pos)
            .saturating_sub(1);

        let col_start = positions.column_on_line(line_idx, start_pos);
        let col_end = positions.column_on_line(end_line_idx, end_pos);

        code_spans.push(SourceCodeSpan {
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

    let mut positions = SourcePositions::new(content, lines);
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

        // Find end column using binary search
        let end_line_idx = lines
            .partition_point(|line| line.byte_offset <= end_pos)
            .saturating_sub(1);

        let col_start = positions.column_on_line(line_idx, start_pos);
        let col_end = positions.column_on_line(end_line_idx, end_pos);

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

/// Parse HTML tags in the content.
///
/// Returns `(html_tags, jsx_component_tags)`. JSX component tags (uppercase-named,
/// only under JSX-supporting flavors) are split out rather than dropped, so that
/// HTML-specific rules keep ignoring them while rules that must look inside a
/// component's opening tag (e.g. MD034 attribute URLs) can still consult them.
pub(super) fn parse_html_tags(
    content: &str,
    lines: &[LineInfo],
    code_blocks: &[(usize, usize)],
    flavor: MarkdownFlavor,
) -> (Vec<HtmlTag>, Vec<HtmlTag>) {
    static HTML_TAG_REGEX: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(?i)<(/?)([a-zA-Z][a-zA-Z0-9-]*)(?:\s+[^>]*?)?\s*(/?)>").unwrap());
    static QUOTED_HTML_TAG_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
        use crate::utils::header_id_utils::{HTML_TAG_ATTRIBUTES_PATTERN, HTML_TAG_NAME_PATTERN};
        regex::Regex::new(&format!(
            r"^<(/?)({HTML_TAG_NAME_PATTERN}){HTML_TAG_ATTRIBUTES_PATTERN}\s*(/?)>"
        ))
        .unwrap()
    });

    let bytes = content.as_bytes();
    let content_len = bytes.len();
    let mut html_tags = Vec::new();
    let mut jsx_component_tags = Vec::new();
    let mut search_pos = 0;
    let mut next_gt = None;
    let mut positions = SourcePositions::new(content, lines);

    // Find each '<' and run the regex from that position instead of scanning full content
    while search_pos < content_len {
        let Some(lt_offset) = bytes[search_pos..].iter().position(|&b| b == b'<') else {
            break;
        };
        let lt_pos = search_pos + lt_offset;
        search_pos = lt_pos + 1;

        // CommonMark backslash escapes turn an odd-backslashed `<` into
        // literal text. An even run leaves the `<` active because the
        // backslashes escape in pairs.
        if bytes[..lt_pos].iter().rev().take_while(|&&byte| byte == b'\\').count() % 2 == 1 {
            continue;
        }

        // Quick check: next char after '<' must be '/' or ASCII alpha for a valid tag
        if lt_pos + 1 >= content_len {
            break;
        }
        let next = bytes[lt_pos + 1];
        if next != b'/' && !next.is_ascii_alphabetic() {
            continue;
        }

        // Skip if in code block
        if CodeBlockUtils::is_in_code_block_or_span(code_blocks, lt_pos) {
            continue;
        }

        // Cache the legacy first-'>' window across rejected '<' candidates.
        // With no remaining '>', no remaining tag can match either grammar.
        if next_gt.is_none_or(|position| position < lt_pos) {
            next_gt = bytes[lt_pos..]
                .iter()
                .position(|&byte| byte == b'>')
                .map(|offset| lt_pos + offset);
        }
        let Some(window_end) = next_gt.map(|position| position + 1) else {
            break;
        };

        // Valid quoted attributes can contain '>' and span several lines.
        // Keep the old fragment recognition for template/JSX syntax and
        // malformed tags that the CommonMark attribute grammar cannot parse.
        let captures = QUOTED_HTML_TAG_REGEX
            .captures(&content[lt_pos..])
            .or_else(|| HTML_TAG_REGEX.captures(&content[lt_pos..window_end]));
        if let Some(cap) = captures {
            let full_match = cap.get(0).unwrap();
            // Only accept matches starting at position 0 (the '<' we found)
            if full_match.start() != 0 {
                continue;
            }

            let match_start = lt_pos;
            let match_end = lt_pos + full_match.end();

            let is_closing = !cap.get(1).unwrap().as_str().is_empty();
            let tag_name_original = cap.get(2).unwrap().as_str();
            let tag_name = tag_name_original.to_lowercase();
            let is_self_closing = !cap.get(3).unwrap().as_str().is_empty();

            // Find which line this tag is on using binary search
            let line_idx = lines.partition_point(|info| info.byte_offset <= match_start);
            let line_idx = line_idx.saturating_sub(1);
            let line_num = line_idx + 1;
            let col_start = positions.column_on_line(line_idx, match_start);
            // Multiline tag end columns are clamped to the starting line.
            let col_end = positions.column_on_line(line_idx, match_end);

            let tag = HtmlTag {
                line: line_num,
                start_col: col_start,
                end_col: col_end,
                byte_offset: match_start,
                byte_end: match_end,
                tag_name,
                is_closing,
                is_self_closing,
            };

            // Split JSX component tags (uppercase-named, JSX flavors only) from real
            // HTML tags. Both are recorded in document order, so each list stays sorted
            // by byte offset for binary search.
            if flavor.supports_jsx() && tag_name_original.chars().next().is_some_and(char::is_uppercase) {
                jsx_component_tags.push(tag);
            } else {
                html_tags.push(tag);
            }

            // Advance past the match to avoid overlapping
            search_pos = match_end;
        }
    }

    (html_tags, jsx_component_tags)
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
        });
    }

    // URLs and emails are collected in two separate passes; sort so consumers
    // can binary-search by byte offset.
    bare_urls.sort_by_key(|u| u.byte_offset);

    bare_urls
}

/// Detect lazy continuation lines within list items using pulldown-cmark's SoftBreak events.
///
/// Lazy continuation occurs when text continues a list item paragraph but with less
/// indentation than expected. pulldown-cmark identifies this via SoftBreak followed by Text
/// within a list Item, where the Text starts at a column less than the item's content column.
pub(super) fn detect_lazy_continuation_lines(
    content: &str,
    lines: &[LineInfo],
    line_offsets: &[usize],
) -> Vec<LazyContLine> {
    use crate::utils::blockquote::effective_indent_in_blockquote;

    let mut lazy_lines = Vec::new();
    let parser = Parser::new_ext(content, rumdl_parser_options_for(content));

    // Stack of (expected_indent_within_context, blockquote_level) for nested items
    let mut item_stack: Vec<(usize, usize)> = vec![];
    let mut after_soft_break = false;

    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::Item) => {
                let line_num = byte_to_line(line_offsets, range.start);
                let line_info = lines.get(line_num.saturating_sub(1));
                let line_content = line_info.map_or("", |li| li.content(content));

                // Determine blockquote level from the line content
                let bq_level = line_content
                    .chars()
                    .take_while(|c| *c == '>' || c.is_whitespace())
                    .filter(|&c| c == '>')
                    .count();

                // Calculate expected indent relative to blockquote context
                let expected_indent = if bq_level > 0 {
                    // For blockquote lists, expected indent is the marker width
                    line_info
                        .and_then(|li| li.list_item.as_ref())
                        .map_or(2, |item| item.content_column.saturating_sub(item.marker_column))
                } else {
                    // For regular lists, use content_column directly
                    line_info
                        .and_then(|li| li.list_item.as_ref())
                        .map_or(0, |item| item.content_column)
                };

                item_stack.push((expected_indent, bq_level));
                after_soft_break = false;
            }
            Event::End(TagEnd::Item) => {
                item_stack.pop();
                after_soft_break = false;
            }
            Event::SoftBreak if !item_stack.is_empty() => {
                after_soft_break = true;
            }
            // Detect content starting after a soft break - text, code, or inline formatting
            Event::Text(_)
            | Event::Code(_)
            | Event::Start(Tag::Emphasis)
            | Event::Start(Tag::Strong)
            | Event::Start(Tag::Strikethrough)
            | Event::Start(Tag::Subscript)
            | Event::Start(Tag::Superscript)
            | Event::Start(Tag::Link { .. })
            | Event::Start(Tag::Image { .. })
                if after_soft_break =>
            {
                if let Some(&(expected_indent, expected_bq_level)) = item_stack.last() {
                    let line_num = byte_to_line(line_offsets, range.start);
                    let line_info = lines.get(line_num.saturating_sub(1));
                    let line_content = line_info.map_or("", |li| li.content(content));
                    let fallback_indent = line_info.map_or(0, |li| li.indent);

                    let actual_indent =
                        effective_indent_in_blockquote(line_content, expected_bq_level, fallback_indent);

                    if actual_indent < expected_indent {
                        lazy_lines.push(LazyContLine {
                            line_num,
                            expected_indent,
                            current_indent: actual_indent,
                            blockquote_level: expected_bq_level,
                        });
                    }
                }
                after_soft_break = false;
            }
            _ => {
                after_soft_break = false;
            }
        }
    }

    lazy_lines
}

/// Convert a byte offset to a 1-indexed line number
fn byte_to_line(line_offsets: &[usize], byte_offset: usize) -> usize {
    match line_offsets.binary_search(&byte_offset) {
        Ok(idx) => idx + 1,
        Err(idx) => idx.max(1),
    }
}

#[cfg(test)]
mod column_tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::utils::range_utils::byte_to_char_count;

    /// The former conversion, independent of the checkpoint index.
    fn legacy_position(content: &str, lines: &[LineInfo], byte: usize) -> (usize, usize) {
        let idx = lines.partition_point(|line| line.byte_offset <= byte).saturating_sub(1);
        let line = &lines[idx];
        (
            idx + 1,
            byte_to_char_count(line.content(content), byte - line.byte_offset) - 1,
        )
    }

    #[test]
    fn code_columns_preserve_unicode_multiline_and_unordered_ranges() {
        for newline in ["\n", "\r\n"] {
            let prefix = "é日本🙂\t".repeat(80);
            let content = format!("{prefix}`one` {prefix}``two ` ticks``\n{prefix}`multi\n{prefix}line`\n")
                .replace('\n', newline);
            for flavor in crate::format_proof::ALL_FLAVORS {
                let ctx = LintContext::new(&content, flavor, None);
                // Query the final span first, then revisit earlier lines.
                let ranges: Vec<_> = ctx.inline_code_ranges().iter().copied().rev().collect();
                let spans = build_code_spans_from_ranges(&content, &ctx.lines, &ranges);
                assert_eq!(spans.len(), 3, "{flavor:?}");
                assert!(spans.windows(2).all(|pair| pair[0].byte_offset < pair[1].byte_offset));
                for span in &spans {
                    assert_eq!(
                        (span.line, span.start_col),
                        legacy_position(&content, &ctx.lines, span.byte_offset)
                    );
                    assert_eq!(
                        (span.end_line, span.end_col),
                        legacy_position(&content, &ctx.lines, span.byte_end)
                    );
                }
                assert_eq!(spans[0].content(&content), "one");
                assert_eq!(spans[1].backtick_count, 2);
                assert_eq!(spans[1].content(&content), "two ` ticks");
                assert_eq!(spans[2].content(&content), format!("multi{newline}{prefix}line"));
                assert!(spans[2].end_line > spans[2].line);
            }
        }
    }

    #[test]
    fn math_columns_preserve_unicode_and_multiline_spans() {
        for newline in ["\n", "\r\n"] {
            let prefix = "é日本🙂\t".repeat(80);
            let content = format!("{prefix}$x + y$ {prefix}$z$\n\n$$\n{prefix}x + y\n$$\n").replace('\n', newline);
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            let spans = parse_math_spans(&content, &ctx.lines);
            assert_eq!(spans.len(), 3);
            assert_eq!(spans[0].content, "x + y");
            assert!(!spans[0].is_display);
            assert!(spans[2].is_display);
            assert!(spans[2].end_line > spans[2].line);
            for span in spans {
                assert_eq!(
                    (span.line, span.start_col),
                    legacy_position(&content, &ctx.lines, span.byte_offset)
                );
                assert_eq!(
                    (span.end_line, span.end_col),
                    legacy_position(&content, &ctx.lines, span.byte_end)
                );
            }
        }
    }

    #[test]
    fn html_columns_preserve_start_line_clamping_and_jsx_split() {
        for newline in ["\n", "\r\n"] {
            let prefix = "é日本🙂\t".repeat(80);
            let content = format!("{prefix}<span title=\"long >\n{prefix}value\">x</span> <Card value=\"日本\"/>\n")
                .replace('\n', newline);
            for flavor in crate::format_proof::ALL_FLAVORS {
                let ctx = LintContext::new(&content, flavor, None);
                let (tags, jsx) = parse_html_tags(&content, &ctx.lines, &[], flavor);
                assert_eq!(tags.len() + jsx.len(), 3, "{flavor:?}");
                assert_eq!(jsx.len(), usize::from(flavor.supports_jsx()));
                assert_eq!(tags[0].tag_name, "span");
                assert!(!tags[0].is_closing);
                assert!(tags[1].is_closing);
                // The quoted '>' and newline belong to the opening tag.
                assert!(content[tags[0].byte_offset..tags[0].byte_end].contains('\n'));
                assert_eq!(tags[0].end_col, ctx.lines[0].content(&content).chars().count());
                for tag in tags.iter().chain(&jsx) {
                    assert_eq!(
                        (tag.line, tag.start_col),
                        legacy_position(&content, &ctx.lines, tag.byte_offset)
                    );
                    let line = &ctx.lines[tag.line - 1];
                    assert_eq!(
                        tag.end_col,
                        byte_to_char_count(line.content(&content), tag.byte_end - line.byte_offset) - 1
                    );
                }
            }
        }
    }
}
