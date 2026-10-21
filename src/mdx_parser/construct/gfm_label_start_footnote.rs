//! Label start (footnote) occurs in the [text][] content type.
//!
//! ## Grammar
//!
//! Label start (footnote) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! gfm_label_start_footnote ::= '[' '^'
//! ```
//!
//! ## HTML
//!
//! Label start (footnote) does not, on its own, relate to anything in HTML.
//! When matched with a [label end][label_end], they together relate to `<sup>`
//! and `<a>` elements in HTML.
//! See [*§ 4.5.19 The `sub` and `sup` elements*][html_sup] and
//! [*§ 4.5.1 The `a` element*][html_a] in the HTML spec for more info.
//! Without an end, the characters (`[^`) are output.
//!
//! ## Tokens
//!
//! * [`GfmFootnoteCallLabel`][Name::GfmFootnoteCallLabel]
//! * [`GfmFootnoteCallMarker`][Name::GfmFootnoteCallMarker]
//! * [`LabelMarker`][Name::LabelMarker]
//!
//! ## References
//!
//! * [`micromark-extension-gfm-footnote`](https://github.com/micromark/micromark-extension-gfm-footnote)
//!
//! > 👉 **Note**: Footnotes are not specified in GFM yet.
//! > See [`github/cmark-gfm#270`](https://github.com/github/cmark-gfm/issues/270)
//! > for the related issue.
//!
//! [text]: crate::mdx_parser::construct::text
//! [label_end]: crate::mdx_parser::construct::label_end
//! [html_a]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-a-element
//! [html_sup]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-sub-and-sup-elements

use crate::mdx_parser::event::Name;
use crate::mdx_parser::resolve::Name as ResolveName;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::{LabelKind, LabelStart, Tokenizer};

/// Start of label (footnote) start.
///
/// ```markdown
/// > | a [^b] c
///       ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.parse_state.options.constructs.gfm_label_start_footnote && tokenizer.current == Some(b'[') {
        tokenizer.enter(Name::GfmFootnoteCallLabel);
        tokenizer.enter(Name::LabelMarker);
        tokenizer.consume();
        tokenizer.exit(Name::LabelMarker);
        State::Next(StateName::GfmLabelStartFootnoteOpen)
    } else {
        State::Nok
    }
}

/// After `[`, at `^`.
///
/// ```markdown
/// > | a [^b] c
///        ^
/// ```
pub fn open(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'^') => {
            tokenizer.enter(Name::GfmFootnoteCallMarker);
            tokenizer.consume();
            tokenizer.exit(Name::GfmFootnoteCallMarker);
            tokenizer.exit(Name::GfmFootnoteCallLabel);
            tokenizer.tokenize_state.label_starts.push(LabelStart {
                kind: LabelKind::GfmFootnote,
                start: (tokenizer.events.len() - 6, tokenizer.events.len() - 1),
            });
            tokenizer.register_resolver_before(ResolveName::Label);
            State::Ok
        }
        _ => State::Nok,
    }
}
