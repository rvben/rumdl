//! The subset of parser observations consumed by immutable format proofs.
//!
//! A native list-safety check already reads its candidate with this parser.
//! Retain positions and small discriminants so an accepted candidate can use
//! the same no-edit proofs without another parse or retained prose strings.

use pulldown_cmark::{CodeBlockKind, CowStr, Event, HeadingLevel, LinkType, Tag, TagEnd};
use std::ops::Range;

pub(crate) struct ProofEvents {
    events: Vec<ProofEvent>,
    metadata_boundaries: bool,
    pub(crate) references: Vec<Range<usize>>,
}

struct ProofEvent {
    kind: Kind,
    range: Range<usize>,
}

#[derive(Clone, Copy)]
enum Kind {
    CodeBlock(bool),
    Code,
    Link {
        is_empty: bool,
        needs_destination_fix: bool,
    },
    Heading(HeadingLevel),
    Strong,
    List(Option<u64>),
    EndList,
    Item,
    Quote,
    EndQuote,
    HtmlBlock,
    EndHtmlBlock,
    InlineHtml,
    Rule,
    Table,
    Unsupported,
    Other,
}

impl ProofEvents {
    pub(crate) fn new(source: &str) -> Self {
        Self {
            events: Vec::new(),
            // Even an otherwise unused paragraph/text event can cross the
            // native metadata boundary. Preserve those ranges whenever the
            // initial proof could recognize metadata on the first line.
            metadata_boundaries: matches!(
                source.lines().next().unwrap_or("").trim_end(),
                "---" | "+++" | "{" | "- --" | "-- -"
            ),
            references: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, event: &Event<'_>, range: &Range<usize>) {
        let kind = match event {
            Event::Start(Tag::CodeBlock(kind)) => Kind::CodeBlock(matches!(kind, CodeBlockKind::Fenced(_))),
            Event::Code(_) => Kind::Code,
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => Kind::Link {
                is_empty: dest_url.is_empty(),
                needs_destination_fix: dest_url.trim().is_empty() || dest_url.trim() == "#",
            },
            Event::Start(Tag::Heading { level, .. }) => Kind::Heading(*level),
            Event::Start(Tag::Strong) => Kind::Strong,
            Event::Start(Tag::List(number)) => Kind::List(*number),
            Event::End(TagEnd::List(_)) => Kind::EndList,
            Event::Start(Tag::Item) => Kind::Item,
            Event::Start(Tag::BlockQuote(_)) => Kind::Quote,
            Event::End(TagEnd::BlockQuote(_)) => Kind::EndQuote,
            Event::Start(Tag::HtmlBlock) => Kind::HtmlBlock,
            Event::End(TagEnd::HtmlBlock) => Kind::EndHtmlBlock,
            Event::InlineHtml(_) => Kind::InlineHtml,
            Event::Rule => Kind::Rule,
            Event::Start(Tag::Table(_)) => Kind::Table,
            Event::Start(Tag::FootnoteDefinition(_) | Tag::DefinitionList | Tag::MetadataBlock(_))
            | Event::InlineMath(_)
            | Event::DisplayMath(_) => Kind::Unsupported,
            _ if self.metadata_boundaries => Kind::Other,
            _ => return,
        };
        self.events.push(ProofEvent {
            kind,
            range: range.clone(),
        });
    }

    /// Replay only facts the proof consumes. Empty payloads are deliberate:
    /// labels, prose, fence info and HTML text are read from the original source
    /// ranges; links retain the exact-empty and MD042 destination predicates. These events
    /// must never be supplied to native context construction or diagnostics.
    pub(crate) fn observe(&self, mut observer: impl FnMut(&Event<'static>, &Range<usize>)) {
        for saved in &self.events {
            let empty = || CowStr::Borrowed("");
            let event = match saved.kind {
                Kind::CodeBlock(true) => Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(empty()))),
                Kind::CodeBlock(false) => Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)),
                Kind::Code => Event::Code(empty()),
                Kind::Link {
                    is_empty,
                    needs_destination_fix,
                } => Event::Start(Tag::Link {
                    link_type: LinkType::Inline,
                    dest_url: CowStr::Borrowed(if is_empty {
                        ""
                    } else if needs_destination_fix {
                        "#"
                    } else {
                        "x"
                    }),
                    title: empty(),
                    id: empty(),
                }),
                Kind::Heading(level) => Event::Start(Tag::Heading {
                    level,
                    id: None,
                    classes: Vec::new(),
                    attrs: Vec::new(),
                }),
                Kind::Strong => Event::Start(Tag::Strong),
                Kind::List(number) => Event::Start(Tag::List(number)),
                Kind::EndList => Event::End(TagEnd::List(false)),
                Kind::Item => Event::Start(Tag::Item),
                Kind::Quote => Event::Start(Tag::BlockQuote(None)),
                Kind::EndQuote => Event::End(TagEnd::BlockQuote(None)),
                Kind::HtmlBlock => Event::Start(Tag::HtmlBlock),
                Kind::EndHtmlBlock => Event::End(TagEnd::HtmlBlock),
                Kind::InlineHtml => Event::InlineHtml(empty()),
                Kind::Rule => Event::Rule,
                Kind::Table => Event::Start(Tag::Table(Vec::new())),
                Kind::Unsupported => Event::InlineMath(empty()),
                Kind::Other => Event::Text(empty()),
            };
            observer(&event, &saved.range);
        }
    }
}
