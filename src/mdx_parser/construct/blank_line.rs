//! Blank lines occur in the [flow][] content type.
//!
//! ## Grammar
//!
//! Blank lines form with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! blank_line ::= *space_or_tab
//! ```
//!
//! As this construct occurs in flow, like all flow constructs, it must be
//! followed by an eol (line ending) or eof (end of file).
//!
//! Blank lines are sometimes needed, such as to differentiate a [paragraph][]
//! from a definition.
//! In several cases, blank lines are not needed between flow constructs,
//! such as between two [heading (atx)][heading_atx]s.
//! Sometimes, whether blank lines are present, changes the behavior of how
//! HTML is rendered, such as whether blank lines are present inside or between
//! [list items][list_item].
//! More than one blank line is never needed in `CommonMark`.
//!
//! Because blank lines can be empty (line endings are not considered part of
//! it), and events cannot be empty, blank lines are not present as an event.
//!
//! ## HTML
//!
//! Blank lines do not relate to an element in HTML, except for the role they
//! play when inside or between [list items][list_item].
//!
//! ## Recommendation
//!
//! It is recommended to always use a blank line between every flow construct,
//! to use blank lines (consistently) between list items as desired, and to
//! never use more than one blank line.
//!
//! ## Tokens
//!
//! * [`SpaceOrTab`][crate::mdx_parser::event::Name::SpaceOrTab]
//!
//! ## References
//!
//! * [`blank-line.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/blank-line.js)
//! * [*§ 4.9 Blank lines* in `CommonMark`](https://spec.commonmark.org/0.31/#blank-lines)
//!
//! [heading_atx]: crate::mdx_parser::construct::heading_atx
//! [list_item]: crate::mdx_parser::construct::list_item
//! [paragraph]: crate::mdx_parser::construct::paragraph
//! [flow]: crate::mdx_parser::construct::flow

use crate::mdx_parser::construct::partial_space_or_tab::space_or_tab;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;

/// Start of blank line.
///
/// > 👉 **Note**: `␠` represents a space character.
///
/// ```markdown
/// > | ␠␠␊
///     ^
/// > | ␊
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, Some(b'\t' | b' ')) {
        tokenizer.attempt(State::Next(StateName::BlankLineAfter), State::Nok);
        State::Retry(space_or_tab(tokenizer))
    } else {
        State::Retry(StateName::BlankLineAfter)
    }
}

/// At eof/eol, after optional whitespace.
///
/// ```markdown
/// > | ␠␠␊
///       ^
/// > | ␊
///     ^
/// ```
pub fn after(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => State::Ok,
        _ => State::Nok,
    }
}

/// Whether the rest of the line, from the current byte, is blank, as a check
/// for [`start`][] would find.
pub fn rest_is_blank(tokenizer: &mut Tokenizer) -> bool {
    let bytes = tokenizer.parse_state.bytes;
    let index = tokenizer.point.index;
    debug_assert_eq!(
        tokenizer.current.is_none(),
        index == bytes.len(),
        "expected all bytes to be available"
    );
    let (from, found) = tokenizer.tokenize_state.document_blank_scan;

    let found = if from <= index && index <= found {
        found
    } else {
        let mut found = index;
        while found < bytes.len() && matches!(bytes[found], b'\t' | b' ') {
            found += 1;
        }
        tokenizer.tokenize_state.document_blank_scan = (index, found);
        found
    };

    found == bytes.len() || matches!(bytes[found], b'\n' | b'\r')
}
