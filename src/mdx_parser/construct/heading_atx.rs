//! Heading (atx) occurs in the [flow][] content type.
//!
//! ## Grammar
//!
//! Heading (atx) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! heading_atx ::= 1*6'#' [ 1*space_or_tab line [ 1*space_or_tab 1*'#' ] ] *space_or_tab
//! ```
//!
//! As this construct occurs in flow, like all flow constructs, it must be
//! followed by an eol (line ending) or eof (end of file).
//!
//! `CommonMark` introduced the requirement on whitespace existing after the
//! opening sequence and before text.
//! In older markdown versions, this was not required, and headings would form
//! without it.
//!
//! In markdown, it is also possible to create headings with a
//! [heading (setext)][heading_setext] construct.
//! The benefit of setext headings is that their text can include line endings,
//! and by extensions also hard breaks (e.g., with
//! [hard break (escape)][hard_break_escape]).
//! However, their limit is that they cannot form `<h3>` through `<h6>`
//! headings.
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
//! Headings in markdown relate to the `<h1>` through `<h6>` elements in HTML.
//! See [*§ 4.3.6 The `h1`, `h2`, `h3`, `h4`, `h5`, and `h6` elements* in the
//! HTML spec][html] for more info.
//!
//! ## Recommendation
//!
//! Always use heading (atx), never heading (setext).
//!
//! ## Tokens
//!
//! * [`HeadingAtx`][Name::HeadingAtx]
//! * [`HeadingAtxSequence`][Name::HeadingAtxSequence]
//! * [`HeadingAtxText`][Name::HeadingAtxText]
//! * [`SpaceOrTab`][Name::SpaceOrTab]
//!
//! ## References
//!
//! * [`heading-atx.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/heading-atx.js)
//! * [*§ 4.2 ATX headings* in `CommonMark`](https://spec.commonmark.org/0.31/#atx-headings)
//!
//! [flow]: crate::mdx_parser::construct::flow
//! [heading_setext]: crate::mdx_parser::construct::heading_setext
//! [hard_break_escape]: crate::mdx_parser::construct::hard_break_escape
//! [html]: https://html.spec.whatwg.org/multipage/sections.html#the-h1,-h2,-h3,-h4,-h5,-and-h6-elements
//! [wiki_setext]: https://en.wikipedia.org/wiki/Setext
//! [atx]: http://www.aaronsw.com/2002/atx/

use crate::mdx_parser::construct::partial_space_or_tab::{space_or_tab, space_or_tab_min_max};
use crate::mdx_parser::event::{Content, Event, Kind, Link, Name};
use crate::mdx_parser::resolve::Name as ResolveName;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::Subresult;
use crate::mdx_parser::tokenizer::{ByteSet, LINE_STOP, Tokenizer, with_bytes};
use crate::mdx_parser::util::constant::{HEADING_ATX_OPENING_FENCE_SIZE_MAX, TAB_SIZE};
use alloc::vec;

/// Bytes that data stops at.
const DATA_STOP: ByteSet = with_bytes(LINE_STOP, b" ");

/// Start of a heading (atx).
///
/// ```markdown
/// > | ## aa
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.parse_state.options.constructs.heading_atx {
        tokenizer.enter(Name::HeadingAtx);
        if matches!(tokenizer.current, Some(b'\t' | b' ')) {
            tokenizer.attempt(State::Next(StateName::HeadingAtxBefore), State::Nok);
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
            State::Retry(StateName::HeadingAtxBefore)
        }
    } else {
        State::Nok
    }
}

/// After optional whitespace, at `#`.
///
/// ```markdown
/// > | ## aa
///     ^
/// ```
pub fn before(tokenizer: &mut Tokenizer) -> State {
    if Some(b'#') == tokenizer.current {
        tokenizer.enter(Name::HeadingAtxSequence);
        State::Retry(StateName::HeadingAtxSequenceOpen)
    } else {
        State::Nok
    }
}

/// In opening sequence.
///
/// ```markdown
/// > | ## aa
///     ^
/// ```
pub fn sequence_open(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(b'#') && tokenizer.tokenize_state.size < HEADING_ATX_OPENING_FENCE_SIZE_MAX {
        tokenizer.tokenize_state.size += 1;
        tokenizer.consume();
        State::Next(StateName::HeadingAtxSequenceOpen)
    }
    // Always at least one `#`.
    else if matches!(tokenizer.current, None | Some(b'\t' | b'\n' | b' ')) {
        tokenizer.tokenize_state.size = 0;
        tokenizer.exit(Name::HeadingAtxSequence);
        State::Retry(StateName::HeadingAtxAtBreak)
    } else {
        tokenizer.tokenize_state.size = 0;
        State::Nok
    }
}

/// After something, before something else.
///
/// ```markdown
/// > | ## aa
///       ^
/// ```
pub fn at_break(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(Name::HeadingAtx);
            tokenizer.register_resolver(ResolveName::HeadingAtx);
            // Feel free to interrupt.
            tokenizer.interrupt = false;
            State::Ok
        }
        Some(b'\t' | b' ') => {
            tokenizer.attempt(State::Next(StateName::HeadingAtxAtBreak), State::Nok);
            State::Retry(space_or_tab(tokenizer))
        }
        Some(b'#') => {
            tokenizer.enter(Name::HeadingAtxSequence);
            State::Retry(StateName::HeadingAtxSequenceFurther)
        }
        Some(_) => {
            tokenizer.enter_link(
                Name::Data,
                Link {
                    previous: None,
                    next: None,
                    content: Content::Text,
                },
            );
            State::Retry(StateName::HeadingAtxData)
        }
    }
}

/// In further sequence (after whitespace).
///
/// Could be normal “visible” hashes in the heading or a final sequence.
///
/// ```markdown
/// > | ## aa ##
///           ^
/// ```
pub fn sequence_further(tokenizer: &mut Tokenizer) -> State {
    if let Some(b'#') = tokenizer.current {
        tokenizer.consume();
        State::Next(StateName::HeadingAtxSequenceFurther)
    } else {
        tokenizer.exit(Name::HeadingAtxSequence);
        State::Retry(StateName::HeadingAtxAtBreak)
    }
}

/// In text.
///
/// ```markdown
/// > | ## aa
///        ^
/// ```
pub fn data(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // Note: `#` for closing sequence must be preceded by whitespace, otherwise it’s just text.
        None | Some(b'\t' | b'\n' | b' ') => {
            tokenizer.exit(Name::Data);
            State::Retry(StateName::HeadingAtxAtBreak)
        }
        _ => {
            tokenizer.consume_run(&DATA_STOP);
            State::Next(StateName::HeadingAtxData)
        }
    }
}

/// Resolve heading (atx).
pub fn resolve(tokenizer: &mut Tokenizer) -> Option<Subresult> {
    let mut index = 0;
    let mut heading_inside = false;
    let mut data_start: Option<usize> = None;
    let mut data_end: Option<usize> = None;

    while index < tokenizer.events.len() {
        let event = &tokenizer.events[index];

        if event.name == Name::HeadingAtx {
            if event.kind == Kind::Enter {
                heading_inside = true;
            } else {
                if let Some(start) = data_start {
                    // If `start` is some, `end` is too.
                    let end = data_end.unwrap();

                    tokenizer.map.add(
                        start,
                        0,
                        vec![Event {
                            kind: Kind::Enter,
                            name: Name::HeadingAtxText,
                            point: tokenizer.events[start].point.clone(),
                            link: None,
                        }],
                    );

                    // Remove everything between the start and the end.
                    tokenizer.map.add(start + 1, end - start - 1, vec![]);

                    tokenizer.map.add(
                        end + 1,
                        0,
                        vec![Event {
                            kind: Kind::Exit,
                            name: Name::HeadingAtxText,
                            point: tokenizer.events[end].point.clone(),
                            link: None,
                        }],
                    );
                }

                heading_inside = false;
                data_start = None;
                data_end = None;
            }
        } else if heading_inside && event.name == Name::Data {
            if event.kind == Kind::Enter {
                if data_start.is_none() {
                    data_start = Some(index);
                }
            } else {
                data_end = Some(index);
            }
        }

        index += 1;
    }

    tokenizer.map.consume(&mut tokenizer.events);
    None
}
