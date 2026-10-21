//! Label occurs in [definition][] and [label end][label_end].
//!
//! ## Grammar
//!
//! Label forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! ; Restriction: maximum `999` codes allowed between brackets.
//! ; Restriction: no blank lines.
//! ; Restriction: at least 1 `text` byte must exist.
//! label ::= '[' *(label_byte | label_escape) ']'
//! label_byte ::= code - '[' - '\\' - ']'
//! label_escape ::= '\\' ['[' | '\\' | ']']
//! ```
//!
//! The maximum allowed size of the label, without the brackets, is `999`
//! (inclusive), which is defined in
//! [`LINK_REFERENCE_SIZE_MAX`][].
//!
//! Labels can contain line endings and whitespace, but they are not allowed to
//! contain blank lines, and they must not be blank themselves.
//!
//! The label is interpreted as the [string][] content type.
//! That means that [character escapes][character_escape] and
//! [character references][character_reference] are allowed.
//!
//! > 👉 **Note**: this label relates to, but is not, the initial “label” of
//! > what is know as a reference in markdown:
//! >
//! > | Kind      | Link     | Image     |
//! > | --------- | -------- | --------- |
//! > | Shortcut  | `[x]`    | `![x]`    |
//! > | Collapsed | `[x][]`  | `![x][]`  |
//! > | Full      | `[x][y]` | `![x][y]` |
//! >
//! > The 6 above things are references, in the three kinds they come in, as
//! > links and images.
//! > The label that this module focusses on is only the thing that contains
//! > `y`.
//! >
//! > The thing that contains `x` is not a single thing when parsing markdown,
//! > but instead constists of an opening
//! > ([label start (image)][label_start_image] or
//! > [label start (link)][label_start_link]) and a closing
//! > ([label end][label_end]), so as to allow further phrasing such as
//! > [code (text)][raw_text] or [attention][].
//!
//! ## References
//!
//! * [`micromark-factory-label/index.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-factory-label/dev/index.js)
//!
//! [definition]: crate::mdx_parser::construct::definition
//! [string]: crate::mdx_parser::construct::string
//! [attention]: crate::mdx_parser::construct::attention
//! [character_escape]: crate::mdx_parser::construct::character_escape
//! [character_reference]: crate::mdx_parser::construct::character_reference
//! [label_start_image]: crate::mdx_parser::construct::label_start_image
//! [label_start_link]: crate::mdx_parser::construct::label_start_link
//! [label_end]: crate::mdx_parser::construct::label_end
//! [raw_text]: crate::mdx_parser::construct::raw_text
//! [link_reference_size_max]: crate::mdx_parser::util::constant::LINK_REFERENCE_SIZE_MAX

use crate::mdx_parser::construct::partial_space_or_tab_eol::{Options, space_or_tab_eol_with_options};
use crate::mdx_parser::event::{Content, Link, Name};
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::link;
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::constant::LINK_REFERENCE_SIZE_MAX;

/// Start of label.
///
/// ```markdown
/// > | [a]
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    debug_assert_eq!(tokenizer.current, Some(b'['), "expected `[`");
    tokenizer.enter(tokenizer.tokenize_state.token_1.clone());
    tokenizer.enter(tokenizer.tokenize_state.token_2.clone());
    tokenizer.consume();
    tokenizer.exit(tokenizer.tokenize_state.token_2.clone());
    tokenizer.enter(tokenizer.tokenize_state.token_3.clone());
    State::Next(StateName::LabelAtBreak)
}

/// In label, at something, before something else.
///
/// ```markdown
/// > | [a]
///      ^
/// ```
pub fn at_break(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.tokenize_state.size > LINK_REFERENCE_SIZE_MAX
        || matches!(tokenizer.current, None | Some(b'['))
        || (matches!(tokenizer.current, Some(b']')) && !tokenizer.tokenize_state.seen)
    {
        State::Retry(StateName::LabelNok)
    } else {
        match tokenizer.current {
            Some(b'\n') => {
                tokenizer.attempt(State::Next(StateName::LabelEolAfter), State::Next(StateName::LabelNok));
                State::Retry(space_or_tab_eol_with_options(
                    tokenizer,
                    Options {
                        content: Some(Content::String),
                        connect: tokenizer.tokenize_state.connect,
                    },
                ))
            }
            Some(b']') => {
                tokenizer.exit(tokenizer.tokenize_state.token_3.clone());
                tokenizer.enter(tokenizer.tokenize_state.token_2.clone());
                tokenizer.consume();
                tokenizer.exit(tokenizer.tokenize_state.token_2.clone());
                tokenizer.exit(tokenizer.tokenize_state.token_1.clone());
                tokenizer.tokenize_state.connect = false;
                tokenizer.tokenize_state.seen = false;
                tokenizer.tokenize_state.size = 0;
                State::Ok
            }
            _ => {
                tokenizer.enter_link(
                    Name::Data,
                    Link {
                        previous: None,
                        next: None,
                        content: Content::String,
                    },
                );

                if tokenizer.tokenize_state.connect {
                    let index = tokenizer.events.len() - 1;
                    link(&mut tokenizer.events, index);
                } else {
                    tokenizer.tokenize_state.connect = true;
                }

                State::Retry(StateName::LabelInside)
            }
        }
    }
}

/// In label, after whitespace.
///
/// ```markdown
///   | [a␊
/// > | b]
///     ^
/// ```
pub fn eol_after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.connect = true;
    State::Retry(StateName::LabelAtBreak)
}

/// In label, on something disallowed.
///
/// ```markdown
/// > | []
///      ^
/// ```
pub fn nok(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.connect = false;
    tokenizer.tokenize_state.seen = false;
    tokenizer.tokenize_state.size = 0;
    State::Nok
}

/// In label, in text.
///
/// ```markdown
/// > | [a]
///      ^
/// ```
pub fn inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n' | b'[' | b']') => {
            tokenizer.exit(Name::Data);
            State::Retry(StateName::LabelAtBreak)
        }
        Some(byte) => {
            if tokenizer.tokenize_state.size > LINK_REFERENCE_SIZE_MAX {
                tokenizer.exit(Name::Data);
                State::Retry(StateName::LabelAtBreak)
            } else {
                tokenizer.consume();
                tokenizer.tokenize_state.size += 1;
                if !tokenizer.tokenize_state.seen && !matches!(byte, b'\t' | b' ') {
                    tokenizer.tokenize_state.seen = true;
                }
                State::Next(if matches!(byte, b'\\') {
                    StateName::LabelEscape
                } else {
                    StateName::LabelInside
                })
            }
        }
    }
}

/// After `\`, at a special character.
///
/// ```markdown
/// > | [a\*a]
///        ^
/// ```
pub fn escape(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'[' | b'\\' | b']') => {
            tokenizer.consume();
            tokenizer.tokenize_state.size += 1;
            State::Next(StateName::LabelInside)
        }
        _ => State::Retry(StateName::LabelInside),
    }
}
