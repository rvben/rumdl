//! Trailing whitespace occurs in [string][] and [text][].
//!
//! ## Grammar
//!
//! Trailing whitespace forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! ; Restriction: the start and end here count as an eol in the case of `text`.
//! ; Restriction: only the whitespace after the eol in the case of `string`.
//! whitespace ::= *space_or_tab eol *space_or_tab
//! ```
//!
//! In text content, it occurs around line endings and at the start or end of
//! the whole.
//! In string content, it occurs after line endings.
//!
//! Normally this whitespace is ignored.
//! In the case of text content, whitespace before a line ending that
//! consistents solely of spaces, at least 2, forms a hard break (trailing).
//!
//! The minimum number of those spaces is defined in
//! [`HARD_BREAK_PREFIX_SIZE_MIN`][].
//!
//! It is also possible to create a hard break with a similar construct: a
//! [hard break (escape)][hard_break_escape] is a backslash followed
//! by a line ending.
//! That construct is recommended because it is similar to a
//! [character escape][character_escape] and similar to how line endings can be
//! “escaped” in other languages.
//! Trailing spaces are typically invisible in editors, or even automatically
//! removed, making hard break (trailing) hard to use.
//!
//! ## HTML
//!
//! Hard breaks in markdown relate to the HTML element `<br>`.
//! See [*§ 4.5.27 The `br` element* in the HTML spec][html] for more info.
//!
//! ## Recommendation
//!
//! Do not use trailing whitespace.
//! It is never needed when using [hard break (escape)][hard_break_escape]
//! to create hard breaks.
//!
//! ## Tokens
//!
//! * [`HardBreakTrailing`][Name::HardBreakTrailing]
//! * [`SpaceOrTab`][Name::SpaceOrTab]
//!
//! ## References
//!
//! * [`initialize/text.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark/dev/lib/initialize/text.js)
//! * [*§ 6.7 Hard line breaks* in `CommonMark`](https://spec.commonmark.org/0.31/#hard-line-breaks)
//!
//! [string]: crate::mdx_parser::construct::string
//! [text]: crate::mdx_parser::construct::text
//! [hard_break_escape]: crate::mdx_parser::construct::hard_break_escape
//! [character_escape]: crate::mdx_parser::construct::character_escape
//! [hard_break_prefix_size_min]: crate::mdx_parser::util::constant::HARD_BREAK_PREFIX_SIZE_MIN
//! [html]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-br-element

use crate::mdx_parser::event::{Event, Kind, Name};
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::{
    constant::HARD_BREAK_PREFIX_SIZE_MIN,
    slice::{Position, Slice},
};
use alloc::vec;

/// Resolve whitespace.
pub fn resolve_whitespace(tokenizer: &mut Tokenizer, hard_break: bool, trim_whole: bool) {
    let mut index = 0;

    while index < tokenizer.events.len() {
        let event = &tokenizer.events[index];

        // Linked data is string content: a title, label, or destination.
        if event.kind == Kind::Exit && event.name == Name::Data && tokenizer.events[index - 1].link.is_none() {
            let trim_start =
                (trim_whole && index == 1) || (index > 1 && tokenizer.events[index - 2].name == Name::LineEnding);
            // String keeps whitespace before line endings.
            let trim_end = trim_whole
                && (index == tokenizer.events.len() - 1
                    || (index + 1 < tokenizer.events.len() && tokenizer.events[index + 1].name == Name::LineEnding));

            trim_data(tokenizer, index, trim_start, trim_end, hard_break);
        }

        index += 1;
    }

    tokenizer.map.consume(&mut tokenizer.events);
}

/// Trim a [`Data`][Name::Data] event.
fn trim_data(tokenizer: &mut Tokenizer, exit_index: usize, trim_start: bool, trim_end: bool, hard_break: bool) {
    let mut slice = Slice::from_position(
        tokenizer.parse_state.bytes,
        &Position::from_exit_event(&tokenizer.events, exit_index),
    );

    if trim_end {
        let mut index = slice.bytes.len();
        let mut spaces_only = slice.after == 0;
        while index > 0 {
            match slice.bytes[index - 1] {
                b' ' => {}
                b'\t' => spaces_only = false,
                _ => break,
            }

            index -= 1;
        }

        let diff = slice.bytes.len() - index;
        let name = if hard_break
            && spaces_only
            && diff >= HARD_BREAK_PREFIX_SIZE_MIN
            && exit_index + 1 < tokenizer.events.len()
        {
            Name::HardBreakTrailing
        } else {
            Name::SpaceOrTab
        };

        // The whole data is whitespace.
        // We can be very fast: we only change the event names.
        if index == 0 {
            tokenizer.events[exit_index - 1].name = name.clone();
            tokenizer.events[exit_index].name = name;
            return;
        }

        if diff > 0 || slice.after > 0 {
            let exit_point = tokenizer.events[exit_index].point.clone();
            let mut enter_point = exit_point.clone();
            enter_point.index -= diff;
            enter_point.column -= diff;
            enter_point.vs = 0;

            tokenizer.map.add(
                exit_index + 1,
                0,
                vec![
                    Event {
                        kind: Kind::Enter,
                        name: name.clone(),
                        point: enter_point.clone(),
                        link: None,
                    },
                    Event {
                        kind: Kind::Exit,
                        name,
                        point: exit_point,
                        link: None,
                    },
                ],
            );

            tokenizer.events[exit_index].point = enter_point;
            slice.bytes = &slice.bytes[..index];
        }
    }

    if trim_start {
        let mut index = 0;
        while index < slice.bytes.len() {
            match slice.bytes[index] {
                b' ' | b'\t' => index += 1,
                _ => break,
            }
        }

        // The whole data is whitespace.
        // We can be very fast: we only change the event names.
        if index == slice.bytes.len() {
            tokenizer.events[exit_index - 1].name = Name::SpaceOrTab;
            tokenizer.events[exit_index].name = Name::SpaceOrTab;
            return;
        }

        if index > 0 || slice.before > 0 {
            let enter_point = tokenizer.events[exit_index - 1].point.clone();
            let mut exit_point = enter_point.clone();
            exit_point.index += index;
            exit_point.column += index;
            exit_point.vs = 0;

            tokenizer.map.add(
                exit_index - 1,
                0,
                vec![
                    Event {
                        kind: Kind::Enter,
                        name: Name::SpaceOrTab,
                        point: enter_point,
                        link: None,
                    },
                    Event {
                        kind: Kind::Exit,
                        name: Name::SpaceOrTab,
                        point: exit_point.clone(),
                        link: None,
                    },
                ],
            );

            tokenizer.events[exit_index - 1].point = exit_point;
        }
    }
}
