//! Data occurs in the [string][] and [text][] content types.
//!
//! It can include anything (except for line endings) and stops at certain
//! characters.
//!
//! [string]: crate::mdx_parser::construct::string
//! [text]: crate::mdx_parser::construct::text

use crate::mdx_parser::construct::text;
use crate::mdx_parser::event::{Kind, Name};
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::Subresult;
use crate::mdx_parser::tokenizer::Tokenizer;
use alloc::vec;

/// At beginning of data.
///
/// ```markdown
/// > | abc
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    // Make sure to eat the first `markers`.
    if let Some(byte) = tokenizer.current {
        if is_marker(tokenizer, byte) {
            tokenizer.enter(Name::Data);
            tokenizer.consume();
            return State::Next(StateName::DataInside);
        }
    }

    State::Retry(StateName::DataAtBreak)
}

/// Before something.
///
/// ```markdown
/// > | abc
///     ^
/// ```
pub fn at_break(tokenizer: &mut Tokenizer) -> State {
    if let Some(byte) = tokenizer.current {
        if !is_marker(tokenizer, byte) {
            if byte == b'\n' {
                tokenizer.enter(Name::LineEnding);
                tokenizer.consume();
                tokenizer.exit(Name::LineEnding);
                return State::Next(StateName::DataAtBreak);
            }
            tokenizer.enter(Name::Data);
            return State::Retry(StateName::DataInside);
        }
    }

    State::Ok
}

/// In data.
///
/// ```markdown
/// > | abc
///     ^^^
/// ```
pub fn inside(tokenizer: &mut Tokenizer) -> State {
    if let Some(byte) = tokenizer.current {
        if byte != b'\n' && !is_marker(tokenizer, byte) {
            tokenizer.consume_run(tokenizer.tokenize_state.markers);
            return State::Next(StateName::DataInside);
        }
    }

    tokenizer.exit(Name::Data);
    State::Retry(StateName::DataAtBreak)
}

/// Whether `byte` may start a construct here (text can start more than string).
fn is_marker(tokenizer: &Tokenizer, byte: u8) -> bool {
    tokenizer.tokenize_state.markers[usize::from(byte)] && text::may_start(tokenizer, byte)
}

/// Merge adjacent data events.
pub fn resolve(tokenizer: &mut Tokenizer) -> Option<Subresult> {
    let mut index = 0;

    // Loop through events and merge adjacent data events.
    while index < tokenizer.events.len() {
        let event = &tokenizer.events[index];

        if event.kind == Kind::Enter && event.name == Name::Data {
            // Move to exit.
            index += 1;

            let mut exit_index = index;

            // Find the farthest `data` event exit event.
            while exit_index + 1 < tokenizer.events.len() && tokenizer.events[exit_index + 1].name == Name::Data {
                exit_index += 2;
            }

            if exit_index > index {
                tokenizer.map.add(index, exit_index - index, vec![]);
                // Change positional info.
                tokenizer.events[index].point = tokenizer.events[exit_index].point.clone();
                // Move to the end.
                index = exit_index;
            }
        }

        index += 1;
    }

    tokenizer.map.consume(&mut tokenizer.events);
    None
}
