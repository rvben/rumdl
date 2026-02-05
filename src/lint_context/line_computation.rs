use crate::config::MarkdownFlavor;
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::utils::code_block_utils::CodeBlockUtils;
use crate::utils::element_cache::ElementCache;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use super::types::*;
use super::{ListItemMap, SkipByteRanges};

/// Pre-compute basic line information (without headings/blockquotes)
/// Also returns emphasis spans detected during the pulldown-cmark parse
pub(super) fn compute_basic_line_info(
    content: &str,
    line_offsets: &[usize],
    code_blocks: &[(usize, usize)],
    flavor: MarkdownFlavor,
    skip_ranges: &SkipByteRanges<'_>,
) -> (Vec<LineInfo>, Vec<EmphasisSpan>) {
    let content_lines: Vec<&str> = content.lines().collect();
    let mut lines = Vec::with_capacity(content_lines.len());

    // Pre-compute which lines are in code blocks
    let code_block_map = compute_code_block_line_map(content, line_offsets, code_blocks);

    // Pre-compute which lines are in math blocks ($$ ... $$)
    let math_block_map = compute_math_block_line_map(content, &code_block_map);

    // Detect front matter boundaries FIRST, before any other parsing
    let front_matter_end = FrontMatterUtils::get_front_matter_end_line(content);

    // Use pulldown-cmark to detect list items AND emphasis spans in a single pass
    let (list_item_map, emphasis_spans) =
        detect_list_items_and_emphasis_with_pulldown(content, line_offsets, flavor, front_matter_end, code_blocks);

    for (i, line) in content_lines.iter().enumerate() {
        let byte_offset = line_offsets.get(i).copied().unwrap_or(0);
        let indent = line.len() - line.trim_start().len();
        // Compute visual indent with proper CommonMark tab expansion
        let visual_indent = ElementCache::calculate_indentation_width_default(line);

        // Parse blockquote prefix once and reuse it (avoid redundant parsing)
        let blockquote_parse = parse_blockquote_prefix(line);

        // For blank detection, consider blockquote context
        let is_blank = if let Some((_, content)) = blockquote_parse {
            content.trim().is_empty()
        } else {
            line.trim().is_empty()
        };

        // Use pre-computed map for O(1) lookup instead of O(m) iteration
        let in_code_block = code_block_map.get(i).copied().unwrap_or(false);

        // Detect list items (skip if in frontmatter, in mkdocstrings block, or in HTML comment)
        let in_mkdocstrings = flavor == MarkdownFlavor::MkDocs
            && crate::utils::mkdocstrings_refs::is_within_autodoc_block_ranges(skip_ranges.autodoc_ranges, byte_offset);
        let line_end_offset = byte_offset + line.len();
        let in_html_comment = crate::utils::skip_context::is_line_entirely_in_html_comment(
            skip_ranges.html_comment_ranges,
            byte_offset,
            line_end_offset,
        );
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

        let in_front_matter = front_matter_end > 0 && i < front_matter_end;
        let is_hr = !in_code_block && !in_front_matter && is_horizontal_rule_line(line);

        let in_math_block = math_block_map.get(i).copied().unwrap_or(false);

        let in_quarto_div = flavor == MarkdownFlavor::Quarto
            && crate::utils::quarto_divs::is_within_div_block_ranges(skip_ranges.quarto_div_ranges, byte_offset);

        let in_pymdown_block = flavor == MarkdownFlavor::MkDocs
            && crate::utils::pymdown_blocks::is_within_block_ranges(skip_ranges.pymdown_block_ranges, byte_offset);

        lines.push(LineInfo {
            byte_offset,
            byte_len: line.len(),
            indent,
            visual_indent,
            is_blank,
            in_code_block,
            in_front_matter,
            in_html_block: false,
            in_html_comment,
            list_item,
            heading: None,
            blockquote: None,
            in_mkdocstrings,
            in_esm_block: false,
            in_code_span_continuation: false,
            is_horizontal_rule: is_hr,
            in_math_block,
            in_quarto_div,
            in_jsx_expression: false,
            in_mdx_comment: false,
            in_jsx_component: false,
            in_jsx_fragment: false,
            in_admonition: false,
            in_content_tab: false,
            in_mkdocs_html_markdown: false,
            in_definition_list: false,
            in_obsidian_comment: false,
            in_pymdown_block,
        });
    }

    (lines, emphasis_spans)
}

/// Pre-compute which lines are in code blocks - O(m*n) where m=code_blocks, n=lines
/// Returns a Vec<bool> where index i indicates if line i is in a code block
pub(super) fn compute_code_block_line_map(
    content: &str,
    line_offsets: &[usize],
    code_blocks: &[(usize, usize)],
) -> Vec<bool> {
    let num_lines = line_offsets.len();
    let mut in_code_block = vec![false; num_lines];

    for &(start, end) in code_blocks {
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

        let first_line_after = line_offsets.partition_point(|&offset| offset <= safe_start);
        let first_line = first_line_after.saturating_sub(1);
        let last_line = line_offsets.partition_point(|&offset| offset < safe_end);

        for flag in in_code_block.iter_mut().take(last_line).skip(first_line) {
            *flag = true;
        }
    }

    in_code_block
}

/// Pre-compute which lines are inside math blocks ($$ ... $$) - O(n) single pass
/// Returns a Vec<bool> where index i indicates if line i is in a math block
pub(super) fn compute_math_block_line_map(content: &str, code_block_map: &[bool]) -> Vec<bool> {
    let content_lines: Vec<&str> = content.lines().collect();
    let num_lines = content_lines.len();
    let mut in_math_block = vec![false; num_lines];

    let mut inside_math = false;

    for (i, line) in content_lines.iter().enumerate() {
        if code_block_map.get(i).copied().unwrap_or(false) {
            continue;
        }

        let trimmed = line.trim();

        if trimmed == "$$" {
            if inside_math {
                in_math_block[i] = true;
                inside_math = false;
            } else {
                in_math_block[i] = true;
                inside_math = true;
            }
        } else if inside_math {
            in_math_block[i] = true;
        }
    }

    in_math_block
}

/// Detect list items and emphasis spans in a single pulldown-cmark pass.
/// Returns both list items (for LineInfo) and emphasis spans (for MD030).
pub(super) fn detect_list_items_and_emphasis_with_pulldown(
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

                if !CodeBlockUtils::is_in_code_block_or_span(code_blocks, match_start) {
                    let marker = content[match_start..].chars().next().unwrap_or('*');
                    if marker == '*' || marker == '_' {
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
                let current_list_is_ordered = list_stack.last().copied().unwrap_or(false);
                let item_start = range.start;

                let mut line_idx = match line_offsets.binary_search(&item_start) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                };

                if item_start < content.len() && content.as_bytes()[item_start] == b'\n' {
                    line_idx += 1;
                }

                if front_matter_end > 0 && line_idx < front_matter_end {
                    continue;
                }

                if line_idx < line_offsets.len() {
                    let line_start_byte = line_offsets[line_idx];
                    let line_end = line_offsets.get(line_idx + 1).copied().unwrap_or(content.len());
                    let line = &content[line_start_byte..line_end.min(content.len())];

                    let line = line
                        .strip_suffix('\n')
                        .or_else(|| line.strip_suffix("\r\n"))
                        .unwrap_or(line);

                    let blockquote_parse = parse_blockquote_prefix(line);
                    let (blockquote_prefix_len, line_to_parse) = if let Some((prefix, content)) = blockquote_parse {
                        (prefix.len(), content)
                    } else {
                        (0, line)
                    };

                    if current_list_is_ordered {
                        if let Some((leading_spaces, number_str, delimiter, spacing, _content)) =
                            parse_ordered_list(line_to_parse)
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
                        parse_unordered_list(line_to_parse)
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

/// Compute character frequency for fast content analysis
pub(super) fn compute_char_frequency(content: &str) -> CharFrequency {
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

/// Fast blockquote prefix parser - replaces regex for 5-10x speedup
/// Handles nested blockquotes like `> > > content`
/// Returns: Some((prefix_with_ws, content_after_prefix)) or None
#[inline]
pub(super) fn parse_blockquote_prefix(line: &str) -> Option<(&str, &str)> {
    let trimmed_start = line.trim_start();
    if !trimmed_start.starts_with('>') {
        return None;
    }

    let mut remaining = line;
    let mut total_prefix_len = 0;

    loop {
        let trimmed = remaining.trim_start();
        if !trimmed.starts_with('>') {
            break;
        }

        let leading_ws_len = remaining.len() - trimmed.len();
        total_prefix_len += leading_ws_len + 1;

        let after_gt = &trimmed[1..];

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

/// Fast unordered list parser - replaces regex for 5-10x speedup
/// Matches: ^(\s*)([-*+])([ \t]*)(.*)
/// Returns: Some((leading_ws, marker, spacing, content)) or None
#[inline]
pub(super) fn parse_unordered_list(line: &str) -> Option<(&str, char, &str, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }

    if i >= bytes.len() {
        return None;
    }
    let marker = bytes[i] as char;
    if marker != '-' && marker != '*' && marker != '+' {
        return None;
    }
    let marker_pos = i;
    i += 1;

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
pub(super) fn parse_ordered_list(line: &str) -> Option<(&str, &str, char, &str, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }

    let number_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == number_start {
        return None;
    }

    if i >= bytes.len() {
        return None;
    }
    let delimiter = bytes[i] as char;
    if delimiter != '.' && delimiter != ')' {
        return None;
    }
    let delimiter_pos = i;
    i += 1;

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
