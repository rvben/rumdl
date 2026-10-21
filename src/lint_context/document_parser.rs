//! Shared source analysis for one immutable document version.
//!
//! Native source scanning handles unambiguous documents; pulldown-cmark handles
//! the general grammar. Structure, links, and compatible line facts share their
//! analysis. Different dialect readings and masked input retain separate parses.
//! Backend events stay private; callers receive facts rather than a full AST.

use super::link_parser::{self, ParsedLinks};
use super::types::{BrokenLinkInfo, LineInfo, SourceEmphasisSpan};
use crate::config::MarkdownFlavor;
use crate::utils::code_block_utils::{CodeBlockUtils, ParseResult};
use crate::utils::parser_options::rumdl_parser_options_for;
use crate::utils::skip_context::ByteRange;
use pulldown_cmark::{BrokenLink, CowStr, Event, LinkType, Parser, Tag, TagEnd};
use std::cell::RefCell;
use std::ops::Range;

pub(crate) mod blocks;

pub(crate) mod handoff;
mod native;

pub(crate) struct ParsedDocument<'a> {
    pub(super) structure: ParseResult,

    pub(super) has_math: bool,

    pub(super) parser_blocks: Option<blocks::BlockFacts>,
    pub(super) pending_links: PendingLinks<'a>,
    pub(super) pending_lines: PendingLineEvents,
    /// The lines, ascending, on which the parser found inline content, a table
    /// or a heading starting: what follows the link reference definitions a
    /// paragraph opens with. `None` when the document was read without the
    /// parser, or holds no definition.
    pub(super) text_start_lines: Option<Vec<usize>>,
}

/// Compact, source-only facts used by line classification. These contain no
/// borrowed strings, so their temporary storage can be reused between files.
#[derive(Debug)]
pub(super) enum LineEvent {
    ListStart(bool),
    ListEnd,
    ItemStart(usize),
    Emphasis { start: usize, end: usize, strong: bool },
}

impl LineEvent {
    pub(super) fn from_event(event: &Event<'_>, range: &Range<usize>) -> Option<Self> {
        match event {
            Event::Start(Tag::List(number)) => Some(Self::ListStart(number.is_some())),
            Event::End(TagEnd::List(_)) => Some(Self::ListEnd),
            Event::Start(Tag::Item) => Some(Self::ItemStart(range.start)),
            Event::Start(Tag::Emphasis | Tag::Strong) => Some(Self::Emphasis {
                start: range.start,
                end: range.end,
                strong: matches!(event, Event::Start(Tag::Strong)),
            }),
            _ => None,
        }
    }
}

thread_local! {
    static LINE_EVENT_BUFFER: RefCell<Vec<LineEvent>> = const { RefCell::new(Vec::new()) };
    static INLINE_LINK_BUFFER: RefCell<Vec<InlineLinkRange>> = const { RefCell::new(Vec::new()) };
}

/// A native inline link needs only three source offsets and its image flag.
/// Its label and destination stay in the source until context filtering runs.
#[derive(Clone, Copy)]
pub(super) struct InlineLinkRange {
    pub(super) start: usize,
    pub(super) label_end: usize,
    pub(super) end: usize,
    pub(super) image: bool,
}

pub(super) struct PendingInlineLinks {
    ranges: Vec<InlineLinkRange>,
}

impl PendingInlineLinks {
    fn new() -> Self {
        Self {
            ranges: INLINE_LINK_BUFFER.with(|buffer| std::mem::take(&mut *buffer.borrow_mut())),
        }
    }

    fn push(&mut self, link: InlineLinkRange) {
        self.ranges.push(link);
    }

    fn events<'a>(&mut self, content: &'a str) -> impl Iterator<Item = (Event<'a>, Range<usize>)> {
        self.ranges.drain(..).flat_map(move |link| {
            let label_start = link.start + 1 + usize::from(link.image);
            let url = CowStr::Borrowed(&content[link.label_end + 2..link.end - 1]);
            let tag = if link.image {
                Tag::Image {
                    link_type: LinkType::Inline,
                    dest_url: url,
                    title: CowStr::Borrowed(""),
                    id: CowStr::Borrowed(""),
                }
            } else {
                Tag::Link {
                    link_type: LinkType::Inline,
                    dest_url: url,
                    title: CowStr::Borrowed(""),
                    id: CowStr::Borrowed(""),
                }
            };
            let range = link.start..link.end;
            [
                Some((Event::Start(tag), range.clone())),
                (!link.image && label_start < link.label_end).then(|| {
                    (
                        Event::Text(CowStr::Borrowed(&content[label_start..link.label_end])),
                        label_start..link.label_end,
                    )
                }),
                Some((Event::End(if link.image { TagEnd::Image } else { TagEnd::Link }), range)),
            ]
            .into_iter()
            .flatten()
        })
    }
}

impl Drop for PendingInlineLinks {
    fn drop(&mut self) {
        let mut ranges = std::mem::take(&mut self.ranges);
        ranges.clear();
        if ranges.capacity() <= 8192 {
            INLINE_LINK_BUFFER.with(|buffer| {
                let mut buffer = buffer.borrow_mut();
                if ranges.capacity() > buffer.capacity() {
                    *buffer = ranges;
                }
            });
        }
    }
}

#[derive(Default)]
pub(super) struct PendingLineEvents {
    events: Option<Vec<LineEvent>>,
}

impl PendingLineEvents {
    fn shared() -> Self {
        Self {
            events: Some(LINE_EVENT_BUFFER.with(|buffer| std::mem::take(&mut *buffer.borrow_mut()))),
        }
    }

    fn push(&mut self, event: LineEvent) {
        if let Some(events) = &mut self.events {
            events.push(event);
        }
    }

    fn disable(&mut self) {
        if let Some(events) = self.events.take() {
            return_line_buffer(events);
        }
    }

    pub(super) fn finish(
        mut self,
        content: &str,
        line_offsets: &[usize],
        front_matter_end: usize,
        code_blocks: &[(usize, usize)],
        flavor: MarkdownFlavor,
        original_input: bool,
    ) -> (super::ListItemMap, Vec<SourceEmphasisSpan>) {
        if original_input && let Some(mut events) = self.events.take() {
            let result = super::line_computation::collect_list_items_and_emphasis(
                content,
                line_offsets,
                front_matter_end,
                code_blocks,
                events.drain(..),
            );
            return_line_buffer(events);
            result
        } else {
            super::line_computation::detect_list_items_and_emphasis_with_pulldown(
                content,
                line_offsets,
                flavor,
                front_matter_end,
                code_blocks,
            )
        }
    }
}

impl Drop for PendingLineEvents {
    fn drop(&mut self) {
        self.disable();
    }
}

fn return_line_buffer(mut events: Vec<LineEvent>) {
    events.clear();
    // Keep normal per-worker capacity without retaining unusually large files.
    if events.capacity() <= 8192 {
        LINE_EVENT_BUFFER.with(|buffer| {
            let mut buffer = buffer.borrow_mut();
            if events.capacity() > buffer.capacity() {
                *buffer = events;
            }
        });
    }
}

/// Only link-related events survive the shared parse. In particular, ordinary
/// prose and code-block text are not retained or copied. Link-label events are
/// kept because their exact extents determine where the source label closes.
pub(super) struct PendingLinks<'a> {
    content: &'a str,
    events: Vec<(Event<'a>, Range<usize>)>,
    inline_links: Option<PendingInlineLinks>,
    broken_links: Vec<BrokenLinkInfo>,
}

/// Whether the event is content of a paragraph or one of the blocks that can
/// follow the definitions it opens with, and not a container or an end.
fn starts_content(event: &Event<'_>) -> bool {
    match event {
        Event::Start(tag) => matches!(tag, Tag::Paragraph | Tag::Table(_) | Tag::Heading { .. }),
        Event::End(_) => false,
        _ => true,
    }
}

impl<'a> ParsedDocument<'a> {
    pub(super) fn matches_source(&self, source: &str) -> bool {
        self.pending_links.content == source
    }

    pub(super) fn parse(content: &'a str, line_offsets: &[usize]) -> Self {
        native::parse(content).unwrap_or_else(|| Self::parse_pulldown(content, line_offsets))
    }

    fn parse_pulldown(content: &'a str, line_offsets: &[usize]) -> Self {
        Self::parse_pulldown_observing(content, line_offsets, |_, _| {})
    }

    /// The candidate's semantic guard and context facts use the same parser.
    /// Broken references return `None`, as in the independent guard reading.
    pub(crate) fn parse_for_guard(content: &'a str) -> (Self, crate::utils::list_fix_guard::Parsed<'a>) {
        #[cfg(test)]
        crate::utils::list_fix_guard::Parsed::record_parse();
        let mut guard = crate::utils::list_fix_guard::Parsed::empty(content);
        let offsets = guard.line_starts().to_vec();
        let document = Self::parse_pulldown_observing(content, &offsets, |event, range| {
            guard.push(event.clone(), range);
        });
        (document, guard)
    }

    pub(crate) fn parse_for_proof_guard(
        content: &'a str,
        flavor: MarkdownFlavor,
    ) -> (
        Self,
        crate::utils::list_fix_guard::Parsed<'a>,
        Option<crate::format_proof::events::ProofEvents>,
    ) {
        if flavor != MarkdownFlavor::Standard || content.contains(['\r', '\0', '\t']) {
            let (document, guard) = Self::parse_for_guard(content);
            return (document, guard, None);
        }
        #[cfg(test)]
        crate::utils::list_fix_guard::Parsed::record_parse();
        let mut guard = crate::utils::list_fix_guard::Parsed::empty(content);
        let offsets = guard.line_starts().to_vec();
        let mut proof_events = crate::format_proof::events::ProofEvents::new(content);
        let mut references = Vec::new();
        let document = Self::parse_pulldown_observing_refs(
            content,
            &offsets,
            |event, range| {
                guard.push(event.clone(), range);
                proof_events.push(event, range);
            },
            Some(&mut references),
        );
        proof_events.references = references;
        (document, guard, Some(proof_events))
    }

    fn parse_pulldown_observing(
        content: &'a str,
        line_offsets: &[usize],
        observe: impl FnMut(&Event<'a>, &Range<usize>),
    ) -> Self {
        Self::parse_pulldown_observing_refs(content, line_offsets, observe, None)
    }

    pub(crate) fn parse_for_format_proof(
        content: &'a str,
        line_offsets: &[usize],
        observe: impl FnMut(&Event<'a>, &Range<usize>),
    ) -> (Self, Vec<Range<usize>>) {
        let mut references = Vec::new();
        let parsed = Self::parse_pulldown_observing_refs(content, line_offsets, observe, Some(&mut references));
        (parsed, references)
    }

    fn parse_pulldown_observing_refs(
        content: &'a str,
        line_offsets: &[usize],
        mut observe: impl FnMut(&Event<'a>, &Range<usize>),
        references: Option<&mut Vec<Range<usize>>>,
    ) -> Self {
        let options = rumdl_parser_options_for(content);
        let mut events = Vec::new();

        let mut parser_blocks = blocks::BlockFacts::default();
        let mut broken_links = Vec::new();
        let mut link_depth = 0usize;
        let mut pending_lines = PendingLineEvents::shared();
        let mut definition_list = false;

        let mut has_math = false;
        let mut text_start_lines = (content.contains("]:") && content.contains('|')).then(Vec::new);
        let parser = Parser::new_with_broken_link_callback(
            content,
            options,
            Some(|link: BrokenLink<'_>| {
                broken_links.push(BrokenLinkInfo {
                    reference: link.reference.to_string(),
                    span: link.span.start
                        ..link_parser::extend_collapsed_byte_end(content, link.link_type, link.span.end),
                    link_type: link.link_type,
                });
                None
            }),
        );

        if let Some(references) = references {
            references.extend(
                parser
                    .reference_definitions()
                    .iter()
                    .map(|(_, definition)| definition.span.clone()),
            );
        }
        let mut parser = parser.into_offset_iter().inspect(|(event, range)| {
            observe(event, range);
            has_math |= matches!(event, Event::InlineMath(_) | Event::DisplayMath(_));

            parser_blocks.push(event, range);
            if let Some(lines) = text_start_lines.as_mut()
                && starts_content(event)
            {
                let line = line_offsets
                    .partition_point(|&offset| offset <= range.start)
                    .saturating_sub(1);
                if lines.last() != Some(&line) {
                    lines.push(line);
                }
            }
            definition_list |= matches!(event, Event::Start(Tag::DefinitionList));
            if let Some(event) = LineEvent::from_event(event, range) {
                pending_lines.push(event);
            }
            let opens_link = matches!(event, Event::Start(Tag::Link { .. }));
            let closes_link = matches!(event, Event::End(TagEnd::Link));
            if opens_link {
                link_depth += 1;
            }
            if link_depth > 0
                || matches!(
                    event,
                    Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) | Event::FootnoteReference(_)
                )
            {
                events.push((event.clone(), range.clone()));
            }
            if closes_link {
                link_depth = link_depth.saturating_sub(1);
            }
        });
        let structure = CodeBlockUtils::collect_code_blocks_and_spans(content, line_offsets, options, parser.by_ref());
        // Keep observed facts complete even if the structural collector stops early.
        for _ in parser {}
        if let Some(lines) = text_start_lines.as_mut() {
            lines.sort_unstable();
            lines.dedup();
        }
        if definition_list || !compatible_line_view(content, &structure) {
            pending_lines.disable();
        }
        Self {
            structure,

            has_math,

            parser_blocks: Some(parser_blocks),
            pending_lines,
            text_start_lines,
            pending_links: PendingLinks {
                content,
                events,
                inline_links: None,
                broken_links,
            },
        }
    }
}

/// The primary parser's additional extensions cannot change list/emphasis
/// facts when their trigger bytes are absent outside literal code. Definitions
/// are handled separately from their actual parser events. Smart punctuation
/// only changes text values, which line classification does not consume.
fn compatible_line_view(content: &str, structure: &ParseResult) -> bool {
    let mut blocks = structure.code_blocks.iter().peekable();
    let mut spans = structure.code_spans.iter().peekable();
    let mut at = 0;
    loop {
        let next = match (blocks.peek(), spans.peek()) {
            (Some(block), Some(span)) if block.0 <= span.0 => blocks.next(),
            (Some(_), Some(_)) => spans.next(),
            (Some(_), None) => blocks.next(),
            (None, Some(_)) => spans.next(),
            (None, None) => break,
        };
        if let Some(&(start, end)) = next {
            if start > at && extension_trigger(&content.as_bytes()[at..start]) {
                return false;
            }
            at = at.max(end);
        }
    }
    !extension_trigger(&content.as_bytes()[at..])
}

fn extension_trigger(bytes: &[u8]) -> bool {
    memchr::memchr3(b'$', b'^', b'~', bytes).is_some()
        || memchr::memchr2(b'{', b'}', bytes).is_some()
        || memchr::memmem::find(bytes, b"[[").is_some()
}

impl<'a> PendingLinks<'a> {
    /// MDX already supplies all resolved link facts. Retain the primary parser's
    /// unresolved references without materializing links and images we discard.
    pub(super) fn finish_mdx(self, mdx: &super::mdx::MdxContext, lines: &[LineInfo]) -> ParsedLinks<'a> {
        let (links, images) = mdx.links_and_images(self.content, lines);
        ParsedLinks {
            link_byte_ranges: links.iter().map(|link| (link.byte_offset, link.byte_end)).collect(),
            link_found_positions: links.iter().map(|link| link.byte_offset).collect(),
            image_found_positions: images.iter().map(|image| image.byte_offset).collect(),
            links,
            images,
            broken_links: self
                .broken_links
                .into_iter()
                .filter(|link| mdx.contains_text(link.span.start, link.span.end))
                .collect(),
            footnote_refs: mdx.footnote_refs(),
        }
    }

    pub(super) fn finish(
        self,
        lines: &[LineInfo],
        code_blocks: &[(usize, usize)],
        flavor: MarkdownFlavor,
        html_comment_ranges: &[ByteRange],
    ) -> ParsedLinks<'a> {
        if let Some(mut links) = self.inline_links {
            link_parser::collect_links_images(
                self.content,
                lines,
                code_blocks,
                flavor,
                html_comment_ranges,
                links.events(self.content),
                self.broken_links,
            )
        } else {
            link_parser::collect_links_images(
                self.content,
                lines,
                code_blocks,
                flavor,
                html_comment_ranges,
                self.events.into_iter(),
                self.broken_links,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn shared_line_facts_preserve_the_independent_reading() {
        for content in [
            "- **bold** and *emphasis*\n- [link](target)\n",
            "1. first\n   - nested\n2. second\n",
            "Term\n: *definition*\n",
            "$*math*$ and *outside*\n",
            "Literal `$*math*` and **outside**.\n",
            "[[note|*text*]] and *outside*\n",
            "# *heading* {#id}\n",
            "a^*super*^ and a~*sub*~\n",
            "A \"*quoted*\" word -- and *another*.\n",
            "**first\nsecond**\n",
            "~~~md\n$ *literal*\n~~~\n\n*outside*\n",
            "-\n\n- next\n",
            "- --\n\nplain\n",
        ] {
            let offsets = std::iter::once(0)
                .chain(content.match_indices('\n').map(|(i, _)| i + 1))
                .collect::<Vec<_>>();
            let parsed = ParsedDocument::parse(content, &offsets);
            let expected = super::super::line_computation::detect_list_items_and_emphasis_with_pulldown(
                content,
                &offsets,
                MarkdownFlavor::Standard,
                0,
                &parsed.structure.code_blocks,
            );
            let actual = parsed.pending_lines.finish(
                content,
                &offsets,
                0,
                &parsed.structure.code_blocks,
                MarkdownFlavor::Standard,
                true,
            );
            assert_eq!(actual.0, expected.0, "{content:?}");
            assert_eq!(format!("{:?}", actual.1), format!("{:?}", expected.1), "{content:?}");
        }
    }

    #[test]
    fn reused_storage_does_not_carry_facts_between_documents() {
        for _ in 0..10 {
            let first = LintContext::new(
                "- **bold** and [link](target) and ![image](image.png)\n",
                MarkdownFlavor::Standard,
                None,
            );
            assert!(first.lines[0].list_item.is_some());
            assert_eq!(first.emphasis_spans().len(), 1);
            assert_eq!(first.links().len(), 1);
            assert_eq!(first.images().len(), 1);
            let next = LintContext::new("plain prose\n", MarkdownFlavor::Standard, None);
            assert!(next.lines[0].list_item.is_none());
            assert!(next.emphasis_spans().is_empty());
            assert!(next.links().is_empty());
            assert!(next.images().is_empty());
        }
    }

    #[test]
    fn shared_line_facts_match_container_and_extension_combinations() {
        let fragments = [
            "plain",
            "**bold**",
            "_emphasis_",
            "`literal $x$ and [[note]]`",
            "[link](target)",
            "![image](target)",
            "# *heading* {#id}",
            "$*math*$",
            "[[note|*label*]]",
            "text^*super*^",
            "text~*sub*~",
            "~~*strike*~~",
            "Term\n: *definition*",
            "Term\n:touching *definition*",
            "---",
            "<div>\n*html*\n</div>",
            "```md\n*literal*\n```",
            "a | *b*\n- | -",
            "[^note]: *footnote*",
            "{#id}\n===",
        ];
        for prefix in ["", "> ", ">> ", "- ", "1. ", "   ", "\t"] {
            for first in fragments {
                for second in fragments {
                    let content = format!("{prefix}{first}\n{prefix}{second}\n");
                    let offsets = std::iter::once(0)
                        .chain(content.match_indices('\n').map(|(i, _)| i + 1))
                        .collect::<Vec<_>>();
                    let parsed = ParsedDocument::parse_pulldown(&content, &offsets);
                    let expected = super::super::line_computation::detect_list_items_and_emphasis_with_pulldown(
                        &content,
                        &offsets,
                        MarkdownFlavor::Standard,
                        0,
                        &parsed.structure.code_blocks,
                    );
                    let actual = parsed.pending_lines.finish(
                        &content,
                        &offsets,
                        0,
                        &parsed.structure.code_blocks,
                        MarkdownFlavor::Standard,
                        true,
                    );
                    assert_eq!(actual.0, expected.0, "{content:?}");
                    assert_eq!(format!("{:?}", actual.1), format!("{:?}", expected.1), "{content:?}");
                }
            }
        }
    }

    #[test]
    fn mdx_native_link_facts_preserve_primary_reference_handoff() {
        for source in [
            "[ok](/x) ![![nested](inner)](outer) [bad][missing]",
            "[full][ref] [ref][] [ref] ![alt][ref]\n\n[ref]: /target \"title\"",
            "<Card title=\"[fake][missing]\">\n{'[fake][missing]'}\n[visible][missing]\n</Card>",
            "```md\n![hidden](image) [fake][missing]\n```\n\n`[fake][missing]` [visible][missing]",
            "{/* [fake][missing] */}\n\n![visible](image) [visible][missing]",
            "Footnote[^n] and [broken][unknown].\n\n[^n]: body [link](url)",
            "---\ntitle: <Card>\n---\n\n![body](image)",
        ] {
            for newline in ["\n", "\r\n"] {
                let content = source.replace('\n', newline);
                let ctx = LintContext::new(&content, MarkdownFlavor::MDX, None);
                let mdx = super::super::mdx::MdxContext::parse(&content, &ctx.lines)
                    .unwrap_or_else(|| panic!("invalid MDX fixture: {content:?}"));
                let mut expected = ParsedDocument::parse(&content, &ctx.line_offsets).pending_links.finish(
                    &ctx.lines,
                    &ctx.code_blocks,
                    MarkdownFlavor::MDX,
                    &ctx.html_comment_ranges,
                );
                let (links, images) = mdx.links_and_images(&content, &ctx.lines);
                expected.link_byte_ranges = links.iter().map(|link| (link.byte_offset, link.byte_end)).collect();
                expected.link_found_positions = links.iter().map(|link| link.byte_offset).collect();
                expected.image_found_positions = images.iter().map(|image| image.byte_offset).collect();
                expected.links = links;
                expected.images = images;
                expected.footnote_refs = mdx.footnote_refs();
                expected
                    .broken_links
                    .retain(|link| mdx.contains_text(link.span.start, link.span.end));
                let actual = ParsedDocument::parse(&content, &ctx.line_offsets)
                    .pending_links
                    .finish_mdx(&mdx, &ctx.lines);
                assert_eq!(actual.link_byte_ranges, expected.link_byte_ranges, "{content:?}");
                assert_eq!(actual.link_found_positions, expected.link_found_positions);
                assert_eq!(actual.image_found_positions, expected.image_found_positions);
                // Debug includes ranges, decoded values, titles, and reference IDs.
                assert_eq!(
                    format!(
                        "{:?}",
                        (actual.links, actual.images, actual.broken_links, actual.footnote_refs)
                    ),
                    format!(
                        "{:?}",
                        (
                            expected.links,
                            expected.images,
                            expected.broken_links,
                            expected.footnote_refs
                        )
                    ),
                    "{content:?}",
                );
            }
        }
    }

    #[test]
    fn malformed_mdx_keeps_primary_image_fallback() {
        let content = "![visible](image.png)\n\n# <Open> #\n";
        let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
        assert!(super::super::mdx::MdxContext::parse(content, &ctx.lines).is_none());
        assert_eq!(ctx.images().len(), 1);
        assert_eq!(ctx.images()[0].alt_text, "visible");
        assert_eq!(ctx.images()[0].url, "image.png");
    }

    #[test]
    fn shared_parse_preserves_independent_link_facts() {
        let cases = [
            "",
            "普通の文 [café](https://example.com \"\") and ![é](image.png '')\r\n",
            "[**bold** and `](` and <i title=\"[\">x</i>](url)\n",
            "[![image][pic][] and *label*](url)\n\n[pic]: image.png \"title\"\n",
            "[full][ref] [collapsed][] [shortcut] [missing][]\n\n[ref]: /r\n[collapsed]: /c\n[shortcut]: /s\n",
            "> [a\n> b](url) and ![multi\n> line](image.png)\n",
            "[[note|*label* `code`]] ![[image.png|100]] <https://example.com> <a@example.com>\n",
            "Footnote[^n] and [broken][unknown].\n\n[^n]: body [link](url)\n",
            "```md\n[code](url) ![code](image) [unknown]\n```\n\n`[inline](url)` [real](url)\n",
            "<!-- [hidden](url) ![hidden](image) -->\n\n[visible](url)\n",
            "---\ntitle: '[metadata](url)'\n---\n\n[body](url)\n",
            "!!! note\n    [inside](url)\n\n--8<-- \"[snippet](file.md)\"\n",
            "{::nomarkdown}\n```\n[hidden](url)\n{:/nomarkdown}\n\n[after](url)\n",
            "Term\n: definition [inside](url)\n\nTerm\n:touching [label](url)\n",
            ":::rust\n[code](url)\n:::\n\n[after](url)\n",
            "<Component value={[1, 2]}>[child](url)</Component>\n",
        ];
        let flavors = crate::format_proof::ALL_FLAVORS;
        for content in cases {
            for flavor in flavors {
                let ctx = LintContext::new(content, flavor, None);
                let shared = ParsedDocument::parse(content, &ctx.line_offsets).pending_links.finish(
                    &ctx.lines,
                    &ctx.code_blocks,
                    flavor,
                    &ctx.html_comment_ranges,
                );
                let independent = link_parser::parse_links_images_pulldown(
                    content,
                    &ctx.lines,
                    &ctx.code_blocks,
                    flavor,
                    &ctx.html_comment_ranges,
                );
                assert_eq!(
                    shared.link_byte_ranges, independent.link_byte_ranges,
                    "{flavor}: {content}"
                );
                assert_eq!(shared.link_found_positions, independent.link_found_positions);
                assert_eq!(shared.image_found_positions, independent.image_found_positions);
                // The public facts do not implement PartialEq; Debug includes
                // their source ranges, decoded text, titles and reference IDs.
                assert_eq!(
                    format!(
                        "{:?}",
                        (shared.links, shared.images, shared.broken_links, shared.footnote_refs)
                    ),
                    format!(
                        "{:?}",
                        (
                            independent.links,
                            independent.images,
                            independent.broken_links,
                            independent.footnote_refs
                        )
                    ),
                    "{flavor}: {content}"
                );
            }
        }
    }

    #[test]
    fn does_not_retain_events_for_prose_or_code_without_links() {
        let content = "plain prose\n\n```md\n[code](url)\n```\n\n`[inline](url)`\n".repeat(1000);
        let line_offsets = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect::<Vec<_>>();
        let parsed = ParsedDocument::parse(&content, &line_offsets);
        assert_eq!(parsed.structure.code_blocks.len(), 1000);
        assert_eq!(parsed.structure.code_spans.len(), 1000);
        assert!(parsed.pending_links.events.is_empty());
        assert!(parsed.pending_links.broken_links.is_empty());
    }
}

#[cfg(test)]
mod block_fact_regressions {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]
        #[test]
        fn shared_block_facts_equal_independent_complete_parse(
            parts in prop::collection::vec(prop::sample::select(vec![
                "- item", "-", "1. list", "> - quote", "# Heading", "Title\n---", "---",
                "plain", "", "```", "<div markdown>", "</div>", "!!! note", "=== \"Tab\"",
                "{::comment}\n```\n{:/comment}", "export const x = 1;", "{/* mdx */}",
                "$$\nmath\n$$", "---\ntitle: Heading\n---", "{::comment}", "{:/comment}", "[id]: target", "[^note]: text", "é🦀", "`code`",
            ]), 0..12),
            ending in prop::sample::select(vec!["\n", "\r\n", "\r"])
        ) {
            let source = parts.join(ending);
            let offsets = super::super::line_computation::source_layout(&source).offsets;
            let parsed = ParsedDocument::parse_pulldown(&source, &offsets);
            prop_assert_eq!(parsed.parser_blocks.unwrap().events, blocks::BlockFacts::parse(&source).events);
            // Every flavor receives the same native source parse; independent
            // fallback and extension carving happen after these facts are read.
            for flavor in [crate::config::MarkdownFlavor::Standard, crate::config::MarkdownFlavor::MDX,
                crate::config::MarkdownFlavor::MkDocs, crate::config::MarkdownFlavor::Pandoc,
                crate::config::MarkdownFlavor::Quarto, crate::config::MarkdownFlavor::Obsidian,
                crate::config::MarkdownFlavor::Kramdown, crate::config::MarkdownFlavor::AzureDevOps,
                crate::config::MarkdownFlavor::MyST, crate::config::MarkdownFlavor::Hugo,
                crate::config::MarkdownFlavor::MDG, crate::config::MarkdownFlavor::GhAw] {
                let ctx = crate::lint_context::LintContext::new(&source, flavor, None);
                prop_assert_eq!(&ctx.parser_blocks().events, &blocks::BlockFacts::parse(&source).events);
            }
        }
    }
}
