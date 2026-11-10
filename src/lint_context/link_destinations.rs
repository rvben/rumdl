//! Link destinations of the document body, as the parser read them, with the
//! source spelling and byte positions retained where they can be located.
//!
//! What a destination is, and what it reads as, always comes from the parser.
//! The source is consulted only to find where it is written, and a place
//! counts only when its text reads as the parser's destination. A place whose
//! text equals that destination byte for byte may also be rewritten; one
//! spelled with backslash escapes or character references may not, because a
//! rewrite derived from the decoded value would change what those spell.

use super::{LintContext, ParsedImage, ParsedLink};
use pulldown_cmark::LinkType;
use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::OnceLock;

/// Splits a destination at its first `#` into the path part and the fragment
/// with its `#`, empty when there is none.
fn split_fragment(url: &str) -> (&str, &str) {
    url.find('#').map_or((url, ""), |hash| url.split_at(hash))
}

/// Where an inline link's destination is written, without angle brackets.
struct SourceDestination {
    range: Range<usize>,
    /// Whether the written text is the parser's destination byte for byte.
    verbatim: bool,
}

impl SourceDestination {
    fn locate(content: &str, link: &ParsedLink<'_>) -> Option<Self> {
        let range = inline_destination_range(content, link)?;
        let verbatim = content[range.clone()] == *link.url;
        Some(Self { range, verbatim })
    }

    /// The written fragment, after its `#`, when the destination is verbatim.
    fn fragment(&self, content: &str) -> Option<Range<usize>> {
        if !self.verbatim {
            return None;
        }
        let hash = content[self.range.clone()].find('#')?;
        Some(self.range.start + hash + 1..self.range.end)
    }

    fn body<'s>(&self, link: &'s ParsedLink<'_>) -> BodyLinkDestination<'s> {
        let (url, fragment) = split_fragment(&link.url);
        BodyLinkDestination {
            link_start: link.byte_offset,
            url,
            fragment,
            url_range: if self.verbatim {
                self.range.start..self.range.start + url.len()
            } else {
                self.range.clone()
            },
            fix_range: self.verbatim.then(|| self.range.clone()),
        }
    }
}

/// The destination of a body link whose source spelling could not be matched
/// to the URL the parser read: the parser's URL, with no source range to
/// rewrite. Diagnostics point at the link's first line.
fn unlocated_body<'s>(content: &str, link: &'s ParsedLink<'_>) -> BodyLinkDestination<'s> {
    let (path, fragment) = split_fragment(&link.url);
    let first_line_end = content[link.byte_offset..link.byte_end]
        .find(['\n', '\r'])
        .map_or(link.byte_end, |end| link.byte_offset + end);
    BodyLinkDestination {
        link_start: link.byte_offset,
        url: path,
        fragment,
        url_range: link.byte_offset..first_line_end,
        fix_range: None,
    }
}

/// Source facts align with the parser's links. The secondary views are built
/// only when requested, and never cache rule configuration or disk answers.
pub(super) struct LinkDestinations {
    /// The written destination of each entry of `ctx.links()`, for inline links.
    destinations: Vec<Option<SourceDestination>>,
    /// Indices into `ctx.links()` of the inline links rendered as body hyperlinks.
    body_links: OnceLock<Vec<usize>>,
}

impl LinkDestinations {
    pub(super) fn new(ctx: &LintContext<'_>) -> Self {
        Self {
            destinations: ctx
                .links()
                .iter()
                .map(|link| {
                    matches!(link.link_type, LinkType::Inline)
                        .then(|| SourceDestination::locate(ctx.content, link))
                        .flatten()
                })
                .collect(),
            body_links: OnceLock::new(),
        }
    }

    fn body_link_indices(&self, ctx: &LintContext<'_>) -> &[usize] {
        self.body_links.get_or_init(|| {
            debug_assert!(
                ctx.links()
                    .windows(2)
                    .all(|pair| pair[0].byte_offset <= pair[1].byte_offset)
            );
            let mut pending_images = ctx.images().iter().peekable();
            let mut open_images: Vec<&ParsedImage<'_>> = Vec::new();
            ctx.links()
                .iter()
                .enumerate()
                .filter_map(|(i, link)| {
                    if !matches!(link.link_type, LinkType::Inline) || !ctx.is_body_markdown(link.line, link.byte_offset)
                    {
                        return None;
                    }
                    // Images nest or are disjoint. Sweep both source-ordered lists
                    // once to exclude links rendered as image description text.
                    while let Some(image) = pending_images.next_if(|image| image.byte_offset <= link.byte_offset) {
                        close_images_ending_by(&mut open_images, image.byte_offset);
                        open_images.push(image);
                    }
                    close_images_ending_by(&mut open_images, link.byte_offset);
                    if open_images
                        .iter()
                        .any(|image| link.byte_end <= image.byte_end && renders_as_image(ctx, image))
                    {
                        return None;
                    }
                    Some(i)
                })
                .collect()
        })
    }
}

fn close_images_ending_by(open_images: &mut Vec<&ParsedImage<'_>>, offset: usize) {
    while open_images.last().is_some_and(|image| image.byte_end <= offset) {
        open_images.pop();
    }
}

fn renders_as_image(ctx: &LintContext<'_>, image: &ParsedImage<'_>) -> bool {
    !image.is_reference
        || image
            .reference_id
            .as_ref()
            .is_some_and(|id| ctx.reference_definition(id).is_some())
}

/// The destination of one inline link of the document body.
pub(crate) struct BodyLinkDestination<'a> {
    /// Byte offset of the link's opening `[`, the `byte_offset` of its entry
    /// in `ctx.links()`.
    pub link_start: usize,
    /// Path, as the parser read it, without the fragment; empty for a
    /// fragment-only link. Filesystem policy is separate.
    pub url: &'a str,
    /// Fragment with its `#`, as the parser read it; empty when there is none.
    pub fragment: &'a str,
    /// Diagnostic range: the written path when the destination is written
    /// verbatim, the whole written destination when it is spelled with
    /// escapes or references, or the link's first line when it could not be
    /// located.
    pub url_range: Range<usize>,
    /// The written path and fragment, which a rewrite replaces. `None` unless
    /// the source spells the destination verbatim: nothing may be rewritten
    /// then.
    pub fix_range: Option<Range<usize>>,
}

impl<'a> BodyLinkDestination<'a> {
    pub(crate) fn full_url(&self) -> Cow<'a, str> {
        if self.fragment.is_empty() {
            Cow::Borrowed(self.url)
        } else {
            Cow::Owned(format!("{}{}", self.url, self.fragment))
        }
    }
}

impl<'a> LintContext<'a> {
    fn link_destinations(&self) -> &LinkDestinations {
        self.link_destinations_cache.get_or_init(|| LinkDestinations::new(self))
    }

    /// Whether the byte at `offset`, on 1-based `line`, is Markdown the renderer
    /// reads as part of the document body.
    ///
    /// Front matter, code, math, comments, raw HTML, and template, shortcode,
    /// JSX and extension regions are text there, however much of it looks like
    /// a link or a reference definition. Inline links and definitions are both
    /// judged by this one test, so the two can never disagree about a region.
    pub(crate) fn is_body_markdown(&self, line: usize, offset: usize) -> bool {
        let in_text_lines = self.line_info(line).is_some_and(|info| {
            info.in_front_matter
                || info.in_code_block
                || info.in_math_block
                || (info.in_html_block && !info.in_mkdocs_html_markdown)
                || info.in_pymdown_block
                || info.in_kramdown_extension_block
                || info.in_mkdocstrings
                || info.in_esm_block
        });
        !(in_text_lines
            || self.is_in_code_span_byte(offset)
            || self.is_in_math_span(offset)
            || self.is_in_html_comment(offset)
            || self.is_in_obsidian_comment(offset)
            || self.is_in_mdx_comment(offset)
            || self.is_in_jsx_expression(offset)
            || self.is_in_jinja_range(offset)
            || self.is_in_shortcode(offset))
    }

    /// The inline links rendered as hyperlinks of the document body, whether
    /// or not their destination could be located in the source.
    pub(crate) fn body_links(&self) -> impl Iterator<Item = &ParsedLink<'a>> + '_ {
        let links = self.links();
        self.link_destinations()
            .body_link_indices(self)
            .iter()
            .map(move |&i| &links[i])
    }

    /// Destinations of the inline links rendered as body hyperlinks,
    /// fragment-only ones included, one per link. Each consumer still applies
    /// its own configuration, path policy and filesystem checks.
    pub(crate) fn body_link_destinations(&self) -> impl Iterator<Item = BodyLinkDestination<'_>> + '_ {
        let facts = self.link_destinations();
        let links = self.links();
        facts
            .body_link_indices(self)
            .iter()
            .map(move |&i| match &facts.destinations[i] {
                Some(destination) => destination.body(&links[i]),
                None => unlocated_body(self.content, &links[i]),
            })
    }

    /// The fragments written in the destinations of body inline links,
    /// fragment-only destinations included, located in the source.
    ///
    /// A link whose source spells the destination differently from the URL the
    /// parser read, as a backslash escape or an entity does, is left out: no
    /// byte range of the source holds the fragment the parser resolved.
    pub(crate) fn body_link_fragments(&self) -> impl Iterator<Item = BodyLinkFragment<'a>> + '_ {
        let content = self.content;
        let facts = self.link_destinations();
        facts.body_link_indices(self).iter().filter_map(move |&i| {
            let destination = facts.destinations[i].as_ref()?;
            let fragment = destination.fragment(content)?;
            Some(BodyLinkFragment {
                path: &content[destination.range.start..fragment.start - 1],
                fragment: &content[fragment.clone()],
                range: fragment,
            })
        })
    }

    /// The link reference definitions of the document body: those the
    /// parser read, which excludes text that only resembles one (after a
    /// paragraph line, indented as code), and those the flavor reads, outside
    /// the regions `is_body_markdown` excludes. Only the first body
    /// definition of a label is a link target, so a later duplicate is not
    /// listed. Regions are excluded before the first is chosen, so text
    /// shaped like a definition in front matter, say, never hides the body
    /// definition of its label.
    pub(crate) fn body_reference_definitions(&self) -> impl Iterator<Item = BodyDefinition<'_>> + '_ {
        let content = self.content;
        let indices = self.body_definitions.get_or_init(|| {
            let mut labels = HashSet::new();
            self.definition_candidates
                .iter()
                .enumerate()
                .filter(|(_, candidate)| {
                    let start = candidate.definition.span.start;
                    let line = self.line_offsets.partition_point(|&offset| offset <= start);
                    self.is_body_markdown(line, start)
                })
                .filter(|(_, candidate)| labels.insert(candidate.label.as_str()))
                .map(|(index, _)| index)
                .collect()
        });
        indices.iter().map(move |&index| {
            let definition = &self.definition_candidates[index].definition;
            let start = definition.span.start;
            let destination = definition_destination_range(content, &definition.span, &definition.dest);
            let verbatim = destination
                .as_ref()
                .is_some_and(|range| content[range.clone()] == *definition.dest);
            BodyDefinition {
                start,
                url: &definition.dest,
                destination,
                verbatim,
            }
        })
    }
}

/// One link reference definition of the document body.
pub(crate) struct BodyDefinition<'a> {
    /// Byte offset of the label's opening `[`.
    pub start: usize,
    /// The destination as the parser read it: escapes and character
    /// references resolved, without angle brackets, fragment included.
    pub url: &'a str,
    /// Byte range of the written destination, without angle brackets. `None`
    /// when no place in the definition is written so that it reads as `url`.
    pub destination: Option<Range<usize>>,
    verbatim: bool,
}

impl BodyDefinition<'_> {
    /// The written destination, when it is `url` byte for byte and so may be
    /// rewritten.
    pub(crate) fn verbatim_range(&self) -> Option<Range<usize>> {
        self.destination.clone().filter(|_| self.verbatim)
    }

    /// Byte range of the written fragment, after its `#`, when the
    /// destination is written verbatim and has one.
    pub(crate) fn fragment_range(&self) -> Option<Range<usize>> {
        let range = self.verbatim_range()?;
        let hash = self.url.find('#')?;
        Some(range.start + hash + 1..range.end)
    }
}

/// The fragment of one body inline link's destination, located in the source.
pub(crate) struct BodyLinkFragment<'a> {
    /// The destination's path part as written; empty for a fragment-only link.
    pub path: &'a str,
    /// The fragment as written, without its `#`.
    pub fragment: &'a str,
    /// Byte range of `fragment` in the document.
    pub range: Range<usize>,
}

/// Byte range of an inline link's destination, without angle brackets, or
/// `None` when no place in the source spells the destination the parser read.
///
/// The destination follows the label's closing `](`. Whitespace and line
/// breaks may come first, and on a continuation line also the `>` markers of
/// the blockquotes holding the link; list indentation is whitespace. Each place
/// the destination could start is tried in turn, and a candidate counts only
/// when it reads as the parser's destination. A container marker, or anything
/// else mistaken for the destination, thus yields no range rather than a
/// wrong one.
fn inline_destination_range(content: &str, link: &ParsedLink<'_>) -> Option<Range<usize>> {
    let span = content.get(link.byte_offset..link.byte_end)?;
    let open = 1 + link.text.len();
    span.get(open..)?.strip_prefix("](")?;
    destination_after(content, link.byte_offset + open + 2, link.byte_end, &link.url)
}

/// Byte range of a reference definition's destination, without angle
/// brackets, or `None` when no place in the definition spanning `span` spells
/// `dest`, the destination the parser read. The destination follows the
/// label's `]:`, as an inline link's follows its `](`.
fn definition_destination_range(content: &str, span: &Range<usize>, dest: &str) -> Option<Range<usize>> {
    let bytes = content.get(span.clone())?.as_bytes();
    if bytes.first() != Some(&b'[') {
        return None;
    }
    // A label may hold an escaped bracket, never an unescaped one.
    let mut close = 1;
    loop {
        match *bytes.get(close)? {
            b'\\' => close += 2,
            b']' => break,
            _ => close += 1,
        }
    }
    if bytes.get(close + 1) != Some(&b':') {
        return None;
    }
    destination_after(content, span.start + close + 2, span.end, dest)
}

/// Byte range of the destination written after `gap_start` and before `end`,
/// without angle brackets, that reads as `dest`.
///
/// Whitespace and line breaks may come first, and on a continuation line also
/// the `>` markers of the blockquotes holding the construct; list indentation
/// is whitespace. Each place the destination could start is tried in turn, and
/// a candidate counts only when it reads as the parser's destination. A
/// container marker, or anything else mistaken for the destination, thus
/// yields no range rather than a wrong one.
fn destination_after(content: &str, gap_start: usize, end: usize, dest: &str) -> Option<Range<usize>> {
    let mut at_line_start = false;
    for (offset, ch) in content.get(gap_start..end)?.char_indices() {
        let start = gap_start + offset;
        match ch {
            ' ' | '\t' => {}
            '\n' | '\r' => at_line_start = true,
            // A `>` opening a continuation line is a blockquote marker, unless
            // the destination itself starts with it.
            '>' if at_line_start => {
                if let Some(range) = destination_at(content, start, end)
                    && decode_destination(&content[range.clone()]) == dest
                {
                    return Some(range);
                }
            }
            _ => {
                let range = destination_at(content, start, end)?;
                return (decode_destination(&content[range.clone()]) == dest).then_some(range);
            }
        }
    }
    None
}

/// A destination as written in the source, read the way the parser reads it
/// (CommonMark 2.4, 2.5): in one pass, a backslash before ASCII punctuation
/// escapes it, an entity or numeric character reference stands for its
/// character, and a carriage return is dropped. One pass means the text a
/// step produces is never read again, so `\&amp;` reads as `&amp;`.
pub(crate) fn decode_destination(written: &str) -> Cow<'_, str> {
    if !written.contains(['\\', '&', '\r']) {
        return Cow::Borrowed(written);
    }
    let bytes = written.as_bytes();
    let mut decoded = String::with_capacity(written.len());
    let mut mark = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if bytes.get(i + 1).is_some_and(u8::is_ascii_punctuation) => {
                decoded.push_str(&written[mark..i]);
                mark = i + 1;
                i += 2;
            }
            b'&' => match character_reference(&written[i..]) {
                Some((len, value)) => {
                    decoded.push_str(&written[mark..i]);
                    decoded.push_str(&value);
                    i += len;
                    mark = i;
                }
                None => i += 1,
            },
            b'\r' => {
                decoded.push_str(&written[mark..i]);
                i += 1;
                mark = i;
            }
            _ => i += 1,
        }
    }
    decoded.push_str(&written[mark..]);
    Cow::Owned(decoded)
}

/// The character reference opening `source`, which starts with `&`: its
/// length and the text it stands for. A numeric reference takes up to seven
/// decimal or six hexadecimal digits; zero, a surrogate or a value past
/// U+10FFFF stands for U+FFFD. A named one is an HTML5 entity name. Both end
/// with `;`.
fn character_reference(source: &str) -> Option<(usize, String)> {
    let bytes = source.as_bytes();
    if bytes.get(1) == Some(&b'#') {
        let hex = bytes.get(2).is_some_and(|&b| b | 0x20 == b'x');
        let (digits, radix, limit) = if hex { (3, 16, 6) } else { (2, 10, 7) };
        let count = bytes[digits..]
            .iter()
            .take(limit)
            .take_while(|b| if hex { b.is_ascii_hexdigit() } else { b.is_ascii_digit() })
            .count();
        if count == 0 || bytes.get(digits + count) != Some(&b';') {
            return None;
        }
        let character = u32::from_str_radix(&source[digits..digits + count], radix)
            .ok()
            .filter(|&codepoint| codepoint != 0)
            .and_then(char::from_u32)
            .unwrap_or('\u{FFFD}');
        return Some((digits + count + 1, character.to_string()));
    }
    let name_end = 1 + bytes[1..].iter().take_while(|b| b.is_ascii_alphanumeric()).count();
    if name_end == 1 || bytes.get(name_end) != Some(&b';') {
        return None;
    }
    let value = crate::mdx_parser::decode_named(&source[1..name_end], true)?;
    Some((name_end + 1, value))
}

/// Byte range of the destination starting at `start`, without angle brackets.
///
/// Written in angle brackets it runs to the closing `>`; otherwise it ends at
/// whitespace, at `end`, or at a `)` that closes no parenthesis opened inside
/// it, with a backslash escaping the character after it (CommonMark 6.3).
fn destination_at(content: &str, start: usize, end: usize) -> Option<Range<usize>> {
    let rest = &content[start..end];
    if let Some(inner) = rest.strip_prefix('<') {
        let close = inner.find(['>', '\n'])?;
        return (inner.as_bytes()[close] == b'>').then_some(start + 1..start + 1 + close);
    }
    let mut depth = 0usize;
    let mut escaped = false;
    for (offset, ch) in rest.char_indices() {
        match ch {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '(' => depth += 1,
            ')' if depth == 0 => return Some(start..start + offset),
            ')' => depth -= 1,
            ' ' | '\t' | '\n' | '\r' => return Some(start..start + offset),
            _ => {}
        }
    }
    Some(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;

    #[test]
    fn body_candidates_reuse_source_ranges_without_losing_exclusions() {
        let content = "[plain](a.md#part)\n\n![alt [hidden](b.md)](img.png)\n\n\
            [![image](img.png)](c.md)\n\n`[code](d.md)`\n\n[wrapped\nlabel](<has space.md>)\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        for _ in 0..2 {
            let found: Vec<_> = ctx
                .body_link_destinations()
                .map(|d| {
                    assert_eq!(&content[d.fix_range.clone().unwrap()], d.full_url().as_ref());
                    (d.url.to_string(), d.fragment.to_string())
                })
                .collect();
            assert_eq!(
                found,
                [
                    ("a.md".into(), "#part".into()),
                    ("c.md".into(), "".into()),
                    ("has space.md".into(), "".into())
                ]
            );
        }
    }

    /// Containers as the prefix of a link's first line and of its continuation
    /// lines.
    const CONTAINERS: &[(&str, &str)] = &[
        ("", ""),
        ("> ", "> "),
        ("> > ", "> > "),
        (">> ", ">>"),
        ("- ", "  "),
        ("- > ", "  > "),
        ("> - ", ">   "),
        ("- ", "\t"),
        ("> -\t", ">\t  "),
    ];

    /// Inline links with the destination written as `D` and continuation
    /// lines opening with `C`.
    const WRAPPED_FORMS: &[&str] = &[
        "[x](\nCD)",
        "[x](D\nC\"title\")",
        "[x](\nCD\nC'title')",
        "[x](  \nC  D  \nC  (title)  )",
        "[x](\nC<D>)",
    ];

    /// Destinations as written, each with whether its source text is the
    /// destination the parser reads byte for byte.
    const SPELLINGS: &[(&str, bool)] = &[
        ("t.md", true),
        ("t.md#sec", true),
        ("f(1).md#sec", true),
        ("a\\_b.md", false),
        ("t&amp;.md", false),
        ("t.md#se&#99;", false),
        ("t.md#se&#x63;", false),
        ("t\\&amp;.md", false),
        ("t&bogus;.md", true),
        ("t&#0;.md", false),
        (">t.md", true),
    ];

    #[test]
    fn located_destinations_read_as_the_parsed_destination() {
        let mut verbatim = 0;
        let mut decoded = 0;
        for (first, continuation) in CONTAINERS {
            for form in WRAPPED_FORMS {
                for &(written, is_verbatim) in SPELLINGS {
                    // A `>` opening a line starts a blockquote, so a
                    // destination starting with one stays on the `[` line.
                    if written.starts_with('>') && !form.starts_with("[x](D") {
                        continue;
                    }
                    let link = form.replace('C', continuation).replace('D', written);
                    let content = format!("# H\n\n{first}{link}\n");
                    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                    let found: Vec<_> = ctx.body_link_destinations().collect();
                    assert_eq!(found.len(), 1, "{content:?}");
                    let destination = &found[0];
                    let parsed = ctx
                        .links()
                        .iter()
                        .find(|link| link.byte_offset == destination.link_start)
                        .unwrap();
                    // The destination is always the parser's.
                    assert_eq!(destination.full_url(), parsed.url.as_ref(), "{content:?}");
                    match destination.fix_range.clone() {
                        Some(range) => {
                            assert!(is_verbatim, "{content:?} must not be rewritten");
                            assert_eq!(&content[range.clone()], parsed.url.as_ref(), "{content:?}");
                            assert_eq!(&content[destination.url_range.clone()], destination.url, "{content:?}");
                            verbatim += 1;
                        }
                        None if destination.url_range.start != destination.link_start => {
                            assert!(!is_verbatim, "{content:?}");
                            assert_eq!(
                                decode_destination(&content[destination.url_range.clone()]),
                                parsed.url.as_ref(),
                                "{content:?}"
                            );
                            decoded += 1;
                        }
                        None => panic!("{content:?} is not located"),
                    }
                }
            }
        }
        assert!(verbatim > 0 && decoded > 0);
    }

    /// Definition sources with the destination written as `D` and
    /// continuation lines opening with `C`.
    const DEFINITION_FORMS: &[&str] = &[
        "[r]: D",
        "[r]:D",
        "[r]: D \"title\"",
        "[r]:\nCD",
        "[r]:\nCD\nC'title'",
        "[r]: <D>",
        "[r\\]]: D",
        "[r]:\nC<D> (title)",
    ];

    #[test]
    fn located_definition_destinations_read_as_the_parsed_destination() {
        let mut checked = 0;
        for (first, continuation) in CONTAINERS {
            for form in DEFINITION_FORMS {
                for &(written, is_verbatim) in SPELLINGS {
                    if written.starts_with('>') && form.contains("\nCD") {
                        continue;
                    }
                    let source = form.replace('C', continuation).replace('D', written);
                    let content = format!("# H\n\n{first}{source}\n\n[x][r]\n");
                    let parsed = pulldown_cmark::Parser::new_ext(
                        &content,
                        crate::utils::parser_options::rumdl_parser_options_for(&content),
                    );
                    let expected: Vec<_> = parsed
                        .reference_definitions()
                        .iter()
                        .map(|(_, definition)| definition.dest.to_string())
                        .collect();
                    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                    let found: Vec<_> = ctx.body_reference_definitions().collect();
                    assert_eq!(
                        found.iter().map(|d| d.url.to_string()).collect::<Vec<_>>(),
                        expected,
                        "{content:?}"
                    );
                    for definition in &found {
                        let Some(range) = definition.destination.clone() else {
                            continue;
                        };
                        assert_eq!(
                            decode_destination(&content[range.clone()]),
                            definition.url,
                            "{content:?}"
                        );
                        assert_eq!(
                            definition.verbatim_range().is_some(),
                            &content[range] == definition.url,
                            "{content:?}"
                        );
                        if is_verbatim {
                            assert!(definition.verbatim_range().is_some(), "{content:?}");
                        }
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 0);
    }

    #[test]
    fn destinations_decode_as_the_parser_reads_them() {
        for written in [
            "plain.md",
            "a\\_b",
            "a\\b",
            "a\\",
            "&amp;",
            "&amp",
            "&AMP;",
            "&bogus;",
            "&;",
            "&#;",
            "&#x;",
            "&#45;",
            "&#X2d;",
            "&#1234567;",
            "&#12345678;",
            "&#x10FFFF;",
            "&#x110000;",
            "&#xFFFFFF;",
            "&#xD800;",
            "&#0;",
            "&#x0000000;",
            "\\&amp;",
            "&amp;amp;",
            "&#38;amp;",
            "&ngE;",
            "&NotNestedGreaterGreater;",
            "é&eacute;",
        ] {
            let content = format!("[r]: {written}\n\n[r]\n");
            let parsed = pulldown_cmark::Parser::new(&content);
            let (_, definition) = parsed.reference_definitions().iter().next().expect(written);
            assert_eq!(decode_destination(written), definition.dest.as_ref(), "{written:?}");
        }
    }

    #[test]
    fn body_link_fragments_cover_exactly_the_written_fragment() {
        let content = "[a](#one) [b](<x y.md#two> \"t#no\") [c](f(1).md#three)\n\
            [d](e.md#four 'x') [esc](e.md#fi\\_ve) `[code](#six)` [ref][r] [none](g.md)\n\n[r]: #seven\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let found: Vec<_> = ctx
            .body_link_fragments()
            .map(|link| {
                assert_eq!(&content[link.range.clone()], link.fragment);
                (link.path, link.fragment)
            })
            .collect();
        // The escaped fragment reads `fi_ve`, which no byte range spells.
        assert_eq!(
            found,
            [("", "one"), ("x y.md", "two"), ("f(1).md", "three"), ("e.md", "four")]
        );
    }
}
