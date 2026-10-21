//! Raw (flow) occurs in the [flow][] content type.
//! It forms code (fenced) and math (flow).
//!
//! ## Grammar
//!
//! Code (fenced) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! raw_flow ::= fence_open *( eol *byte ) [ eol fence_close ]
//!
//! ; Restriction: math (flow) does not support the `info` part.
//! fence_open ::= sequence [*space_or_tab info [1*space_or_tab meta]] *space_or_tab
//! ; Restriction: the number of markers in the closing fence sequence must be
//! ; equal to or greater than the number of markers in the opening fence
//! ; sequence.
//! ; Restriction: the marker in the closing fence sequence must match the
//! ; marker in the opening fence sequence
//! fence_close ::= sequence *space_or_tab
//! sequence ::= 3*'`' | 3*'~' | 2*'$'
//! ; Restriction: the marker cannot occur in `info` if it is the `$` or `` ` `` character.
//! info ::= 1*text
//! ; Restriction: the marker cannot occur in `meta` if it is the `$` or `` ` `` character.
//! meta ::= 1*text *(*space_or_tab 1*text)
//! ```
//!
//! As this construct occurs in flow, like all flow constructs, it must be
//! followed by an eol (line ending) or eof (end of file).
//!
//! The above grammar does not show how indentation (with `space_or_tab`) of
//! each line is handled.
//! To parse raw (flow), let `x` be the number of `space_or_tab` characters
//! before the opening fence sequence.
//! Each line of text is then allowed (not required) to be indented with up
//! to `x` spaces or tabs, which are then ignored as an indent instead of being
//! considered as part of the content.
//! This indent does not affect the closing fence.
//! It can be indented up to a separate 3 spaces or tabs.
//! A bigger indent makes it part of the content instead of a fence.
//!
//! The `info` and `meta` parts are interpreted as the [string][] content type.
//! That means that [character escapes][character_escape] and
//! [character references][character_reference] are allowed.
//! Math (flow) does not support `info`.
//!
//! The optional `meta` part is ignored: it is not used when parsing or
//! rendering.
//!
//! The optional `info` part is used and is expected to specify the programming
//! language that the content is in.
//! Which value it holds depends on what your syntax highlighter supports, if
//! one is used.
//!
//! In markdown, it is also possible to use [raw (text)][raw_text] in the
//! [text][] content type.
//! It is also possible to create code with the
//! [code (indented)][code_indented] construct.
//!
//! ## HTML
//!
//! Code (fenced) relates to both the `<pre>` and the `<code>` elements in
//! HTML.
//! See [*§ 4.4.3 The `pre` element*][html_pre] and the [*§ 4.5.15 The `code`
//! element*][html_code] in the HTML spec for more info.
//!
//! Math (flow) does not relate to HTML elements.
//! `MathML`, which is sort of like SVG but for math, exists but it doesn’t work
//! well and isn’t widely supported.
//! Instead, it is recommended to use client side JavaScript with something like
//! `KaTeX` or `MathJax` to process the math
//! For that, the math is compiled as a `<pre>`, and a `<code>` element with two
//! classes: `language-math` and `math-display`.
//! Client side JavaScript can look for these classes to process them further.
//!
//! The `info` is, when rendering to HTML, typically exposed as a class.
//! This behavior stems from the HTML spec ([*§ 4.5.15 The `code`
//! element*][html_code]).
//! For example:
//!
//! ```markdown
//! ~~~css
//! * { color: tomato }
//! ~~~
//! ```
//!
//! Yields:
//!
//! ```html
//! <pre><code class="language-css">* { color: tomato }
//! </code></pre>
//! ```
//!
//! ## Recommendation
//!
//! It is recommended to use code (fenced) instead of code (indented).
//! Code (fenced) is more explicit, similar to code (text), and has support
//! for specifying the programming language.
//!
//! When authoring markdown with math, keep in mind that math doesn’t work in
//! most places.
//! Notably, GitHub currently has a really weird crappy client-side regex-based
//! thing.
//! But on your own (math-heavy?) site it can be great!
//! You can use code (fenced) with an info string of `math` to improve this, as
//! that works in many places.
//!
//! ## Tokens
//!
//! * [`CodeFenced`][Name::CodeFenced]
//! * [`CodeFencedFence`][Name::CodeFencedFence]
//! * [`CodeFencedFenceInfo`][Name::CodeFencedFenceInfo]
//! * [`CodeFencedFenceMeta`][Name::CodeFencedFenceMeta]
//! * [`CodeFencedFenceSequence`][Name::CodeFencedFenceSequence]
//! * [`CodeFlowChunk`][Name::CodeFlowChunk]
//! * [`LineEnding`][Name::LineEnding]
//! * [`MathFlow`][Name::MathFlow]
//! * [`MathFlowFence`][Name::MathFlowFence]
//! * [`MathFlowFenceMeta`][Name::MathFlowFenceMeta]
//! * [`MathFlowFenceSequence`][Name::MathFlowFenceSequence]
//! * [`MathFlowChunk`][Name::MathFlowChunk]
//! * [`SpaceOrTab`][Name::SpaceOrTab]
//!
//! ## References
//!
//! * [`code-fenced.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/code-fenced.js)
//! * [`micromark-extension-math`](https://github.com/micromark/micromark-extension-math)
//! * [*§ 4.5 Fenced code blocks* in `CommonMark`](https://spec.commonmark.org/0.31/#fenced-code-blocks)
//!
//! > 👉 **Note**: math is not specified anywhere.
//!
//! [flow]: crate::mdx_parser::construct::flow
//! [string]: crate::mdx_parser::construct::string
//! [text]: crate::mdx_parser::construct::text
//! [character_escape]: crate::mdx_parser::construct::character_escape
//! [character_reference]: crate::mdx_parser::construct::character_reference
//! [code_indented]: crate::mdx_parser::construct::code_indented
//! [raw_text]: crate::mdx_parser::construct::raw_text
//! [html_code]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-code-element
//! [html_pre]: https://html.spec.whatwg.org/multipage/grouping-content.html#the-pre-element

use crate::mdx_parser::construct::partial_space_or_tab::{space_or_tab, space_or_tab_min_max};
use crate::mdx_parser::event::{Content, Link, Name};
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::{
    constant::{CODE_FENCED_SEQUENCE_SIZE_MIN, MATH_FLOW_SEQUENCE_SIZE_MIN, TAB_SIZE},
    slice::{Position, Slice},
};

/// Start of raw.
///
/// ```markdown
/// > | ~~~js
///     ^
///   | console.log(1)
///   | ~~~
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.parse_state.options.constructs.code_fenced || tokenizer.parse_state.options.constructs.math_flow {
        if matches!(tokenizer.current, Some(b'\t' | b' ')) {
            tokenizer.attempt(State::Next(StateName::RawFlowBeforeSequenceOpen), State::Nok);
            return State::Retry(space_or_tab_min_max(
                tokenizer,
                0,
                if tokenizer.parse_state.options.constructs.code_indented {
                    TAB_SIZE - 1
                } else {
                    usize::MAX
                },
            ));
        }

        if matches!(tokenizer.current, Some(b'$' | b'`' | b'~')) {
            return State::Retry(StateName::RawFlowBeforeSequenceOpen);
        }
    }

    State::Nok
}

/// In opening fence, after prefix, at sequence.
///
/// ```markdown
/// > | ~~~js
///     ^
///   | console.log(1)
///   | ~~~
/// ```
pub fn before_sequence_open(tokenizer: &mut Tokenizer) -> State {
    let tail = tokenizer.events.last();
    let mut prefix = 0;

    if let Some(event) = tail {
        if event.name == Name::SpaceOrTab {
            prefix = Slice::from_position(
                tokenizer.parse_state.bytes,
                &Position::from_exit_event(&tokenizer.events, tokenizer.events.len() - 1),
            )
            .len();
        }
    }

    // Code (fenced).
    if (tokenizer.parse_state.options.constructs.code_fenced
        && matches!(tokenizer.current, Some(b'`' | b'~')))
        // Math (flow).
        || (tokenizer.parse_state.options.constructs.math_flow && tokenizer.current == Some(b'$'))
    {
        tokenizer.tokenize_state.marker = tokenizer.current.unwrap();
        tokenizer.tokenize_state.size_c = prefix;
        if tokenizer.tokenize_state.marker == b'$' {
            tokenizer.tokenize_state.token_1 = Name::MathFlow;
            tokenizer.tokenize_state.token_2 = Name::MathFlowFence;
            tokenizer.tokenize_state.token_3 = Name::MathFlowFenceSequence;
            // Math (flow) does not support an `info` part: everything after the
            // opening sequence is the `meta` part.
            tokenizer.tokenize_state.token_5 = Name::MathFlowFenceMeta;
            tokenizer.tokenize_state.token_6 = Name::MathFlowChunk;
        } else {
            tokenizer.tokenize_state.token_1 = Name::CodeFenced;
            tokenizer.tokenize_state.token_2 = Name::CodeFencedFence;
            tokenizer.tokenize_state.token_3 = Name::CodeFencedFenceSequence;
            tokenizer.tokenize_state.token_4 = Name::CodeFencedFenceInfo;
            tokenizer.tokenize_state.token_5 = Name::CodeFencedFenceMeta;
            tokenizer.tokenize_state.token_6 = Name::CodeFlowChunk;
        }

        tokenizer.enter(tokenizer.tokenize_state.token_1.clone());
        tokenizer.enter(tokenizer.tokenize_state.token_2.clone());
        tokenizer.enter(tokenizer.tokenize_state.token_3.clone());
        State::Retry(StateName::RawFlowSequenceOpen)
    } else {
        State::Nok
    }
}

/// In opening fence sequence.
///
/// ```markdown
/// > | ~~~js
///      ^
///   | console.log(1)
///   | ~~~
/// ```
pub fn sequence_open(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.tokenize_state.size += 1;
        tokenizer.consume();
        State::Next(StateName::RawFlowSequenceOpen)
    } else if tokenizer.tokenize_state.size
        < (if tokenizer.tokenize_state.marker == b'$' {
            MATH_FLOW_SEQUENCE_SIZE_MIN
        } else {
            CODE_FENCED_SEQUENCE_SIZE_MIN
        })
    {
        tokenizer.tokenize_state.marker = 0;
        tokenizer.tokenize_state.size_c = 0;
        tokenizer.tokenize_state.size = 0;
        tokenizer.tokenize_state.token_1 = Name::Data;
        tokenizer.tokenize_state.token_2 = Name::Data;
        tokenizer.tokenize_state.token_3 = Name::Data;
        tokenizer.tokenize_state.token_4 = Name::Data;
        tokenizer.tokenize_state.token_5 = Name::Data;
        tokenizer.tokenize_state.token_6 = Name::Data;
        State::Nok
    } else {
        // Math (flow) does not support an `info` part: everything after the
        // opening sequence is the `meta` part.
        let next = if tokenizer.tokenize_state.marker == b'$' {
            StateName::RawFlowMetaBefore
        } else {
            StateName::RawFlowInfoBefore
        };

        if matches!(tokenizer.current, Some(b'\t' | b' ')) {
            tokenizer.exit(tokenizer.tokenize_state.token_3.clone());
            tokenizer.attempt(State::Next(next), State::Nok);
            State::Retry(space_or_tab(tokenizer))
        } else {
            tokenizer.exit(tokenizer.tokenize_state.token_3.clone());
            State::Retry(next)
        }
    }
}

/// In opening fence, after the sequence (and optional whitespace), before info.
///
/// ```markdown
/// > | ~~~js
///        ^
///   | console.log(1)
///   | ~~~
/// ```
pub fn info_before(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(tokenizer.tokenize_state.token_2.clone());
            // Do not form containers.
            tokenizer.concrete = true;
            tokenizer.check(
                State::Next(StateName::RawFlowAtNonLazyBreak),
                State::Next(StateName::RawFlowAfter),
            );
            State::Retry(StateName::NonLazyContinuationStart)
        }
        _ => {
            tokenizer.enter(tokenizer.tokenize_state.token_4.clone());
            tokenizer.enter_link(
                Name::Data,
                Link {
                    previous: None,
                    next: None,
                    content: Content::String,
                },
            );
            State::Retry(StateName::RawFlowInfo)
        }
    }
}

/// In info.
///
/// ```markdown
/// > | ~~~js
///        ^
///   | console.log(1)
///   | ~~~
/// ```
pub fn info(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(Name::Data);
            tokenizer.exit(tokenizer.tokenize_state.token_4.clone());
            State::Retry(StateName::RawFlowInfoBefore)
        }
        Some(b'\t' | b' ') => {
            tokenizer.exit(Name::Data);
            tokenizer.exit(tokenizer.tokenize_state.token_4.clone());
            tokenizer.attempt(State::Next(StateName::RawFlowMetaBefore), State::Nok);
            State::Retry(space_or_tab(tokenizer))
        }
        Some(byte) => {
            // This looks like code (text) / math (text).
            // Note: no reason to check for `~`, because 3 of them can‘t be
            // used as strikethrough in text.
            if tokenizer.tokenize_state.marker == byte && matches!(byte, b'$' | b'`') {
                tokenizer.concrete = false;
                tokenizer.tokenize_state.marker = 0;
                tokenizer.tokenize_state.size_c = 0;
                tokenizer.tokenize_state.size = 0;
                tokenizer.tokenize_state.token_1 = Name::Data;
                tokenizer.tokenize_state.token_2 = Name::Data;
                tokenizer.tokenize_state.token_3 = Name::Data;
                tokenizer.tokenize_state.token_4 = Name::Data;
                tokenizer.tokenize_state.token_5 = Name::Data;
                tokenizer.tokenize_state.token_6 = Name::Data;
                State::Nok
            } else {
                tokenizer.consume();
                State::Next(StateName::RawFlowInfo)
            }
        }
    }
}

/// In opening fence, after info and whitespace, before meta.
///
/// ```markdown
/// > | ~~~js eval
///           ^
///   | console.log(1)
///   | ~~~
/// ```
pub fn meta_before(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => State::Retry(StateName::RawFlowInfoBefore),
        _ => {
            tokenizer.enter(tokenizer.tokenize_state.token_5.clone());
            tokenizer.enter_link(
                Name::Data,
                Link {
                    previous: None,
                    next: None,
                    content: Content::String,
                },
            );
            State::Retry(StateName::RawFlowMeta)
        }
    }
}

/// In meta.
///
/// ```markdown
/// > | ~~~js eval
///           ^
///   | console.log(1)
///   | ~~~
/// ```
pub fn meta(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(Name::Data);
            tokenizer.exit(tokenizer.tokenize_state.token_5.clone());
            State::Retry(StateName::RawFlowInfoBefore)
        }
        Some(byte) => {
            // This looks like code (text) / math (text).
            // Note: no reason to check for `~`, because 3 of them can‘t be
            // used as strikethrough in text.
            if tokenizer.tokenize_state.marker == byte && matches!(byte, b'$' | b'`') {
                tokenizer.concrete = false;
                tokenizer.tokenize_state.marker = 0;
                tokenizer.tokenize_state.size_c = 0;
                tokenizer.tokenize_state.size = 0;
                tokenizer.tokenize_state.token_1 = Name::Data;
                tokenizer.tokenize_state.token_2 = Name::Data;
                tokenizer.tokenize_state.token_3 = Name::Data;
                tokenizer.tokenize_state.token_4 = Name::Data;
                tokenizer.tokenize_state.token_5 = Name::Data;
                tokenizer.tokenize_state.token_6 = Name::Data;
                State::Nok
            } else {
                tokenizer.consume();
                State::Next(StateName::RawFlowMeta)
            }
        }
    }
}

/// At eol/eof in raw, before a non-lazy closing fence or content.
///
/// ```markdown
/// > | ~~~js
///          ^
/// > | console.log(1)
///                   ^
///   | ~~~
/// ```
pub fn at_non_lazy_break(tokenizer: &mut Tokenizer) -> State {
    tokenizer.attempt(
        State::Next(StateName::RawFlowAfter),
        State::Next(StateName::RawFlowContentBefore),
    );
    tokenizer.enter(Name::LineEnding);
    tokenizer.consume();
    tokenizer.exit(Name::LineEnding);
    State::Next(StateName::RawFlowCloseStart)
}

/// Before closing fence, at optional whitespace.
///
/// ```markdown
///   | ~~~js
///   | console.log(1)
/// > | ~~~
///     ^
/// ```
pub fn close_start(tokenizer: &mut Tokenizer) -> State {
    tokenizer.enter(tokenizer.tokenize_state.token_2.clone());

    if matches!(tokenizer.current, Some(b'\t' | b' ')) {
        tokenizer.attempt(State::Next(StateName::RawFlowBeforeSequenceClose), State::Nok);

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
        State::Retry(StateName::RawFlowBeforeSequenceClose)
    }
}

/// In closing fence, after optional whitespace, at sequence.
///
/// ```markdown
///   | ~~~js
///   | console.log(1)
/// > | ~~~
///     ^
/// ```
pub fn before_sequence_close(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.enter(tokenizer.tokenize_state.token_3.clone());
        State::Retry(StateName::RawFlowSequenceClose)
    } else {
        State::Nok
    }
}

/// In closing fence sequence.
///
/// ```markdown
///   | ~~~js
///   | console.log(1)
/// > | ~~~
///     ^
/// ```
pub fn sequence_close(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.tokenize_state.size_b += 1;
        tokenizer.consume();
        State::Next(StateName::RawFlowSequenceClose)
    } else if tokenizer.tokenize_state.size_b >= tokenizer.tokenize_state.size {
        tokenizer.tokenize_state.size_b = 0;
        tokenizer.exit(tokenizer.tokenize_state.token_3.clone());

        if matches!(tokenizer.current, Some(b'\t' | b' ')) {
            tokenizer.attempt(State::Next(StateName::RawFlowAfterSequenceClose), State::Nok);
            State::Retry(space_or_tab(tokenizer))
        } else {
            State::Retry(StateName::RawFlowAfterSequenceClose)
        }
    } else {
        tokenizer.tokenize_state.size_b = 0;
        State::Nok
    }
}

/// After closing fence sequence, after optional whitespace.
///
/// ```markdown
///   | ~~~js
///   | console.log(1)
/// > | ~~~
///        ^
/// ```
pub fn sequence_close_after(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(tokenizer.tokenize_state.token_2.clone());
            State::Ok
        }
        _ => State::Nok,
    }
}

/// Before raw content, not a closing fence, at eol.
///
/// ```markdown
///   | ~~~js
/// > | console.log(1)
///                   ^
///   | ~~~
/// ```
pub fn content_before(tokenizer: &mut Tokenizer) -> State {
    tokenizer.enter(Name::LineEnding);
    tokenizer.consume();
    tokenizer.exit(Name::LineEnding);
    State::Next(StateName::RawFlowContentStart)
}

/// Before raw content, not a closing fence.
///
/// ```markdown
///   | ~~~js
/// > | console.log(1)
///     ^
///   | ~~~
/// ```
pub fn content_start(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, Some(b'\t' | b' ')) {
        tokenizer.attempt(State::Next(StateName::RawFlowBeforeContentChunk), State::Nok);
        State::Retry(space_or_tab_min_max(tokenizer, 0, tokenizer.tokenize_state.size_c))
    } else {
        State::Retry(StateName::RawFlowBeforeContentChunk)
    }
}

/// Before raw content, after optional prefix.
///
/// ```markdown
///   | ~~~js
/// > | console.log(1)
///     ^
///   | ~~~
/// ```
pub fn before_content_chunk(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.check(
                State::Next(StateName::RawFlowAtNonLazyBreak),
                State::Next(StateName::RawFlowAfter),
            );
            State::Retry(StateName::NonLazyContinuationStart)
        }
        _ => {
            tokenizer.enter(tokenizer.tokenize_state.token_6.clone());
            State::Retry(StateName::RawFlowContentChunk)
        }
    }
}

/// In raw content.
///
/// ```markdown
///   | ~~~js
/// > | console.log(1)
///     ^^^^^^^^^^^^^^
///   | ~~~
/// ```
pub fn content_chunk(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(tokenizer.tokenize_state.token_6.clone());
            State::Retry(StateName::RawFlowBeforeContentChunk)
        }
        _ => {
            tokenizer.consume_line_run();
            State::Next(StateName::RawFlowContentChunk)
        }
    }
}

/// After raw.
///
/// ```markdown
///   | ~~~js
///   | console.log(1)
/// > | ~~~
///        ^
/// ```
pub fn after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.exit(tokenizer.tokenize_state.token_1.clone());
    tokenizer.tokenize_state.marker = 0;
    tokenizer.tokenize_state.size_c = 0;
    tokenizer.tokenize_state.size = 0;
    tokenizer.tokenize_state.token_1 = Name::Data;
    tokenizer.tokenize_state.token_2 = Name::Data;
    tokenizer.tokenize_state.token_3 = Name::Data;
    tokenizer.tokenize_state.token_4 = Name::Data;
    tokenizer.tokenize_state.token_5 = Name::Data;
    tokenizer.tokenize_state.token_6 = Name::Data;
    // Feel free to interrupt.
    tokenizer.interrupt = false;
    // No longer concrete.
    tokenizer.concrete = false;
    State::Ok
}
