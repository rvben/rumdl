//! MDX ESM occurs in the [flow][] content type.
//!
//! ## Grammar
//!
//! MDX expression (flow) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! mdx_esm ::= word *line *(eol *line)
//!
//! word ::= 'e' 'x' 'p' 'o' 'r' 't' | 'i' 'm' 'p' 'o' 'r' 't'
//! ```
//!
//! This construct must be followed by a blank line or eof (end of file).
//! It can include blank lines if [`MdxEsmParse`][crate::mdx_parser::MdxEsmParse] passed in
//! [`ParseOptions`][parse_options] allows it.
//!
//! ## Tokens
//!
//! * [`LineEnding`][Name::LineEnding]
//! * [`MdxEsm`][Name::MdxEsm]
//! * [`MdxEsmData`][Name::MdxEsmData]
//!
//! ## References
//!
//! * [`syntax.js` in `micromark-extension-mdxjs-esm`](https://github.com/micromark/micromark-extension-mdxjs-esm/blob/main/dev/lib/syntax.js)
//! * [`mdxjs.com`](https://mdxjs.com)
//!
//! [flow]: crate::mdx_parser::construct::flow
//! [parse_options]: crate::mdx_parser::ParseOptions

use crate::mdx_parser::MdxSignal;
use crate::mdx_parser::event::Name;
use crate::mdx_parser::message;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::location::Location;
use crate::mdx_parser::util::{
    mdx_collect::{charge_parse_budget, collect_new, collected, reset_collect},
    slice::Slice,
};
use alloc::boxed::Box;

/// Start of MDX ESM.
///
/// ```markdown
/// > | import a from 'b'
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    // If it’s turned on.
    if tokenizer.parse_state.options.constructs.mdx_esm
        // If there is a gnostic parser.
        && tokenizer.parse_state.options.mdx_esm_parse.is_some()
        // When not interrupting.
        && !tokenizer.interrupt
        // Only at the start of a line, not at whitespace or in a container.
        && tokenizer.point.column == 1
        && matches!(tokenizer.current, Some(b'e' | b'i'))
    {
        // Place where keyword starts.
        tokenizer.tokenize_state.start = tokenizer.point.index;
        tokenizer.enter(Name::MdxEsm);
        tokenizer.enter(Name::MdxEsmData);
        tokenizer.consume();
        State::Next(StateName::MdxEsmWord)
    } else {
        State::Nok
    }
}

/// In keyword.
///
/// ```markdown
/// > | import a from 'b'
///     ^^^^^^
/// ```
pub fn word(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, Some(b'a'..=b'z')) {
        tokenizer.consume();
        State::Next(StateName::MdxEsmWord)
    } else {
        let slice = Slice::from_indices(
            tokenizer.parse_state.bytes,
            tokenizer.tokenize_state.start,
            tokenizer.point.index,
        );

        if matches!(slice.as_str(), "export" | "import") && tokenizer.current == Some(b' ') {
            tokenizer.concrete = true;
            tokenizer.tokenize_state.start = tokenizer.events.len() - 1;
            reset_collect(tokenizer);
            tokenizer.consume();
            State::Next(StateName::MdxEsmInside)
        } else {
            tokenizer.tokenize_state.start = 0;
            State::Nok
        }
    }
}

/// In data.
///
/// ```markdown
/// > | import a from 'b'
///           ^
/// ```
pub fn inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(Name::MdxEsmData);
            State::Retry(StateName::MdxEsmLineStart)
        }
        _ => {
            tokenizer.consume();
            State::Next(StateName::MdxEsmInside)
        }
    }
}

/// At start of line.
///
/// ```markdown
///   | import a from 'b'
/// > | export {a}
///     ^
/// ```
pub fn line_start(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None => State::Retry(StateName::MdxEsmAtEnd),
        Some(b'\n') => {
            tokenizer.check(
                State::Next(StateName::MdxEsmAtEnd),
                State::Next(StateName::MdxEsmContinuationStart),
            );
            State::Retry(StateName::MdxEsmBlankLineBefore)
        }
        _ => {
            tokenizer.enter(Name::MdxEsmData);
            tokenizer.consume();
            State::Next(StateName::MdxEsmInside)
        }
    }
}

/// At start of line that continues.
///
/// ```markdown
///   | import a from 'b'
/// > | export {a}
///     ^
/// ```
pub fn continuation_start(tokenizer: &mut Tokenizer) -> State {
    tokenizer.enter(Name::LineEnding);
    tokenizer.consume();
    tokenizer.exit(Name::LineEnding);
    State::Next(StateName::MdxEsmLineStart)
}

/// At start of a potentially blank line.
///
/// ```markdown
///   | import a from 'b'
/// > | export {a}
///     ^
/// ```
pub fn blank_line_before(tokenizer: &mut Tokenizer) -> State {
    tokenizer.enter(Name::LineEnding);
    tokenizer.consume();
    tokenizer.exit(Name::LineEnding);
    State::Next(StateName::BlankLineStart)
}

/// At end of line (blank or eof).
///
/// ```markdown
/// > | import a from 'b'
///                      ^
/// ```
pub fn at_end(tokenizer: &mut Tokenizer) -> State {
    let result = parse_esm(tokenizer);

    // Done!.
    if matches!(result, State::Ok) {
        tokenizer.concrete = false;
        tokenizer.exit(Name::MdxEsm);
    }

    result
}

/// Parse ESM with a given function.
fn parse_esm(tokenizer: &mut Tokenizer) -> State {
    // We can `unwrap` because we don’t parse if this is `None`.
    let parse = tokenizer.parse_state.options.mdx_esm_parse.as_ref().unwrap();

    // Collect the body of the ESM and positional info for each run of it.
    collect_new(tokenizer, &[Name::MdxEsmData, Name::LineEnding]);
    // At the keyword.
    let place = &tokenizer.events[tokenizer.tokenize_state.start].point;
    let result = collected(tokenizer);
    if let Some(error) = charge_parse_budget(tokenizer, result.value.len(), place) {
        return error;
    }

    // Parse and handle what was signaled back.
    match parse(&result.value) {
        MdxSignal::Ok => State::Ok,
        MdxSignal::Error(message, relative, source, rule_id) => {
            let point = Location::new(tokenizer.parse_state.bytes)
                .relative_to_point(&result.stops, relative)
                .expect("expected non-empty string");
            State::error(message::Message {
                place: Some(Box::new(message::Place::Point(point))),
                reason: message,
                source,
                rule_id,
            })
        }
        MdxSignal::Eof(message, source, rule_id) => {
            if tokenizer.current.is_none() {
                State::error(message::Message {
                    place: Some(Box::new(message::Place::Point(tokenizer.point.to_unist()))),
                    reason: message,
                    source,
                    rule_id,
                })
            } else {
                tokenizer.tokenize_state.mdx_last_parse_error = Some((message, *source, *rule_id));
                State::Retry(StateName::MdxEsmContinuationStart)
            }
        }
    }
}
