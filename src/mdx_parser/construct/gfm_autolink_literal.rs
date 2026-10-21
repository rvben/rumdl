//! GFM: autolink literal occurs in the [text][] content type.
//!
//! ## Grammar
//!
//! Autolink literals form with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! gfm_autolink_literal ::= gfm_protocol_autolink | gfm_www_autolink | gfm_email_autolink
//!
//! ; Restriction: the code before must be `www_autolink_before`.
//! ; Restriction: the code after `.` must not be eof.
//! www_autolink ::= 3('w' | 'W') '.' [domain [path]]
//! www_autolink_before ::= eof | eol | space_or_tab | '(' | '*' | '_' | '[' | ']' | '~'
//!
//! ; Restriction: the code before must be `http_autolink_before`.
//! ; Restriction: the code after the protocol must be `http_autolink_protocol_after`.
//! http_autolink ::= ('h' | 'H') 2('t' | 'T') ('p' | 'P') ['s' | 'S'] ':' 2'/' domain [path]
//! http_autolink_before ::= byte - ascii_alpha
//! http_autolink_protocol_after ::= byte - eof - eol - ascii_control - unicode_whitespace - unicode_punctuation
//!
//! ; Restriction: the code before must be `email_autolink_before`.
//! ; Restriction: `ascii_digit` may not occur in the last label part of the label.
//! email_autolink ::= 1*('+' | '-' | '.' | '_' | ascii_alphanumeric) '@' 1*(1*label_segment label_dot_cont) 1*label_segment
//! email_autolink_before ::= byte - ascii_alpha - '/'
//!
//! ; Restriction: `_` may not occur in the last two domain parts.
//! domain ::= 1*(url_ampt_cont | domain_punct_cont | '-' | byte - eof - ascii_control - unicode_whitespace - unicode_punctuation)
//! ; Restriction: must not be followed by `punct`.
//! domain_punct_cont ::= '.' | '_'
//! ; Restriction: must not be followed by `char-ref`.
//! url_ampt_cont ::= '&'
//!
//! ; Restriction: a counter `balance = 0` is increased for every `(`, and decreased for every `)`.
//! ; Restriction: `)` must not be `paren_at_end`.
//! path ::= 1*(url_ampt_cont | path_punctuation_cont | '(' | ')' | byte - eof - eol - space_or_tab)
//! ; Restriction: must not be followed by `punct`.
//! path_punctuation_cont ::= trailing_punctuation - '<'
//! ; Restriction: must be followed by `punct` and `balance` must be less than `0`.
//! paren_at_end ::= ')'
//!
//! label_segment ::= label_dash_underscore_cont | ascii_alpha | ascii_digit
//! ; Restriction: if followed by `punct`, the whole email autolink is invalid.
//! label_dash_underscore_cont ::= '-' | '_'
//! ; Restriction: must not be followed by `punct`.
//! label_dot_cont ::= '.'
//!
//! punct ::= *trailing_punctuation ( byte - eof - eol - space_or_tab - '<' )
//! char_ref ::= *ascii_alpha ';' path_end
//! trailing_punctuation ::= '!' | '"' | '\'' | ')' | '*' | ',' | '.' | ':' | ';' | '<' | '?' | '_' | '~'
//! ```
//!
//! The grammar for GFM autolink literal is very relaxed: basically anything
//! except for whitespace is allowed after a prefix.
//! To use whitespace characters and otherwise impossible characters, in URLs,
//! you can use percent encoding:
//!
//! ```markdown
//! https://example.com/alpha%20bravo
//! ```
//!
//! Yields:
//!
//! ```html
//! <p><a href="https://example.com/alpha%20bravo">https://example.com/alpha%20bravo</a></p>
//! ```
//!
//! There are several cases where incorrect encoding of URLs would, in other
//! languages, result in a parse error.
//! In markdown, there are no errors, and URLs are normalized.
//! In addition, many characters are percent encoded
//! ([`sanitize_uri`][sanitize_uri]).
//! For example:
//!
//! ```markdown
//! www.a👍b%
//! ```
//!
//! Yields:
//!
//! ```html
//! <p><a href="http://www.a%F0%9F%91%8Db%25">www.a👍b%</a></p>
//! ```
//!
//! There is a big difference between how www and protocol literals work
//! compared to how email literals work.
//! The first two are done when parsing, and work like anything else in
//! markdown.
//! But email literals are handled afterwards: when everything is parsed, we
//! look back at the events to figure out if there were email addresses.
//! This particularly affects how they interleave with character escapes and
//! character references.
//!
//! ## HTML
//!
//! GFM autolink literals relate to the `<a>` element in HTML.
//! See [*§ 4.5.1 The `a` element*][html_a] in the HTML spec for more info.
//! When an email autolink is used, the string `mailto:` is prepended when
//! generating the `href` attribute of the hyperlink.
//! When a www autolink is used, the string `http:` is prepended.
//!
//! ## Recommendation
//!
//! It is recommended to use labels ([label start link][label_start_link],
//! [label end][label_end]), either with a resource or a definition
//! ([definition][]), instead of autolink literals, as those allow relative
//! URLs and descriptive text to explain the URL in prose.
//!
//! ## Bugs
//!
//! GitHub’s own algorithm to parse autolink literals contains three bugs.
//! A smaller bug is left unfixed in this project for consistency.
//! Two main bugs are not present in this project.
//! The issues relating to autolink literals are:
//!
//! * [GFM autolink extension (`www.`, `https?://` parts): links don’t work when after bracket](https://github.com/github/cmark-gfm/issues/278)\
//!   fixed here ✅
//! * [GFM autolink extension (`www.` part): uppercase does not match on issues/PRs/comments](https://github.com/github/cmark-gfm/issues/280)\
//!   fixed here ✅
//! * [GFM autolink extension (`www.` part): the word `www` matches](https://github.com/github/cmark-gfm/issues/279)\
//!   present here for consistency
//!
//! ## Tokens
//!
//! * [`GfmAutolinkLiteralEmail`][Name::GfmAutolinkLiteralEmail]
//! * [`GfmAutolinkLiteralMailto`][Name::GfmAutolinkLiteralMailto]
//! * [`GfmAutolinkLiteralProtocol`][Name::GfmAutolinkLiteralProtocol]
//! * [`GfmAutolinkLiteralWww`][Name::GfmAutolinkLiteralWww]
//! * [`GfmAutolinkLiteralXmpp`][Name::GfmAutolinkLiteralXmpp]
//!
//! ## References
//!
//! * [`micromark-extension-gfm-autolink-literal`](https://github.com/micromark/micromark-extension-gfm-autolink-literal)
//! * [*§ 6.9 Autolinks (extension)* in `GFM`](https://github.github.com/gfm/#autolinks-extension-)
//!
//! > 👉 **Note**: `mailto:` and `xmpp:` protocols before email autolinks were
//! > added in `cmark-gfm@0.29.0.gfm.5` and are as of yet undocumented.
//!
//! [text]: crate::mdx_parser::construct::text
//! [definition]: crate::mdx_parser::construct::definition
//! [attention]: crate::mdx_parser::construct::attention
//! [label_start_link]: crate::mdx_parser::construct::label_start_link
//! [label_end]: crate::mdx_parser::construct::label_end
//! [sanitize_uri]: crate::mdx_parser::util::sanitize_uri
//! [html_a]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-a-element

use crate::mdx_parser::event::{Event, Kind, Name};
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::{
    char::{Kind as CharacterKind, kind_after_index},
    slice::{Position, Slice},
};
use alloc::vec::Vec;

/// Whether a protocol literal can start after `previous`: not in a word.
pub fn protocol_may_start(previous: Option<u8>) -> bool {
    // Source: <https://github.com/github/cmark-gfm/blob/ef1cfcb/extensions/autolink.c#L214>.
    !matches!(previous, Some(b'A'..=b'Z' | b'a'..=b'z'))
}

/// Whether a `www.` literal can start after `previous`.
pub fn www_may_start(previous: Option<u8>) -> bool {
    // Source: <https://github.com/github/cmark-gfm/blob/ef1cfcb/extensions/autolink.c#L156>.
    matches!(
        previous,
        None | Some(b'\t' | b'\n' | b' ' | b'(' | b'*' | b'_' | b'[' | b']' | b'~')
    )
}

/// Start of protocol autolink literal.
///
/// ```markdown
/// > | https://example.com/a?b#c
///     ^
/// ```
pub fn protocol_start(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.gfm_autolink_literal_www_start = usize::MAX;
    if tokenizer.parse_state.options.constructs.gfm_autolink_literal
        && matches!(tokenizer.current, Some(b'H' | b'h'))
        && protocol_may_start(tokenizer.previous)
    {
        tokenizer.enter(Name::GfmAutolinkLiteralProtocol);
        tokenizer.attempt(State::Next(StateName::GfmAutolinkLiteralProtocolAfter), State::Nok);
        tokenizer.attempt(State::Next(StateName::GfmAutolinkLiteralDomainInside), State::Nok);
        tokenizer.tokenize_state.start = tokenizer.point.index;
        State::Retry(StateName::GfmAutolinkLiteralProtocolPrefixInside)
    } else {
        State::Nok
    }
}

/// After a protocol autolink literal.
///
/// ```markdown
/// > | https://example.com/a?b#c
///                              ^
/// ```
pub fn protocol_after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.exit(Name::GfmAutolinkLiteralProtocol);
    State::Ok
}

/// In protocol.
///
/// ```markdown
/// > | https://example.com/a?b#c
///     ^^^^^
/// ```
pub fn protocol_prefix_inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'A'..=b'Z' | b'a'..=b'z')
            // `5` is size of `https`
            if tokenizer.point.index - tokenizer.tokenize_state.start < 5 =>
        {
            tokenizer.consume();
            State::Next(StateName::GfmAutolinkLiteralProtocolPrefixInside)
        }
        Some(b':') => {
            let slice = Slice::from_indices(
                tokenizer.parse_state.bytes,
                tokenizer.tokenize_state.start,
                tokenizer.point.index,
            );
            let name = slice.as_str().to_ascii_lowercase();

            tokenizer.tokenize_state.start = 0;

            if name == "http" || name == "https" {
                tokenizer.consume();
                State::Next(StateName::GfmAutolinkLiteralProtocolSlashesInside)
            } else {
                State::Nok
            }
        }
        _ => {
            tokenizer.tokenize_state.start = 0;
            State::Nok
        }
    }
}

/// In protocol slashes.
///
/// ```markdown
/// > | https://example.com/a?b#c
///           ^^
/// ```
pub fn protocol_slashes_inside(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(b'/') {
        tokenizer.consume();
        if tokenizer.tokenize_state.size == 0 {
            tokenizer.tokenize_state.size += 1;
            State::Next(StateName::GfmAutolinkLiteralProtocolSlashesInside)
        } else {
            tokenizer.tokenize_state.size = 0;
            State::Ok
        }
    } else {
        tokenizer.tokenize_state.size = 0;
        State::Nok
    }
}

/// Start of www autolink literal.
///
/// ```markdown
/// > | www.example.com/a?b#c
///     ^
/// ```
pub fn www_start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.parse_state.options.constructs.gfm_autolink_literal
        && matches!(tokenizer.current, Some(b'W' | b'w'))
        && www_may_start(tokenizer.previous)
        && !www_known_nok(tokenizer)
    {
        tokenizer.tokenize_state.gfm_autolink_literal_www_start = tokenizer.point.index;
        tokenizer.enter(Name::GfmAutolinkLiteralWww);
        tokenizer.attempt(State::Next(StateName::GfmAutolinkLiteralWwwAfter), State::Nok);
        // Note: we *check*, so we can discard the `www.` we parsed.
        // If it worked, we consider it as a part of the domain.
        tokenizer.check(State::Next(StateName::GfmAutolinkLiteralDomainInside), State::Nok);
        State::Retry(StateName::GfmAutolinkLiteralWwwPrefixInside)
    } else {
        State::Nok
    }
}

/// After a www autolink literal.
///
/// ```markdown
/// > | www.example.com/a?b#c
///                          ^
/// ```
pub fn www_after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.exit(Name::GfmAutolinkLiteralWww);
    State::Ok
}

/// In www prefix.
///
/// ```markdown
/// > | www.example.com
///     ^^^^
/// ```
pub fn www_prefix_inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'.') if tokenizer.tokenize_state.size == 3 => {
            tokenizer.tokenize_state.size = 0;
            tokenizer.consume();
            State::Next(StateName::GfmAutolinkLiteralWwwPrefixAfter)
        }
        Some(b'W' | b'w') if tokenizer.tokenize_state.size < 3 => {
            tokenizer.tokenize_state.size += 1;
            tokenizer.consume();
            State::Next(StateName::GfmAutolinkLiteralWwwPrefixInside)
        }
        _ => {
            tokenizer.tokenize_state.size = 0;
            State::Nok
        }
    }
}

/// After www prefix.
///
/// ```markdown
/// > | www.example.com
///         ^
/// ```
pub fn www_prefix_after(tokenizer: &mut Tokenizer) -> State {
    // If there is *anything*, we can link.
    if tokenizer.current.is_none() {
        State::Nok
    } else {
        State::Ok
    }
}

/// In domain.
///
/// ```markdown
/// > | https://example.com/a
///             ^^^^^^^^^^^
/// ```
pub fn domain_inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // Check whether this marker, which is a trailing punctuation
        // marker, optionally followed by more trailing markers, and then
        // followed by an end.
        Some(b'.' | b'_') => {
            if trail_known_nok(tokenizer) {
                return State::Retry(StateName::GfmAutolinkLiteralDomainAtPunctuation);
            }
            tokenizer.tokenize_state.gfm_autolink_literal_trail_start = tokenizer.point.index;
            tokenizer.check(
                State::Next(StateName::GfmAutolinkLiteralDomainAfter),
                State::Next(StateName::GfmAutolinkLiteralDomainAtPunctuation),
            );
            State::Retry(StateName::GfmAutolinkLiteralTrail)
        }
        // Dashes and continuation bytes are fine.
        Some(b'-' | 0x80..=0xBF) => {
            tokenizer.consume();
            State::Next(StateName::GfmAutolinkLiteralDomainInside)
        }
        _ => {
            // Source: <https://github.com/github/cmark-gfm/blob/ef1cfcb/extensions/autolink.c#L12>.
            if kind_after_index(tokenizer.parse_state.bytes, tokenizer.point.index) == CharacterKind::Other {
                tokenizer.tokenize_state.seen = true;
                tokenizer.consume();
                State::Next(StateName::GfmAutolinkLiteralDomainInside)
            } else {
                State::Retry(StateName::GfmAutolinkLiteralDomainAfter)
            }
        }
    }
}

/// In domain, at potential trailing punctuation, that was not trailing.
///
/// ```markdown
/// > | https://example.com
///                    ^
/// ```
pub fn domain_at_punctuation(tokenizer: &mut Tokenizer) -> State {
    // There is an underscore in the last segment of the domain
    if matches!(tokenizer.current, Some(b'_')) {
        tokenizer.tokenize_state.marker = b'_';
    }
    // Otherwise, it’s a `.`: save the last segment underscore in the
    // penultimate segment slot.
    else {
        tokenizer.tokenize_state.marker_b = tokenizer.tokenize_state.marker;
        tokenizer.tokenize_state.marker = 0;
    }

    tokenizer.consume();
    State::Next(StateName::GfmAutolinkLiteralDomainInside)
}

/// After domain
///
/// ```markdown
/// > | https://example.com/a
///                        ^
/// ```
pub fn domain_after(tokenizer: &mut Tokenizer) -> State {
    // No underscores allowed in last two segments.
    let result = if tokenizer.tokenize_state.marker_b == b'_'
        || tokenizer.tokenize_state.marker == b'_'
        // At least one character must be seen.
        || !tokenizer.tokenize_state.seen
    // Note: that’s GH says a dot is needed, but it’s not true:
    // <https://github.com/github/cmark-gfm/issues/279>
    {
        www_nok(tokenizer);
        State::Nok
    } else {
        State::Retry(StateName::GfmAutolinkLiteralPathInside)
    };

    tokenizer.tokenize_state.seen = false;
    tokenizer.tokenize_state.marker = 0;
    tokenizer.tokenize_state.marker_b = 0;
    result
}

/// In path.
///
/// ```markdown
/// > | https://example.com/a
///                        ^^
/// ```
pub fn path_inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // Continuation bytes are fine, we’ve already checked the first one.
        Some(0x80..=0xBF) => {
            tokenizer.consume();
            State::Next(StateName::GfmAutolinkLiteralPathInside)
        }
        // Count opening parens.
        Some(b'(') => {
            tokenizer.tokenize_state.size += 1;
            tokenizer.consume();
            State::Next(StateName::GfmAutolinkLiteralPathInside)
        }
        // Check whether this trailing punctuation marker is optionally
        // followed by more trailing markers, and then followed
        // by an end.
        // If this is a paren (followed by trailing, then the end), we
        // *continue* if we saw less closing parens than opening parens.
        Some(
            b'!' | b'"' | b'&' | b'\'' | b')' | b'*' | b',' | b'.' | b':' | b';' | b'<' | b'?' | b']' | b'_' | b'~',
        ) => {
            if trail_known_nok(tokenizer) {
                return State::Retry(StateName::GfmAutolinkLiteralPathAtPunctuation);
            }
            let next =
                if tokenizer.current == Some(b')') && tokenizer.tokenize_state.size_b < tokenizer.tokenize_state.size {
                    StateName::GfmAutolinkLiteralPathAtPunctuation
                } else {
                    StateName::GfmAutolinkLiteralPathAfter
                };
            tokenizer.tokenize_state.gfm_autolink_literal_trail_start = tokenizer.point.index;
            tokenizer.check(
                State::Next(next),
                State::Next(StateName::GfmAutolinkLiteralPathAtPunctuation),
            );
            State::Retry(StateName::GfmAutolinkLiteralTrail)
        }
        _ => {
            // Source: <https://github.com/github/cmark-gfm/blob/ef1cfcb/extensions/autolink.c#L12>.
            if tokenizer.current.is_none()
                || kind_after_index(tokenizer.parse_state.bytes, tokenizer.point.index) == CharacterKind::Whitespace
            {
                State::Retry(StateName::GfmAutolinkLiteralPathAfter)
            } else {
                tokenizer.consume();
                State::Next(StateName::GfmAutolinkLiteralPathInside)
            }
        }
    }
}

/// In path, at potential trailing punctuation, that was not trailing.
///
/// ```markdown
/// > | https://example.com/a"b
///                          ^
/// ```
pub fn path_at_punctuation(tokenizer: &mut Tokenizer) -> State {
    // Count closing parens.
    if tokenizer.current == Some(b')') {
        tokenizer.tokenize_state.size_b += 1;
    }

    tokenizer.consume();
    State::Next(StateName::GfmAutolinkLiteralPathInside)
}

/// At end of path, reset parens.
///
/// ```markdown
/// > | https://example.com/asd(qwe).
///                                 ^
/// ```
pub fn path_after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.size = 0;
    tokenizer.tokenize_state.size_b = 0;
    State::Ok
}

/// In trail of domain or path.
///
/// ```markdown
/// > | https://example.com").
///                        ^
/// ```
pub fn trail(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // Regular trailing punctuation.
        Some(b'!' | b'"' | b'\'' | b')' | b'*' | b',' | b'.' | b':' | b';' | b'?' | b'_' | b'~') => {
            tokenizer.consume();
            State::Next(StateName::GfmAutolinkLiteralTrail)
        }
        // `&` followed by one or more alphabeticals and then a `;`, is
        // as a whole considered as trailing punctuation.
        // In all other cases, it is considered as continuation of the URL.
        Some(b'&') => {
            tokenizer.consume();
            State::Next(StateName::GfmAutolinkLiteralTrailCharRefStart)
        }
        // `<` is an end.
        Some(b'<') => State::Ok,
        // Needed because we allow literals after `[`, as we fix:
        // <https://github.com/github/cmark-gfm/issues/278>.
        // Check that it is not followed by `(` or `[`.
        Some(b']') => {
            tokenizer.consume();
            State::Next(StateName::GfmAutolinkLiteralTrailBracketAfter)
        }
        _ => {
            // Whitespace is the end of the URL, anything else is continuation.
            if kind_after_index(tokenizer.parse_state.bytes, tokenizer.point.index) == CharacterKind::Whitespace {
                State::Ok
            } else {
                trail_nok(tokenizer)
            }
        }
    }
}

/// In trail, after `]`.
///
/// > 👉 **Note**: this deviates from `cmark-gfm` to fix a bug.
/// > See end of <https://github.com/github/cmark-gfm/issues/278> for more.
///
/// ```markdown
/// > | https://example.com](
///                         ^
/// ```
pub fn trail_bracket_after(tokenizer: &mut Tokenizer) -> State {
    // Whitespace or something that could start a resource or reference is the end.
    // Switch back to trail otherwise.
    if matches!(tokenizer.current, None | Some(b'\t' | b'\n' | b' ' | b'(' | b'[')) {
        State::Ok
    } else {
        State::Retry(StateName::GfmAutolinkLiteralTrail)
    }
}

/// In character-reference like trail, after `&`.
///
/// ```markdown
/// > | https://example.com&amp;).
///                         ^
/// ```
pub fn trail_char_ref_start(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, Some(b'A'..=b'Z' | b'a'..=b'z')) {
        State::Retry(StateName::GfmAutolinkLiteralTrailCharRefInside)
    } else {
        trail_nok(tokenizer)
    }
}

/// In character-reference like trail.
///
/// ```markdown
/// > | https://example.com&amp;).
///                         ^
/// ```
pub fn trail_char_ref_inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'A'..=b'Z' | b'a'..=b'z') => {
            tokenizer.consume();
            State::Next(StateName::GfmAutolinkLiteralTrailCharRefInside)
        }
        // Switch back to trail if this is well-formed.
        Some(b';') => {
            tokenizer.consume();
            State::Next(StateName::GfmAutolinkLiteralTrail)
        }
        _ => trail_nok(tokenizer),
    }
}

/// Whether an earlier invalid `www.` domain covers a literal starting here.
fn www_known_nok(tokenizer: &Tokenizer) -> bool {
    tokenizer
        .tokenize_state
        .gfm_autolink_literal_www_nok
        .contains(&tokenizer.point.index)
}

/// At an invalid `www.` domain: remember up to where later starts fail too.
fn www_nok(tokenizer: &mut Tokenizer) {
    let start = tokenizer.tokenize_state.gfm_autolink_literal_www_start;

    if start == usize::MAX {
        return;
    }

    debug_assert!(tokenizer.tokenize_state.seen, "expected letters in `www.`");
    let bytes = tokenizer.parse_state.bytes;
    let mut index = tokenizer.point.index;
    let mut dots = 0;

    while index > start && dots < 2 {
        index -= 1;
        match bytes[index] {
            b'.' => dots += 1,
            b'_' => {
                tokenizer.tokenize_state.gfm_autolink_literal_www_nok = start..index + 1;
                return;
            }
            _ => {}
        }
    }
}

/// Whether an earlier failed trailing punctuation check covers this place.
fn trail_known_nok(tokenizer: &Tokenizer) -> bool {
    tokenizer
        .tokenize_state
        .gfm_autolink_literal_trail_nok
        .contains(&tokenizer.point.index)
}

/// Trailing punctuation not at the end: remember where this check failed.
fn trail_nok(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.gfm_autolink_literal_trail_nok =
        tokenizer.tokenize_state.gfm_autolink_literal_trail_start..tokenizer.point.index;
    State::Nok
}

/// Resolve: postprocess text to find email autolink literals.
pub fn resolve(tokenizer: &mut Tokenizer) {
    tokenizer.map.consume(&mut tokenizer.events);

    let mut index = 0;
    let mut links = 0;

    while index < tokenizer.events.len() {
        let event = &tokenizer.events[index];

        if event.kind == Kind::Enter {
            if event.name == Name::Link {
                links += 1;
            }
        } else {
            // Linked data is string content (such as a title), not text.
            if event.name == Name::Data && links == 0 && tokenizer.events[index - 1].link.is_none() {
                let slice = Slice::from_position(
                    tokenizer.parse_state.bytes,
                    &Position::from_exit_event(&tokenizer.events, index),
                );
                let bytes = slice.bytes;
                let mut byte_index = 0;
                let mut replace = Vec::new();
                let mut point = tokenizer.events[index - 1].point.clone();
                let start_index = point.index;
                let mut min = 0;

                while byte_index < bytes.len() {
                    if bytes[byte_index] == b'@' {
                        let mut range = (0, 0, Name::GfmAutolinkLiteralEmail);

                        if let Some(start) = peek_bytes_atext(bytes, min, byte_index) {
                            let (start, kind) = peek_protocol(bytes, min, start);

                            if let Some(end) =
                                peek_bytes_email_domain(bytes, byte_index + 1, kind == Name::GfmAutolinkLiteralXmpp)
                            {
                                // Note: normally we’d truncate trailing
                                // punctuation from the link.
                                // However, email autolink literals cannot
                                // contain any of those markers, except for
                                // `.`, but that can only occur if it isn’t
                                // trailing.
                                // So we can ignore truncating while
                                // postprocessing!
                                range = (start, end, kind);
                            }
                        }

                        if range.1 != 0 {
                            byte_index = range.1;

                            // If there is something between the last link
                            // (or `min`) and this link.
                            if min != range.0 {
                                replace.push(Event {
                                    kind: Kind::Enter,
                                    name: Name::Data,
                                    point: point.clone(),
                                    link: None,
                                });
                                point = point.shift_to(tokenizer.parse_state.bytes, start_index + range.0);
                                replace.push(Event {
                                    kind: Kind::Exit,
                                    name: Name::Data,
                                    point: point.clone(),
                                    link: None,
                                });
                            }

                            // Add the link.
                            replace.push(Event {
                                kind: Kind::Enter,
                                name: range.2.clone(),
                                point: point.clone(),
                                link: None,
                            });
                            point = point.shift_to(tokenizer.parse_state.bytes, start_index + range.1);
                            replace.push(Event {
                                kind: Kind::Exit,
                                name: range.2.clone(),
                                point: point.clone(),
                                link: None,
                            });
                            min = range.1;
                        }
                    }

                    byte_index += 1;
                }

                // If there was a link, and we have more bytes left.
                if min != 0 && min < bytes.len() {
                    replace.push(Event {
                        kind: Kind::Enter,
                        name: Name::Data,
                        point: point.clone(),
                        link: None,
                    });
                    replace.push(Event {
                        kind: Kind::Exit,
                        name: Name::Data,
                        point: event.point.clone(),
                        link: None,
                    });
                }

                // If there were links.
                if !replace.is_empty() {
                    tokenizer.map.add(index - 1, 2, replace);
                }
            }

            if event.name == Name::Link {
                links -= 1;
            }
        }

        index += 1;
    }
}

/// Move back past atext.
///
/// Moving back is only used when post processing text: so for the email address
/// algorithm.
///
/// ```markdown
/// > | a contact@example.org b
///              ^-- from
///       ^-- to
/// ```
fn peek_bytes_atext(bytes: &[u8], min: usize, end: usize) -> Option<usize> {
    let mut index = end;

    // Take simplified atext.
    // See `email_atext` in `autolink.rs` for a similar algorithm.
    // Source: <https://github.com/github/cmark-gfm/blob/ef1cfcb/extensions/autolink.c#L301>.
    while index > min && matches!(bytes[index - 1], b'+' | b'-' | b'.' | b'0'..=b'9' | b'A'..=b'Z' | b'_' | b'a'..=b'z')
    {
        index -= 1;
    }

    // Do not allow a slash “inside” atext.
    // The reference code is a bit weird, but that’s what it results in.
    // Source: <https://github.com/github/cmark-gfm/blob/ef1cfcb/extensions/autolink.c#L307>.
    // Other than slash, every preceding character is allowed.
    if index == end || (index > min && bytes[index - 1] == b'/') {
        None
    } else {
        Some(index)
    }
}

/// Move back past a `mailto:` or `xmpp:` protocol.
///
/// Moving back is only used when post processing text: so for the email address
/// algorithm.
///
/// ```markdown
/// > | a mailto:contact@example.org b
///              ^-- from
///       ^-- to
/// ```
fn peek_protocol(bytes: &[u8], min: usize, end: usize) -> (usize, Name) {
    let mut index = end;

    if index > min && bytes[index - 1] == b':' {
        index -= 1;

        // Take alphanumerical.
        while index > min && matches!(bytes[index - 1], b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z') {
            index -= 1;
        }

        let slice = Slice::from_indices(bytes, index, end - 1);
        let name = slice.as_str().to_ascii_lowercase();

        if name == "xmpp" {
            return (index, Name::GfmAutolinkLiteralXmpp);
        } else if name == "mailto" {
            return (index, Name::GfmAutolinkLiteralMailto);
        }
    }

    (end, Name::GfmAutolinkLiteralEmail)
}

/// Move past email domain.
///
/// Peeking like this only used when post processing text: so for the email
/// address algorithm.
///
/// ```markdown
/// > | a contact@example.org b
///               ^-- from
///                         ^-- to
/// ```
fn peek_bytes_email_domain(bytes: &[u8], start: usize, xmpp: bool) -> Option<usize> {
    let mut index = start;
    let mut dot = false;

    // Move past “domain”.
    // The reference code is a bit overly complex as it handles the `@`, of which there may be just one.
    // Source: <https://github.com/github/cmark-gfm/blob/ef1cfcb/extensions/autolink.c#L318>
    while index < bytes.len() {
        match bytes[index] {
            // Alphanumerical, `-`, and `_`.
            b'-' | b'0'..=b'9' | b'A'..=b'Z' | b'_' | b'a'..=b'z' => {}
            b'/' if xmpp => {}
            // Dot followed by alphanumerical (not `-` or `_`).
            b'.' if index + 1 < bytes.len() && matches!(bytes[index + 1], b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z') => {
                dot = true;
            }
            _ => break,
        }

        index += 1;
    }

    // Domain must not be empty, must include a dot, and must end in alphabetical or `.`.
    // Source: <https://github.com/github/cmark-gfm/blob/ef1cfcb/extensions/autolink.c#L332>.
    if index > start && dot && matches!(bytes[index - 1], b'.' | b'A'..=b'Z' | b'a'..=b'z') {
        Some(index)
    } else {
        None
    }
}
