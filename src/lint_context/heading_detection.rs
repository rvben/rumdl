use crate::config::MarkdownFlavor;
use crate::rules::front_matter_utils::FrontMatterUtils;
use std::sync::LazyLock;

use super::types::*;

/// Detailed blockquote parse result with all components
pub(super) struct BlockquoteComponents<'a> {
    pub indent: &'a str,
    pub markers: &'a str,
    pub spaces_after: &'a str,
    pub content: &'a str,
}

/// Parse blockquote prefix with detailed components using manual parsing
#[inline]
pub(super) fn parse_blockquote_detailed(line: &str) -> Option<BlockquoteComponents<'_>> {
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

/// Detect headings and blockquotes (called after HTML block detection)
pub(super) fn detect_headings_and_blockquotes(
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
            if !lines[i].in_code_block && is_horizontal_rule_content(bq.content.trim()) {
                lines[i].is_horizontal_rule = true;
            }
        }

        // Now apply skip conditions for heading detection
        if lines[i].in_code_block {
            continue;
        }

        if front_matter_end > 0 && i < front_matter_end {
            continue;
        }

        if lines[i].in_html_block {
            continue;
        }

        if lines[i].is_blank {
            continue;
        }

        // Check for ATX headings (but skip MkDocs snippet lines)
        let is_snippet_line = if flavor == MarkdownFlavor::MkDocs {
            crate::utils::mkdocs_snippets::is_snippet_section_start(line)
                || crate::utils::mkdocs_snippets::is_snippet_section_end(line)
        } else {
            false
        };

        if !is_snippet_line && let Some(caps) = ATX_HEADING_REGEX.captures(line) {
            if crate::utils::skip_context::is_in_html_comment_ranges(html_comment_ranges, lines[i].byte_offset) {
                continue;
            }
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
                let (rest_without_id, custom_id_part) = if let Some(id_start) = rest.rfind(" {#") {
                    if rest[id_start..].trim_end().ends_with('}') {
                        (&rest[..id_start], &rest[id_start..])
                    } else {
                        (rest, "")
                    }
                } else {
                    (rest, "")
                };

                let trimmed_rest = rest_without_id.trim_end();
                if let Some(last_hash_byte_pos) = trimmed_rest.rfind('#') {
                    let char_positions: Vec<(usize, char)> = trimmed_rest.char_indices().collect();

                    let last_hash_char_idx = char_positions
                        .iter()
                        .position(|(byte_pos, _)| *byte_pos == last_hash_byte_pos);

                    if let Some(mut char_idx) = last_hash_char_idx {
                        while char_idx > 0 && char_positions[char_idx - 1].1 == '#' {
                            char_idx -= 1;
                        }

                        let start_of_hashes = char_positions[char_idx].0;

                        let has_space_before = char_idx == 0 || char_positions[char_idx - 1].1.is_whitespace();

                        let potential_closing = &trimmed_rest[start_of_hashes..];
                        let is_all_hashes = potential_closing.chars().all(|c| c == '#');

                        if is_all_hashes && has_space_before {
                            let closing_hashes = potential_closing.to_string();
                            let text_part = if !custom_id_part.is_empty() {
                                format!("{}{}", trimmed_rest[..start_of_hashes].trim_end(), custom_id_part)
                            } else {
                                trimmed_rest[..start_of_hashes].trim_end().to_string()
                            };
                            (text_part, true, closing_hashes)
                        } else {
                            (rest.to_string(), false, String::new())
                        }
                    } else {
                        (rest.to_string(), false, String::new())
                    }
                } else {
                    (rest.to_string(), false, String::new())
                }
            };

            let content_column = marker_column + hashes.len() + spaces_after.len();

            let raw_text = text.trim().to_string();
            let (clean_text, mut custom_id) = crate::utils::header_id_utils::extract_header_id(&raw_text);

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
                if front_matter_end > 0 && i < front_matter_end {
                    continue;
                }

                if crate::utils::skip_context::is_in_html_comment_ranges(html_comment_ranges, lines[i].byte_offset) {
                    continue;
                }

                let content_line = line.trim();

                if content_line.starts_with('-') || content_line.starts_with('*') || content_line.starts_with('+') {
                    continue;
                }

                if content_line.starts_with('_') {
                    let non_ws: String = content_line.chars().filter(|c| !c.is_whitespace()).collect();
                    if non_ws.len() >= 3 && non_ws.chars().all(|c| c == '_') {
                        continue;
                    }
                }

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

                if ATX_HEADING_REGEX.is_match(line) {
                    continue;
                }

                if content_line.starts_with('>') {
                    continue;
                }

                let trimmed_start = line.trim_start();
                if trimmed_start.len() >= 3 {
                    let first_three: String = trimmed_start.chars().take(3).collect();
                    if first_three == "```" || first_three == "~~~" {
                        continue;
                    }
                }

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

                let raw_text = line.trim().to_string();
                let (clean_text, mut custom_id) = crate::utils::header_id_utils::extract_header_id(&raw_text);

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
                    is_valid: true,
                });
            }
        }
    }
}

/// Detect HTML blocks in the content
pub(super) fn detect_html_blocks(content: &str, lines: &mut [LineInfo]) {
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
        if lines[i].in_code_block || lines[i].in_front_matter {
            i += 1;
            continue;
        }

        let trimmed = lines[i].content(content).trim_start();

        if trimmed.starts_with('<') && trimmed.len() > 1 {
            let after_bracket = &trimmed[1..];
            let is_closing = after_bracket.starts_with('/');
            let tag_start = if is_closing { &after_bracket[1..] } else { after_bracket };

            let tag_name = tag_start
                .chars()
                .take_while(|c| c.is_ascii_alphabetic() || *c == '-' || c.is_ascii_digit())
                .collect::<String>()
                .to_lowercase();

            if !tag_name.is_empty() && BLOCK_ELEMENTS.contains(&tag_name.as_str()) {
                lines[i].in_html_block = true;

                if !is_closing {
                    let closing_tag = format!("</{tag_name}>");

                    let same_line_close = lines[i].content(content).contains(&closing_tag);

                    if !same_line_close {
                        let allow_blank_lines = tag_name == "style" || tag_name == "script";
                        let mut j = i + 1;
                        let mut found_closing_tag = false;
                        while j < lines.len() && j < i + 100 {
                            if !allow_blank_lines && lines[j].is_blank {
                                break;
                            }

                            lines[j].in_html_block = true;

                            if lines[j].content(content).contains(&closing_tag) {
                                found_closing_tag = true;
                            }

                            if found_closing_tag {
                                j += 1;
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
        }

        i += 1;
    }
}
