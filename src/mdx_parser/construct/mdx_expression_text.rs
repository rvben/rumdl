//! MDX expression (text) occurs in the [text][] content type.
//!
//! ## Grammar
//!
//! MDX expression (text) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! mdx_expression_text ::= mdx_expression
//!
//! ; See the `partial_mdx_expression` construct for the BNF of that part.
//! ```
//!
//! See [`mdx_expression`][mdx_expression] for more info.
//!
//! ## Tokens
//!
//! * [`MdxTextExpression`][Name::MdxTextExpression]
//! * see [`mdx_expression`][mdx_expression] for more
//!
//! ## Recommendation
//!
//! See [`mdx_expression`][mdx_expression] for recommendations.
//!
//! ## References
//!
//! * [`syntax.js` in `micromark-extension-mdx-expression`](https://github.com/micromark/micromark-extension-mdx-expression/blob/main/packages/micromark-extension-mdx-expression/dev/lib/syntax.js)
//! * [`mdxjs.com`](https://mdxjs.com)
//!
//! [text]: crate::mdx_parser::construct::text
//! [mdx_expression]: crate::mdx_parser::construct::partial_mdx_expression

use crate::mdx_parser::event::Name;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;

/// Start of an MDX expression (text).
///
/// ```markdown
/// > | a {Math.PI} c
///       ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if Some(b'{') == tokenizer.current && tokenizer.parse_state.options.constructs.mdx_expression_text {
        tokenizer.tokenize_state.token_1 = Name::MdxTextExpression;
        tokenizer.attempt(State::Next(StateName::MdxExpressionTextAfter), State::Nok);
        State::Retry(StateName::MdxExpressionStart)
    } else {
        State::Nok
    }
}

/// After expression.
///
/// ```markdown
/// > | a {Math.PI} c
///                ^
/// ```
pub fn after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::Data;
    State::Ok
}
