//! Flat source parser for documents whose structure is unambiguous.
//!
//! Facts point into the original UTF-8 source. Unsupported syntax returns
//! `None` for the whole document, so the general backend handles it without
//! mixing interpretations or accepting a partial parse.

use super::{InlineLinkRange, LineEvent, ParsedDocument, PendingInlineLinks, PendingLineEvents, PendingLinks};
use crate::utils::code_block_utils::{CodeBlockDetail, ParseResult, StrongSpanDetail};

pub(super) fn parse(content: &str) -> Option<ParsedDocument<'_>> {
    // Table confirmation reads the block and list-item layout, which this parser
    // does not collect. Without a pipe there is no table to confirm.
    if content.contains('|') {
        return None;
    }
    let mut structure = ParseResult::default();
    let mut links = PendingInlineLinks::new();
    let mut pending_lines = PendingLineEvents::shared();
    let mut fence: Option<(usize, u8, usize, String)> = None;
    let mut offset = 0;
    for raw_line in content.split_inclusive('\n') {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        // A lone CR is another CommonMark line ending; the outer source index
        // counts LF only. Leave that reading to the general backend.
        if line.contains('\r') || (!raw_line.ends_with('\n') && raw_line.ends_with('\r')) {
            return None;
        }
        let bytes = line.as_bytes();
        let end = offset + raw_line.len();
        if let Some((_, marker, count, _)) = &fence {
            let indent = bytes.iter().take_while(|&&b| b == b' ').count();
            let run = bytes[indent..].iter().take_while(|&&b| b == *marker).count();
            if indent <= 3 && run >= *count && bytes[indent + run..].contains(&b'\t') {
                return None;
            }
            if indent <= 3
                && run >= *count
                && bytes[indent + run..].iter().all(|&b| b == b' ')
                && let Some((start, _, _, info_string)) = fence.take()
            {
                let fence_end = offset + line.len();
                structure.code_blocks.push((start, fence_end));
                structure.code_block_details.push(CodeBlockDetail {
                    start,
                    end: fence_end,
                    is_fenced: true,
                    info_string,
                    content_column: None,
                    closed: true,
                    enclosing: Vec::new(),
                });
            }
            offset = end;
            continue;
        }
        if bytes.iter().all(|b| matches!(b, b' ' | b'\t')) {
            offset = end;
            continue;
        }
        if matches!(bytes.first(), Some(b' ' | b'\t' | b'>' | b':')) {
            return None;
        }
        if matches!(bytes.first(), Some(b'`' | b'~')) {
            let marker = bytes[0];
            let count = bytes.iter().take_while(|&&b| b == marker).count();
            if count >= 3 {
                let info = line[count..].trim_matches([' ', '\t']);
                if info
                    .bytes()
                    .any(|b| matches!(b, b'`' | b'\\' | b'&') || b.is_ascii_control())
                {
                    return None;
                }
                fence = Some((offset, marker, count, info.to_owned()));
                offset = end;
                continue;
            }
        }
        let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
        if digits > 0
            && matches!(bytes.get(digits), Some(b'.' | b')'))
            && bytes.get(digits + 1).is_none_or(|b| matches!(b, b' ' | b'\t'))
        {
            return None;
        }
        if matches!(bytes.first(), Some(b'-' | b'*' | b'_'))
            && bytes.iter().all(|&b| b == bytes[0] || matches!(b, b' ' | b'\t'))
            && bytes.iter().filter(|&&b| b == bytes[0]).nth(2).is_some()
        {
            offset = end;
            continue;
        }
        // Flat unordered items need only their source start for line facts.
        let mut at = usize::from(
            matches!(bytes.first(), Some(b'-' | b'+' | b'*'))
                && bytes.get(1).is_some_and(|b| matches!(b, b' ' | b'\t')),
        );
        if at > 0 {
            let padding = bytes[at..].iter().take_while(|&&b| matches!(b, b' ' | b'\t')).count();
            // Wider padding can open indented code inside the item. Tabs
            // depend on visual columns, so leave both to the general grammar.
            if padding > 4 || bytes[at..at + padding].contains(&b'\t') {
                return None;
            }
            let body = &bytes[at + padding..];
            let digits = body.iter().take_while(|b| b.is_ascii_digit()).count();
            // A quote opened on the item's line holds blocks of its own, code among them.
            if body.first() == Some(&b'>') {
                return None;
            }
            if matches!(body.first(), Some(b'-' | b'+' | b'*')) && body.get(1).is_none_or(|b| matches!(b, b' ' | b'\t'))
                || digits > 0
                    && matches!(body.get(digits), Some(b'.' | b')'))
                    && body.get(digits + 1).is_none_or(|b| matches!(b, b' ' | b'\t'))
            {
                return None;
            }
            if body.is_empty() {
                pending_lines.disable();
            } else {
                pending_lines.push(LineEvent::ListStart(false));
                pending_lines.push(LineEvent::ItemStart(offset));
                pending_lines.push(LineEvent::ListEnd);
            }
        } else if bytes.len() == 1 && matches!(bytes[0], b'-' | b'+') {
            pending_lines.disable();
        }
        while at < bytes.len() {
            match bytes[at] {
                b'`' => {
                    let count = bytes[at..].iter().take_while(|&&b| b == b'`').count();
                    let mut close = at + count;
                    loop {
                        close += bytes[close..].iter().position(|&b| b == b'`')?;
                        let run = bytes[close..].iter().take_while(|&&b| b == b'`').count();
                        if run == count {
                            structure.code_spans.push((offset + at, offset + close + run));
                            at = close + run;
                            break;
                        }
                        close += run;
                    }
                }
                b'[' | b'!' if bytes[at] == b'[' || bytes.get(at + 1) == Some(&b'[') => {
                    let image = bytes[at] == b'!';
                    let label_start = at + 1 + usize::from(image);
                    let label_end = label_start + bytes[label_start..].iter().position(|&b| b == b']')?;
                    let label = &line[label_start..label_end];
                    if label
                        .bytes()
                        .any(|b| matches!(b, b'[' | b'`' | b'*' | b'_' | b'\\' | b'<' | b'$' | b'^' | b'~'))
                        || bytes.get(label_end + 1) != Some(&b'(')
                    {
                        return None;
                    }
                    let url_start = label_end + 2;
                    let mut url_end = url_start;
                    let mut depth = 0;
                    while let Some(&b) = bytes.get(url_end) {
                        if b == b')' && depth == 0 {
                            break;
                        }
                        if b.is_ascii_control()
                            || matches!(b, b' ' | b'\\' | b'&' | b'<' | b'>' | b'[' | b']' | b'"' | b'\'')
                        {
                            return None;
                        }
                        if b == b'(' {
                            depth += 1;
                            if depth > 8 {
                                return None;
                            }
                        } else if b == b')' {
                            depth -= 1;
                        }
                        url_end += 1;
                    }
                    if bytes.get(url_end) != Some(&b')') {
                        return None;
                    }
                    links.push(InlineLinkRange {
                        start: offset + at,
                        label_end: offset + label_end,
                        end: offset + url_end + 1,
                        image,
                    });
                    at = url_end + 1;
                }
                b'*' | b'_' => {
                    let marker = bytes[at];
                    let count = bytes[at..].iter().take_while(|&&b| b == marker).count();
                    if count > 2 || at > 0 && !boundary(bytes[at - 1]) {
                        return None;
                    }
                    let text_start = at + count;
                    if !bytes.get(text_start).is_some_and(u8::is_ascii_alphanumeric) {
                        return None;
                    }
                    let close = text_start + bytes[text_start..].iter().position(|&b| b == marker)?;
                    if !bytes[close - 1].is_ascii_alphanumeric()
                        || bytes[close..].iter().take_while(|&&b| b == marker).count() != count
                        || bytes.get(close + count).is_some_and(|&b| !boundary(b))
                        || bytes[text_start..close]
                            .iter()
                            .any(|b| matches!(b, b'*' | b'_' | b'`' | b'[' | b'<' | b'\\' | b'$' | b'^' | b'~'))
                    {
                        return None;
                    }
                    if count == 2 {
                        structure.strong_spans.push(StrongSpanDetail {
                            start: offset + at,
                            end: offset + close + count,
                            is_asterisk: marker == b'*',
                        });
                    }
                    pending_lines.push(LineEvent::Emphasis {
                        start: offset + at,
                        end: offset + close + count,
                        strong: count == 2,
                    });
                    at = close + count;
                }
                b'<' | b'|' | b'\\' | b'$' | b'^' | b'~' | b'{' | b'}' => return None,
                _ => at += 1,
            }
        }
        offset = end;
    }
    if let Some((start, _, _, info_string)) = fence {
        structure.code_blocks.push((start, content.len()));
        structure.code_block_details.push(CodeBlockDetail {
            start,
            end: content.len(),
            is_fenced: true,
            info_string,
            content_column: None,
            closed: false,
            enclosing: Vec::new(),
        });
    }
    Some(ParsedDocument {
        structure,

        has_math: false,

        parser_blocks: None,
        pending_lines,
        text_start_lines: None,
        pending_links: PendingLinks {
            content,
            events: Vec::new(),
            inline_links: Some(links),
            broken_links: Vec::new(),
        },
    })
}

fn boundary(byte: u8) -> bool {
    byte.is_ascii_whitespace() || byte.is_ascii_punctuation() && !matches!(byte, b'*' | b'_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    fn compare(content: &str, require_native: bool) {
        let Some(native) = parse(content) else {
            assert!(!require_native, "native parser rejected {content:?}");
            return;
        };
        let line_offsets = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect::<Vec<_>>();
        let mut reference = ParsedDocument::parse_pulldown(content, &line_offsets);
        // Only table confirmation reads these, and the native parser takes no
        // document with a pipe.
        reference.structure.top_level_blocks.clear();
        reference.structure.list_items.clear();
        assert_eq!(
            format!("{:?}", native.structure),
            format!("{:?}", reference.structure),
            "{content:?}"
        );
        let expected_lines = crate::lint_context::line_computation::detect_list_items_and_emphasis_with_pulldown(
            content,
            &line_offsets,
            MarkdownFlavor::Standard,
            0,
            &reference.structure.code_blocks,
        );
        let actual_lines = native.pending_lines.finish(
            content,
            &line_offsets,
            0,
            &native.structure.code_blocks,
            MarkdownFlavor::Standard,
            true,
        );
        assert_eq!(actual_lines.0, expected_lines.0, "{content:?}");
        assert_eq!(
            format!("{:?}", actual_lines.1),
            format!("{:?}", expected_lines.1),
            "{content:?}"
        );
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let native = native
            .pending_links
            .finish(&ctx.lines, &ctx.code_blocks, ctx.flavor, &ctx.html_comment_ranges);
        let reference =
            reference
                .pending_links
                .finish(&ctx.lines, &ctx.code_blocks, ctx.flavor, &ctx.html_comment_ranges);
        assert_eq!(native.link_byte_ranges, reference.link_byte_ranges, "{content:?}");
        assert_eq!(
            native.link_found_positions, reference.link_found_positions,
            "{content:?}"
        );
        assert_eq!(
            native.image_found_positions, reference.image_found_positions,
            "{content:?}"
        );
        assert_eq!(
            format!(
                "{:?}",
                (native.links, native.images, native.broken_links, native.footnote_refs)
            ),
            format!(
                "{:?}",
                (
                    reference.links,
                    reference.images,
                    reference.broken_links,
                    reference.footnote_refs
                )
            ),
            "{content:?}"
        );
    }

    #[test]
    fn native_source_facts_match_general_parser() {
        for content in [
            "",
            "prose\n",
            "# Heading\n\nText with `code` and **bold**.\n",
            "__bold__ and *emphasis* and _emphasis_.\n",
            "[label](https://example.com) ![image](image.png)\n",
            "[a &amp; b](target) []() ![](target) [a](b(c))\n",
            "[café](café)\r\n",
            "- item with [link](target)\n- more text\n",
            "```rust\n[code](url)\n```\n",
            "~~~js\n**literal**\n~~~~  \n",
            "```\n\n",
            "```md\ntext\n\n",
            "```md\ntext\n",
            "```\r\ncode\r\n```\r\n",
            "```\ncode\n   ```\n",
            "A ``two ` ticks`` span.\n",
            "### [heading](target) ###\n",
        ] {
            compare(content, true);
        }
    }

    #[test]
    fn generated_documents_preserve_general_parser_facts() {
        let fragments = [
            "plain",
            "é",
            "**bold**",
            "_x_",
            "`code`",
            "[a](b)",
            "![x](y)",
            "1. a",
            "- 1. a",
            "- - 1. a",
            ": term",
            "[]()",
            "* * *",
            "~~~\nx\n~~~",
            "```md\n[code](url)\n```",
            "<x>",
            "[ref]",
            "\\*",
            "$x$",
            "{{x}}",
            "[a](b(c))",
            "a**b**c",
            "***a***",
            "[a](b 't')",
            "a_b",
            "[a &amp; b](c)",
            "\tcode",
            "--\n",
            "> [q](r)",
            "- `x`",
            "[ a ](b)",
            "\r",
            "[a](b\u{a0}c)",
            "```\nx\n```\t\n",
            "-     `code`",
            "-\t\tcode",
            "- # *heading*",
            "\u{b}**text**",
            "\u{c}`code`",
        ];
        for first in fragments {
            for second in fragments {
                for prefix in ["", "# ", "- "] {
                    compare(&format!("{prefix}{first}\n\n{second}\n"), false);
                }
            }
        }
    }

    #[test]
    fn short_delimiter_sequences_preserve_general_parser_facts() {
        let alphabet = [b'a', b'*', b'_', b'`', b'[', b']', b'(', b')', b' ', b'\n', b'-'];
        for length in 1..=5 {
            for mut number in 0..alphabet.len().pow(length) {
                let mut input = vec![0; length as usize];
                for byte in &mut input {
                    *byte = alphabet[number % alphabet.len()];
                    number /= alphabet.len();
                }
                compare(std::str::from_utf8(&input).unwrap(), false);
            }
        }
    }
}
