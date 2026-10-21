//! Paragraph occurs in the [content][] content type.
//!
//! ## Grammar
//!
//! Paragraph forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! ; Restriction: lines cannot start other flow constructs.
//! ; Restriction: lines cannot be blank.
//! paragraph ::= 1*line *(eol 1*line)
//! ```
//!
//! This construct must be followed by an eol (line ending) or eof (end of
//! file), like flow constructs.
//!
//! Paragraphs can contain line endings and whitespace, but they are not
//! allowed to contain blank lines, or to be blank themselves.
//!
//! The paragraph is interpreted as the [text][] content type.
//! That means that [autolinks][autolink], [code (text)][raw_text], etc are
//! allowed.
//!
//! ## HTML
//!
//! Paragraphs in markdown relate to the `<p>` element in HTML.
//! See [*§ 4.4.1 The `p` element* in the HTML spec][html] for more info.
//!
//! ## Tokens
//!
//! * [`Paragraph`][Name::Paragraph]
//!
//! ## References
//!
//! * [`content.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/content.js)
//! * [*§ 4.8 Paragraphs* in `CommonMark`](https://spec.commonmark.org/0.31/#paragraphs)
//!
//! [content]: crate::mdx_parser::construct::content
//! [text]: crate::mdx_parser::construct::text
//! [autolink]: crate::mdx_parser::construct::autolink
//! [raw_text]: crate::mdx_parser::construct::raw_text
//! [html]: https://html.spec.whatwg.org/multipage/grouping-content.html#the-p-element

use crate::mdx_parser::event::{Content, Link, Name};
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::link;
use crate::mdx_parser::tokenizer::Tokenizer;

/// Paragraph start.
///
/// ```markdown
/// > | abc
///     ^
///   | def
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    debug_assert!(tokenizer.current.is_some());
    tokenizer.enter(Name::Paragraph);
    State::Retry(StateName::ParagraphLineStart)
}

/// Start of a line in a paragraph.
///
/// ```markdown
/// > | abc
///     ^
/// > | def
///     ^
/// ```
pub fn line_start(tokenizer: &mut Tokenizer) -> State {
    debug_assert!(tokenizer.current.is_some());
    tokenizer.enter_link(
        Name::Data,
        Link {
            previous: None,
            next: None,
            content: Content::Text,
        },
    );

    if tokenizer.tokenize_state.connect {
        let index = tokenizer.events.len() - 1;
        link(&mut tokenizer.events, index);
    } else {
        tokenizer.tokenize_state.connect = true;
    }

    State::Retry(StateName::ParagraphInside)
}

/// In paragraph.
///
/// ```markdown
/// > | abc
///     ^^^
/// ```
pub fn inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None => {
            tokenizer.tokenize_state.connect = false;
            tokenizer.exit(Name::Data);
            tokenizer.exit(Name::Paragraph);
            State::Ok
        }
        Some(b'\n') => {
            tokenizer.consume();
            tokenizer.exit(Name::Data);
            State::Next(StateName::ParagraphLineStart)
        }
        _ => {
            tokenizer.consume_line_run();
            State::Next(StateName::ParagraphInside)
        }
    }
}
