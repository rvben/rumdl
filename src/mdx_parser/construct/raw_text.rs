//! Raw (text) occurs in the [text][] content type.
//! It forms code (text) and math (text).
//!
//! ## Grammar
//!
//! Raw (text) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! ; Restriction: the number of markers in the closing sequence must be equal
//! ; to the number of markers in the opening sequence.
//! raw_text ::= sequence 1*byte sequence
//!
//! ; Restriction: not preceded or followed by the same marker.
//! sequence ::= 1*'`' | 1*'$'
//! ```
//!
//! The above grammar shows that it is not possible to create empty raw (text).
//! It is possible to include the sequence marker (grave accent for code,
//! dollar for math) in raw (text), by wrapping it in bigger or smaller
//! sequences:
//!
//! ```markdown
//! Include more: `a``b` or include less: ``a`b``.
//! ```
//!
//! It is also possible to include just one marker:
//!
//! ```markdown
//! Include just one: `` ` ``.
//! ```
//!
//! Sequences are “gready”, in that they cannot be preceded or followed by
//! more markers.
//! To illustrate:
//!
//! ```markdown
//! Not code: ``x`.
//!
//! Not code: `x``.
//!
//! Escapes work, this is code: \``x`.
//!
//! Escapes work, this is code: `x`\`.
//! ```
//!
//! Yields:
//!
//! ```html
//! <p>Not code: ``x`.</p>
//! <p>Not code: `x``.</p>
//! <p>Escapes work, this is code: `<code>x</code>.</p>
//! <p>Escapes work, this is code: <code>x</code>`.</p>
//! ```
//!
//! That is because, when turning markdown into HTML, the first and last space,
//! if both exist and there is also a non-space in the code, are removed.
//! Line endings, at that stage, are considered as spaces.
//!
//! In markdown, it is possible to create code or math with the
//! [raw (flow)][raw_flow] (or [code (indented)][code_indented]) constructs
//! in the [flow][] content type.
//!
//! ## HTML
//!
//! Code (text) relates to the `<code>` element in HTML.
//! See [*§ 4.5.15 The `code` element*][html_code] in the HTML spec for more
//! info.
//!
//! Math (text) does not relate to HTML elements.
//! `MathML`, which is sort of like SVG but for math, exists but it doesn’t work
//! well and isn’t widely supported.
//! Instead, it is recommended to use client side JavaScript with something like
//! `KaTeX` or `MathJax` to process the math
//! For that, the math is compiled as a `<code>` element with two classes:
//! `language-math` and `math-inline`.
//! Client side JavaScript can look for these classes to process them further.
//!
//! When turning markdown into HTML, each line ending in raw (text) is turned
//! into a space.
//!
//! ## Recommendations
//!
//! When authoring markdown with math, keep in mind that math doesn’t work in
//! most places.
//! Notably, GitHub currently has a really weird crappy client-side regex-based
//! thing.
//! But on your own (math-heavy?) site it can be great!
//! You can set [`parse_options.math_text_single_dollar: false`][parse_options]
//! to improve this, as it prevents single dollars from being seen as math, and
//! thus prevents normal dollars in text from being seen as math.
//!
//! ## Tokens
//!
//! * [`CodeText`][Name::CodeText]
//! * [`CodeTextData`][Name::CodeTextData]
//! * [`CodeTextSequence`][Name::CodeTextSequence]
//! * [`MathText`][Name::MathText]
//! * [`MathTextData`][Name::MathTextData]
//! * [`MathTextSequence`][Name::MathTextSequence]
//! * [`LineEnding`][Name::LineEnding]
//!
//! ## References
//!
//! * [`code-text.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/code-text.js)
//! * [`micromark-extension-math`](https://github.com/micromark/micromark-extension-math)
//! * [*§ 6.1 Code spans* in `CommonMark`](https://spec.commonmark.org/0.31/#code-spans)
//!
//! > 👉 **Note**: math is not specified anywhere.
//!
//! [flow]: crate::mdx_parser::construct::flow
//! [text]: crate::mdx_parser::construct::text
//! [code_indented]: crate::mdx_parser::construct::code_indented
//! [raw_flow]: crate::mdx_parser::construct::raw_flow
//! [html_code]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-code-element
//! [parse_options]: crate::mdx_parser::ParseOptions

use crate::mdx_parser::event::Name;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::{ByteSet, LINE_STOP, RawTextRuns, Tokenizer, with_bytes};

/// Bytes that data in code (text) stops at.
const CODE_STOP: ByteSet = with_bytes(LINE_STOP, b"`");

/// Bytes that data in math (text) stops at.
const MATH_STOP: ByteSet = with_bytes(LINE_STOP, b"$");
use alloc::boxed::Box;

/// Start of raw (text).
///
/// ```markdown
/// > | `a`
///     ^
/// > | \`a`
///      ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    // Code (text):
    if ((tokenizer.parse_state.options.constructs.code_text && tokenizer.current == Some(b'`'))
        // Math (text):
        || (tokenizer.parse_state.options.constructs.math_text && tokenizer.current == Some(b'$')))
        // Not the same marker (except when escaped).
        && (tokenizer.previous != tokenizer.current
            || (!tokenizer.events.is_empty()
                && tokenizer.events[tokenizer.events.len() - 1].name == Name::CharacterEscape))
    {
        let marker = tokenizer.current.unwrap();
        if marker == b'`' {
            tokenizer.tokenize_state.token_1 = Name::CodeText;
            tokenizer.tokenize_state.token_2 = Name::CodeTextSequence;
            tokenizer.tokenize_state.token_3 = Name::CodeTextData;
        } else {
            tokenizer.tokenize_state.token_1 = Name::MathText;
            tokenizer.tokenize_state.token_2 = Name::MathTextSequence;
            tokenizer.tokenize_state.token_3 = Name::MathTextData;
        }
        tokenizer.tokenize_state.marker = marker;
        tokenizer.enter(tokenizer.tokenize_state.token_1.clone());
        tokenizer.enter(tokenizer.tokenize_state.token_2.clone());
        State::Retry(StateName::RawTextSequenceOpen)
    } else {
        State::Nok
    }
}

/// In opening sequence.
///
/// ```markdown
/// > | `a`
///     ^
/// ```
pub fn sequence_open(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.tokenize_state.size += 1;
        tokenizer.consume();
        State::Next(StateName::RawTextSequenceOpen)
    }
    // Not enough markers in the sequence, or no closing sequence of that size.
    else if (tokenizer.tokenize_state.marker == b'$'
        && tokenizer.tokenize_state.size == 1
        && !tokenizer.parse_state.options.math_text_single_dollar)
        || !has_closing_run(tokenizer)
    {
        tokenizer.tokenize_state.marker = 0;
        tokenizer.tokenize_state.size = 0;
        tokenizer.tokenize_state.token_1 = Name::Data;
        tokenizer.tokenize_state.token_2 = Name::Data;
        tokenizer.tokenize_state.token_3 = Name::Data;
        State::Nok
    } else {
        let index = tokenizer.point.index;
        // Once complete, later scans would record older runs.
        if let Some(runs) = runs_mut(tokenizer) {
            if runs.complete_from.is_none() {
                runs.recording = Some(index);
            }
        }
        tokenizer.exit(tokenizer.tokenize_state.token_2.clone());
        State::Retry(StateName::RawTextBetween)
    }
}

/// Between something and something else.
///
/// ```markdown
/// > | `a`
///      ^^
/// ```
pub fn between(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None => {
            let runs = runs_mut(tokenizer).get_or_insert_with(Box::default);
            if let Some(from) = runs.recording.take() {
                runs.complete_from = Some(from);
            }
            tokenizer.tokenize_state.marker = 0;
            tokenizer.tokenize_state.size = 0;
            tokenizer.tokenize_state.token_1 = Name::Data;
            tokenizer.tokenize_state.token_2 = Name::Data;
            tokenizer.tokenize_state.token_3 = Name::Data;
            State::Nok
        }
        Some(b'\n') => {
            tokenizer.enter(Name::LineEnding);
            tokenizer.consume();
            tokenizer.exit(Name::LineEnding);
            State::Next(StateName::RawTextBetween)
        }
        _ => {
            if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
                tokenizer.enter(tokenizer.tokenize_state.token_2.clone());
                State::Retry(StateName::RawTextSequenceClose)
            } else {
                tokenizer.enter(tokenizer.tokenize_state.token_3.clone());
                State::Retry(StateName::RawTextData)
            }
        }
    }
}

/// In data.
///
/// ```markdown
/// > | `a`
///      ^
/// ```
pub fn data(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, None | Some(b'\n')) || tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.exit(tokenizer.tokenize_state.token_3.clone());
        State::Retry(StateName::RawTextBetween)
    } else {
        tokenizer.consume_run(if tokenizer.tokenize_state.marker == b'$' {
            &MATH_STOP
        } else {
            &CODE_STOP
        });
        State::Next(StateName::RawTextData)
    }
}

/// In closing sequence.
///
/// ```markdown
/// > | `a`
///       ^
/// ```
pub fn sequence_close(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.tokenize_state.size_b += 1;
        tokenizer.consume();
        State::Next(StateName::RawTextSequenceClose)
    } else {
        tokenizer.exit(tokenizer.tokenize_state.token_2.clone());
        if tokenizer.tokenize_state.size == tokenizer.tokenize_state.size_b {
            if let Some(runs) = runs_mut(tokenizer) {
                runs.recording = None;
            }
            tokenizer.exit(tokenizer.tokenize_state.token_1.clone());
            tokenizer.tokenize_state.marker = 0;
            tokenizer.tokenize_state.size = 0;
            tokenizer.tokenize_state.size_b = 0;
            tokenizer.tokenize_state.token_1 = Name::Data;
            tokenizer.tokenize_state.token_2 = Name::Data;
            tokenizer.tokenize_state.token_3 = Name::Data;
            State::Ok
        } else {
            // More or less accents: mark as data.
            let size = tokenizer.tokenize_state.size_b;
            let start = tokenizer.point.index - size;
            if let Some(runs) = runs_mut(tokenizer) {
                if runs.recording.is_some() {
                    match runs.last_start.binary_search_by_key(&size, |run| run.0) {
                        Ok(position) => runs.last_start[position].1 = start,
                        Err(position) => runs.last_start.insert(position, (size, start)),
                    }
                }
            }
            let len = tokenizer.events.len();
            tokenizer.events[len - 2].name = tokenizer.tokenize_state.token_3.clone();
            tokenizer.events[len - 1].name = tokenizer.tokenize_state.token_3.clone();
            tokenizer.tokenize_state.size_b = 0;
            State::Retry(StateName::RawTextBetween)
        }
    }
}

/// Runs of the current marker.
fn runs_mut<'a>(tokenizer: &'a mut Tokenizer) -> &'a mut Option<Box<RawTextRuns>> {
    let index = usize::from(tokenizer.tokenize_state.marker == b'$');
    &mut tokenizer.tokenize_state.raw_text_runs[index]
}

/// Whether a closer of the opening size may follow, as far as the runs show.
fn has_closing_run(tokenizer: &mut Tokenizer) -> bool {
    let index = tokenizer.point.index;
    let size = tokenizer.tokenize_state.size;
    match runs_mut(tokenizer) {
        Some(runs) if runs.complete_from.map_or(false, |from| from <= index) => runs
            .last_start
            .binary_search_by_key(&size, |run| run.0)
            .map_or(false, |position| runs.last_start[position].1 >= index),
        _ => true,
    }
}
