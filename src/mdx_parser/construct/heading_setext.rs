//! Heading (setext) occurs in the [flow][] content type.
//!
//! ## Grammar
//!
//! Heading (setext) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! heading_setext ::= paragraph eol *space_or_tab (1*'-' | 1*'=') *space_or_tab
//!
//! ; See the `paragraph` construct for the BNF of that part.
//! ```
//!
//! As this construct occurs in flow, like all flow constructs, it must be
//! followed by an eol (line ending) or eof (end of file).
//!
//! See [`paragraph`][paragraph] for grammar, notes, and recommendations on
//! that part.
//!
//! In markdown, it is also possible to create headings with a
//! [heading (atx)][heading_atx] construct.
//! The benefit of setext headings is that their text can include line endings,
//! and by extensions also hard breaks (e.g., with
//! [hard break (escape)][hard_break_escape]).
//! However, their limit is that they cannot form `<h3>` through `<h6>`
//! headings.
//!
//! [Thematic breaks][thematic_break] formed with dashes and without whitespace
//! could be interpreted as a heading (setext).
//! Which one forms depends on whether there is text directly in fron of the
//! sequence.
//!
//! > 🏛 **Background**: the word *setext* originates from a small markup
//! > language by Ian Feldman from 1991.
//! > See [*§ Setext* on Wikipedia][wiki_setext] for more info.
//! > The word *atx* originates from a tiny markup language by Aaron Swartz
//! > from 2002.
//! > See [*§ atx, the true structured text format* on `aaronsw.com`][atx] for
//! > more info.
//!
//! ## HTML
//!
//! Heading (setext) in markdown relates to the `<h1>` and `<h2>` elements in
//! HTML.
//! See [*§ 4.3.6 The `h1`, `h2`, `h3`, `h4`, `h5`, and `h6` elements* in the
//! HTML spec][html] for more info.
//!
//! ## Recommendation
//!
//! Always use heading (atx), never heading (setext).
//!
//! ## Tokens
//!
//! * [`HeadingSetext`][Name::HeadingSetext]
//! * [`HeadingSetextText`][Name::HeadingSetextText]
//! * [`HeadingSetextUnderline`][Name::HeadingSetextUnderline]
//! * [`HeadingSetextUnderlineSequence`][Name::HeadingSetextUnderlineSequence]
//!
//! ## References
//!
//! * [`setext-underline.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/setext-underline.js)
//! * [*§ 4.3 Setext headings* in `CommonMark`](https://spec.commonmark.org/0.31/#setext-headings)
//!
//! [flow]: crate::mdx_parser::construct::flow
//! [paragraph]: crate::mdx_parser::construct::paragraph
//! [heading_atx]: crate::mdx_parser::construct::heading_atx
//! [thematic_break]: crate::mdx_parser::construct::thematic_break
//! [hard_break_escape]: crate::mdx_parser::construct::hard_break_escape
//! [html]: https://html.spec.whatwg.org/multipage/sections.html#the-h1,-h2,-h3,-h4,-h5,-and-h6-elements
//! [wiki_setext]: https://en.wikipedia.org/wiki/Setext
//! [atx]: http://www.aaronsw.com/2002/atx/

use crate::mdx_parser::construct::partial_space_or_tab::{space_or_tab, space_or_tab_min_max};
use crate::mdx_parser::event::{Content, Event, Kind, Link, Name};
use crate::mdx_parser::resolve::Name as ResolveName;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::Subresult;
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::{
    constant::{TAB_SIZE, THEMATIC_BREAK_MARKER_COUNT_MIN},
    skip,
};
use alloc::vec;

/// At start of heading (setext) underline.
///
/// ```markdown
///   | aa
/// > | ==
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.parse_state.options.constructs.heading_setext
        && !tokenizer.lazy
        && !tokenizer.pierce
        // Require a paragraph before.
        && (!tokenizer.events.is_empty()
            && matches!(tokenizer.events[skip::opt_back(
                &tokenizer.events,
                tokenizer.events.len() - 1,
                &[Name::LineEnding, Name::SpaceOrTab],
            )]
            .name, Name::Content | Name::HeadingSetextUnderline))
    {
        tokenizer.enter(Name::HeadingSetextUnderline);

        if matches!(tokenizer.current, Some(b'\t' | b' ')) {
            tokenizer.attempt(State::Next(StateName::HeadingSetextBefore), State::Nok);
            State::Retry(space_or_tab_min_max(
                tokenizer,
                0,
                if tokenizer.parse_state.options.constructs.code_indented {
                    TAB_SIZE - 1
                } else {
                    usize::MAX
                },
            ))
        } else {
            State::Retry(StateName::HeadingSetextBefore)
        }
    } else {
        State::Nok
    }
}

/// After optional whitespace, at `-` or `=`.
///
/// ```markdown
///   | aa
/// > | ==
///     ^
/// ```
pub fn before(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'-' | b'=') => {
            tokenizer.tokenize_state.marker = tokenizer.current.unwrap();
            tokenizer.enter(Name::HeadingSetextUnderlineSequence);
            State::Retry(StateName::HeadingSetextInside)
        }
        _ => State::Nok,
    }
}

/// In sequence.
///
/// ```markdown
///   | aa
/// > | ==
///     ^
/// ```
pub fn inside(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.consume();
        State::Next(StateName::HeadingSetextInside)
    } else {
        tokenizer.tokenize_state.marker = 0;
        tokenizer.exit(Name::HeadingSetextUnderlineSequence);

        if matches!(tokenizer.current, Some(b'\t' | b' ')) {
            tokenizer.attempt(State::Next(StateName::HeadingSetextAfter), State::Nok);
            State::Retry(space_or_tab(tokenizer))
        } else {
            State::Retry(StateName::HeadingSetextAfter)
        }
    }
}

/// After sequence, after optional whitespace.
///
/// ```markdown
///   | aa
/// > | ==
///       ^
/// ```
pub fn after(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            // Feel free to interrupt.
            tokenizer.interrupt = false;
            tokenizer.register_resolver(ResolveName::HeadingSetext);
            tokenizer.exit(Name::HeadingSetextUnderline);
            State::Ok
        }
        _ => State::Nok,
    }
}

/// Resolve heading (setext).
pub fn resolve(tokenizer: &mut Tokenizer) -> Option<Subresult> {
    let mut enter = skip::to(&tokenizer.events, 0, &[Name::HeadingSetextUnderline]);

    while enter < tokenizer.events.len() {
        let exit = skip::to(&tokenizer.events, enter + 1, &[Name::HeadingSetextUnderline]);

        // Find paragraph before
        let paragraph_exit_before = skip::opt_back(
            &tokenizer.events,
            enter - 1,
            &[Name::SpaceOrTab, Name::LineEnding, Name::BlockQuotePrefix],
        );

        // There’s a paragraph before: this is a setext heading.
        if tokenizer.events[paragraph_exit_before].name == Name::Paragraph {
            let paragraph_enter = skip::to_back(&tokenizer.events, paragraph_exit_before - 1, &[Name::Paragraph]);

            // Change types of Enter:Paragraph, Exit:Paragraph.
            tokenizer.events[paragraph_enter].name = Name::HeadingSetextText;
            tokenizer.events[paragraph_exit_before].name = Name::HeadingSetextText;

            // Add Enter:HeadingSetext, Exit:HeadingSetext.
            let mut heading_enter = tokenizer.events[paragraph_enter].clone();
            heading_enter.name = Name::HeadingSetext;
            tokenizer.map.add(paragraph_enter, 0, vec![heading_enter]);
            let mut heading_exit = tokenizer.events[exit].clone();
            heading_exit.name = Name::HeadingSetext;
            tokenizer.map.add(exit + 1, 0, vec![heading_exit]);
        } else {
            let sequence = skip::to(&tokenizer.events, enter + 1, &[Name::HeadingSetextUnderlineSequence]);
            let start = tokenizer.events[sequence].point.index;
            let end = tokenizer.events[sequence + 1].point.index;

            if tokenizer.parse_state.options.constructs.thematic_break
                && tokenizer.parse_state.bytes[start] == b'-'
                && end - start >= THEMATIC_BREAK_MARKER_COUNT_MIN
            {
                // Not an underline: three or more dashes form a thematic break.
                tokenizer.events[enter].name = Name::ThematicBreak;
                tokenizer.events[exit].name = Name::ThematicBreak;
                tokenizer.events[sequence].name = Name::ThematicBreakSequence;
                tokenizer.events[sequence + 1].name = Name::ThematicBreakSequence;
            } else if exit + 3 < tokenizer.events.len()
                && tokenizer.events[exit + 1].name == Name::LineEnding
                && tokenizer.events[exit + 3].name == Name::Paragraph
            {
                // There’s a following paragraph, move this underline inside it.
                // Swap type, HeadingSetextUnderline:Enter -> Paragraph:Enter.
                tokenizer.events[enter].name = Name::Paragraph;
                // Swap type, LineEnding -> Data.
                tokenizer.events[exit + 1].name = Name::Data;
                tokenizer.events[exit + 2].name = Name::Data;
                // Move new data (was line ending) back to include whole line,
                // and link data together.
                tokenizer.events[exit + 1].point = tokenizer.events[enter].point.clone();
                tokenizer.events[exit + 1].link = Some(Link {
                    previous: None,
                    next: Some(exit + 4),
                    content: Content::Text,
                });
                tokenizer.events[exit + 4].link.as_mut().unwrap().previous = Some(exit + 1);
                // Remove *including* HeadingSetextUnderline:Exit, until the line ending.
                tokenizer.map.add(enter + 1, exit - enter, vec![]);
                // Remove old Paragraph:Enter, renamed so later underlines skip it.
                tokenizer.events[exit + 3].name = Name::Data;
                tokenizer.map.add(exit + 3, 1, vec![]);
            } else {
                // Swap type.
                tokenizer.events[enter].name = Name::Paragraph;
                tokenizer.events[exit].name = Name::Paragraph;
                // Replace what’s inside the underline (whitespace, sequence).
                tokenizer.map.add(
                    enter + 1,
                    exit - enter - 1,
                    vec![
                        Event {
                            name: Name::Data,
                            kind: Kind::Enter,
                            point: tokenizer.events[enter].point.clone(),
                            link: Some(Link {
                                previous: None,
                                next: None,
                                content: Content::Text,
                            }),
                        },
                        Event {
                            name: Name::Data,
                            kind: Kind::Exit,
                            point: tokenizer.events[exit].point.clone(),
                            link: None,
                        },
                    ],
                );
            }
        }

        enter = skip::to(&tokenizer.events, exit + 1, &[Name::HeadingSetextUnderline]);
    }

    tokenizer.map.consume(&mut tokenizer.events);
    None
}
