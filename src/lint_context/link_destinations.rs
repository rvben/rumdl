//! Raw inline destinations, with source spelling and byte positions retained.

use super::{LintContext, ParsedImage, ParsedLink};
use crate::workspace_index::{URL_EXTRACT_ANGLE_BRACKET_REGEX, URL_EXTRACT_REGEX};
use pulldown_cmark::LinkType;
use regex::Regex;
use std::borrow::Cow;
use std::ops::Range;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceDestination {
    syntax: Range<usize>,
    pub url: Range<usize>,
    fragment: Option<Range<usize>>,
    angle: bool,
}

impl SourceDestination {
    fn from_captures(caps: &regex::Captures<'_>, base: usize, angle: bool) -> Option<Self> {
        let absolute = |range: Range<usize>| base + range.start..base + range.end;
        Some(Self {
            syntax: absolute(caps.get(0)?.range()),
            url: absolute(caps.get(1)?.range()),
            fragment: caps.get(2).map(|group| absolute(group.range())),
            angle,
        })
    }

    pub(crate) fn fragment<'a>(&self, content: &'a str) -> &'a str {
        self.fragment.as_ref().map_or("", |range| &content[range.clone()])
    }

    fn body<'a>(&self, content: &'a str) -> BodyLinkDestination<'a> {
        BodyLinkDestination {
            url: content[self.url.clone()].trim(),
            fragment: self.fragment(content),
            url_range: self.url.clone(),
            fix_range: self.url.start..self.fragment.as_ref().map_or(self.url.end, |range| range.end),
        }
    }
}

/// Source facts align with the parser's links. The secondary views are built
/// only when requested, and never cache rule configuration or disk answers.
pub(super) struct LinkDestinations {
    destinations: Vec<LocatedDestination>,
    by_anchor: OnceLock<Vec<(usize, usize)>>,
    body: OnceLock<Vec<usize>>,
}

struct LocatedDestination {
    link: usize,
    source: SourceDestination,
}

impl LinkDestinations {
    pub(super) fn new(ctx: &LintContext<'_>) -> Self {
        Self {
            destinations: ctx
                .links()
                .iter()
                .enumerate()
                .filter_map(|(link, parsed)| {
                    locate_destination(ctx.content, parsed).map(|source| LocatedDestination { link, source })
                })
                .collect(),
            by_anchor: OnceLock::new(),
            body: OnceLock::new(),
        }
    }

    fn at(&self, anchor: usize) -> Option<&SourceDestination> {
        let indices = self.by_anchor.get_or_init(|| {
            let mut indices: Vec<_> = self
                .destinations
                .iter()
                .enumerate()
                .map(|(i, destination)| (destination.source.syntax.start, i))
                .collect();
            indices.sort_unstable();
            indices
        });
        let i = indices.binary_search_by_key(&anchor, |&(anchor, _)| anchor).ok()?;
        Some(&self.destinations[indices[i].1].source)
    }

    fn body_indices(&self, ctx: &LintContext<'_>) -> &[usize] {
        self.body.get_or_init(|| {
            debug_assert!(
                ctx.links()
                    .windows(2)
                    .all(|pair| pair[0].byte_offset <= pair[1].byte_offset)
            );
            let mut pending_images = ctx.images().iter().peekable();
            let mut open_images: Vec<&ParsedImage<'_>> = Vec::new();
            self.destinations
                .iter()
                .enumerate()
                .filter_map(|(i, destination)| {
                    let link = &ctx.links()[destination.link];
                    if !matches!(link.link_type, LinkType::Inline)
                        || ctx
                            .line_info(link.line)
                            .is_some_and(|info| info.in_front_matter || info.in_pymdown_block)
                        || ctx.is_in_code_span_byte(link.byte_offset)
                        || ctx.is_in_math_span(link.byte_offset)
                        || ctx.is_in_shortcode(link.byte_offset)
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

/// The destination of one inline link, located in the original document.
pub(crate) struct BodyLinkDestination<'a> {
    /// Raw path spelling, without the fragment; filesystem policy is separate.
    pub url: &'a str,
    pub fragment: &'a str,
    /// Diagnostic range. A rewrite also includes the fragment in `fix_range`.
    pub url_range: Range<usize>,
    pub fix_range: Range<usize>,
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

/// `captures_at` searches forward; a parsed link must use its own bracket's
/// destination, never one borrowed from a later link in the same span.
fn extract_url_at<'a>(re: &Regex, source: &'a str, anchor: usize) -> Option<regex::Captures<'a>> {
    let caps = re.captures_at(source, anchor)?;
    (caps.get(0)?.start() == anchor).then_some(caps)
}

fn locate_destination(content: &str, link: &ParsedLink<'_>) -> Option<SourceDestination> {
    if !matches!(link.link_type, LinkType::Inline) {
        return None;
    }
    let span = content.get(link.byte_offset..link.byte_end)?;
    let anchor = 1 + link.text.len();
    if !span.get(anchor..).is_some_and(|rest| rest.starts_with("](")) {
        return None;
    }
    if span[anchor..].contains('<')
        && let Some(caps) = extract_url_at(&URL_EXTRACT_ANGLE_BRACKET_REGEX, span, anchor)
    {
        return SourceDestination::from_captures(&caps, link.byte_offset, true);
    }
    SourceDestination::from_captures(
        &extract_url_at(&URL_EXTRACT_REGEX, span, anchor)?,
        link.byte_offset,
        false,
    )
}

impl<'a> LintContext<'a> {
    fn link_destinations(&self) -> &LinkDestinations {
        self.link_destinations_cache.get_or_init(|| LinkDestinations::new(self))
    }

    /// Inline destinations rendered as body hyperlinks. Each consumer still
    /// applies its own configuration, path policy and filesystem checks.
    pub(crate) fn body_link_destinations(&self) -> impl Iterator<Item = BodyLinkDestination<'a>> + '_ {
        let facts = self.link_destinations();
        facts
            .body_indices(self)
            .iter()
            .map(|&i| facts.destinations[i].source.body(self.content))
    }

    /// The workspace index historically searches ahead on a single line and
    /// prefers any angle destination over a bare one. Reuse a parsed source
    /// destination only when those rules give the same answer; retain the
    /// original search for unmatched brackets and multiline syntax.
    pub(crate) fn indexed_link_destination(&self, line: usize, anchor: usize) -> Option<SourceDestination> {
        let info = self.line_info(line)?;
        let source = info.content(self.content);
        let base = info.byte_offset;
        if let Some(destination) = self.link_destinations().at(base + anchor)
            && destination.syntax.end <= base + source.len()
            && (destination.angle || !source[anchor..].contains('<'))
        {
            return Some(destination.clone());
        }
        if let Some(caps) = URL_EXTRACT_ANGLE_BRACKET_REGEX.captures_at(source, anchor) {
            return SourceDestination::from_captures(&caps, base, true);
        }
        SourceDestination::from_captures(&URL_EXTRACT_REGEX.captures_at(source, anchor)?, base, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use proptest::prelude::*;

    fn compare_index_search(content: &str) {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        for (i, line) in ctx.lines.iter().enumerate() {
            let source = line.content(content);
            for (anchor, _) in source.match_indices(']') {
                let expected = URL_EXTRACT_ANGLE_BRACKET_REGEX
                    .captures_at(source, anchor)
                    .and_then(|caps| SourceDestination::from_captures(&caps, line.byte_offset, true))
                    .or_else(|| {
                        URL_EXTRACT_REGEX
                            .captures_at(source, anchor)
                            .and_then(|caps| SourceDestination::from_captures(&caps, line.byte_offset, false))
                    });
                assert_eq!(
                    ctx.indexed_link_destination(i + 1, anchor),
                    expected,
                    "{content:?}, anchor {anchor}"
                );
            }
        }
    }

    #[test]
    fn cached_destinations_preserve_each_consumers_search_boundaries() {
        for source in [
            "[first](one.md) [second](<two.md>)",
            "[first](#fragment) [second](two.md#part)",
            "[first]() [second](<two.md>)",
            "[outer [inner]](one.md) [after](two.md)",
            "é [label](one(inner).md#part) [after](two.md)",
            "[label](<with space.md> \"title\") [after](two.md)",
            "[multi\nline](one.md#part) [after](two.md)",
            "[label](\n<one.md>\n) [after](two.md)\r\n",
            "[a](one.md) text <unclosed [b](two.md)",
            "![outer [inner](one.md)](image.png)",
            "[link](one.md \"title](<two.md>)\")",
        ] {
            compare_index_search(source);
        }
    }

    #[test]
    fn body_candidates_reuse_source_ranges_without_losing_exclusions() {
        let content = "[plain](a.md#part)\n\n![alt [hidden](b.md)](img.png)\n\n\
            [![image](img.png)](c.md)\n\n`[code](d.md)`\n\n[wrapped\nlabel](<has space.md>)\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        for _ in 0..2 {
            let found: Vec<_> = ctx
                .body_link_destinations()
                .map(|d| {
                    assert_eq!(&content[d.fix_range.clone()], d.full_url().as_ref());
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

    proptest! {
        #[test]
        fn cached_index_search_matches_original_regexes(
            pieces in prop::collection::vec(prop_oneof![
                Just("[x](a.md#part)"), Just("[x](<b.md>)"), Just("[x](#part)"),
                Just("[x]()"), Just("![a [b](c.md)](d.png)"), Just("[x]"),
                Just("[x](a(b).md)"), Just("[x](<a b.md> \"title\")"),
                Just("é🦀"), Just("\n"), Just("\r\n"), Just(" "),
                Just("("), Just(")"), Just("<"), Just(">"), Just("#"), Just("\\"),
                Just("["), Just("]"), Just("\""), Just("`"),
            ], 0..35)
        ) {
            compare_index_search(&pieces.concat());
        }
    }
}
