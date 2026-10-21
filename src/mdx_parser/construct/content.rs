//! Content occurs in the [flow][] content type.
//!
//! Content contains zero or more [definition][definition]s, followed by zero
//! or one [paragraph][].
//!
//! The constructs found in flow are:
//!
//! * [Definition][crate::mdx_parser::construct::definition]
//! * [Paragraph][crate::mdx_parser::construct::paragraph]
//!
//! ## Tokens
//!
//! * [`Content`][Name::Content]
//!
//! > 👉 **Note**: while parsing, [`Content`][Name::Content]
//! > is used, which is later compiled away.
//!
//! ## References
//!
//! * [`content.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/content.js)
//!
//! [flow]: crate::mdx_parser::construct::flow
//! [definition]: crate::mdx_parser::construct::definition
//! [paragraph]: crate::mdx_parser::construct::paragraph

use crate::mdx_parser::event::{Content, Kind, Link, Name};
use crate::mdx_parser::message;
use crate::mdx_parser::resolve::Name as ResolveName;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::{Subresult, subtokenize};
use crate::mdx_parser::tokenizer::Tokenizer;
use alloc::vec;

/// Before a content chunk.
///
/// ```markdown
/// > | abc
///     ^
/// ```
pub fn chunk_start(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => unreachable!("unexpected eol/eof"),
        _ => {
            tokenizer.enter_link(
                Name::Content,
                Link {
                    previous: None,
                    next: None,
                    content: Content::Content,
                },
            );
            State::Retry(StateName::ContentChunkInside)
        }
    }
}

/// In a content chunk.
///
/// ```markdown
/// > | abc
///     ^^^
/// ```
pub fn chunk_inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(Name::Content);
            tokenizer.register_resolver_before(ResolveName::Content);
            // You’d be interrupting.
            tokenizer.interrupt = true;
            State::Ok
        }
        _ => {
            tokenizer.consume_line_run();
            State::Next(StateName::ContentChunkInside)
        }
    }
}

/// Before a definition.
///
/// ```markdown
/// > | [a]: b
///     ^
/// ```
pub fn definition_before(tokenizer: &mut Tokenizer) -> State {
    tokenizer.attempt(
        State::Next(StateName::ContentDefinitionAfter),
        State::Next(StateName::ParagraphStart),
    );
    State::Retry(StateName::DefinitionStart)
}

/// After a definition.
///
/// ```markdown
/// > | [a]: b
///           ^
///   | c
/// ```
pub fn definition_after(tokenizer: &mut Tokenizer) -> State {
    debug_assert!(matches!(tokenizer.current, None | Some(b'\n')));
    if tokenizer.current.is_none() {
        State::Ok
    } else {
        tokenizer.enter(Name::LineEnding);
        tokenizer.consume();
        tokenizer.exit(Name::LineEnding);
        State::Next(StateName::ContentDefinitionBefore)
    }
}

/// Merge `Content` chunks, which currently span a single line, into actual
/// `Content`s that span multiple lines.
pub fn resolve(tokenizer: &mut Tokenizer) -> Result<Option<Subresult>, message::Message> {
    let mut index = 0;

    while index < tokenizer.events.len() {
        let event = &tokenizer.events[index];

        if event.kind == Kind::Enter && event.name == Name::Content {
            // Exit:Content
            let mut exit_index = index + 1;

            loop {
                let mut enter_index = exit_index + 1;

                if enter_index == tokenizer.events.len() || tokenizer.events[enter_index].name != Name::LineEnding {
                    break;
                }

                // Skip past line ending.
                enter_index += 2;

                // Skip past prefix.
                while enter_index < tokenizer.events.len() {
                    let event = &tokenizer.events[enter_index];

                    if event.name != Name::SpaceOrTab
                        && event.name != Name::BlockQuotePrefix
                        && event.name != Name::BlockQuoteMarker
                    {
                        break;
                    }

                    enter_index += 1;
                }

                if enter_index == tokenizer.events.len() || tokenizer.events[enter_index].name != Name::Content {
                    break;
                }

                // Set Exit:Content point to Exit:LineEnding.
                tokenizer.events[exit_index].point = tokenizer.events[exit_index + 2].point.clone();
                // Remove Enter:LineEnding, Exit:LineEnding.
                tokenizer.map.add(exit_index + 1, 2, vec![]);

                // Link Enter:Content to Enter:Content on this line and vice versa.
                tokenizer.events[exit_index - 1].link.as_mut().unwrap().next = Some(enter_index);
                tokenizer.events[enter_index].link.as_mut().unwrap().previous = Some(exit_index - 1);

                // Potential next start.
                exit_index = enter_index + 1;
            }

            // Move to `Exit:Content`.
            index = exit_index;
        }

        index += 1;
    }

    tokenizer.map.consume(&mut tokenizer.events);

    let result = subtokenize(&mut tokenizer.events, tokenizer.parse_state, Some(&Content::Content))?;

    Ok(Some(result))
}
