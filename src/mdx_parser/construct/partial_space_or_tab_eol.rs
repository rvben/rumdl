//! Space or tab (eol) occurs in [destination][], [label][], and [title][].
//!
//! ## Grammar
//!
//! Space or tab (eol) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! space_or_tab_eol ::= 1*space_or_tab | *space_or_tab eol *space_or_tab
//! ```
//!
//! Importantly, this allows one line ending, but not blank lines.
//!
//! ## References
//!
//! * [`micromark-factory-space/index.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-factory-space/dev/index.js)
//!
//! [destination]: crate::mdx_parser::construct::partial_destination
//! [label]: crate::mdx_parser::construct::partial_label
//! [title]: crate::mdx_parser::construct::partial_title

use crate::mdx_parser::construct::partial_space_or_tab::{Options as SpaceOrTabOptions, space_or_tab_with_options};
use crate::mdx_parser::event::{Content, Link, Name};
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::link;
use crate::mdx_parser::tokenizer::Tokenizer;

/// Configuration.
#[derive(Debug)]
pub struct Options {
    /// Connect this whitespace to the previous.
    pub connect: bool,
    /// Embedded content type to use.
    pub content: Option<Content>,
}

/// `space_or_tab_eol`
pub fn space_or_tab_eol(tokenizer: &mut Tokenizer) -> StateName {
    space_or_tab_eol_with_options(
        tokenizer,
        Options {
            content: None,
            connect: false,
        },
    )
}

/// `space_or_tab_eol`, with the given options.
pub fn space_or_tab_eol_with_options(tokenizer: &mut Tokenizer, options: Options) -> StateName {
    tokenizer.tokenize_state.space_or_tab_eol_content = options.content;
    tokenizer.tokenize_state.space_or_tab_eol_connect = options.connect;
    StateName::SpaceOrTabEolStart
}

/// Start of whitespace with at most one eol.
///
/// ```markdown
/// > | a␠␠b
///      ^
/// > | a␠␠␊
///      ^
///   | ␠␠b
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\t' | b' ') => {
            tokenizer.attempt(
                State::Next(StateName::SpaceOrTabEolAfterFirst),
                State::Next(StateName::SpaceOrTabEolAtEol),
            );

            State::Retry(space_or_tab_with_options(
                tokenizer,
                SpaceOrTabOptions {
                    kind: Name::SpaceOrTab,
                    min: 1,
                    max: usize::MAX,
                    content: tokenizer.tokenize_state.space_or_tab_eol_content.clone(),
                    connect: tokenizer.tokenize_state.space_or_tab_eol_connect,
                },
            ))
        }
        _ => State::Retry(StateName::SpaceOrTabEolAtEol),
    }
}

/// After initial whitespace, at optional eol.
///
/// ```markdown
/// > | a␠␠b
///        ^
/// > | a␠␠␊
///        ^
///   | ␠␠b
/// ```
pub fn after_first(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.space_or_tab_eol_ok = true;
    debug_assert!(
        tokenizer.tokenize_state.space_or_tab_eol_content.is_none(),
        "expected no content"
    );
    // If the above ever errors, set `tokenizer.tokenize_state.space_or_tab_eol_connect: true` in that case.
    State::Retry(StateName::SpaceOrTabEolAtEol)
}

/// After optional whitespace, at eol.
///
/// ```markdown
/// > | a␠␠b
///        ^
/// > | a␠␠␊
///        ^
///   | ␠␠b
/// > | a␊
///      ^
///   | ␠␠b
/// ```
pub fn at_eol(tokenizer: &mut Tokenizer) -> State {
    if let Some(b'\n') = tokenizer.current {
        if let Some(ref content) = tokenizer.tokenize_state.space_or_tab_eol_content {
            tokenizer.enter_link(
                Name::LineEnding,
                Link {
                    previous: None,
                    next: None,
                    content: content.clone(),
                },
            );
        } else {
            tokenizer.enter(Name::LineEnding);
        }

        if tokenizer.tokenize_state.space_or_tab_eol_connect {
            let index = tokenizer.events.len() - 1;
            link(&mut tokenizer.events, index);
        } else if tokenizer.tokenize_state.space_or_tab_eol_content.is_some() {
            tokenizer.tokenize_state.space_or_tab_eol_connect = true;
        }

        tokenizer.consume();
        tokenizer.exit(Name::LineEnding);
        State::Next(StateName::SpaceOrTabEolAfterEol)
    } else {
        let ok = tokenizer.tokenize_state.space_or_tab_eol_ok;
        tokenizer.tokenize_state.space_or_tab_eol_content = None;
        tokenizer.tokenize_state.space_or_tab_eol_connect = false;
        tokenizer.tokenize_state.space_or_tab_eol_ok = false;
        if ok { State::Ok } else { State::Nok }
    }
}

/// After eol.
///
/// ```markdown
///   | a␠␠␊
/// > | ␠␠b
///     ^
///   | a␊
/// > | ␠␠b
///     ^
/// ```
pub fn after_eol(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, Some(b'\t' | b' ')) {
        tokenizer.attempt(State::Next(StateName::SpaceOrTabEolAfterMore), State::Nok);
        State::Retry(space_or_tab_with_options(
            tokenizer,
            SpaceOrTabOptions {
                kind: Name::SpaceOrTab,
                min: 1,
                max: usize::MAX,
                content: tokenizer.tokenize_state.space_or_tab_eol_content.clone(),
                connect: tokenizer.tokenize_state.space_or_tab_eol_connect,
            },
        ))
    } else {
        State::Retry(StateName::SpaceOrTabEolAfterMore)
    }
}

/// After optional final whitespace.
///
/// ```markdown
///   | a␠␠␊
/// > | ␠␠b
///       ^
///   | a␊
/// > | ␠␠b
///       ^
/// ```
pub fn after_more(tokenizer: &mut Tokenizer) -> State {
    debug_assert!(
        !matches!(tokenizer.current, None | Some(b'\n')),
        "did not expect blank line"
    );
    // If the above ever starts erroring, gracefully `State::Nok` on it.
    // Currently it doesn’t happen, as we only use this in content, which does
    // not allow blank lines.
    tokenizer.tokenize_state.space_or_tab_eol_content = None;
    tokenizer.tokenize_state.space_or_tab_eol_connect = false;
    tokenizer.tokenize_state.space_or_tab_eol_ok = false;
    State::Ok
}
