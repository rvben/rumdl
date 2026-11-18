//! Inline-link destinations written inside HTML comments.
//!
//! Comments are taken from the parser, never rescanned from the source: a
//! comment is what pulldown-cmark reports as one, so every container rule
//! (blockquotes, lists, footnotes, lazy continuation) and every context where
//! `<!--` is plain text (a tag attribute, a link title, `<script>`, a code
//! span or block) is already settled. A comment that starts a block arrives as
//! one `Html` event per line with the container prefix removed; a comment
//! inside a paragraph arrives as one `InlineHtml` event holding the source
//! slice. The comment's interior is parsed again, as the body is, and every
//! offset is mapped back to the document through a segment table.
//!
//! Nothing here is ever rewritten, so no destination carries a fix range.
//! Whatever cannot be located or read with certainty yields no link: the cost
//! of a doubtful comment is a missed link, never an invented one.

use super::LintContext;
use super::link_destinations::{BodyLinkDestination, destination_after, split_fragment};
use crate::utils::parser_options::rumdl_parser_options_for;
use pulldown_cmark::{Event, LinkType, Parser, Tag, TagEnd};
use std::ops::Range;

/// One link of a comment, owned so the shared destination view can borrow it.
pub(crate) struct CommentLink {
    link_start: usize,
    url: String,
    fragment: String,
    url_range: Range<usize>,
    full_range: Range<usize>,
}

/// The destination of one inline link written inside an HTML comment.
pub(crate) struct CommentLinkDestination<'a> {
    /// The destination in the shape body links use. `fix_range` is always `None`.
    pub destination: BodyLinkDestination<'a>,
    /// The whole written destination, path and fragment, as one source range;
    /// the link's first line when it could not be located.
    pub full_range: Range<usize>,
}

/// A run of text and the source offset it was copied from.
struct Segment {
    start: usize,
    len: usize,
    src: usize,
}

/// Text assembled from pieces of the source, with the map back to it.
#[derive(Default)]
struct Mapped {
    text: String,
    segments: Vec<Segment>,
}

impl Mapped {
    /// Appends `text`, which the source spells at `src`. Pieces that are
    /// contiguous in the source form one segment, so a destination that wraps
    /// onto the next line of a container-free comment still maps as a whole.
    fn push(&mut self, text: &str, src: usize) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        match self.segments.last_mut() {
            Some(last) if last.src + last.len == src && last.start + last.len == start => last.len += text.len(),
            _ => self.segments.push(Segment {
                start,
                len: text.len(),
                src,
            }),
        }
        self.text.push_str(text);
    }

    /// Appends `range` of `from`, with its source offsets.
    fn copy(&mut self, from: &Mapped, range: Range<usize>) {
        for segment in &from.segments {
            let start = segment.start.max(range.start);
            let end = (segment.start + segment.len).min(range.end);
            if start < end {
                self.push(&from.text[start..end], segment.src + (start - segment.start));
            }
        }
    }

    /// The source offset of `offset`, which must lie inside a segment.
    fn source_of(&self, offset: usize) -> Option<usize> {
        let segment = self.segment_holding(offset)?;
        Some(segment.src + (offset - segment.start))
    }

    /// The source range of `range`, when one segment holds all of it.
    fn source_range(&self, range: &Range<usize>) -> Option<Range<usize>> {
        let segment = self.segment_holding(range.start)?;
        (range.end <= segment.start + segment.len)
            .then(|| segment.src + (range.start - segment.start)..segment.src + (range.end - segment.start))
    }

    fn segment_holding(&self, offset: usize) -> Option<&Segment> {
        let after = self
            .segments
            .partition_point(|segment| segment.start + segment.len <= offset);
        self.segments.get(after).filter(|segment| segment.start <= offset)
    }
}

impl<'a> LintContext<'a> {
    /// Destinations of the inline links written inside HTML comments, in
    /// source order, one per link. Directive comments are left out. Each
    /// consumer applies its own option, path policy and filesystem checks.
    pub(crate) fn html_comment_link_destinations(&self) -> impl Iterator<Item = CommentLinkDestination<'_>> + '_ {
        self.html_comment_links_cache
            .get_or_init(|| extract(self))
            .iter()
            .map(|link| CommentLinkDestination {
                destination: BodyLinkDestination {
                    link_start: link.link_start,
                    url: &link.url,
                    fragment: &link.fragment,
                    url_range: link.url_range.clone(),
                    fix_range: None,
                },
                full_range: link.full_range.clone(),
            })
    }
}

/// Reads every comment the parser reports and collects the inline links of
/// their interiors.
fn extract(ctx: &LintContext<'_>) -> Vec<CommentLink> {
    let content = ctx.content;
    let body_start = ctx
        .line_offsets
        .get(ctx.front_matter_end)
        .copied()
        .unwrap_or(content.len());
    let body = &content[body_start..];
    let mut links = Vec::new();
    if !body.contains("<!--") {
        return links;
    }
    let mut image_depth = 0usize;
    let mut block: Option<Vec<(Range<usize>, String)>> = None;
    for (event, range) in Parser::new_ext(body, rumdl_parser_options_for(body)).into_offset_iter() {
        match event {
            Event::Start(Tag::Image { .. }) => image_depth += 1,
            Event::End(TagEnd::Image) => image_depth = image_depth.saturating_sub(1),
            Event::Start(Tag::HtmlBlock) => block = Some(Vec::new()),
            Event::Html(text) => {
                if let Some(lines) = block.as_mut() {
                    lines.push((range, text.to_string()));
                }
            }
            Event::End(TagEnd::HtmlBlock) => {
                if let Some(lines) = block.take() {
                    let mut raw = Mapped::default();
                    let mut sound = true;
                    for (line_range, text) in &lines {
                        sound &= body.get(line_range.clone()) == Some(text.as_str());
                        raw.push(text, body_start + line_range.start);
                    }
                    if sound {
                        read_comment(ctx, &raw, false, &mut links);
                    }
                }
            }
            Event::InlineHtml(text) if image_depth == 0 && body.get(range.clone()) == Some(&*text) => {
                let mut raw = Mapped::default();
                raw.push(&text, body_start + range.start);
                read_comment(ctx, &raw, true, &mut links);
            }
            _ => {}
        }
    }
    links
}

/// Whether the byte at `offset` is text the renderer takes for its own,
/// whatever the parser made of it: the regions `is_body_markdown` excludes,
/// less the comment and raw HTML ones this module is about.
fn in_foreign_region(ctx: &LintContext<'_>, offset: usize) -> bool {
    let line = ctx.line_offsets.partition_point(|&start| start <= offset);
    ctx.line_info(line).is_some_and(|info| {
        info.in_front_matter
            || info.in_code_block
            || info.in_math_block
            || info.in_pymdown_block
            || info.in_kramdown_extension_block
            || info.in_mkdocstrings
            || info.in_esm_block
    }) || ctx.is_in_math_span(offset)
        || ctx.is_in_obsidian_comment(offset)
        || ctx.is_in_mdx_comment(offset)
        || ctx.is_in_jsx_expression(offset)
        || ctx.is_in_jinja_range(offset)
        || ctx.is_in_shortcode(offset)
}

/// Collects the links of one comment: `raw` holds it from `<!--` on.
fn read_comment(ctx: &LintContext<'_>, raw: &Mapped, inline: bool, out: &mut Vec<CommentLink>) {
    const OPEN: &str = "<!--";
    let Some(after_open) = raw.text.strip_prefix(OPEN) else {
        return;
    };
    let Some(close) = after_open.find("-->") else {
        return;
    };
    let Some(start) = raw.source_of(0) else {
        return;
    };
    let interior_text = &after_open[..close];
    if crate::inline_config::is_directive_comment(interior_text) || in_foreign_region(ctx, start) {
        return;
    }
    // A comment inside a blockquote that goes on past its first line carries
    // the quote markers of the later lines in its text.
    let line = ctx.line_offsets.partition_point(|&offset| offset <= start);
    if inline && interior_text.contains('\n') && ctx.line_info(line).is_some_and(|info| info.blockquote.is_some()) {
        return;
    }
    let lead = interior_text
        .find(|c: char| c != ' ' && c != '\t')
        .unwrap_or(interior_text.len());
    let mut interior = Mapped::default();
    interior.copy(raw, OPEN.len() + lead..OPEN.len() + close);
    collect_links(ctx, &interior, out);
}

/// Parses `interior` as the body is parsed and collects its inline links
/// that render as hyperlinks (not images, not inside an image description).
fn collect_links(ctx: &LintContext<'_>, interior: &Mapped, out: &mut Vec<CommentLink>) {
    let text = interior.text.as_str();
    let mut image_depth = 0usize;
    for (event, range) in Parser::new_ext(text, rumdl_parser_options_for(text)).into_offset_iter() {
        match event {
            Event::Start(Tag::Image { .. }) => image_depth += 1,
            Event::End(TagEnd::Image) => image_depth = image_depth.saturating_sub(1),
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url,
                ..
            }) if image_depth == 0 => {
                if let Some(link) = locate(ctx, interior, &range, &dest_url) {
                    out.push(link);
                }
            }
            _ => {}
        }
    }
}

/// Maps one parsed link back to the document. A destination that cannot be
/// located exactly is reported at the link's first line.
fn locate(ctx: &LintContext<'_>, interior: &Mapped, link: &Range<usize>, dest: &str) -> Option<CommentLink> {
    let text = interior.text.as_str();
    let link_start = interior.source_of(link.start)?;
    let slice = text.get(link.clone())?;
    // A `](` can also sit inside the label or inside the title, so a candidate
    // counts only when the destination it yields is followed by nothing but
    // an optional title and the closing parenthesis. Of the candidates that
    // do, the last one is taken.
    let written = slice.rmatch_indices("](").find_map(|(at, _)| {
        destination_after(text, link.start + at + 2, link.end, dest).filter(|range| {
            let tail = &text[range.end..link.end];
            // The closing `>` of an angle-bracketed destination is part of it.
            let tail = if text[..range.start].ends_with('<') {
                tail.strip_prefix('>').unwrap_or(tail)
            } else {
                tail
            };
            ends_like_a_link_tail(tail)
        })
    });
    let (url, fragment) = split_fragment(dest);
    let located = written.and_then(|range| {
        let source = interior.source_range(&range)?;
        let verbatim = text[range] == *dest;
        Some((
            if verbatim {
                source.start..source.start + url.len()
            } else {
                source.clone()
            },
            source,
        ))
    });
    let (url_range, full_range) = located.unwrap_or_else(|| {
        let end = ctx.content[link_start..]
            .find(['\n', '\r'])
            .map_or(ctx.content.len(), |at| link_start + at);
        (link_start..end, link_start..end)
    });
    Some(CommentLink {
        link_start,
        url: url.to_string(),
        fragment: fragment.to_string(),
        url_range,
        full_range,
    })
}

/// Whether `tail`, the text between a destination and the end of its link,
/// reads as an optional title followed by the closing parenthesis.
fn ends_like_a_link_tail(tail: &str) -> bool {
    let tail = tail.trim_start();
    tail == ")" || (tail.starts_with(['"', '\'', '(']) && tail.ends_with(')'))
}

#[cfg(test)]
mod tests {
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    /// (url, fragment, source text of `url_range`, source text of `full_range`)
    fn links(src: &str) -> Vec<(String, String, String, String)> {
        links_for(src, MarkdownFlavor::Standard)
    }

    fn links_for(src: &str, flavor: MarkdownFlavor) -> Vec<(String, String, String, String)> {
        let ctx = LintContext::new(src, flavor, None);
        ctx.html_comment_link_destinations()
            .map(|link| {
                assert!(
                    link.destination.fix_range.is_none(),
                    "a comment link is never rewritten"
                );
                (
                    link.destination.url.to_string(),
                    link.destination.fragment.to_string(),
                    src[link.destination.url_range.clone()].to_string(),
                    src[link.full_range.clone()].to_string(),
                )
            })
            .collect()
    }

    fn one(src: &str, url: &str, fragment: &str, written: &str, full: &str) {
        assert_eq!(
            links(src),
            vec![(url.into(), fragment.into(), written.into(), full.into())],
            "{src:?}"
        );
    }

    fn none(src: &str) {
        assert_eq!(links(src), vec![], "{src:?}");
    }

    #[test]
    fn single_line_comment_gives_exact_destination() {
        one(
            "<!-- [x](missing.md) -->\n",
            "missing.md",
            "",
            "missing.md",
            "missing.md",
        );
    }

    #[test]
    fn mixed_line_comment_keeps_path_and_fragment_apart() {
        one(
            "text <!-- [x](a.md#frag) --> more\n",
            "a.md",
            "#frag",
            "a.md",
            "a.md#frag",
        );
    }

    #[test]
    fn multi_line_block_comment_maps_to_its_own_line() {
        let src = "intro\n\n<!--\nnote\n[x](gone.md)\n-->\n";
        let found = links(src);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].2, "gone.md");
        let ctx = LintContext::new(src, MarkdownFlavor::Standard, None);
        let dest = ctx.html_comment_link_destinations().next().unwrap();
        assert_eq!(dest.destination.url_range.start, src.find("gone.md").unwrap());
    }

    #[test]
    fn destination_is_not_taken_from_a_lookalike_inside_the_title() {
        let src = "<!-- [x](missing.md \"mention ](missing.md again\") -->\n";
        let ctx = LintContext::new(src, MarkdownFlavor::Standard, None);
        let found: Vec<_> = ctx
            .html_comment_link_destinations()
            .map(|l| (l.destination.url_range.start, l.full_range.start))
            .collect();
        let real = src.find("missing.md").unwrap();
        assert_eq!(found, vec![(real, real)], "{src:?}");
    }

    #[test]
    fn angle_bracketed_destination_is_located_inside_the_brackets() {
        let src = "<!-- [x](<missing.md>) -->\n";
        let ctx = LintContext::new(src, MarkdownFlavor::Standard, None);
        let found: Vec<_> = ctx
            .html_comment_link_destinations()
            .map(|l| (l.destination.url_range.start, l.full_range.start))
            .collect();
        let real = src.find("missing.md").unwrap();
        assert_eq!(found, vec![(real, real)], "{src:?}");
    }

    #[test]
    fn destination_is_the_one_after_the_label_not_a_lookalike_inside_it() {
        // The label holds a code span that reads as a link with the same destination.
        let src = "<!-- [`[fake](missing.md)`](missing.md) -->\n";
        let ctx = LintContext::new(src, MarkdownFlavor::Standard, None);
        let found: Vec<_> = ctx
            .html_comment_link_destinations()
            .map(|l| (l.destination.url_range.start, l.full_range.start))
            .collect();
        let outer = src.rfind("missing.md").unwrap();
        assert_eq!(found, vec![(outer, outer)], "{src:?}");
    }

    #[test]
    fn link_start_is_the_opening_bracket_of_the_link() {
        for src in [
            "<!-- [x](a.md) -->\n",
            "é <!-- see [x](a.md) -->\n",
            "<!--\nnote\n[x](a.md)\n-->\n",
            "> <!--\n> [x](a.md)\n> -->\n",
        ] {
            let ctx = LintContext::new(src, MarkdownFlavor::Standard, None);
            let starts: Vec<usize> = ctx
                .html_comment_link_destinations()
                .map(|l| l.destination.link_start)
                .collect();
            assert_eq!(starts, vec![src.find("[x]").unwrap()], "{src:?}");
        }
    }

    #[test]
    fn unicode_before_the_comment_maps_by_bytes() {
        one("é日本 <!-- [x](ü.md) -->\n", "ü.md", "", "ü.md", "ü.md");
    }

    #[test]
    fn crlf_block_comment_maps_back() {
        let src = "<!--\r\n[x](a.md#h)\r\n-->\r\n";
        assert_eq!(
            links(src),
            vec![("a.md".into(), "#h".into(), "a.md".into(), "a.md#h".into())]
        );
    }

    #[test]
    fn link_wrapping_lines_inside_a_comment() {
        one("<!--\n[x](\na.md)\n-->\n", "a.md", "", "a.md", "a.md");
        one("<!--\n[x\ny](a.md)\n-->\n", "a.md", "", "a.md", "a.md");
    }

    #[test]
    fn link_text_with_brackets_and_code_spans() {
        one("<!-- [a `b` [c]](x.md) -->\n", "x.md", "", "x.md", "x.md");
        one("<!-- [`code`](x.md) -->\n", "x.md", "", "x.md", "x.md");
    }

    #[test]
    fn escaped_destination_is_decoded_and_whole_destination_is_the_range() {
        let src = "<!-- [x](a\\_b.md) -->\n";
        let found = links(src);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "a_b.md");
        assert_eq!(found[0].2, "a\\_b.md");
    }

    #[test]
    fn several_links_in_one_comment_in_source_order() {
        let found = links("<!-- [a](one.md) and [b](two.md#s) -->\n");
        let urls: Vec<_> = found.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(urls, ["one.md", "two.md"]);
    }

    #[test]
    fn directive_comments_are_skipped_but_prose_starting_alike_is_read() {
        none("<!-- rumdl-disable MD057 [x](missing.md) -->\n");
        none("<!-- markdownlint-disable-next-line [x](missing.md) -->\n");
        none("<!-- rumdl-configure-file { \"MD057\": {} } [x](missing.md) -->\n");
        none("<!--\nrumdl-disable\n[x](missing.md)\n-->\n");
        one(
            "<!-- rumdlfoo [x](missing.md) -->\n",
            "missing.md",
            "",
            "missing.md",
            "missing.md",
        );
        one(
            "<!-- see rumdl-disable [x](missing.md) -->\n",
            "missing.md",
            "",
            "missing.md",
            "missing.md",
        );
    }

    #[test]
    fn code_is_not_a_comment() {
        none("`<!-- [x](a.md) -->`\n");
        none("```\n<!-- [x](a.md) -->\n```\n");
        none("para\n\n    <!-- [x](a.md) -->\n");
        none("~~~md\n<!--\n[x](a.md)\n-->\n~~~\n");
    }

    #[test]
    fn unclosed_comment_yields_nothing() {
        none("<!-- [x](a.md)\n\ntext\n");
        none("<!--\n[x](a.md)\n");
    }

    #[test]
    fn front_matter_is_not_a_comment() {
        none("---\nnote: |\n  <!-- [x](a.md) -->\n---\n\nBody\n");
    }

    #[test]
    fn body_comment_after_front_matter_holding_an_opener_is_found() {
        let src = "---\nexample: |\n  <!--\n---\n\n<!-- [x](missing.md) -->\n";
        one(src, "missing.md", "", "missing.md", "missing.md");
    }

    #[test]
    fn comment_in_blockquote_nested_blockquote_and_lists() {
        one("> <!--\n> [x](a.md)\n> -->\n", "a.md", "", "a.md", "a.md");
        one("> > <!--\n> > [x](a.md)\n> > -->\n", "a.md", "", "a.md", "a.md");
        one("- item\n\n  <!--\n  [x](a.md)\n  -->\n", "a.md", "", "a.md", "a.md");
        one(
            "100. Text\nlazy\n\n     <!--\n     [x](a.md)\n     -->\n",
            "a.md",
            "",
            "a.md",
            "a.md",
        );
        one(
            "[^1]: note\n\n    <!--\n    [x](a.md)\n    -->\n",
            "a.md",
            "",
            "a.md",
            "a.md",
        );
        one(
            "Term\n:   Def\n\n    <!--\n    [x](a.md)\n    -->\n",
            "a.md",
            "",
            "a.md",
            "a.md",
        );
    }

    #[test]
    fn link_wrapping_lines_in_a_quoted_comment() {
        one("> <!--\n> [x](\n> a.md)\n> -->\n", "a.md", "", "a.md", "a.md");
    }

    #[test]
    fn indented_code_inside_a_container_comment_stays_code() {
        none("- item\n\n    <!--\n      [code](a.md)\n    -->\n");
    }

    #[test]
    fn what_the_parser_does_not_read_as_a_comment_is_not_one() {
        none("A <span title=\"<!-- [x](a.md) -->\">B</span>\n");
        none("<div title=\"\n<!-- [x](a.md) -->\n\">\n");
        none("<script>\nx = \"<!-- [x](a.md) -->\";\n</script>\n");
        none("<pre>\n<!-- [x](a.md) -->\n</pre>\n");
        none("[a](b.md \"<!-- [x](c.md) -->\")\n");
        none("![Diagram <!-- [x](missing.md) -->](diagram.png)\n");
    }

    #[test]
    fn images_autolinks_and_references_are_excluded() {
        none("<!-- ![a](x.png) -->\n");
        none("<!-- <https://example.com/a.md> -->\n");
        none("<!-- [r][ref]\n\n[ref]: a.md -->\n");
        none("<!-- [[wiki]] -->\n");
        none("<!-- ![alt [x](a.md)](p.png) -->\n");
    }

    #[test]
    fn empty_comments_do_not_panic_and_yield_nothing() {
        none("<!-->\n");
        none("<!--->\n");
        none("<!---->\n");
        none("<!-- -->\n");
        none("text <!----> text\n");
    }

    #[test]
    fn multi_line_inline_comment_strips_small_indentation_only() {
        one("text <!--\n  [x](a.md)\n-->\n", "a.md", "", "a.md", "a.md");
        none("text <!--\n    [x](a.md)\n-->\n");
        none("> text <!-- `a\n> [x](a.md)` -->\n");
        none("text <!--\n```\n[x](a.md)\n```\n-->\n");
    }

    #[test]
    fn inline_comment_in_a_paragraph_between_text() {
        one("before <!-- [x](a.md) --> after\n", "a.md", "", "a.md", "a.md");
    }

    #[test]
    fn leading_whitespace_after_the_opener_is_not_indented_code() {
        one("<!--     [x](a.md) -->\n", "a.md", "", "a.md", "a.md");
    }

    #[test]
    fn documented_limitations_report_nothing() {
        none("<!-- a --> <!-- [x](a.md) -->\n");
        none("<div>\n<!-- [x](a.md) -->\n</div>\n");
        assert_eq!(
            links_for("!!! note\n\n    <!-- [x](a.md) -->\n", MarkdownFlavor::MkDocs),
            vec![]
        );
    }

    #[test]
    fn mdx_expression_obsidian_and_myst_openers_are_not_comments() {
        assert_eq!(
            links_for("{\"<!-- [x](missing.md) -->\"}\n", MarkdownFlavor::MDX),
            vec![]
        );
        assert_eq!(
            links_for("%% <!-- [x](missing.md) --> %%\n", MarkdownFlavor::Obsidian),
            vec![]
        );
    }

    #[test]
    fn indented_block_comment_is_found() {
        one("  <!-- [x](a.md) -->\n", "a.md", "", "a.md", "a.md");
    }
}
