//! HTML (text) occurs in the [text][] content type.
//!
//! ## Grammar
//!
//! HTML (text) forms with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! html_text ::= comment | instruction | declaration | cdata | tag_close | tag_open
//!
//! ; Restriction: the text is not allowed to start with `>`, `->`, or to contain `--`.
//! comment ::= '<!--' *byte '-->'
//! instruction ::= '<?' *byte '?>'
//! declaration ::= '<!' ascii_alphabetic *byte '>'
//! ; Restriction: the text is not allowed to contain `]]`.
//! cdata ::= '<![CDATA[' *byte ']]>'
//! tag_close ::= '</' tag_name [space_or_tab_eol] '>'
//! opening_tag ::= '<' tag_name *(space_or_tab_eol attribute) [[space_or_tab_eol] '/'] [space_or_tab_eol] '>'
//!
//! tag_name ::= ascii_alphabetic *( '-' | ascii_alphanumeric )
//! attribute ::= attribute_name [[space_or_tab_eol] '=' [space_or_tab_eol] attribute_value]
//! attribute_name ::= (':' | '_' | ascii_alphabetic) *('-' | '.' | ':' | '_' | ascii_alphanumeric)
//! attribute_value ::= '"' *(byte - '"') '"' | "'" *(byte - "'")  "'" | 1*(text - '"' - "'" - '/' - '<' - '=' - '>' - '`')
//! ```
//!
//! The grammar for HTML in markdown does not follow the rules of parsing
//! HTML according to the [*§ 13.2 Parsing HTML documents* in the HTML
//! spec][html_parsing].
//! See the related flow construct [HTML (flow)][html_flow] for more info.
//!
//! Because the **tag open** and **tag close** productions in the grammar form
//! with just tags instead of complete elements, it is possible to interleave
//! (a word for switching between languages) markdown and HTML together.
//! For example:
//!
//! ```markdown
//! This is equivalent to <code>*emphasised* code</code>.
//! ```
//!
//! ## Tokens
//!
//! * [`HtmlText`][Name::HtmlText]
//! * [`HtmlTextData`][Name::HtmlTextData]
//!
//! ## References
//!
//! * [`html-text.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/html-text.js)
//! * [*§ 6.6 Raw HTML* in `CommonMark`](https://spec.commonmark.org/0.31/#raw-html)
//!
//! [text]: crate::mdx_parser::construct::text
//! [html_flow]: crate::mdx_parser::construct::html_flow
//! [html_parsing]: https://html.spec.whatwg.org/multipage/parsing.html#parsing

use crate::mdx_parser::construct::partial_space_or_tab::space_or_tab;
use crate::mdx_parser::event::Name;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::constant::HTML_CDATA_PREFIX;

/// Start of HTML (text).
///
/// ```markdown
/// > | a <b> c
///       ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if Some(b'<') == tokenizer.current && tokenizer.parse_state.options.constructs.html_text {
        tokenizer.tokenize_state.html_text_start = tokenizer.point.index;
        tokenizer.enter(Name::HtmlText);
        tokenizer.enter(Name::HtmlTextData);
        tokenizer.consume();
        State::Next(StateName::HtmlTextOpen)
    } else {
        State::Nok
    }
}

/// After `<`, at tag name or other stuff.
///
/// ```markdown
/// > | a <b> c
///        ^
/// > | a <!doctype> c
///        ^
/// > | a <!--b--> c
///        ^
/// ```
pub fn open(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'!') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextDeclarationOpen)
        }
        Some(b'/') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagCloseStart)
        }
        Some(b'?') if !known_unclosed(tokenizer, INSTRUCTION) => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextInstruction)
        }
        // ASCII alphabetical.
        Some(b'A'..=b'Z' | b'a'..=b'z') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagOpen)
        }
        _ => State::Nok,
    }
}

/// After `<!`, at declaration, comment, or CDATA.
///
/// ```markdown
/// > | a <!doctype> c
///         ^
/// > | a <!--b--> c
///         ^
/// > | a <![CDATA[>&<]]> c
///         ^
/// ```
pub fn declaration_open(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'-') if !known_unclosed(tokenizer, COMMENT) => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextCommentOpenInside)
        }
        // ASCII alphabetical.
        Some(b'A'..=b'Z' | b'a'..=b'z') if !known_unclosed(tokenizer, DECLARATION) => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextDeclaration)
        }
        Some(b'[') if !known_unclosed(tokenizer, CDATA) => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextCdataOpenInside)
        }
        _ => State::Nok,
    }
}

/// In a comment, after `<!-`, at another `-`.
///
/// ```markdown
/// > | a <!--b--> c
///          ^
/// ```
pub fn comment_open_inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'-') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextCommentEnd)
        }
        _ => State::Nok,
    }
}

/// In comment.
///
/// ```markdown
/// > | a <!--b--> c
///           ^
/// ```
pub fn comment(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None => unclosed(tokenizer, COMMENT),
        Some(b'\n') => {
            tokenizer.attempt(State::Next(StateName::HtmlTextComment), State::Nok);
            State::Retry(StateName::HtmlTextLineEndingBefore)
        }
        Some(b'-') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextCommentClose)
        }
        _ => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextComment)
        }
    }
}

/// In comment, after `-`.
///
/// ```markdown
/// > | a <!--b--> c
///             ^
/// ```
pub fn comment_close(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'-') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextCommentEnd)
        }
        _ => State::Retry(StateName::HtmlTextComment),
    }
}
/// In comment, after `-`.
///
/// ```markdown
/// > | a <!--b--> c
///             ^
/// ```
pub fn comment_end(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'>') => State::Retry(StateName::HtmlTextEnd),
        Some(b'-') => State::Retry(StateName::HtmlTextCommentClose),
        _ => State::Retry(StateName::HtmlTextComment),
    }
}

/// After `<![`, in CDATA, expecting `CDATA[`.
///
/// ```markdown
/// > | a <![CDATA[>&<]]> b
///          ^^^^^^
/// ```
pub fn cdata_open_inside(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(HTML_CDATA_PREFIX[tokenizer.tokenize_state.size]) {
        tokenizer.tokenize_state.size += 1;
        tokenizer.consume();

        if tokenizer.tokenize_state.size == HTML_CDATA_PREFIX.len() {
            tokenizer.tokenize_state.size = 0;
            State::Next(StateName::HtmlTextCdata)
        } else {
            State::Next(StateName::HtmlTextCdataOpenInside)
        }
    } else {
        tokenizer.tokenize_state.size = 0;
        State::Nok
    }
}

/// In CDATA.
///
/// ```markdown
/// > | a <![CDATA[>&<]]> b
///                ^^^
/// ```
pub fn cdata(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None => unclosed(tokenizer, CDATA),
        Some(b'\n') => {
            tokenizer.attempt(State::Next(StateName::HtmlTextCdata), State::Nok);
            State::Retry(StateName::HtmlTextLineEndingBefore)
        }
        Some(b']') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextCdataClose)
        }
        _ => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextCdata)
        }
    }
}

/// In CDATA, after `]`, at another `]`.
///
/// ```markdown
/// > | a <![CDATA[>&<]]> b
///                    ^
/// ```
pub fn cdata_close(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b']') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextCdataEnd)
        }
        _ => State::Retry(StateName::HtmlTextCdata),
    }
}

/// In CDATA, after `]]`, at `>`.
///
/// ```markdown
/// > | a <![CDATA[>&<]]> b
///                     ^
/// ```
pub fn cdata_end(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'>') => State::Retry(StateName::HtmlTextEnd),
        Some(b']') => State::Retry(StateName::HtmlTextCdataClose),
        _ => State::Retry(StateName::HtmlTextCdata),
    }
}

/// In declaration.
///
/// ```markdown
/// > | a <!b> c
///          ^
/// ```
pub fn declaration(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None => unclosed(tokenizer, DECLARATION),
        Some(b'>') => State::Retry(StateName::HtmlTextEnd),
        Some(b'\n') => {
            tokenizer.attempt(State::Next(StateName::HtmlTextDeclaration), State::Nok);
            State::Retry(StateName::HtmlTextLineEndingBefore)
        }
        _ => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextDeclaration)
        }
    }
}

/// In instruction.
///
/// ```markdown
/// > | a <?b?> c
///         ^
/// ```
pub fn instruction(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None => unclosed(tokenizer, INSTRUCTION),
        Some(b'\n') => {
            tokenizer.attempt(State::Next(StateName::HtmlTextInstruction), State::Nok);
            State::Retry(StateName::HtmlTextLineEndingBefore)
        }
        Some(b'?') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextInstructionClose)
        }
        _ => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextInstruction)
        }
    }
}

/// In instruction, after `?`, at `>`.
///
/// ```markdown
/// > | a <?b?> c
///           ^
/// ```
pub fn instruction_close(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'>') => State::Retry(StateName::HtmlTextEnd),
        _ => State::Retry(StateName::HtmlTextInstruction),
    }
}

/// After `</`, in closing tag, at tag name.
///
/// ```markdown
/// > | a </b> c
///         ^
/// ```
pub fn tag_close_start(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // ASCII alphabetical.
        Some(b'A'..=b'Z' | b'a'..=b'z') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagClose)
        }
        _ => State::Nok,
    }
}

/// After `</x`, in a tag name.
///
/// ```markdown
/// > | a </b> c
///          ^
/// ```
pub fn tag_close(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // ASCII alphanumerical and `-`.
        Some(b'-' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagClose)
        }
        _ => State::Retry(StateName::HtmlTextTagCloseBetween),
    }
}

/// In closing tag, after tag name.
///
/// ```markdown
/// > | a </b> c
///          ^
/// ```
pub fn tag_close_between(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\n') => {
            tokenizer.attempt(State::Next(StateName::HtmlTextTagCloseBetween), State::Nok);
            State::Retry(StateName::HtmlTextLineEndingBefore)
        }
        Some(b'\t' | b' ') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagCloseBetween)
        }
        _ => State::Retry(StateName::HtmlTextEnd),
    }
}

/// After `<x`, in opening tag name.
///
/// ```markdown
/// > | a <b> c
///         ^
/// ```
pub fn tag_open(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // ASCII alphanumerical and `-`.
        Some(b'-' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagOpen)
        }
        Some(b'\t' | b'\n' | b' ' | b'/' | b'>') => State::Retry(StateName::HtmlTextTagOpenBetween),
        _ => State::Nok,
    }
}

/// In opening tag, after tag name.
///
/// ```markdown
/// > | a <b> c
///         ^
/// ```
pub fn tag_open_between(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\n') => {
            tokenizer.attempt(State::Next(StateName::HtmlTextTagOpenBetween), State::Nok);
            State::Retry(StateName::HtmlTextLineEndingBefore)
        }
        Some(b'\t' | b' ') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagOpenBetween)
        }
        Some(b'/') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextEnd)
        }
        // ASCII alphabetical and `:` and `_`.
        Some(b':' | b'A'..=b'Z' | b'_' | b'a'..=b'z') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagOpenAttributeName)
        }
        _ => State::Retry(StateName::HtmlTextEnd),
    }
}

/// In attribute name.
///
/// ```markdown
/// > | a <b c> d
///          ^
/// ```
pub fn tag_open_attribute_name(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // ASCII alphabetical and `-`, `.`, `:`, and `_`.
        Some(b'-' | b'.' | b'0'..=b'9' | b':' | b'A'..=b'Z' | b'_' | b'a'..=b'z') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagOpenAttributeName)
        }
        _ => State::Retry(StateName::HtmlTextTagOpenAttributeNameAfter),
    }
}

/// After attribute name, before initializer, the end of the tag, or
/// whitespace.
///
/// ```markdown
/// > | a <b c> d
///           ^
/// ```
pub fn tag_open_attribute_name_after(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\n') => {
            tokenizer.attempt(State::Next(StateName::HtmlTextTagOpenAttributeNameAfter), State::Nok);
            State::Retry(StateName::HtmlTextLineEndingBefore)
        }
        Some(b'\t' | b' ') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagOpenAttributeNameAfter)
        }
        Some(b'=') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagOpenAttributeValueBefore)
        }
        _ => State::Retry(StateName::HtmlTextTagOpenBetween),
    }
}

/// Before unquoted, double quoted, or single quoted attribute value, allowing
/// whitespace.
///
/// ```markdown
/// > | a <b c=d> e
///            ^
/// ```
pub fn tag_open_attribute_value_before(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'<' | b'=' | b'>' | b'`') => State::Nok,
        Some(b'\n') => {
            tokenizer.attempt(State::Next(StateName::HtmlTextTagOpenAttributeValueBefore), State::Nok);
            State::Retry(StateName::HtmlTextLineEndingBefore)
        }
        Some(b'\t' | b' ') => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagOpenAttributeValueBefore)
        }
        Some(b'"' | b'\'') => {
            tokenizer.tokenize_state.marker = tokenizer.current.unwrap();
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagOpenAttributeValueQuoted)
        }
        Some(_) => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagOpenAttributeValueUnquoted)
        }
    }
}

/// In double or single quoted attribute value.
///
/// ```markdown
/// > | a <b c="d"> e
///             ^
/// ```
pub fn tag_open_attribute_value_quoted(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.tokenize_state.marker = 0;
        tokenizer.consume();
        State::Next(StateName::HtmlTextTagOpenAttributeValueQuotedAfter)
    } else {
        match tokenizer.current {
            None => {
                tokenizer.tokenize_state.marker = 0;
                State::Nok
            }
            Some(b'\n') => {
                tokenizer.attempt(State::Next(StateName::HtmlTextTagOpenAttributeValueQuoted), State::Nok);
                State::Retry(StateName::HtmlTextLineEndingBefore)
            }
            _ => {
                tokenizer.consume();
                State::Next(StateName::HtmlTextTagOpenAttributeValueQuoted)
            }
        }
    }
}

/// In unquoted attribute value.
///
/// ```markdown
/// > | a <b c=d> e
///            ^
/// ```
pub fn tag_open_attribute_value_unquoted(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'"' | b'\'' | b'<' | b'=' | b'`') => State::Nok,
        Some(b'\t' | b'\n' | b' ' | b'/' | b'>') => State::Retry(StateName::HtmlTextTagOpenBetween),
        Some(_) => {
            tokenizer.consume();
            State::Next(StateName::HtmlTextTagOpenAttributeValueUnquoted)
        }
    }
}

/// After double or single quoted attribute value, before whitespace or the end
/// of the tag.
///
/// ```markdown
/// > | a <b c="d"> e
///               ^
/// ```
pub fn tag_open_attribute_value_quoted_after(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\t' | b'\n' | b' ' | b'/' | b'>') => State::Retry(StateName::HtmlTextTagOpenBetween),
        _ => State::Nok,
    }
}

/// In certain circumstances of a tag where only an `>` is allowed.
///
/// ```markdown
/// > | a <b c="d"> e
///               ^
/// ```
pub fn end(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'>') => {
            tokenizer.consume();
            tokenizer.exit(Name::HtmlTextData);
            tokenizer.exit(Name::HtmlText);
            State::Ok
        }
        _ => State::Nok,
    }
}

/// At eol.
///
/// > 👉 **Note**: we can’t have blank lines in text, so no need to worry about
/// > empty tokens.
///
/// ```markdown
/// > | a <!--a
///            ^
///   | b-->
/// ```
pub fn line_ending_before(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\n') => {
            tokenizer.exit(Name::HtmlTextData);
            tokenizer.enter(Name::LineEnding);
            tokenizer.consume();
            tokenizer.exit(Name::LineEnding);
            State::Next(StateName::HtmlTextLineEndingAfter)
        }
        _ => unreachable!("expected eol"),
    }
}

/// After eol, at optional whitespace.
///
/// > 👉 **Note**: we can’t have blank lines in text, so no need to worry about
/// > empty tokens.
///
/// ```markdown
///   | a <!--a
/// > | b-->
///     ^
/// ```
pub fn line_ending_after(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, Some(b'\t' | b' ')) {
        tokenizer.attempt(State::Next(StateName::HtmlTextLineEndingAfterPrefix), State::Nok);
        State::Retry(space_or_tab(tokenizer))
    } else {
        State::Retry(StateName::HtmlTextLineEndingAfterPrefix)
    }
}

/// After eol, after optional whitespace.
///
/// > 👉 **Note**: we can’t have blank lines in text, so no need to worry about
/// > empty tokens.
///
/// ```markdown
///   | a <!--a
/// > | b-->
///     ^
/// ```
pub fn line_ending_after_prefix(tokenizer: &mut Tokenizer) -> State {
    tokenizer.enter(Name::HtmlTextData);
    State::Ok
}

/// Kinds of HTML (text) with a closer, as indices into `html_text_unclosed`.
const COMMENT: usize = 0;
const CDATA: usize = 1;
const DECLARATION: usize = 2;
const INSTRUCTION: usize = 3;

/// Whether an earlier HTML (text) of `kind` found no closer after here.
fn known_unclosed(tokenizer: &Tokenizer, kind: usize) -> bool {
    tokenizer.tokenize_state.html_text_start >= tokenizer.tokenize_state.html_text_unclosed[kind]
}

/// At the end without a closer: remember it for later starts of `kind`.
fn unclosed(tokenizer: &mut Tokenizer, kind: usize) -> State {
    let state = &mut tokenizer.tokenize_state;
    state.html_text_unclosed[kind] = state.html_text_unclosed[kind].min(state.html_text_start);
    State::Nok
}
