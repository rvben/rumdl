//! Block quotes occur in the [document][] content type.
//!
//! ## Grammar
//!
//! Block quotes form with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! block_quote_start ::= '>' [ space_or_tab ]
//! block_quote_cont ::= '>' [ space_or_tab ]
//! ```
//!
//! Further lines that are not prefixed with `block_quote_cont` cause the block
//! quote to be exited, except when those lines are lazy continuation.
//! Like so many things in markdown, block quotes too are complex.
//! See [*§ Phase 1: block structure* in `CommonMark`][commonmark-block] for
//! more on parsing details.
//!
//! As block quote is a container, it takes several bytes from the start of the
//! line, while the rest of the line includes more containers or flow.
//!
//! ## HTML
//!
//! Block quote relates to the `<blockquote>` element in HTML.
//! See [*§ 4.4.4 The `blockquote` element*][html-blockquote] in the HTML spec
//! for more info.
//!
//! ## Recommendation
//!
//! Always use a single space after a block quote marker (`>`).
//! Never use lazy continuation.
//!
//! ## Tokens
//!
//! * [`BlockQuote`][Name::BlockQuote]
//! * [`BlockQuoteMarker`][Name::BlockQuoteMarker]
//! * [`BlockQuotePrefix`][Name::BlockQuotePrefix]
//! * [`SpaceOrTab`][Name::SpaceOrTab]
//!
//! ## References
//!
//! * [`block-quote.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/block-quote.js)
//! * [*§ 5.1 Block quotes* in `CommonMark`](https://spec.commonmark.org/0.31/#block-quotes)
//!
//! [document]: crate::mdx_parser::construct::document
//! [html-blockquote]: https://html.spec.whatwg.org/multipage/grouping-content.html#the-blockquote-element
//! [commonmark-block]: https://spec.commonmark.org/0.31/#phase-1-block-structure

use crate::mdx_parser::construct::partial_space_or_tab::space_or_tab_min_max;
use crate::mdx_parser::event::Name;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::constant::TAB_SIZE;

/// Start of block quote.
///
/// ```markdown
/// > | > a
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.parse_state.options.constructs.block_quote {
        tokenizer.enter(Name::BlockQuote);
        State::Retry(StateName::BlockQuoteContStart)
    } else {
        State::Nok
    }
}

/// Start of block quote continuation.
///
/// Also used to parse the first block quote opening.
///
/// ```markdown
///   | > a
/// > | > b
///     ^
/// ```
pub fn cont_start(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, Some(b'\t' | b' ')) {
        tokenizer.attempt(State::Next(StateName::BlockQuoteContBefore), State::Nok);
        State::Retry(space_or_tab_min_max(
            tokenizer,
            1,
            if tokenizer.parse_state.options.constructs.code_indented {
                TAB_SIZE - 1
            } else {
                usize::MAX
            },
        ))
    } else {
        State::Retry(StateName::BlockQuoteContBefore)
    }
}

/// At `>`, after optional whitespace.
///
/// Also used to parse the first block quote opening.
///
/// ```markdown
///   | > a
/// > | > b
///     ^
/// ```
pub fn cont_before(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'>') => {
            tokenizer.enter(Name::BlockQuotePrefix);
            tokenizer.enter(Name::BlockQuoteMarker);
            tokenizer.consume();
            tokenizer.exit(Name::BlockQuoteMarker);
            State::Next(StateName::BlockQuoteContAfter)
        }
        _ => State::Nok,
    }
}

/// After `>`, before optional whitespace.
///
/// ```markdown
/// > | > a
///      ^
/// > | >b
///      ^
/// ```
pub fn cont_after(tokenizer: &mut Tokenizer) -> State {
    if let Some(b'\t' | b' ') = tokenizer.current {
        tokenizer.enter(Name::SpaceOrTab);
        tokenizer.consume();
        tokenizer.exit(Name::SpaceOrTab);
    }

    tokenizer.exit(Name::BlockQuotePrefix);
    State::Ok
}
