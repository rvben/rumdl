//! MDX expression (flow) occurs in the [flow][] content type.
//!
//! ## Grammar
//!
//! MDX expression (flow) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! mdx_expression_flow ::= mdx_expression *space_or_tab
//!
//! ; See the `partial_mdx_expression` construct for the BNF of that part.
//! ```
//!
//! As this construct occurs in flow, like all flow constructs, it must be
//! followed by an eol (line ending) or eof (end of file).
//!
//! See [`mdx_expression`][mdx_expression] for more info.
//!
//! ## Tokens
//!
//! * [`MdxFlowExpression`][Name::MdxFlowExpression]
//! * [`SpaceOrTab`][Name::SpaceOrTab]
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
//! [flow]: crate::mdx_parser::construct::flow
//! [mdx_expression]: crate::mdx_parser::construct::partial_mdx_expression

use crate::mdx_parser::construct::partial_space_or_tab::{space_or_tab, space_or_tab_min_max};
use crate::mdx_parser::event::Name;
use crate::mdx_parser::parser::ParseState;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::constant::TAB_SIZE;
use alloc::boxed::Box;

/// Start of an MDX expression (flow).
///
/// ```markdown
/// > | {Math.PI}
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.parse_state.options.constructs.mdx_expression_flow {
        tokenizer.tokenize_state.token_1 = Name::MdxFlowExpression;
        if matches!(tokenizer.current, Some(b'\t' | b' ')) {
            tokenizer.attempt(State::Next(StateName::MdxExpressionFlowBefore), State::Nok);
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
            State::Retry(StateName::MdxExpressionFlowBefore)
        }
    } else {
        State::Nok
    }
}

/// After optional whitespace, before expression.
///
/// ```markdown
/// > | {Math.PI}
///     ^
/// ```
pub fn before(tokenizer: &mut Tokenizer) -> State {
    if Some(b'{') == tokenizer.current {
        if known_nok(tokenizer) {
            reset(tokenizer);
            State::Nok
        } else {
            tokenizer.concrete = true;
            tokenizer.attempt(State::Next(StateName::MdxExpressionFlowAfter), State::Nok);
            State::Retry(StateName::MdxExpressionStart)
        }
    } else {
        State::Nok
    }
}

/// After expression.
///
/// ```markdown
/// > | {Math.PI}
///              ^
/// ```
pub fn after(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\t' | b' ') => {
            tokenizer.attempt(State::Next(StateName::MdxExpressionFlowEnd), State::Nok);
            State::Retry(space_or_tab(tokenizer))
        }
        _ => State::Retry(StateName::MdxExpressionFlowEnd),
    }
}

/// After expression, after optional whitespace.
///
/// ```markdown
/// > | {Math.PI}␠␊
///               ^
/// ```
pub fn end(tokenizer: &mut Tokenizer) -> State {
    // We want to allow tags directly after expressions.
    //
    // This case is useful:
    //
    // ```mdx
    // <a>{b}</a>
    // ```
    //
    // This case is not (very?) useful:
    //
    // ```mdx
    // {a}<b/>
    // ```
    //
    // …but it would be tougher than needed to disallow.
    //
    // To allow that, here we call the MDX JSX flow construct, and there we
    // call this one.
    //
    // It would introduce a cyclical interdependency if we test JSX and
    // expressions here.
    // Because the JSX extension already uses parts of this monorepo, we
    // instead test it there.
    //
    // Note: in the JS version of micromark, arbitrary extensions could be
    // loaded.
    // Here we know that only our own construct `mdx_expression_flow` can be
    // enabled.

    // if matches!(tokenizer.current, None | Some(b'\n')) {
    //     State::Ok
    // } else {
    //     State::Nok
    // }
    match tokenizer.current {
        None | Some(b'\n') => {
            reset(tokenizer);
            State::Ok
        }
        // Tag.
        Some(b'<') if tokenizer.parse_state.options.constructs.mdx_jsx_flow => {
            // We can’t just say: fine.
            // Lines of blocks have to be parsed until an eol/eof.
            tokenizer.tokenize_state.token_1 = Name::MdxJsxFlowTag;
            tokenizer.attempt(
                State::Next(StateName::MdxJsxFlowAfter),
                State::Next(StateName::MdxJsxFlowNok),
            );
            State::Retry(StateName::MdxJsxStart)
        }
        // // An expression.
        // Some(b'{') if tokenizer.parse_state.options.constructs.mdx_expression_flow => {
        //     tokenizer.attempt(
        //         State::Next(StateName::MdxExpressionFlowAfter),
        //         State::Next(StateName::MdxExpressionFlowNok),
        //     );
        //     State::Retry(StateName::MdxExpressionFlowStart)
        // }
        _ => {
            // Only brace counting records.
            if tokenizer.parse_state.options.mdx_expression_parse.is_none() {
                tokenizer.tokenize_state.mdx_braces.get_or_insert_with(Box::default);
            }
            reset(tokenizer);
            State::Nok
        }
    }
}

/// Whether an expression closing before `index` fails at `end`: something
/// other than a line ending or tag follows on its line.
pub fn fails_after(parse_state: &ParseState, index: usize) -> bool {
    let bytes = parse_state.bytes;
    let mut index = index;

    while index < bytes.len() && matches!(bytes[index], b'\t' | b' ') {
        index += 1;
    }

    match bytes.get(index).copied() {
        None | Some(b'\n' | b'\r') => false,
        Some(b'<') => !parse_state.options.constructs.mdx_jsx_flow,
        Some(_) => true,
    }
}

/// Whether an earlier expression that counted past here found this one fails.
fn known_nok(tokenizer: &Tokenizer) -> bool {
    match &tokenizer.tokenize_state.mdx_braces {
        Some(braces) if !tokenizer.lazy => braces
            .opening
            .binary_search_by_key(&tokenizer.point.index, |brace| brace.0)
            .map_or(false, |position| braces.opening[position].1),
        _ => false,
    }
}

/// Reset state.
fn reset(tokenizer: &mut Tokenizer) {
    tokenizer.concrete = false;
    tokenizer.tokenize_state.token_1 = Name::Data;
}
