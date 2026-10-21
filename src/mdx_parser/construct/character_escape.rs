//! Character escapes occur in the [string][] and [text][] content types.
//!
//! ## Grammar
//!
//! Character escapes form with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! character_escape ::= '\\' ascii_punctuation
//! ```
//!
//! Like much of markdown, there are no “invalid” character escapes: just a
//! slash, or a slash followed by anything other than an ASCII punctuation
//! character, is just a slash.
//!
//! To escape other characters, use a [character reference][character_reference]
//! instead (as in, `&amp;`, `&#123;`, or say `&#x9;`).
//!
//! It is also possible to escape a line ending in text with a similar
//! construct: a [hard break (escape)][hard_break_escape] is a backslash followed
//! by a line ending (that is part of the construct instead of ending it).
//!
//! ## Recommendation
//!
//! If possible, use a character escape.
//! Otherwise, use a character reference.
//!
//! ## Tokens
//!
//! * [`CharacterEscape`][Name::CharacterEscape]
//! * [`CharacterEscapeMarker`][Name::CharacterEscapeMarker]
//! * [`CharacterEscapeValue`][Name::CharacterEscapeValue]
//!
//! ## References
//!
//! * [`character-escape.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/character-escape.js)
//! * [*§ 2.4 Backslash escapes* in `CommonMark`](https://spec.commonmark.org/0.31/#backslash-escapes)
//!
//! [string]: crate::mdx_parser::construct::string
//! [text]: crate::mdx_parser::construct::text
//! [character_reference]: crate::mdx_parser::construct::character_reference
//! [hard_break_escape]: crate::mdx_parser::construct::hard_break_escape

use crate::mdx_parser::event::Name;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;

/// Start of character escape.
///
/// ```markdown
/// > | a\*b
///      ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.parse_state.options.constructs.character_escape && tokenizer.current == Some(b'\\') {
        tokenizer.enter(Name::CharacterEscape);
        tokenizer.enter(Name::CharacterEscapeMarker);
        tokenizer.consume();
        tokenizer.exit(Name::CharacterEscapeMarker);
        State::Next(StateName::CharacterEscapeInside)
    } else {
        State::Nok
    }
}

/// After `\`, at punctuation.
///
/// ```markdown
/// > | a\*b
///       ^
/// ```
pub fn inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // ASCII punctuation.
        Some(b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~') => {
            tokenizer.enter(Name::CharacterEscapeValue);
            tokenizer.consume();
            tokenizer.exit(Name::CharacterEscapeValue);
            tokenizer.exit(Name::CharacterEscape);
            State::Ok
        }
        _ => State::Nok,
    }
}
