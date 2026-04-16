use crate::config::MarkdownFlavor;
use crate::utils::code_block_utils::CodeBlockUtils;
use crate::utils::rumdl_parser_options;
use pulldown_cmark::{BrokenLink, Event, LinkType, Tag, TagEnd};
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashSet;
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

/// Intermediate result from the pulldown-cmark parse phase.
/// Regex fallback and code_span filtering happen in the finalize phase.
pub(super) struct PulldownParseResult<'a> {
    pub link_byte_ranges: Vec<(usize, usize)>,
    pub links: Vec<ParsedLink<'a>>,
    pub images: Vec<ParsedImage<'a>>,
    pub broken_links: Vec<BrokenLinkInfo>,
    pub footnote_refs: Vec<FootnoteRef>,
    pub link_found_positions: HashSet<usize>,
    pub image_found_positions: HashSet<usize>,
}

/// Phase A: Run a single pulldown-cmark parse to collect link byte ranges,
/// links, images, broken links, and footnote references.
/// Does NOT require code_spans (those are computed later).
pub(super) fn parse_links_images_pulldown<'a>(
    content: &'a str,
    lines: &[LineInfo],
    code_blocks: &[(usize, usize)],
    flavor: MarkdownFlavor,
    html_comment_ranges: &[crate::utils::skip_context::ByteRange],
) -> PulldownParseResult<'a> {
    use crate::utils::skip_context::{is_in_html_comment_ranges, is_mkdocs_snippet_line};

    let mut link_byte_ranges = Vec::new();
    let mut links = Vec::with_capacity(content.len() / 500);
    let mut images = Vec::with_capacity(content.len() / 1000);
    let mut broken_links = Vec::new();
    let mut footnote_refs = Vec::new();
    let mut link_found_positions = HashSet::new();
    let mut image_found_positions = HashSet::new();

    let options = rumdl_parser_options();

    let parser = pulldown_cmark::Parser::new_with_broken_link_callback(
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

    let mut link_stack: Vec<(usize, pulldown_cmark::CowStr<'a>, LinkType, pulldown_cmark::CowStr<'a>)> = Vec::new();
    let mut image_stack: Vec<(usize, pulldown_cmark::CowStr<'a>, LinkType, pulldown_cmark::CowStr<'a>)> = Vec::new();
    let mut link_text_chunks: Vec<(String, usize, usize)> = Vec::new();

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                id,
                ..
            }) => {
                link_stack.push((range.start, dest_url, link_type, id));
                link_text_chunks.clear();
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                id,
                ..
            }) => {
                image_stack.push((range.start, dest_url, link_type, id));
                link_text_chunks.clear();
            }
            // Shared between links and images. Safe because markdown does not
            // allow nesting images inside links at the same depth, and each
            // Start handler clears the chunks.
            Event::Text(text) if !link_stack.is_empty() || !image_stack.is_empty() => {
                link_text_chunks.push((text.to_string(), range.start, range.end));
            }
            Event::Code(code) if !link_stack.is_empty() || !image_stack.is_empty() => {
                let code_text = format!("`{code}`");
                link_text_chunks.push((code_text, range.start, range.end));
            }
            Event::End(TagEnd::Link) => {
                if let Some((start_pos, url, link_type, ref_id)) = link_stack.pop() {
                    // Track link byte range for heading detection
                    link_byte_ranges.push((start_pos, range.end));

                    if is_in_html_comment_ranges(html_comment_ranges, start_pos) {
                        link_text_chunks.clear();
                        continue;
                    }

                    let (line_idx, line_num, col_start) = super::LintContext::find_line_for_offset(lines, start_pos);

                    if is_mkdocs_snippet_line(lines[line_idx].content(content), flavor) {
                        link_text_chunks.clear();
                        continue;
                    }

                    let (_, _end_line_num, col_end) = super::LintContext::find_line_for_offset(lines, range.end);

                    let is_reference = matches!(
                        link_type,
                        LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
                    );

                    // Extract link text directly from source bytes to preserve escaping
                    let link_text = if matches!(link_type, LinkType::WikiLink { .. }) {
                        if !link_text_chunks.is_empty() {
                            let text: String = link_text_chunks.iter().map(|(t, _, _)| t.as_str()).collect();
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

                    link_found_positions.insert(start_pos);

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

                    link_text_chunks.clear();
                }
            }
            Event::End(TagEnd::Image) => {
                if let Some((start_pos, url, link_type, ref_id)) = image_stack.pop() {
                    if CodeBlockUtils::is_in_code_block(code_blocks, start_pos) {
                        link_text_chunks.clear();
                        continue;
                    }

                    // Skip code_span check here; deferred to finalize phase
                    // where code_spans are available.

                    if is_in_html_comment_ranges(html_comment_ranges, start_pos) {
                        link_text_chunks.clear();
                        continue;
                    }

                    let (_, line_num, col_start) = super::LintContext::find_line_for_offset(lines, start_pos);
                    let (_, _end_line_num, col_end) = super::LintContext::find_line_for_offset(lines, range.end);

                    let is_reference = matches!(
                        link_type,
                        LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
                    );

                    let alt_text = if matches!(link_type, LinkType::WikiLink { has_pothole: true }) {
                        // ![[file.png|alt text]] — pulldown-cmark emits the alt
                        // text after the pipe as Text events
                        if !link_text_chunks.is_empty() {
                            let text: String = link_text_chunks.iter().map(|(t, _, _)| t.as_str()).collect();
                            // pulldown-cmark may emit trailing "]]" as part of the text
                            let text = text.strip_suffix("]]").unwrap_or(&text).to_string();
                            Cow::Owned(text)
                        } else {
                            Cow::Borrowed("")
                        }
                    } else if matches!(link_type, LinkType::WikiLink { has_pothole: false }) {
                        // ![[file.png]] — no pipe means no alt text; the text
                        // events just contain the filename, not actual alt text
                        Cow::Borrowed("")
                    } else if start_pos < content.len() {
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

                    let url = Cow::Owned(url.to_string());

                    let reference_id = if is_reference && !ref_id.is_empty() {
                        Some(Cow::Owned(ref_id.to_lowercase()))
                    } else if is_reference {
                        Some(Cow::Owned(alt_text.to_lowercase()))
                    } else {
                        None
                    };

                    image_found_positions.insert(start_pos);
                    images.push(ParsedImage {
                        line: line_num,
                        start_col: col_start,
                        end_col: col_end,
                        byte_offset: start_pos,
                        byte_end: range.end,
                        alt_text,
                        url,
                        is_reference,
                        reference_id,
                        link_type,
                    });

                    link_text_chunks.clear();
                }
            }
            Event::FootnoteReference(footnote_id) => {
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

    PulldownParseResult {
        link_byte_ranges,
        links,
        images,
        broken_links,
        footnote_refs,
        link_found_positions,
        image_found_positions,
    }
}

/// Phase B: Filter images by code_spans, run regex fallbacks, and sort results.
/// Requires code_spans which are computed after heading detection.
pub(super) fn finalize_links_and_images<'a>(
    content: &'a str,
    lines: &[LineInfo],
    code_blocks: &[(usize, usize)],
    code_spans: &[CodeSpan],
    flavor: MarkdownFlavor,
    html_comment_ranges: &[crate::utils::skip_context::ByteRange],
    mut result: PulldownParseResult<'a>,
) -> (
    Vec<ParsedLink<'a>>,
    Vec<ParsedImage<'a>>,
    Vec<BrokenLinkInfo>,
    Vec<FootnoteRef>,
) {
    use crate::utils::skip_context::{is_in_html_comment_ranges, is_mkdocs_snippet_line};

    // Filter out images that fall inside code spans (deferred from Phase A)
    result
        .images
        .retain(|img| !super::LintContext::is_offset_in_code_span(code_spans, img.byte_offset));

    // Regex fallback for links: find undefined references missed by pulldown-cmark
    for cap in LINK_PATTERN.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        let match_start = full_match.start();
        let match_end = full_match.end();

        if result.link_found_positions.contains(&match_start) {
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

            result.links.push(ParsedLink {
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
        } else if let Some(line_info) = lines.get(line_idx)
            && line_info.in_mkdocs_container()
        {
            // Inline links inside MkDocs admonitions/tabs that pulldown-cmark missed
            // because it treated the indented content as code blocks.
            let url = cap.get(2).or_else(|| cap.get(3)).map_or("", |m| m.as_str().trim());
            result.links.push(ParsedLink {
                line: line_num,
                start_col: col_start,
                end_col: col_end,
                byte_offset: match_start,
                byte_end: match_end,
                text: Cow::Borrowed(text),
                url: Cow::Borrowed(url),
                is_reference: false,
                reference_id: None,
                link_type: LinkType::Inline,
            });
        }
    }

    // Regex fallback for images: find undefined references missed by pulldown-cmark
    for cap in IMAGE_PATTERN.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        let match_start = full_match.start();
        let match_end = full_match.end();

        if result.image_found_positions.contains(&match_start) {
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

        let (line_idx, line_num, col_start) = super::LintContext::find_line_for_offset(lines, match_start);
        let (_, _end_line_num, col_end) = super::LintContext::find_line_for_offset(lines, match_end);
        let alt_text = cap.get(1).map_or("", |m| m.as_str());

        if let Some(ref_id) = cap.get(6) {
            let ref_id_str = ref_id.as_str();
            let normalized_ref = if ref_id_str.is_empty() {
                Cow::Owned(alt_text.to_lowercase())
            } else {
                Cow::Owned(ref_id_str.to_lowercase())
            };

            result.images.push(ParsedImage {
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
        } else if let Some(line_info) = lines.get(line_idx)
            && line_info.in_mkdocs_container()
        {
            // Inline images inside MkDocs admonitions/tabs that pulldown-cmark missed
            // because it treated the indented content as code blocks.
            let url = cap.get(2).or_else(|| cap.get(3)).map_or("", |m| m.as_str().trim());
            result.images.push(ParsedImage {
                line: line_num,
                start_col: col_start,
                end_col: col_end,
                byte_offset: match_start,
                byte_end: match_end,
                alt_text: Cow::Borrowed(alt_text),
                url: Cow::Borrowed(url),
                is_reference: false,
                reference_id: None,
                link_type: LinkType::Inline,
            });
        }
    }

    // Sort by line number so binary search consumers work correctly.
    result.links.sort_by_key(|l| (l.line, l.byte_offset));
    result.images.sort_by_key(|i| (i.line, i.byte_offset));

    (result.links, result.images, result.broken_links, result.footnote_refs)
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
