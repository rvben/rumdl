//! Label end occurs in the [text][] content type.
//!
//! ## Grammar
//!
//! Label end forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! label_end ::= ']' [resource | reference_full | reference_collapsed]
//!
//! resource ::= '(' [space_or_tab_eol] destination [space_or_tab_eol title] [space_or_tab_eol] ')'
//! reference_full ::= '[' label ']'
//! reference_collapsed ::= '[' ']'
//!
//! ; See the `destination`, `title`, and `label` constructs for the BNF of
//! ; those parts.
//! ```
//!
//! See [`destination`][destination], [`label`][label], and [`title`][title]
//! for grammar, notes, and recommendations on each part.
//!
//! In the case of a resource, the destination and title are given directly
//! with the label end.
//! In the case of a reference, this information is provided by a matched
//! [definition][].
//! Full references (`[x][y]`) match to definitions through their explicit,
//! second, label (`y`).
//! Collapsed references (`[x][]`) and shortcut references (`[x]`) match by
//! interpreting the text provided between the first, implicit, label (`x`).
//! To match, the effective label of the reference must be equal to the label
//! of the definition after normalizing with
//! [`normalize_identifier`][].
//!
//! Importantly, while the label of a full reference *can* include [string][]
//! content, and in case of collapsed and shortcut references even [text][]
//! content, that content is not considered when matching.
//! To illustrate, neither label matches the definition:
//!
//! ```markdown
//! [a&b]: https://example.com
//!
//! [x][a&amp;b], [a\&b][]
//! ```
//!
//! When the resource or reference matches, the destination forms the `href`
//! attribute in case of a [label start (link)][label_start_link], and an
//! `src` attribute in case of a [label start (image)][label_start_image].
//! The title is formed, optionally, on either `<a>` or `<img>`.
//! When matched with a [gfm label start (footnote)][gfm_label_start_footnote],
//! no reference or resource can follow the label end.
//!
//! For info on how to encode characters in URLs, see
//! [`destination`][destination].
//! For info on how characters are encoded as `href` on `<a>` or `src` on
//! `<img>` when compiling, see
//! [`sanitize_uri`][sanitize_uri].
//!
//! In case of a matched [gfm label start (footnote)][gfm_label_start_footnote],
//! a counter is injected.
//! In case of a matched [label start (link)][label_start_link], the interpreted
//! content between it and the label end, is placed between the opening and
//! closing tags.
//! In case of a matched [label start (image)][label_start_image], the text is
//! also interpreted, but used *without* the resulting tags:
//!
//! ```markdown
//! [a *b* c](#)
//!
//! ![a *b* c](#)
//! ```
//!
//! Yields:
//!
//! ```html
//! <p><a href="#">a <em>b</em> c</a></p>
//! <p><img src="#" alt="a b c" /></p>
//! ```
//!
//! It is possible to use images in links.
//! It’s somewhat possible to have links in images (the text will be used, not
//! the HTML, see above).
//! But it’s not possible to use links (or footnotes, which result in links)
//! in links.
//! The “deepest” link (or footnote) wins.
//! To illustrate:
//!
//! ```markdown
//! a [b [c](#) d](#) e
//! ```
//!
//! Yields:
//!
//! ```html
//! <p>a [b <a href="#">c</a> d](#) e</p>
//! ```
//!
//! This limitation is imposed because links in links is invalid according to
//! HTML.
//! Technically though, in markdown it is still possible to construct them by
//! using an [autolink][] in a link.
//! You definitely should not do that.
//!
//! ## HTML
//!
//! Label end does not, on its own, relate to anything in HTML.
//! When matched with a [label start (link)][label_start_link], they together
//! relate to the `<a>` element in HTML.
//! See [*§ 4.5.1 The `a` element*][html_a] in the HTML spec for more info.
//! It can also match with [label start (image)][label_start_image], in which
//! case they form an `<img>` element.
//! See [*§ 4.8.3 The `img` element*][html_img] in the HTML spec for more info.
//! It can also match with [gfm label start (footnote)][gfm_label_start_footnote],
//! in which case they form `<sup>` and `<a>` elements in HTML.
//! See [*§ 4.5.19 The `sub` and `sup` elements*][html_sup] and
//! [*§ 4.5.1 The `a` element*][html_a] in the HTML spec for more info.
//!
//! ## Recommendation
//!
//! It is recommended to use labels for links instead of [autolinks][autolink].
//! Labels allow more characters in URLs, and allow relative URLs and `www.`
//! URLs.
//! They also allow for descriptive text to explain the URL in prose.
//!
//! In footnotes, it’s recommended to use words instead of numbers (or letters
//! or anything with an order) as calls.
//! That makes it easier to reuse and reorder footnotes.
//!
//! ## Tokens
//!
//! * [`Data`][Name::Data]
//! * [`GfmFootnoteCall`][Name::GfmFootnoteCall]
//! * [`Image`][Name::Image]
//! * [`Label`][Name::Label]
//! * [`LabelEnd`][Name::LabelEnd]
//! * [`LabelMarker`][Name::LabelMarker]
//! * [`LabelText`][Name::LabelText]
//! * [`LineEnding`][Name::LineEnding]
//! * [`Link`][Name::Link]
//! * [`Reference`][Name::Reference]
//! * [`ReferenceMarker`][Name::ReferenceMarker]
//! * [`ReferenceString`][Name::ReferenceString]
//! * [`Resource`][Name::Resource]
//! * [`ResourceDestination`][Name::ResourceDestination]
//! * [`ResourceDestinationLiteral`][Name::ResourceDestinationLiteral]
//! * [`ResourceDestinationLiteralMarker`][Name::ResourceDestinationLiteralMarker]
//! * [`ResourceDestinationRaw`][Name::ResourceDestinationRaw]
//! * [`ResourceDestinationString`][Name::ResourceDestinationString]
//! * [`ResourceMarker`][Name::ResourceMarker]
//! * [`ResourceTitle`][Name::ResourceTitle]
//! * [`ResourceTitleMarker`][Name::ResourceTitleMarker]
//! * [`ResourceTitleString`][Name::ResourceTitleString]
//! * [`SpaceOrTab`][Name::SpaceOrTab]
//!
//! ## References
//!
//! * [`label-end.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/label-end.js)
//! * [`micromark-extension-gfm-task-list-item`](https://github.com/micromark/micromark-extension-gfm-footnote)
//! * [*§ 4.7 Link reference definitions* in `CommonMark`](https://spec.commonmark.org/0.31/#link-reference-definitions)
//! * [*§ 6.3 Links* in `CommonMark`](https://spec.commonmark.org/0.31/#links)
//! * [*§ 6.4 Images* in `CommonMark`](https://spec.commonmark.org/0.31/#images)
//!
//! > 👉 **Note**: Footnotes are not specified in GFM yet.
//! > See [`github/cmark-gfm#270`](https://github.com/github/cmark-gfm/issues/270)
//! > for the related issue.
//!
//! [string]: crate::mdx_parser::construct::string
//! [text]: crate::mdx_parser::construct::text
//! [destination]: crate::mdx_parser::construct::partial_destination
//! [title]: crate::mdx_parser::construct::partial_title
//! [label]: crate::mdx_parser::construct::partial_label
//! [label_start_image]: crate::mdx_parser::construct::label_start_image
//! [label_start_link]: crate::mdx_parser::construct::label_start_link
//! [gfm_label_start_footnote]: crate::mdx_parser::construct::gfm_label_start_footnote
//! [definition]: crate::mdx_parser::construct::definition
//! [autolink]: crate::mdx_parser::construct::autolink
//! [sanitize_uri]: crate::mdx_parser::util::sanitize_uri::sanitize
//! [normalize_identifier]: crate::mdx_parser::util::normalize_identifier::normalize_identifier
//! [html_a]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-a-element
//! [html_img]: https://html.spec.whatwg.org/multipage/embedded-content.html#the-img-element
//! [html_sup]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-sub-and-sup-elements

use crate::mdx_parser::construct::partial_space_or_tab_eol::space_or_tab_eol;
use crate::mdx_parser::event::{Event, Kind, Name};
use crate::mdx_parser::resolve::Name as ResolveName;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::Subresult;
use crate::mdx_parser::tokenizer::{Label, LabelKind, LabelStart, Tokenizer};
use crate::mdx_parser::util::{
    constant::RESOURCE_DESTINATION_BALANCE_MAX,
    normalize_identifier::normalize_identifier,
    skip,
    slice::{Position, Slice},
};
use alloc::vec;

/// Start of label end.
///
/// ```markdown
/// > | [a](b) c
///       ^
/// > | [a][b] c
///       ^
/// > | [a][] b
///       ^
/// > | [a] b
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if Some(b']') == tokenizer.current && tokenizer.parse_state.options.constructs.label_end {
        // If there is an okay opening:
        if !tokenizer.tokenize_state.label_starts.is_empty() {
            let index = tokenizer.tokenize_state.label_starts.len() - 1;
            let label_start = &tokenizer.tokenize_state.label_starts[index];

            tokenizer.tokenize_state.end = tokenizer.events.len();

            // If the corresponding label (link) start is marked as inactive,
            // it means we’d be wrapping a link, like this:
            //
            // ```markdown
            // > | a [b [c](d) e](f) g.
            //                  ^
            // ```
            //
            // We can’t have that, so it’s just balanced brackets.
            if label_start.kind != LabelKind::Image && index < tokenizer.tokenize_state.label_starts_inactive {
                return State::Retry(StateName::LabelEndNok);
            }

            tokenizer.enter(Name::LabelEnd);
            tokenizer.enter(Name::LabelMarker);
            tokenizer.consume();
            tokenizer.exit(Name::LabelMarker);
            tokenizer.exit(Name::LabelEnd);
            return State::Next(StateName::LabelEndAfter);
        }
    }

    State::Nok
}

/// After `]`.
///
/// ```markdown
/// > | [a](b) c
///       ^
/// > | [a][b] c
///       ^
/// > | [a][] b
///       ^
/// > | [a] b
///       ^
/// ```
pub fn after(tokenizer: &mut Tokenizer) -> State {
    let start_index = tokenizer.tokenize_state.label_starts.len() - 1;
    let start = &tokenizer.tokenize_state.label_starts[start_index];
    let is_footnote = start.kind == LabelKind::GfmFootnote;
    let parse_state = tokenizer.parse_state;

    // With character escapes, a label with another label start in it cannot
    // be defined; without them, text also starts labels at `\[`.
    let may_be_defined = !(parse_state.options.constructs.character_escape
        && has_label_start_after(tokenizer, start.start.0))
        && (!parse_state.definitions.is_empty() || (is_footnote && !parse_state.gfm_footnote_definitions.is_empty()));

    let indices = (
        tokenizer.events[start.start.1].point.index,
        tokenizer.events[tokenizer.tokenize_state.end].point.index,
    );

    let mut id = if may_be_defined {
        // We don’t care about virtual spaces, so `indices` and `as_str` are fine.
        normalize_identifier(Slice::from_indices(parse_state.bytes, indices.0, indices.1).as_str())
    } else {
        String::new()
    };

    // See if this matches a footnote definition.
    if is_footnote {
        if may_be_defined && parse_state.gfm_footnote_definitions.binary_search(&id).is_ok() {
            return State::Retry(StateName::LabelEndOk);
        }

        // Nope, this might be a normal link?
        tokenizer.tokenize_state.label_starts[start_index].kind = LabelKind::GfmUndefinedFootnote;
        // Include the caret, as definitions do.
        id = normalize_identifier(Slice::from_indices(tokenizer.parse_state.bytes, indices.0 - 1, indices.1).as_str());
    }

    let defined = may_be_defined && parse_state.definitions.binary_search(&id).is_ok();

    match tokenizer.current {
        // Resource (`[asd](fgh)`)?
        Some(b'(') => {
            tokenizer.attempt(
                State::Next(StateName::LabelEndOk),
                State::Next(if defined {
                    StateName::LabelEndOk
                } else {
                    StateName::LabelEndNok
                }),
            );
            State::Retry(StateName::LabelEndResourceStart)
        }
        // Full (`[asd][fgh]`) or collapsed (`[asd][]`) reference?
        Some(b'[') => {
            tokenizer.attempt(
                State::Next(StateName::LabelEndOk),
                State::Next(if defined {
                    StateName::LabelEndReferenceNotFull
                } else {
                    StateName::LabelEndNok
                }),
            );
            State::Retry(StateName::LabelEndReferenceFull)
        }
        // Shortcut (`[asd]`) reference?
        _ => State::Retry(if defined {
            StateName::LabelEndOk
        } else {
            StateName::LabelEndNok
        }),
    }
}

/// After `]`, at `[`, but not at a full reference.
///
/// > 👉 **Note**: we only get here if the label is defined.
///
/// ```markdown
/// > | [a][] b
///        ^
/// > | [a] b
///        ^
/// ```
pub fn reference_not_full(tokenizer: &mut Tokenizer) -> State {
    tokenizer.attempt(State::Next(StateName::LabelEndOk), State::Next(StateName::LabelEndNok));
    State::Retry(StateName::LabelEndReferenceCollapsed)
}

/// Done, we found something.
///
/// ```markdown
/// > | [a](b) c
///           ^
/// > | [a][b] c
///           ^
/// > | [a][] b
///          ^
/// > | [a] b
///        ^
/// ```
pub fn ok(tokenizer: &mut Tokenizer) -> State {
    // Remove the start.
    let label_start = tokenizer.tokenize_state.label_starts.pop().unwrap();

    // If this is a link or footnote, we need to mark earlier link starts as no
    // longer viable for use (as they would otherwise contain a link).
    // These link starts are still looking for balanced closing brackets, so
    // we can’t remove them, but we can mark them: all at once.
    let len = tokenizer.tokenize_state.label_starts.len();
    tokenizer.tokenize_state.label_starts_inactive = if label_start.kind == LabelKind::Image {
        tokenizer.tokenize_state.label_starts_inactive.min(len)
    } else {
        len
    };

    tokenizer.tokenize_state.labels.push(Label {
        kind: label_start.kind,
        start: label_start.start,
        end: (tokenizer.tokenize_state.end, tokenizer.events.len() - 1),
    });
    tokenizer.tokenize_state.end = 0;
    tokenizer.register_resolver_before(ResolveName::Label);
    State::Ok
}

/// Whether a label start was pushed after the one at event `start`.
fn has_label_start_after(tokenizer: &Tokenizer, start: usize) -> bool {
    let state = &tokenizer.tokenize_state;
    state.labels.last().map_or(false, |label| label.start.0 > start)
        || state
            .label_starts_loose
            .last()
            .map_or(false, |loose| loose.start.0 > start)
}

/// Done, it’s nothing.
///
/// There was an okay opening, but we didn’t match anything.
///
/// ```markdown
/// > | [a](b c
///        ^
/// > | [a][b c
///        ^
/// > | [a] b
///        ^
/// ```
pub fn nok(tokenizer: &mut Tokenizer) -> State {
    let start = tokenizer.tokenize_state.label_starts.pop().unwrap();
    let len = tokenizer.tokenize_state.label_starts.len();
    tokenizer.tokenize_state.label_starts_inactive = tokenizer.tokenize_state.label_starts_inactive.min(len);
    tokenizer.tokenize_state.label_starts_loose.push(start);
    tokenizer.tokenize_state.end = 0;
    State::Nok
}

/// At a resource.
///
/// ```markdown
/// > | [a](b) c
///        ^
/// ```
pub fn resource_start(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'(') => {
            tokenizer.enter(Name::Resource);
            tokenizer.enter(Name::ResourceMarker);
            tokenizer.consume();
            tokenizer.exit(Name::ResourceMarker);
            State::Next(StateName::LabelEndResourceBefore)
        }
        _ => unreachable!("expected `(`"),
    }
}

/// In resource, after `(`, at optional whitespace.
///
/// ```markdown
/// > | [a](b) c
///         ^
/// ```
pub fn resource_before(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, Some(b'\t' | b'\n' | b' ')) {
        tokenizer.attempt(
            State::Next(StateName::LabelEndResourceOpen),
            State::Next(StateName::LabelEndResourceOpen),
        );
        State::Retry(space_or_tab_eol(tokenizer))
    } else {
        State::Retry(StateName::LabelEndResourceOpen)
    }
}

/// In resource, after optional whitespace, at `)` or a destination.
///
/// ```markdown
/// > | [a](b) c
///         ^
/// ```
pub fn resource_open(tokenizer: &mut Tokenizer) -> State {
    if let Some(b')') = tokenizer.current {
        State::Retry(StateName::LabelEndResourceEnd)
    } else {
        tokenizer.tokenize_state.token_1 = Name::ResourceDestination;
        tokenizer.tokenize_state.token_2 = Name::ResourceDestinationLiteral;
        tokenizer.tokenize_state.token_3 = Name::ResourceDestinationLiteralMarker;
        tokenizer.tokenize_state.token_4 = Name::ResourceDestinationRaw;
        tokenizer.tokenize_state.token_5 = Name::ResourceDestinationString;
        tokenizer.tokenize_state.size_b = RESOURCE_DESTINATION_BALANCE_MAX;

        tokenizer.attempt(
            State::Next(StateName::LabelEndResourceDestinationAfter),
            State::Next(StateName::LabelEndResourceDestinationMissing),
        );
        State::Retry(StateName::DestinationStart)
    }
}

/// In resource, after destination, at optional whitespace.
///
/// ```markdown
/// > | [a](b) c
///          ^
/// ```
pub fn resource_destination_after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::Data;
    tokenizer.tokenize_state.token_2 = Name::Data;
    tokenizer.tokenize_state.token_3 = Name::Data;
    tokenizer.tokenize_state.token_4 = Name::Data;
    tokenizer.tokenize_state.token_5 = Name::Data;
    tokenizer.tokenize_state.size_b = 0;

    if matches!(tokenizer.current, Some(b'\t' | b'\n' | b' ')) {
        tokenizer.attempt(
            State::Next(StateName::LabelEndResourceBetween),
            State::Next(StateName::LabelEndResourceEnd),
        );
        State::Retry(space_or_tab_eol(tokenizer))
    } else {
        State::Retry(StateName::LabelEndResourceEnd)
    }
}

/// At invalid destination.
///
/// ```markdown
/// > | [a](<<) b
///         ^
/// ```
pub fn resource_destination_missing(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::Data;
    tokenizer.tokenize_state.token_2 = Name::Data;
    tokenizer.tokenize_state.token_3 = Name::Data;
    tokenizer.tokenize_state.token_4 = Name::Data;
    tokenizer.tokenize_state.token_5 = Name::Data;
    tokenizer.tokenize_state.size_b = 0;
    State::Nok
}

/// In resource, after destination and whitespace, at `(` or title.
///
/// ```markdown
/// > | [a](b ) c
///           ^
/// ```
pub fn resource_between(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'"' | b'\'' | b'(') => {
            tokenizer.tokenize_state.token_1 = Name::ResourceTitle;
            tokenizer.tokenize_state.token_2 = Name::ResourceTitleMarker;
            tokenizer.tokenize_state.token_3 = Name::ResourceTitleString;
            tokenizer.attempt(State::Next(StateName::LabelEndResourceTitleAfter), State::Nok);
            State::Retry(StateName::TitleStart)
        }
        _ => State::Retry(StateName::LabelEndResourceEnd),
    }
}

/// In resource, after title, at optional whitespace.
///
/// ```markdown
/// > | [a](b "c") d
///              ^
/// ```
pub fn resource_title_after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::Data;
    tokenizer.tokenize_state.token_2 = Name::Data;
    tokenizer.tokenize_state.token_3 = Name::Data;

    if matches!(tokenizer.current, Some(b'\t' | b'\n' | b' ')) {
        tokenizer.attempt(
            State::Next(StateName::LabelEndResourceEnd),
            State::Next(StateName::LabelEndResourceEnd),
        );
        State::Retry(space_or_tab_eol(tokenizer))
    } else {
        State::Retry(StateName::LabelEndResourceEnd)
    }
}

/// In resource, at `)`.
///
/// ```markdown
/// > | [a](b) d
///          ^
/// ```
pub fn resource_end(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b')') => {
            tokenizer.enter(Name::ResourceMarker);
            tokenizer.consume();
            tokenizer.exit(Name::ResourceMarker);
            tokenizer.exit(Name::Resource);
            State::Ok
        }
        _ => State::Nok,
    }
}

/// In reference (full), at `[`.
///
/// ```markdown
/// > | [a][b] d
///        ^
/// ```
pub fn reference_full(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'[') => {
            tokenizer.tokenize_state.token_1 = Name::Reference;
            tokenizer.tokenize_state.token_2 = Name::ReferenceMarker;
            tokenizer.tokenize_state.token_3 = Name::ReferenceString;
            tokenizer.attempt(
                State::Next(StateName::LabelEndReferenceFullAfter),
                State::Next(StateName::LabelEndReferenceFullMissing),
            );
            State::Retry(StateName::LabelStart)
        }
        _ => unreachable!("expected `[`"),
    }
}

/// In reference (full), after `]`.
///
/// ```markdown
/// > | [a][b] d
///          ^
/// ```
pub fn reference_full_after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::Data;
    tokenizer.tokenize_state.token_2 = Name::Data;
    tokenizer.tokenize_state.token_3 = Name::Data;

    if tokenizer
        .parse_state
        .definitions
        // We don’t care about virtual spaces, so `as_str` is fine.
        .binary_search(&normalize_identifier(
            Slice::from_position(
                tokenizer.parse_state.bytes,
                &Position::from_exit_event(
                    &tokenizer.events,
                    skip::to_back(
                        &tokenizer.events,
                        tokenizer.events.len() - 1,
                        &[Name::ReferenceString],
                    ),
                ),
            )
            .as_str(),
        ))
        .is_ok()
    {
        State::Ok
    } else {
        State::Nok
    }
}

/// In reference (full) that was missing.
///
/// ```markdown
/// > | [a][b d
///        ^
/// ```
pub fn reference_full_missing(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.token_1 = Name::Data;
    tokenizer.tokenize_state.token_2 = Name::Data;
    tokenizer.tokenize_state.token_3 = Name::Data;
    State::Nok
}

/// In reference (collapsed), at `[`.
///
/// > 👉 **Note**: we only get here if the label is defined.
///
/// ```markdown
/// > | [a][] d
///        ^
/// ```
pub fn reference_collapsed(tokenizer: &mut Tokenizer) -> State {
    // We only attempt a collapsed label if there’s a `[`.
    debug_assert_eq!(tokenizer.current, Some(b'['), "expected opening bracket");
    tokenizer.enter(Name::Reference);
    tokenizer.enter(Name::ReferenceMarker);
    tokenizer.consume();
    tokenizer.exit(Name::ReferenceMarker);
    State::Next(StateName::LabelEndReferenceCollapsedOpen)
}

/// In reference (collapsed), at `]`.
///
/// > 👉 **Note**: we only get here if the label is defined.
///
/// ```markdown
/// > | [a][] d
///         ^
/// ```
pub fn reference_collapsed_open(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b']') => {
            tokenizer.enter(Name::ReferenceMarker);
            tokenizer.consume();
            tokenizer.exit(Name::ReferenceMarker);
            tokenizer.exit(Name::Reference);
            State::Ok
        }
        _ => State::Nok,
    }
}

/// Resolve images, links, and footnotes.
///
/// This turns matching label starts and label ends into links, images, and
/// footnotes, and turns unmatched label starts back into data.
pub fn resolve(tokenizer: &mut Tokenizer) -> Option<Subresult> {
    // Inject labels.
    let labels = tokenizer.tokenize_state.labels.split_off(0);
    inject_labels(tokenizer, &labels);
    // Handle loose starts.
    let starts = tokenizer.tokenize_state.label_starts.split_off(0);
    tokenizer.tokenize_state.label_starts_inactive = 0;
    mark_as_data(tokenizer, &starts);
    let starts = tokenizer.tokenize_state.label_starts_loose.split_off(0);
    mark_as_data(tokenizer, &starts);

    tokenizer.map.consume(&mut tokenizer.events);
    None
}

/// Inject links/images/footnotes.
fn inject_labels(tokenizer: &mut Tokenizer, labels: &[Label]) {
    // Add grouping events.
    let mut index = 0;
    while index < labels.len() {
        let label = &labels[index];
        let group_name = if label.kind == LabelKind::GfmFootnote {
            Name::GfmFootnoteCall
        } else if label.kind == LabelKind::Image {
            Name::Image
        } else {
            Name::Link
        };

        // If this is a fine link, which starts with a footnote start that did
        // not match, we need to inject the caret as data.
        let mut caret = vec![];

        if label.kind == LabelKind::GfmUndefinedFootnote {
            // Add caret.
            caret.push(Event {
                kind: Kind::Enter,
                name: Name::Data,
                // Enter:GfmFootnoteCallMarker.
                point: tokenizer.events[label.start.1 - 2].point.clone().clone(),
                link: None,
            });
            caret.push(Event {
                kind: Kind::Exit,
                name: Name::Data,
                // Exit:GfmFootnoteCallMarker.
                point: tokenizer.events[label.start.1 - 1].point.clone(),
                link: None,
            });
            // Change and move label end.
            tokenizer.events[label.start.0].name = Name::LabelLink;
            tokenizer.events[label.start.1].name = Name::LabelLink;
            tokenizer.events[label.start.1].point = caret[0].point.clone();
            // Remove the caret.
            // Enter:GfmFootnoteCallMarker, Exit:GfmFootnoteCallMarker.
            tokenizer.map.add(label.start.1 - 2, 2, vec![]);
        }

        // Insert a group enter and label enter.
        tokenizer.map.add(
            label.start.0,
            0,
            vec![
                Event {
                    kind: Kind::Enter,
                    name: group_name.clone(),
                    point: tokenizer.events[label.start.0].point.clone(),
                    link: None,
                },
                Event {
                    kind: Kind::Enter,
                    name: Name::Label,
                    point: tokenizer.events[label.start.0].point.clone(),
                    link: None,
                },
            ],
        );

        // Empty events not allowed.
        // Though: if this was what looked like a footnote, but didn’t match,
        // it’s a link instead, and we need to inject the `^`.
        if label.start.1 != label.end.0 || !caret.is_empty() {
            tokenizer.map.add_before(
                label.start.1 + 1,
                0,
                vec![Event {
                    kind: Kind::Enter,
                    name: Name::LabelText,
                    point: tokenizer.events[label.start.1].point.clone(),
                    link: None,
                }],
            );
            tokenizer.map.add(
                label.end.0,
                0,
                vec![Event {
                    kind: Kind::Exit,
                    name: Name::LabelText,
                    point: tokenizer.events[label.end.0].point.clone(),
                    link: None,
                }],
            );
        }

        if !caret.is_empty() {
            tokenizer.map.add(label.start.1 + 1, 0, caret);
        }

        // Insert a label exit.
        tokenizer.map.add(
            label.end.0 + 4,
            0,
            vec![Event {
                kind: Kind::Exit,
                name: Name::Label,
                point: tokenizer.events[label.end.0 + 3].point.clone(),
                link: None,
            }],
        );

        // Insert a group exit.
        tokenizer.map.add(
            label.end.1 + 1,
            0,
            vec![Event {
                kind: Kind::Exit,
                name: group_name,
                point: tokenizer.events[label.end.1].point.clone(),
                link: None,
            }],
        );

        index += 1;
    }
}

/// Remove loose label starts.
fn mark_as_data(tokenizer: &mut Tokenizer, events: &[LabelStart]) {
    let mut index = 0;

    while index < events.len() {
        let data_enter_index = events[index].start.0;
        let data_exit_index = events[index].start.1;

        tokenizer.map.add(
            data_enter_index,
            data_exit_index - data_enter_index + 1,
            vec![
                Event {
                    kind: Kind::Enter,
                    name: Name::Data,
                    point: tokenizer.events[data_enter_index].point.clone(),
                    link: None,
                },
                Event {
                    kind: Kind::Exit,
                    name: Name::Data,
                    point: tokenizer.events[data_exit_index].point.clone(),
                    link: None,
                },
            ],
        );

        index += 1;
    }
}
