//! Label start (image) occurs in the [text][] content type.
//!
//! ## Grammar
//!
//! Label start (image) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! label_start_image ::= '!' '['
//! ```
//!
//! ## HTML
//!
//! Label start (image) does not, on its own, relate to anything in HTML.
//! When matched with a [label end][label_end], they together relate to the
//! `<img>` element in HTML.
//! See [*§ 4.8.3 The `img` element*][html_img] in the HTML spec for more info.
//! Without an end, the characters (`![`) are output.
//!
//! ## Tokens
//!
//! * [`LabelImage`][Name::LabelImage]
//! * [`LabelImageMarker`][Name::LabelImageMarker]
//! * [`LabelMarker`][Name::LabelMarker]
//!
//! ## References
//!
//! * [`label-start-image.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/label-start-image.js)
//! * [*§ 6.4 Images* in `CommonMark`](https://spec.commonmark.org/0.31/#images)
//!
//! [text]: crate::mdx_parser::construct::text
//! [label_end]: crate::mdx_parser::construct::label_end
//! [html_img]: https://html.spec.whatwg.org/multipage/embedded-content.html#the-img-element

use crate::mdx_parser::event::Name;
use crate::mdx_parser::resolve::Name as ResolveName;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::{LabelKind, LabelStart, Tokenizer};

/// Start of label (image) start.
///
/// ```markdown
/// > | a ![b] c
///       ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.parse_state.options.constructs.label_start_image && tokenizer.current == Some(b'!') {
        tokenizer.enter(Name::LabelImage);
        tokenizer.enter(Name::LabelImageMarker);
        tokenizer.consume();
        tokenizer.exit(Name::LabelImageMarker);
        State::Next(StateName::LabelStartImageOpen)
    } else {
        State::Nok
    }
}

/// After `!`, at `[`.
///
/// ```markdown
/// > | a ![b] c
///        ^
/// ```
pub fn open(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'[') => {
            tokenizer.enter(Name::LabelMarker);
            tokenizer.consume();
            tokenizer.exit(Name::LabelMarker);
            tokenizer.exit(Name::LabelImage);
            State::Next(StateName::LabelStartImageAfter)
        }
        _ => State::Nok,
    }
}

/// After `![`.
///
/// ```markdown
/// > | a ![b] c
///         ^
/// ```
///
/// This is needed in because, when GFM footnotes are enabled, images never
/// form when started with a `^`.
/// Instead, links form:
///
/// ```markdown
/// ![^a](b)
///
/// ![^a][b]
///
/// [b]: c
/// ```
///
/// ```html
/// <p>!<a href=\"b\">^a</a></p>
/// <p>!<a href=\"c\">^a</a></p>
/// ```
pub fn after(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.parse_state.options.constructs.gfm_label_start_footnote && tokenizer.current == Some(b'^') {
        State::Nok
    } else {
        tokenizer.tokenize_state.label_starts.push(LabelStart {
            kind: LabelKind::Image,
            start: (tokenizer.events.len() - 6, tokenizer.events.len() - 1),
        });
        tokenizer.register_resolver_before(ResolveName::Label);
        State::Ok
    }
}
