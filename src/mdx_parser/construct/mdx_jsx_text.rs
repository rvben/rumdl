//! MDX JSX (text) occurs in the [text][] content type.
//!
//! ## Grammar
//!
//! MDX JSX (text) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! mdx_jsx_text ::= mdx_jsx
//!
//! ; See the `partial_mdx_jsx` construct for the BNF of that part.
//! ```
//!
//! See [`mdx_jsx`][mdx_jsx] for more info.
//!
//! ## Tokens
//!
//! * [`MdxJsxTextTag`][Name::MdxJsxTextTag]
//! * see [`mdx_jsx`][mdx_jsx] for more
//!
//! ## Recommendation
//!
//! See [`mdx_jsx`][mdx_jsx] for recommendations.
//!
//! ## References
//!
//! * [`jsx-text.js` in `micromark-extension-mdx-jsx`](https://github.com/micromark/micromark-extension-mdx-jsx/blob/main/dev/lib/jsx-text.js)
//! * [`mdxjs.com`](https://mdxjs.com)
//!
//! [text]: crate::mdx_parser::construct::text
//! [mdx_jsx]: crate::mdx_parser::construct::partial_mdx_jsx

use crate::mdx_parser::event::Name;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;

/// Start of MDX: JSX (text).
///
/// ```markdown
/// > | a <B /> c
///       ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if Some(b'<') == tokenizer.current && tokenizer.parse_state.options.constructs.mdx_jsx_text {
        tokenizer.tokenize_state.token_1 = Name::MdxJsxTextTag;
        tokenizer.attempt(
            State::Next(StateName::MdxJsxTextAfter),
            State::Next(StateName::MdxJsxTextNok),
        );
        State::Retry(StateName::MdxJsxStart)
    } else {
        State::Nok
    }
}

/// After an MDX JSX (text) tag.
///
/// ```markdown
/// > | a <b> c
///          ^
/// ```
pub fn after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::Data;
    State::Ok
}

/// At something that wasn’t an MDX JSX (text) tag.
///
/// ```markdown
/// > | a < b
///       ^
/// ```
pub fn nok(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::Data;
    State::Nok
}
