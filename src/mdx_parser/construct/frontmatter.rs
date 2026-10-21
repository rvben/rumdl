//! Frontmatter occurs at the start of the document.
//!
//! ## Grammar
//!
//! Frontmatter forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! frontmatter ::= fence_open *( eol *byte ) eol fence_close
//! fence_open ::= sequence *space_or_tab
//! ; Restriction: markers in `sequence` must match markers in opening sequence.
//! fence_close ::= sequence *space_or_tab
//! sequence ::= 3'+' | 3'-'
//! ```
//!
//! Frontmatter can only occur once.
//! It cannot occur in a container.
//! It must have a closing fence.
//! Like flow constructs, it must be followed by an eol (line ending) or
//! eof (end of file).
//!
//! ## Extension
//!
//! > 👉 **Note**: frontmatter is not part of `CommonMark`, so frontmatter is
//! > not enabled by default.
//! > You need to enable it manually.
//! > See [`Constructs`][constructs] for more info.
//!
//! As there is no spec for frontmatter in markdown, this extension follows how
//! YAML frontmatter works on `github.com`.
//! It also parses TOML frontmatter, just like YAML except that it uses a `+`.
//!
//! ## Recommendation
//!
//! When authoring markdown with frontmatter, it’s recommended to use YAML
//! frontmatter if possible.
//! While YAML has some warts, it works in the most places, so using it
//! guarantees the highest chance of portability.
//!
//! In certain ecosystems, other flavors are widely used.
//! For example, in the Rust ecosystem, TOML is often used.
//! In such cases, using TOML is an okay choice.
//!
//! ## Tokens
//!
//! * [`Frontmatter`][Name::Frontmatter]
//! * [`FrontmatterFence`][Name::FrontmatterFence]
//! * [`FrontmatterSequence`][Name::FrontmatterSequence]
//! * [`FrontmatterChunk`][Name::FrontmatterChunk]
//! * [`LineEnding`][Name::LineEnding]
//! * [`SpaceOrTab`][Name::SpaceOrTab]
//!
//! ## References
//!
//! * [`micromark-extension-frontmatter`](https://github.com/micromark/micromark-extension-frontmatter)
//!
//! [constructs]: crate::mdx_parser::Constructs

use crate::mdx_parser::construct::partial_space_or_tab::space_or_tab;
use crate::mdx_parser::event::Name;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::constant::FRONTMATTER_SEQUENCE_SIZE;

/// Start of frontmatter.
///
/// ```markdown
/// > | ---
///     ^
///   | title: "Venus"
///   | ---
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    // Indent not allowed.
    if tokenizer.parse_state.options.constructs.frontmatter && matches!(tokenizer.current, Some(b'+' | b'-')) {
        tokenizer.tokenize_state.marker = tokenizer.current.unwrap();
        tokenizer.enter(Name::Frontmatter);
        tokenizer.enter(Name::FrontmatterFence);
        tokenizer.enter(Name::FrontmatterSequence);
        State::Retry(StateName::FrontmatterOpenSequence)
    } else {
        State::Nok
    }
}

/// In open sequence.
///
/// ```markdown
/// > | ---
///     ^
///   | title: "Venus"
///   | ---
/// ```
pub fn open_sequence(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.tokenize_state.size += 1;
        tokenizer.consume();
        State::Next(StateName::FrontmatterOpenSequence)
    } else if tokenizer.tokenize_state.size == FRONTMATTER_SEQUENCE_SIZE {
        tokenizer.tokenize_state.size = 0;
        tokenizer.exit(Name::FrontmatterSequence);

        if matches!(tokenizer.current, Some(b'\t' | b' ')) {
            tokenizer.attempt(State::Next(StateName::FrontmatterOpenAfter), State::Nok);
            State::Retry(space_or_tab(tokenizer))
        } else {
            State::Retry(StateName::FrontmatterOpenAfter)
        }
    } else {
        tokenizer.tokenize_state.marker = 0;
        tokenizer.tokenize_state.size = 0;
        State::Nok
    }
}

/// After open sequence.
///
/// ```markdown
/// > | ---
///        ^
///   | title: "Venus"
///   | ---
/// ```
pub fn open_after(tokenizer: &mut Tokenizer) -> State {
    if let Some(b'\n') = tokenizer.current {
        tokenizer.exit(Name::FrontmatterFence);
        tokenizer.enter(Name::LineEnding);
        tokenizer.consume();
        tokenizer.exit(Name::LineEnding);
        tokenizer.attempt(
            State::Next(StateName::FrontmatterAfter),
            State::Next(StateName::FrontmatterContentStart),
        );
        State::Next(StateName::FrontmatterCloseStart)
    } else {
        tokenizer.tokenize_state.marker = 0;
        State::Nok
    }
}

/// Start of close sequence.
///
/// ```markdown
///   | ---
///   | title: "Venus"
/// > | ---
///     ^
/// ```
pub fn close_start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.enter(Name::FrontmatterFence);
        tokenizer.enter(Name::FrontmatterSequence);
        State::Retry(StateName::FrontmatterCloseSequence)
    } else {
        State::Nok
    }
}

/// In close sequence.
///
/// ```markdown
///   | ---
///   | title: "Venus"
/// > | ---
///     ^
/// ```
pub fn close_sequence(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.tokenize_state.size += 1;
        tokenizer.consume();
        State::Next(StateName::FrontmatterCloseSequence)
    } else if tokenizer.tokenize_state.size == FRONTMATTER_SEQUENCE_SIZE {
        tokenizer.tokenize_state.size = 0;
        tokenizer.exit(Name::FrontmatterSequence);

        if matches!(tokenizer.current, Some(b'\t' | b' ')) {
            tokenizer.attempt(State::Next(StateName::FrontmatterCloseAfter), State::Nok);
            State::Retry(space_or_tab(tokenizer))
        } else {
            State::Retry(StateName::FrontmatterCloseAfter)
        }
    } else {
        tokenizer.tokenize_state.size = 0;
        State::Nok
    }
}

/// After close sequence.
///
/// ```markdown
///   | ---
///   | title: "Venus"
/// > | ---
///        ^
/// ```
pub fn close_after(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(Name::FrontmatterFence);
            State::Ok
        }
        _ => State::Nok,
    }
}

/// Start of content chunk.
///
/// ```markdown
///   | ---
/// > | title: "Venus"
///     ^
///   | ---
/// ```
pub fn content_start(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => State::Retry(StateName::FrontmatterContentEnd),
        Some(_) => {
            tokenizer.enter(Name::FrontmatterChunk);
            State::Retry(StateName::FrontmatterContentInside)
        }
    }
}

/// In content chunk.
///
/// ```markdown
///   | ---
/// > | title: "Venus"
///     ^
///   | ---
/// ```
pub fn content_inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(Name::FrontmatterChunk);
            State::Retry(StateName::FrontmatterContentEnd)
        }
        Some(_) => {
            tokenizer.consume();
            State::Next(StateName::FrontmatterContentInside)
        }
    }
}

/// End of content chunk.
///
/// ```markdown
///   | ---
/// > | title: "Venus"
///                   ^
///   | ---
/// ```
pub fn content_end(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None => {
            tokenizer.tokenize_state.marker = 0;
            State::Nok
        }
        Some(b'\n') => {
            tokenizer.enter(Name::LineEnding);
            tokenizer.consume();
            tokenizer.exit(Name::LineEnding);
            tokenizer.attempt(
                State::Next(StateName::FrontmatterAfter),
                State::Next(StateName::FrontmatterContentStart),
            );
            State::Next(StateName::FrontmatterCloseStart)
        }
        Some(_) => unreachable!("expected eof/eol"),
    }
}

/// After frontmatter.
///
/// ```markdown
///   | ---
///   | title: "Venus"
/// > | ---
///        ^
/// ```
pub fn after(tokenizer: &mut Tokenizer) -> State {
    debug_assert!(
        matches!(tokenizer.current, None | Some(b'\n')),
        "expected eol/eof after closing fence"
    );
    tokenizer.exit(Name::Frontmatter);
    State::Ok
}
