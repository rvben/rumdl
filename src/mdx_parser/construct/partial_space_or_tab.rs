//! Space or tab occurs in tons of places.
//!
//! ## Grammar
//!
//! Space or tab forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! space_or_tab ::= 1*('\t' | ' ')
//! ```
//!
//! ## References
//!
//! * [`micromark-factory-space/index.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-factory-space/dev/index.js)

use crate::mdx_parser::event::{Content, Link, Name};
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::link;
use crate::mdx_parser::tokenizer::Tokenizer;

/// Configuration.
#[derive(Debug)]
pub struct Options {
    /// Minimum allowed bytes (inclusive).
    pub min: usize,
    /// Maximum allowed bytes (inclusive).
    pub max: usize,
    /// Name to use for events.
    pub kind: Name,
    /// Connect this event to the previous.
    pub connect: bool,
    /// Embedded content type to use.
    pub content: Option<Content>,
}

/// One or more `space_or_tab`.
///
/// ```bnf
/// space_or_tab ::= 1*( ' ' '\t' )
/// ```
pub fn space_or_tab(tokenizer: &mut Tokenizer) -> StateName {
    space_or_tab_min_max(tokenizer, 1, usize::MAX)
}

/// Between `x` and `y` `space_or_tab`.
///
/// ```bnf
/// space_or_tab_min_max ::= x*y( ' ' '\t' )
/// ```
pub fn space_or_tab_min_max(tokenizer: &mut Tokenizer, min: usize, max: usize) -> StateName {
    space_or_tab_with_options(
        tokenizer,
        Options {
            kind: Name::SpaceOrTab,
            min,
            max,
            content: None,
            connect: false,
        },
    )
}

/// `space_or_tab`, with the given options.
pub fn space_or_tab_with_options(tokenizer: &mut Tokenizer, options: Options) -> StateName {
    tokenizer.tokenize_state.space_or_tab_connect = options.connect;
    tokenizer.tokenize_state.space_or_tab_content = options.content;
    tokenizer.tokenize_state.space_or_tab_min = options.min;
    tokenizer.tokenize_state.space_or_tab_max = options.max;
    tokenizer.tokenize_state.space_or_tab_token = options.kind;
    StateName::SpaceOrTabStart
}

/// Start of `space_or_tab`.
///
/// ```markdown
/// > | a␠␠b
///      ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.tokenize_state.space_or_tab_max > 0 && matches!(tokenizer.current, Some(b'\t' | b' ')) {
        if let Some(ref content) = tokenizer.tokenize_state.space_or_tab_content {
            tokenizer.enter_link(
                tokenizer.tokenize_state.space_or_tab_token.clone(),
                Link {
                    previous: None,
                    next: None,
                    content: content.clone(),
                },
            );
        } else {
            tokenizer.enter(tokenizer.tokenize_state.space_or_tab_token.clone());
        }

        if tokenizer.tokenize_state.space_or_tab_connect {
            let index = tokenizer.events.len() - 1;
            link(&mut tokenizer.events, index);
        } else {
            tokenizer.tokenize_state.space_or_tab_connect = true;
        }

        State::Retry(StateName::SpaceOrTabInside)
    } else {
        State::Retry(StateName::SpaceOrTabAfter)
    }
}

/// In `space_or_tab`.
///
/// ```markdown
/// > | a␠␠b
///       ^
/// ```
pub fn inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\t' | b' ')
            if tokenizer.tokenize_state.space_or_tab_size < tokenizer.tokenize_state.space_or_tab_max =>
        {
            tokenizer.consume();
            tokenizer.tokenize_state.space_or_tab_size += 1;
            State::Next(StateName::SpaceOrTabInside)
        }
        _ => {
            tokenizer.exit(tokenizer.tokenize_state.space_or_tab_token.clone());
            State::Retry(StateName::SpaceOrTabAfter)
        }
    }
}

/// After `space_or_tab`.
///
/// ```markdown
/// > | a␠␠b
///        ^
/// ```
pub fn after(tokenizer: &mut Tokenizer) -> State {
    let state = if tokenizer.tokenize_state.space_or_tab_size >= tokenizer.tokenize_state.space_or_tab_min {
        State::Ok
    } else {
        State::Nok
    };
    tokenizer.tokenize_state.space_or_tab_connect = false;
    tokenizer.tokenize_state.space_or_tab_content = None;
    tokenizer.tokenize_state.space_or_tab_size = 0;
    tokenizer.tokenize_state.space_or_tab_max = 0;
    tokenizer.tokenize_state.space_or_tab_min = 0;
    tokenizer.tokenize_state.space_or_tab_token = Name::SpaceOrTab;
    state
}
