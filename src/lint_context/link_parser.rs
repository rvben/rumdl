use crate::config::MarkdownFlavor;
use crate::utils::code_block_utils::CodeBlockUtils;
use pulldown_cmark::{BrokenLink, Event, LinkType, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

use super::types::*;

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

/// Collect byte ranges of all links using pulldown-cmark
/// This is used to skip heading detection for lines that fall within link syntax
/// (e.g., multiline links like `[text](url\n#fragment)`)
pub(super) fn collect_link_byte_ranges(content: &str) -> Vec<(usize, usize)> {
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
pub(super) fn parse_links<'a>(
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
                    let (line_idx, line_num, col_start) = super::LintContext::find_line_for_offset(lines, start_pos);

                    // Skip if this link is on a MkDocs snippet line
                    if is_mkdocs_snippet_line(lines[line_idx].content(content), flavor) {
                        text_chunks.clear();
                        continue;
                    }

                    let (_, _end_line_num, col_end) = super::LintContext::find_line_for_offset(lines, range.end);

                    let is_reference = matches!(
                        link_type,
                        LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
                    );

                    // Extract link text directly from source bytes to preserve escaping
                    let link_text = if matches!(link_type, LinkType::WikiLink { .. }) {
                        if !text_chunks.is_empty() {
                            let text: String = text_chunks.iter().map(|(t, _, _)| t.as_str()).collect();
                            Cow::Owned(text)
                        } else {
                            Cow::Owned(url.to_string())
                        }
                    } else if start_pos < content.len() {
                        let link_bytes = &content.as_bytes()[start_pos..range.end.min(content.len())];

                        let mut close_pos = None;
                        let mut depth = 0;
                        let mut in_code_span = false;

                        for (i, &byte) in link_bytes.iter().enumerate().skip(1) {
                            let mut backslash_count = 0;
                            let mut j = i;
                            while j > 0 && link_bytes[j - 1] == b'\\' {
                                backslash_count += 1;
                                j -= 1;
                            }
                            let is_escaped = backslash_count % 2 != 0;

                            if byte == b'`' && !is_escaped {
                                in_code_span = !in_code_span;
                            }

                            if !is_escaped && !in_code_span {
                                if byte == b'[' {
                                    depth += 1;
                                } else if byte == b']' {
                                    if depth == 0 {
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

                    let reference_id = if is_reference && !ref_id.is_empty() {
                        Some(Cow::Owned(ref_id.to_lowercase()))
                    } else if is_reference {
                        Some(Cow::Owned(link_text.to_lowercase()))
                    } else {
                        None
                    };

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
                // Skip if in HTML comment
                if is_in_html_comment_ranges(html_comment_ranges, range.start) {
                    continue;
                }

                let (_, line_num, _) = super::LintContext::find_line_for_offset(lines, range.start);
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
    for cap in LINK_PATTERN.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        let match_start = full_match.start();
        let match_end = full_match.end();

        if found_positions.contains(&match_start) {
            continue;
        }

        if match_start > 0 && content.as_bytes().get(match_start - 1) == Some(&b'\\') {
            continue;
        }

        if match_start > 0 && content.as_bytes().get(match_start - 1) == Some(&b'!') {
            continue;
        }

        if CodeBlockUtils::is_in_code_block(code_blocks, match_start) {
            continue;
        }

        if super::LintContext::is_offset_in_code_span(code_spans, match_start) {
            continue;
        }

        if is_in_html_comment_ranges(html_comment_ranges, match_start) {
            continue;
        }

        let (line_idx, line_num, col_start) = super::LintContext::find_line_for_offset(lines, match_start);

        if is_mkdocs_snippet_line(lines[line_idx].content(content), flavor) {
            continue;
        }

        let (_, _end_line_num, col_end) = super::LintContext::find_line_for_offset(lines, match_end);

        let text = cap.get(1).map_or("", |m| m.as_str());

        if let Some(ref_id) = cap.get(6) {
            let ref_id_str = ref_id.as_str();
            let normalized_ref = if ref_id_str.is_empty() {
                Cow::Owned(text.to_lowercase())
            } else {
                Cow::Owned(ref_id_str.to_lowercase())
            };

            links.push(ParsedLink {
                line: line_num,
                start_col: col_start,
                end_col: col_end,
                byte_offset: match_start,
                byte_end: match_end,
                text: Cow::Borrowed(text),
                url: Cow::Borrowed(""),
                is_reference: true,
                reference_id: Some(normalized_ref),
                link_type: LinkType::Reference,
            });
        }
    }

    (links, broken_links, footnote_refs)
}

/// Parse all images in the content
pub(super) fn parse_images<'a>(
    content: &'a str,
    lines: &[LineInfo],
    code_blocks: &[(usize, usize)],
    code_spans: &[CodeSpan],
    html_comment_ranges: &[crate::utils::skip_context::ByteRange],
) -> Vec<ParsedImage<'a>> {
    use crate::utils::skip_context::is_in_html_comment_ranges;
    use std::collections::HashSet;

    let mut images = Vec::with_capacity(content.len() / 1000);
    let mut found_positions = HashSet::new();

    let parser = Parser::new(content).into_offset_iter();
    let mut image_stack: Vec<(usize, pulldown_cmark::CowStr<'a>, LinkType, pulldown_cmark::CowStr<'a>)> = Vec::new();
    let mut text_chunks: Vec<(String, usize, usize)> = Vec::new();

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
                    if CodeBlockUtils::is_in_code_block(code_blocks, start_pos) {
                        continue;
                    }

                    if super::LintContext::is_offset_in_code_span(code_spans, start_pos) {
                        continue;
                    }

                    if is_in_html_comment_ranges(html_comment_ranges, start_pos) {
                        continue;
                    }

                    let (_, line_num, col_start) = super::LintContext::find_line_for_offset(lines, start_pos);
                    let (_, _end_line_num, col_end) = super::LintContext::find_line_for_offset(lines, range.end);

                    let is_reference = matches!(
                        link_type,
                        LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
                    );

                    let alt_text = if start_pos < content.len() {
                        let image_bytes = &content.as_bytes()[start_pos..range.end.min(content.len())];

                        let mut close_pos = None;
                        let mut depth = 0;

                        if image_bytes.len() > 2 {
                            for (i, &byte) in image_bytes.iter().enumerate().skip(2) {
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
                        Some(Cow::Owned(alt_text.to_lowercase()))
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

    // Regex fallback for undefined references
    for cap in IMAGE_PATTERN.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        let match_start = full_match.start();
        let match_end = full_match.end();

        if found_positions.contains(&match_start) {
            continue;
        }

        if match_start > 0 && content.as_bytes().get(match_start - 1) == Some(&b'\\') {
            continue;
        }

        if CodeBlockUtils::is_in_code_block(code_blocks, match_start)
            || super::LintContext::is_offset_in_code_span(code_spans, match_start)
            || is_in_html_comment_ranges(html_comment_ranges, match_start)
        {
            continue;
        }

        if let Some(ref_id) = cap.get(6) {
            let (_, line_num, col_start) = super::LintContext::find_line_for_offset(lines, match_start);
            let (_, _end_line_num, col_end) = super::LintContext::find_line_for_offset(lines, match_end);
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
                link_type: LinkType::Reference,
            });
        }
    }

    images
}

/// Parse reference definitions
pub(super) fn parse_reference_defs(content: &str, lines: &[LineInfo]) -> Vec<ReferenceDef> {
    let mut refs = Vec::with_capacity(lines.len() / 20);

    for (line_idx, line_info) in lines.iter().enumerate() {
        if line_info.in_code_block {
            continue;
        }

        let line = line_info.content(content);
        let line_num = line_idx + 1;

        if let Some(cap) = REF_DEF_PATTERN.captures(line) {
            let id_raw = cap.get(1).unwrap().as_str();

            // Skip footnote definitions
            if id_raw.starts_with('^') {
                continue;
            }

            let id = id_raw.to_lowercase();
            let url = cap.get(2).unwrap().as_str().to_string();
            let title_match = cap.get(3).or_else(|| cap.get(4));
            let title = title_match.map(|m| m.as_str().to_string());

            let match_obj = cap.get(0).unwrap();
            let byte_offset = line_info.byte_offset + match_obj.start();
            let byte_end = line_info.byte_offset + match_obj.end();

            let (title_byte_start, title_byte_end) = if let Some(m) = title_match {
                let start = line_info.byte_offset + m.start().saturating_sub(1);
                let end = line_info.byte_offset + m.end() + 1;
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
