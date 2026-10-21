//! Definition occurs in the [content] content type.
//!
//! ## Grammar
//!
//! Definition forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! definition ::= label ':' [ space_or_tab_eol ] destination [ space_or_tab_eol title ] [ space_or_tab ]
//!
//! ; See the `destination`, `title`, and `label` constructs for the BNF of
//! ; those parts.
//! ```
//!
//! This construct must be followed by an eol (line ending) or eof (end of
//! file), like flow constructs.
//!
//! See [`destination`][destination], [`label`][label], and [`title`][title]
//! for grammar, notes, and recommendations on each part.
//!
//! The `destination`, `label`, and `title` parts are interpreted as the
//! [string][] content type.
//! That means that [character escapes][character_escape] and
//! [character references][character_reference] are allowed.
//!
//! Definitions match to references through identifiers.
//! To match, both labels must be equal after normalizing with
//! [`normalize_identifier`][normalize_identifier].
//! One definition can match to multiple references.
//! Multiple definitions with the same, normalized, identifier are ignored: the
//! first definition is preferred.
//! To illustrate, the definition with a destination of `x` wins:
//!
//! ```markdown
//! [a]: x
//! [a]: y
//!
//! [a]
//! ```
//!
//! Importantly, while labels *can* include [string][] content (character
//! escapes and character references), these are not considered when matching.
//! To illustrate, neither definition matches the reference:
//!
//! ```markdown
//! [a&amp;b]: x
//! [a\&b]: y
//!
//! [a&b]
//! ```
//!
//! For info on how to encode characters in URLs, see
//! [`destination`][destination].
//! For info on how characters are encoded as `href` on `<a>` or `src` on
//! `<img>` when compiling, see
//! [`sanitize_uri`][sanitize_uri].
//!
//! ## HTML
//!
//! Definitions in markdown do not, on their own, relate to anything in HTML.
//! When matched with a [label end (reference)][label_end], they together
//! relate to the `<a>` or `<img>` elements in HTML.
//! The definition forms its `href` or `src`, and optionally `title`,
//! attributes.
//! See [*§ 4.5.1 The `a` element*][html_a] and
//! [*§ 4.8.3 The `img` element*][html_img] in the HTML spec for more info.
//!
//! ## Tokens
//!
//! * [`Definition`][Name::Definition]
//! * [`DefinitionDestination`][Name::DefinitionDestination]
//! * [`DefinitionDestinationLiteral`][Name::DefinitionDestinationLiteral]
//! * [`DefinitionDestinationLiteralMarker`][Name::DefinitionDestinationLiteralMarker]
//! * [`DefinitionDestinationRaw`][Name::DefinitionDestinationRaw]
//! * [`DefinitionDestinationString`][Name::DefinitionDestinationString]
//! * [`DefinitionLabel`][Name::DefinitionLabel]
//! * [`DefinitionLabelMarker`][Name::DefinitionLabelMarker]
//! * [`DefinitionLabelString`][Name::DefinitionLabelString]
//! * [`DefinitionMarker`][Name::DefinitionMarker]
//! * [`DefinitionTitle`][Name::DefinitionTitle]
//! * [`DefinitionTitleMarker`][Name::DefinitionTitleMarker]
//! * [`DefinitionTitleString`][Name::DefinitionTitleString]
//! * [`LineEnding`][Name::LineEnding]
//! * [`SpaceOrTab`][Name::SpaceOrTab]
//!
//! ## References
//!
//! * [`definition.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/definition.js)
//! * [*§ 4.7 Link reference definitions* in `CommonMark`](https://spec.commonmark.org/0.31/#link-reference-definitions)
//!
//! [content]: crate::mdx_parser::construct::content
//! [string]: crate::mdx_parser::construct::string
//! [character_escape]: crate::mdx_parser::construct::character_escape
//! [character_reference]: crate::mdx_parser::construct::character_reference
//! [destination]: crate::mdx_parser::construct::partial_destination
//! [label]: crate::mdx_parser::construct::partial_label
//! [label_end]: crate::mdx_parser::construct::label_end
//! [title]: crate::mdx_parser::construct::partial_title
//! [sanitize_uri]: crate::mdx_parser::util::sanitize_uri::sanitize
//! [normalize_identifier]: crate::mdx_parser::util::normalize_identifier
//! [html_a]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-a-element
//! [html_img]: https://html.spec.whatwg.org/multipage/embedded-content.html#the-img-element

use crate::mdx_parser::construct::partial_space_or_tab::space_or_tab;
use crate::mdx_parser::construct::partial_space_or_tab_eol::space_or_tab_eol;
use crate::mdx_parser::event::Name;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::{
    normalize_identifier::normalize_identifier,
    skip,
    slice::{Position, Slice},
};

/// At start of a definition.
///
/// ```markdown
/// > | [a]: b "c"
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    // Do not interrupt paragraphs (but do follow definitions).
    if tokenizer.parse_state.options.constructs.definition
        && (!tokenizer.interrupt
            || (!tokenizer.events.is_empty()
                && tokenizer.events[skip::opt_back(
                    &tokenizer.events,
                    tokenizer.events.len() - 1,
                    &[Name::LineEnding, Name::SpaceOrTab],
                )]
                .name
                    == Name::Definition))
    {
        tokenizer.enter(Name::Definition);

        if matches!(tokenizer.current, Some(b'\t' | b' ')) {
            // Note: arbitrary whitespace allowed even if code (indented) is on.
            tokenizer.attempt(State::Next(StateName::DefinitionBefore), State::Nok);
            State::Retry(space_or_tab(tokenizer))
        } else {
            State::Retry(StateName::DefinitionBefore)
        }
    } else {
        State::Nok
    }
}

/// After optional whitespace, at `[`.
///
/// ```markdown
/// > | [a]: b "c"
///     ^
/// ```
pub fn before(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'[') => {
            tokenizer.tokenize_state.token_1 = Name::DefinitionLabel;
            tokenizer.tokenize_state.token_2 = Name::DefinitionLabelMarker;
            tokenizer.tokenize_state.token_3 = Name::DefinitionLabelString;
            tokenizer.attempt(
                State::Next(StateName::DefinitionLabelAfter),
                State::Next(StateName::DefinitionLabelNok),
            );
            State::Retry(StateName::LabelStart)
        }
        _ => State::Nok,
    }
}

/// After label.
///
/// ```markdown
/// > | [a]: b "c"
///        ^
/// ```
pub fn label_after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::Data;
    tokenizer.tokenize_state.token_2 = Name::Data;
    tokenizer.tokenize_state.token_3 = Name::Data;

    match tokenizer.current {
        Some(b':') => {
            tokenizer.tokenize_state.end = skip::to_back(
                &tokenizer.events,
                tokenizer.events.len() - 1,
                &[Name::DefinitionLabelString],
            );

            tokenizer.enter(Name::DefinitionMarker);
            tokenizer.consume();
            tokenizer.exit(Name::DefinitionMarker);
            State::Next(StateName::DefinitionMarkerAfter)
        }
        _ => State::Nok,
    }
}

/// At a non-label
///
/// ```markdown
/// > | []
///     ^
/// ```
pub fn label_nok(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::Data;
    tokenizer.tokenize_state.token_2 = Name::Data;
    tokenizer.tokenize_state.token_3 = Name::Data;
    State::Nok
}

/// After marker.
///
/// ```markdown
/// > | [a]: b "c"
///         ^
/// ```
pub fn marker_after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.attempt(
        State::Next(StateName::DefinitionDestinationBefore),
        State::Next(StateName::DefinitionDestinationBefore),
    );
    State::Retry(space_or_tab_eol(tokenizer))
}

/// Before destination.
///
/// ```markdown
/// > | [a]: b "c"
///          ^
/// ```
pub fn destination_before(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::DefinitionDestination;
    tokenizer.tokenize_state.token_2 = Name::DefinitionDestinationLiteral;
    tokenizer.tokenize_state.token_3 = Name::DefinitionDestinationLiteralMarker;
    tokenizer.tokenize_state.token_4 = Name::DefinitionDestinationRaw;
    tokenizer.tokenize_state.token_5 = Name::DefinitionDestinationString;
    tokenizer.tokenize_state.size_b = usize::MAX;
    tokenizer.attempt(
        State::Next(StateName::DefinitionDestinationAfter),
        State::Next(StateName::DefinitionDestinationMissing),
    );
    State::Retry(StateName::DestinationStart)
}

/// After destination.
///
/// ```markdown
/// > | [a]: b "c"
///           ^
/// ```
pub fn destination_after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::Data;
    tokenizer.tokenize_state.token_2 = Name::Data;
    tokenizer.tokenize_state.token_3 = Name::Data;
    tokenizer.tokenize_state.token_4 = Name::Data;
    tokenizer.tokenize_state.token_5 = Name::Data;
    tokenizer.tokenize_state.size_b = 0;
    tokenizer.attempt(
        State::Next(StateName::DefinitionAfter),
        State::Next(StateName::DefinitionAfter),
    );
    State::Retry(StateName::DefinitionTitleBefore)
}

/// Without destination.
pub fn destination_missing(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::Data;
    tokenizer.tokenize_state.token_2 = Name::Data;
    tokenizer.tokenize_state.token_3 = Name::Data;
    tokenizer.tokenize_state.token_4 = Name::Data;
    tokenizer.tokenize_state.token_5 = Name::Data;
    tokenizer.tokenize_state.size_b = 0;
    tokenizer.tokenize_state.end = 0;
    State::Nok
}

/// After definition.
///
/// ```markdown
/// > | [a]: b
///           ^
/// > | [a]: b "c"
///               ^
/// ```
pub fn after(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, Some(b'\t' | b' ')) {
        tokenizer.attempt(State::Next(StateName::DefinitionAfterWhitespace), State::Nok);
        State::Retry(space_or_tab(tokenizer))
    } else {
        State::Retry(StateName::DefinitionAfterWhitespace)
    }
}

/// After definition, after optional whitespace.
///
/// ```markdown
/// > | [a]: b
///           ^
/// > | [a]: b "c"
///               ^
/// ```
pub fn after_whitespace(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(Name::Definition);

            // Note: duplicates are removed by the parser, once per pass.
            tokenizer.tokenize_state.definitions.push(
                // Note: we don’t care about virtual spaces, so `as_str` is fine.
                normalize_identifier(
                    Slice::from_position(
                        tokenizer.parse_state.bytes,
                        &Position::from_exit_event(&tokenizer.events, tokenizer.tokenize_state.end),
                    )
                    .as_str(),
                ),
            );

            tokenizer.tokenize_state.end = 0;

            // You’d be interrupting.
            tokenizer.interrupt = true;
            State::Ok
        }
        _ => {
            tokenizer.tokenize_state.end = 0;
            State::Nok
        }
    }
}

/// After destination, at whitespace.
///
/// ```markdown
/// > | [a]: b
///           ^
/// > | [a]: b "c"
///           ^
/// ```
pub fn title_before(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, Some(b'\t' | b'\n' | b' ')) {
        tokenizer.attempt(State::Next(StateName::DefinitionTitleBeforeMarker), State::Nok);
        State::Retry(space_or_tab_eol(tokenizer))
    } else {
        State::Nok
    }
}

/// At title.
///
/// ```markdown
///   | [a]: b
/// > | "c"
///     ^
/// ```
pub fn title_before_marker(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::DefinitionTitle;
    tokenizer.tokenize_state.token_2 = Name::DefinitionTitleMarker;
    tokenizer.tokenize_state.token_3 = Name::DefinitionTitleString;
    tokenizer.attempt(State::Next(StateName::DefinitionTitleAfter), State::Nok);
    State::Retry(StateName::TitleStart)
}

/// After title.
///
/// ```markdown
/// > | [a]: b "c"
///               ^
/// ```
pub fn title_after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::Data;
    tokenizer.tokenize_state.token_2 = Name::Data;
    tokenizer.tokenize_state.token_3 = Name::Data;
    if matches!(tokenizer.current, Some(b'\t' | b' ')) {
        tokenizer.attempt(
            State::Next(StateName::DefinitionTitleAfterOptionalWhitespace),
            State::Nok,
        );
        State::Retry(space_or_tab(tokenizer))
    } else {
        State::Retry(StateName::DefinitionTitleAfterOptionalWhitespace)
    }
}

/// After title, after optional whitespace.
///
/// ```markdown
/// > | [a]: b "c"
///               ^
/// ```
pub fn title_after_optional_whitespace(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => State::Ok,
        _ => State::Nok,
    }
}
