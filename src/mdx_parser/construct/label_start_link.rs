//! Label start (link) occurs in the [text][] content type.
//!
//! ## Grammar
//!
//! Label start (link) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! label_start_link ::= '['
//! ```
//!
//! ## HTML
//!
//! Label start (link) does not, on its own, relate to anything in HTML.
//! When matched with a [label end][label_end], they together relate to the
//! `<a>` element in HTML.
//! See [*§ 4.5.1 The `a` element*][html_a] in the HTML spec for more info.
//! Without an end, the character (`[`) is output.
//!
//! ## Tokens
//!
//! * [`LabelLink`][Name::LabelLink]
//! * [`LabelMarker`][Name::LabelMarker]
//!
//! ## References
//!
//! * [`label-start-link.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/label-start-link.js)
//! * [*§ 6.3 Links* in `CommonMark`](https://spec.commonmark.org/0.31/#links)
//!
//! [text]: crate::mdx_parser::construct::text
//! [label_end]: crate::mdx_parser::construct::label_end
//! [html_a]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-a-element

use crate::mdx_parser::event::Name;
use crate::mdx_parser::resolve::Name as ResolveName;
use crate::mdx_parser::state::State;
use crate::mdx_parser::tokenizer::{LabelKind, LabelStart, Tokenizer};

/// Start of label (link) start.
///
/// ```markdown
/// > | a [b] c
///       ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.parse_state.options.constructs.label_start_link && tokenizer.current == Some(b'[') {
        let start = tokenizer.events.len();
        tokenizer.enter(Name::LabelLink);
        tokenizer.enter(Name::LabelMarker);
        tokenizer.consume();
        tokenizer.exit(Name::LabelMarker);
        tokenizer.exit(Name::LabelLink);
        tokenizer.tokenize_state.label_starts.push(LabelStart {
            kind: LabelKind::Link,
            start: (start, tokenizer.events.len() - 1),
        });
        tokenizer.register_resolver_before(ResolveName::Label);
        State::Ok
    } else {
        State::Nok
    }
}
