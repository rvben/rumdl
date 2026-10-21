//! Code (indented) occurs in the [flow][] content type.
//!
//! ## Grammar
//!
//! Code (indented) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! code_indented ::= filled_line *( eol *( blank_line eol ) filled_line )
//!
//! ; Restriction: at least one `line` byte must be `text`.
//! filled_line ::= 4(space_or_tab) *line
//! blank_line ::= *space_or_tab
//! ```
//!
//! As this construct occurs in flow, like all flow constructs, it must be
//! followed by an eol (line ending) or eof (end of file).
//!
//! In markdown, it is also possible to use [code (text)][raw_text] in the
//! [text][] content type.
//! It is also possible to create code with the [code (fenced)][raw_flow]
//! construct.
//!
//! ## HTML
//!
//! Code (indented) relates to both the `<pre>` and the `<code>` elements in
//! HTML.
//! See [*§ 4.4.3 The `pre` element*][html_pre] and the [*§ 4.5.15 The `code`
//! element*][html_code] in the HTML spec for more info.
//!
//! ## Recommendation
//!
//! It is recommended to use code (fenced) instead of code (indented).
//! Code (fenced) is more explicit, similar to code (text), and has support
//! for specifying the programming language.
//!
//! ## Tokens
//!
//! * [`CodeIndented`][Name::CodeIndented]
//! * [`CodeFlowChunk`][Name::CodeFlowChunk]
//! * [`LineEnding`][Name::LineEnding]
//! * [`SpaceOrTab`][Name::SpaceOrTab]
//!
//! ## References
//!
//! * [`code-indented.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/code-indented.js)
//! * [*§ 4.4 Indented code blocks* in `CommonMark`](https://spec.commonmark.org/0.31/#indented-code-blocks)
//!
//! [flow]: crate::mdx_parser::construct::flow
//! [text]: crate::mdx_parser::construct::text
//! [raw_flow]: crate::mdx_parser::construct::raw_flow
//! [raw_text]: crate::mdx_parser::construct::raw_text
//! [html_code]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-code-element
//! [html_pre]: https://html.spec.whatwg.org/multipage/grouping-content.html#the-pre-element

use crate::mdx_parser::construct::partial_space_or_tab::{space_or_tab, space_or_tab_min_max};
use crate::mdx_parser::event::Name;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::constant::TAB_SIZE;

/// Start of code (indented).
///
/// > **Parsing note**: it is not needed to check if this first line is a
/// > filled line (that it has a non-whitespace character), because blank lines
/// > are parsed already, so we never run into that.
///
/// ```markdown
/// > |     aaa
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    // Do not interrupt paragraphs.
    if !tokenizer.interrupt
        && tokenizer.parse_state.options.constructs.code_indented
        && matches!(tokenizer.current, Some(b'\t' | b' '))
    {
        tokenizer.enter(Name::CodeIndented);
        tokenizer.attempt(State::Next(StateName::CodeIndentedAtBreak), State::Nok);
        State::Retry(space_or_tab_min_max(tokenizer, TAB_SIZE, TAB_SIZE))
    } else {
        State::Nok
    }
}

/// At a break.
///
/// ```markdown
/// > |     aaa
///         ^  ^
/// ```
pub fn at_break(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None => State::Retry(StateName::CodeIndentedAfter),
        Some(b'\n') => {
            tokenizer.attempt(
                State::Next(StateName::CodeIndentedAtBreak),
                State::Next(StateName::CodeIndentedAfter),
            );
            State::Retry(StateName::CodeIndentedFurtherStart)
        }
        _ => {
            tokenizer.enter(Name::CodeFlowChunk);
            State::Retry(StateName::CodeIndentedInside)
        }
    }
}

/// In code content.
///
/// ```markdown
/// > |     aaa
///         ^^^^
/// ```
pub fn inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(Name::CodeFlowChunk);
            State::Retry(StateName::CodeIndentedAtBreak)
        }
        _ => {
            tokenizer.consume();
            State::Next(StateName::CodeIndentedInside)
        }
    }
}

/// After indented code.
///
/// ```markdown
/// > |     aaa
///            ^
/// ```
pub fn after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.exit(Name::CodeIndented);
    // Feel free to interrupt.
    tokenizer.interrupt = false;
    State::Ok
}

/// At eol, trying to parse another indent.
///
/// ```markdown
/// > |     aaa
///            ^
///   |     bbb
/// ```
pub fn further_start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.lazy || tokenizer.pierce {
        return State::Nok;
    }

    if tokenizer.current == Some(b'\n') {
        tokenizer.enter(Name::LineEnding);
        tokenizer.consume();
        tokenizer.exit(Name::LineEnding);
        State::Next(StateName::CodeIndentedFurtherStart)
    } else {
        tokenizer.attempt(State::Ok, State::Next(StateName::CodeIndentedFurtherBegin));
        State::Retry(space_or_tab_min_max(tokenizer, TAB_SIZE, TAB_SIZE))
    }
}

/// At the beginning of a line that is not indented enough.
///
/// ```markdown
///   |     aaa
/// > |   bbb
///     ^
/// ```
pub fn further_begin(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, Some(b'\t' | b' ')) {
        tokenizer.attempt(State::Next(StateName::CodeIndentedFurtherAfter), State::Nok);
        State::Retry(space_or_tab(tokenizer))
    } else {
        State::Nok
    }
}

/// After whitespace, not indented enough.
///
/// ```markdown
///   |     aaa
/// > |   bbb
///       ^
/// ```
pub fn further_after(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\n') => State::Retry(StateName::CodeIndentedFurtherStart),
        _ => State::Nok,
    }
}
