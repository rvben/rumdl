//! Shared HTML block-start classification.
//!
//! A line whose first tag names one of these elements starts an HTML block in
//! rumdl's parser (`lint_context::heading_detection::detect_html_blocks`).
//! Reflow consults the same predicate so a wrapped line can never introduce a
//! construct the parser would classify differently: the two must agree, and
//! sharing the list keeps them from drifting apart.

/// Type-1 tags per CommonMark: blank lines inside these blocks do not
/// terminate them — only a matching end tag (or EOF) does.
pub const TYPE_1_BLOCK_ELEMENTS: &[&str] = &["pre", "script", "style", "textarea"];

/// HTML elements whose tags open an HTML block at line start (CommonMark
/// type-1 and type-6 conditions, as recognized by rumdl's parser).
pub const BLOCK_ELEMENTS: &[&str] = &[
    "address",
    "article",
    "aside",
    "audio",
    "base",
    "basefont",
    "blockquote",
    "body",
    "canvas",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "embed",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "iframe",
    "legend",
    "li",
    "link",
    "main",
    "menu",
    "menuitem",
    "nav",
    "noframes",
    "noscript",
    "object",
    "ol",
    "optgroup",
    "option",
    "p",
    "param",
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
    "title",
    "tr",
    "track",
    "ul",
    "video",
];

/// The elements of [`BLOCK_ELEMENTS`] that CommonMark does not name in its
/// type-1 or type-6 start conditions. A spec parser opens a block on one only
/// through start condition 7, which parsers disagree on where a block may
/// start: after a table row, markdown-rs ends the table there and
/// pulldown-cmark reads the tag as the next row.
pub const NON_SPEC_BLOCK_ELEMENTS: &[&str] = &[
    "audio", "canvas", "embed", "noscript", "object", "picture", "source", "svg", "template", "video",
];

/// If `trimmed` (a line with leading whitespace already stripped) opens an
/// HTML block per rumdl's parser, return the lowercased tag name and whether
/// it is a closing tag. Returns `None` for text, autolinks, and inline-level
/// tags (`<span>`, `<b>`, ...), which cannot interrupt a paragraph.
///
/// The tag name has to end the way CommonMark's start conditions require:
/// whitespace, the end of the line, `>` or `/>` may follow it, so `<div.class>`
/// or `<p,` is text that happens to begin with a block element's name.
pub fn parse_html_block_start(trimmed: &str) -> Option<(String, bool)> {
    let after_bracket = trimmed.strip_prefix('<')?;
    if after_bracket.is_empty() {
        return None;
    }
    let is_closing = after_bracket.starts_with('/');
    let tag_start = if is_closing { &after_bracket[1..] } else { after_bracket };

    let tag_name = tag_start
        .chars()
        .take_while(|c| c.is_ascii_alphabetic() || *c == '-' || c.is_ascii_digit())
        .collect::<String>()
        .to_lowercase();

    let rest = &tag_start[tag_name.len()..];
    let terminated =
        rest.is_empty() || rest.starts_with(|c: char| c.is_ascii_whitespace() || c == '>') || rest.starts_with("/>");

    if terminated && !tag_name.is_empty() && BLOCK_ELEMENTS.contains(&tag_name.as_str()) {
        Some((tag_name, is_closing))
    } else {
        None
    }
}

/// Whether `trimmed` (a line with leading whitespace already stripped) opens
/// an HTML block that no tag name identifies: a comment, a processing
/// instruction, a declaration or a CDATA section (CommonMark start conditions
/// 2 to 5). Each interrupts a paragraph like a block-level tag does, and none
/// of them is a tag `parse_html_block_start` can name, so a caller asking
/// "does this line start a block?" needs both.
pub fn opens_untagged_html_block(trimmed: &str) -> bool {
    let Some(after_bracket) = trimmed.strip_prefix('<') else {
        return false;
    };
    after_bracket.starts_with("!--")
        || after_bracket.starts_with('?')
        || after_bracket.starts_with("![CDATA[")
        || after_bracket
            .strip_prefix('!')
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_alphabetic()))
}

/// The byte length of the complete HTML open tag or closing tag `text` starts
/// with, as CommonMark defines one: `<name`, attributes each preceded by
/// whitespace (with an unquoted, single-quoted or double-quoted value), then
/// optional whitespace and `>` or `/>`; or `</name`, optional whitespace and
/// `>`. Only spaces and tabs count as whitespace, since the caller holds a
/// single line.
pub fn complete_tag_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = 1;
    if bytes.first() != Some(&b'<') {
        return None;
    }
    let closing = bytes.get(1) == Some(&b'/');
    if closing {
        i += 1;
    }
    if !bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    while bytes.get(i).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-') {
        i += 1;
    }
    let is_space = |b: Option<&u8>| matches!(b, Some(b' ' | b'\t'));
    let skip_space = |mut i: usize| {
        while is_space(bytes.get(i)) {
            i += 1;
        }
        i
    };
    if closing {
        i = skip_space(i);
        return (bytes.get(i) == Some(&b'>')).then_some(i + 1);
    }
    loop {
        let after_space = skip_space(i);
        match bytes.get(after_space) {
            Some(b'>') => return Some(after_space + 1),
            Some(b'/') => return (bytes.get(after_space + 1) == Some(&b'>')).then_some(after_space + 2),
            _ => {}
        }
        // Another attribute, which needs whitespace before it.
        if after_space == i {
            return None;
        }
        i = after_space;
        if !bytes
            .get(i)
            .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_' || *b == b':')
        {
            return None;
        }
        while bytes
            .get(i)
            .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'))
        {
            i += 1;
        }
        let before_value = skip_space(i);
        if bytes.get(before_value) != Some(&b'=') {
            continue;
        }
        i = skip_space(before_value + 1);
        match bytes.get(i) {
            Some(&quote @ (b'"' | b'\'')) => {
                let close = bytes[i + 1..].iter().position(|&b| b == quote)?;
                i += close + 2;
            }
            Some(_) => {
                let start = i;
                while bytes
                    .get(i)
                    .is_some_and(|b| !b.is_ascii_whitespace() && !matches!(b, b'"' | b'\'' | b'=' | b'<' | b'>' | b'`'))
                {
                    i += 1;
                }
                if i == start {
                    return None;
                }
            }
            None => return None,
        }
    }
}

/// Whether `trimmed` (a line with leading whitespace already stripped) opens
/// an HTML block through CommonMark start condition 7: a complete open or
/// closing tag of any element but the type-1 raw ones, followed by nothing but
/// whitespace. Unlike every other HTML block such a line cannot interrupt a
/// paragraph, so it matters only where a paragraph starts, and whether a line
/// qualifies depends on all of its text, not on how it begins.
pub fn opens_tag_line_html_block(trimmed: &str) -> bool {
    let Some(len) = complete_tag_len(trimmed) else {
        return false;
    };
    let name_start = if trimmed.starts_with("</") { 2 } else { 1 };
    let name: String = trimmed[name_start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_ascii_lowercase();
    !TYPE_1_BLOCK_ELEMENTS.contains(&name.as_str()) && trimmed[len..].trim_matches([' ', '\t']).is_empty()
}

#[cfg(test)]
mod tests {
    use super::{complete_tag_len, opens_tag_line_html_block, opens_untagged_html_block, parse_html_block_start};

    #[test]
    fn untagged_html_block_openers_are_the_spec_start_conditions() {
        for (line, expected) in [
            ("<!-- note -->", true),
            ("<!--", true),
            ("<?php echo 1; ?>", true),
            ("<!DOCTYPE html>", true),
            ("<![CDATA[x]]>", true),
            ("<!>", false),
            ("<!1>", false),
            ("<![cdata[x]]>", false),
            ("<div>", false),
            ("text <!-- note -->", false),
            ("", false),
        ] {
            assert_eq!(opens_untagged_html_block(line), expected, "{line:?}");
        }
    }

    #[test]
    fn a_block_tag_name_needs_a_terminator() {
        for (line, expected) in [
            ("<div>", Some(("div".to_string(), false))),
            ("<div class=\"x\">", Some(("div".to_string(), false))),
            ("<div", Some(("div".to_string(), false))),
            ("<div/>", Some(("div".to_string(), false))),
            ("<DIV\tid=x>", Some(("div".to_string(), false))),
            ("</div>", Some(("div".to_string(), true))),
            ("</div", Some(("div".to_string(), true))),
            ("<pre>", Some(("pre".to_string(), false))),
            ("<h1>", Some(("h1".to_string(), false))),
            ("<div.class>", None),
            ("<p,", None),
            ("<div/x>", None),
            ("<div=1>", None),
            ("<span>", None),
            ("<div-custom>", None),
            ("<h1foo>", None),
            ("<", None),
            ("</", None),
            ("text <div>", None),
        ] {
            assert_eq!(parse_html_block_start(line), expected, "{line:?}");
        }
    }

    #[test]
    fn a_complete_tag_is_measured_to_its_closing_bracket() {
        for (text, expected) in [
            ("<img />", Some(7)),
            ("<img/>", Some(6)),
            ("<a href=\"x y\">text", Some(14)),
            ("<a href='x'>", Some(12)),
            ("<a href=x>", Some(10)),
            ("<a href = x title>", Some(18)),
            ("<a\thidden>", Some(10)),
            ("</span >", Some(8)),
            ("<custom-el data-x=\"1\">", Some(22)),
            ("<a href=\"x>", None),
            ("<a href=>", None),
            ("<a href=\"x\"title=\"y\">", None),
            ("</span x>", None),
            ("<1a>", None),
            ("<a", None),
            ("< a>", None),
            ("text", None),
        ] {
            assert_eq!(complete_tag_len(text), expected, "{text:?}");
        }
    }

    #[test]
    fn a_complete_tag_alone_on_the_line_opens_a_type_seven_block() {
        for (line, expected) in [
            ("<img alt=\"a b c\" src=\"x\" />", true),
            ("<span>  ", true),
            ("</span>", true),
            ("<div>", true),
            ("<img src=\"x\" /> and text", false),
            ("<img alt=\"a b", false),
            ("<pre>", false),
            ("<SCRIPT>", false),
            ("</textarea>", false),
            ("<http://example.com>", false),
            ("text <span>", false),
        ] {
            assert_eq!(opens_tag_line_html_block(line), expected, "{line:?}");
        }
    }
}
