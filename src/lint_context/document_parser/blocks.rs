//! Source ranges for block events shared by structural rules. No source text
//! or inline events are retained. The primary parse and the fallback use the
//! same options and ignore unresolved links in the same way.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockKind {
    ItemStart,
    ItemEnd,
    QuoteStart,
    QuoteEnd,
    HeadingStart,
    Rule,
    Other,
}

impl BlockKind {
    pub(crate) fn of(event: &Event<'_>) -> Self {
        match event {
            Event::Start(Tag::Item) => Self::ItemStart,
            Event::End(TagEnd::Item) => Self::ItemEnd,
            Event::Start(Tag::BlockQuote(_)) => Self::QuoteStart,
            Event::End(TagEnd::BlockQuote(_)) => Self::QuoteEnd,
            Event::Start(Tag::Heading { .. }) => Self::HeadingStart,
            Event::Rule => Self::Rule,
            _ => Self::Other,
        }
    }
}

#[derive(Default)]
pub(crate) struct BlockFacts {
    // Preserve a single separator for consecutive uninteresting events: MD065
    // asks whether a rule is the event immediately after an item opens.
    pub(crate) events: Vec<(BlockKind, Range<usize>)>,
}

impl BlockFacts {
    pub(crate) fn push(&mut self, event: &Event<'_>, range: &Range<usize>) {
        let kind = BlockKind::of(event);
        if kind != BlockKind::Other || self.events.last().is_none_or(|(last, _)| *last != BlockKind::Other) {
            self.events
                .push((kind, if kind == BlockKind::Other { 0..0 } else { range.clone() }));
        }
    }

    pub(crate) fn parse(content: &str) -> Self {
        let mut facts = Self::default();
        for (event, range) in
            Parser::new_ext(content, crate::utils::parser_options::rumdl_parser_options_for(content)).into_offset_iter()
        {
            facts.push(&event, &range);
        }
        facts
    }
}
