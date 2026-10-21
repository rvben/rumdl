//! Thematic break occurs in the [flow][] content type.
//!
//! ## Grammar
//!
//! Thematic break forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! ; Restriction: all markers must be identical.
//! ; Restriction: at least 3 markers must be used.
//! thematic_break ::= *space_or_tab 1*(1*marker *space_or_tab)
//!
//! marker ::= '*' | '-' | '_'
//! ```
//!
//! As this construct occurs in flow, like all flow constructs, it must be
//! followed by an eol (line ending) or eof (end of file).
//!
//! ## HTML
//!
//! Thematic breaks in markdown typically relate to the HTML element `<hr>`.
//! See [*§ 4.4.2 The `hr` element* in the HTML spec][html] for more info.
//!
//! ## Recommendation
//!
//! It is recommended to use exactly three asterisks without whitespace when
//! writing markdown.
//! As using more than three markers has no effect other than wasting space,
//! it is recommended to use exactly three markers.
//! Thematic breaks formed with asterisks or dashes can interfere with
//! [list][list-item]s if there is whitespace between them: `* * *` and `- - -`.
//! For these reasons, it is recommend to not use spaces or tabs between the
//! markers.
//! Thematic breaks formed with dashes (without whitespace) can also form
//! [heading (setext)][heading_setext].
//! As dashes and underscores frequently occur in natural language and URLs, it
//! is recommended to use asterisks for thematic breaks to distinguish from
//! such use.
//! Because asterisks can be used to form the most markdown constructs, using
//! them has the added benefit of making it easier to gloss over markdown: you
//! can look for asterisks to find syntax while not worrying about other
//! characters.
//!
//! ## Tokens
//!
//! * [`ThematicBreak`][Name::ThematicBreak]
//! * [`ThematicBreakSequence`][Name::ThematicBreakSequence]
//!
//! ## References
//!
//! * [`thematic-break.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/thematic-break.js)
//! * [*§ 4.1 Thematic breaks* in `CommonMark`](https://spec.commonmark.org/0.31/#thematic-breaks)
//!
//! [flow]: crate::mdx_parser::construct::flow
//! [heading_setext]: crate::mdx_parser::construct::heading_setext
//! [list-item]: crate::mdx_parser::construct::list_item
//! [html]: https://html.spec.whatwg.org/multipage/grouping-content.html#the-hr-element

use crate::mdx_parser::construct::partial_space_or_tab::{space_or_tab, space_or_tab_min_max};
use crate::mdx_parser::event::Name;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::constant::{TAB_SIZE, THEMATIC_BREAK_MARKER_COUNT_MIN};

/// Start of thematic break.
///
/// ```markdown
/// > | ***
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.parse_state.options.constructs.thematic_break {
        tokenizer.enter(Name::ThematicBreak);

        if matches!(tokenizer.current, Some(b'\t' | b' ')) {
            tokenizer.attempt(State::Next(StateName::ThematicBreakBefore), State::Nok);
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
            State::Retry(StateName::ThematicBreakBefore)
        }
    } else {
        State::Nok
    }
}

/// After optional whitespace, at marker.
///
/// ```markdown
/// > | ***
///     ^
/// ```
pub fn before(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'*' | b'-' | b'_') => {
            tokenizer.tokenize_state.marker = tokenizer.current.unwrap();
            State::Retry(StateName::ThematicBreakAtBreak)
        }
        _ => State::Nok,
    }
}

/// After something, before something else.
///
/// ```markdown
/// > | ***
///     ^
/// ```
pub fn at_break(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.enter(Name::ThematicBreakSequence);
        State::Retry(StateName::ThematicBreakSequence)
    } else if tokenizer.tokenize_state.size >= THEMATIC_BREAK_MARKER_COUNT_MIN
        && matches!(tokenizer.current, None | Some(b'\n'))
    {
        tokenizer.tokenize_state.marker = 0;
        tokenizer.tokenize_state.size = 0;
        tokenizer.exit(Name::ThematicBreak);
        // Feel free to interrupt.
        tokenizer.interrupt = false;
        State::Ok
    } else {
        tokenizer.tokenize_state.marker = 0;
        tokenizer.tokenize_state.size = 0;
        State::Nok
    }
}

/// In sequence.
///
/// ```markdown
/// > | ***
///     ^
/// ```
pub fn sequence(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.consume();
        tokenizer.tokenize_state.size += 1;
        State::Next(StateName::ThematicBreakSequence)
    } else if matches!(tokenizer.current, Some(b'\t' | b' ')) {
        tokenizer.exit(Name::ThematicBreakSequence);
        tokenizer.attempt(State::Next(StateName::ThematicBreakAtBreak), State::Nok);
        State::Retry(space_or_tab(tokenizer))
    } else {
        tokenizer.exit(Name::ThematicBreakSequence);
        State::Retry(StateName::ThematicBreakAtBreak)
    }
}
