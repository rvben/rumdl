//! The text content type.
//!
//! **Text** contains phrasing content such as
//! [attention][crate::mdx_parser::construct::attention] (emphasis, gfm strikethrough, strong),
//! [raw (text)][crate::mdx_parser::construct::raw_text] (code (text), math (text)), and actual text.
//!
//! The constructs found in text are:
//!
//! * [Attention][crate::mdx_parser::construct::attention] (emphasis, gfm strikethrough, strong)
//! * [Autolink][crate::mdx_parser::construct::autolink]
//! * [Character escape][crate::mdx_parser::construct::character_escape]
//! * [Character reference][crate::mdx_parser::construct::character_reference]
//! * [Raw (text)][crate::mdx_parser::construct::raw_text] (code (text), math (text))
//! * [GFM: Label start (footnote)][crate::mdx_parser::construct::gfm_label_start_footnote]
//! * [GFM: Task list item check][crate::mdx_parser::construct::gfm_task_list_item_check]
//! * [Hard break (escape)][crate::mdx_parser::construct::hard_break_escape]
//! * [HTML (text)][crate::mdx_parser::construct::html_text]
//! * [Label start (image)][crate::mdx_parser::construct::label_start_image]
//! * [Label start (link)][crate::mdx_parser::construct::label_start_link]
//! * [Label end][crate::mdx_parser::construct::label_end]
//! * [MDX: expression (text)][crate::mdx_parser::construct::mdx_expression_text]
//! * [MDX: JSX (text)][crate::mdx_parser::construct::mdx_jsx_text]
//!
//! > 👉 **Note**: for performance reasons, hard break (trailing) is formed by
//! > [whitespace][crate::mdx_parser::construct::partial_whitespace].

use crate::mdx_parser::construct::gfm_autolink_literal::resolve as resolve_gfm_autolink_literal;
use crate::mdx_parser::construct::gfm_autolink_literal::{protocol_may_start, www_may_start};
use crate::mdx_parser::construct::partial_whitespace::resolve_whitespace;
use crate::mdx_parser::resolve::Name as ResolveName;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::Subresult;
use crate::mdx_parser::tokenizer::{ByteSet, LINE_STOP, Tokenizer, with_bytes};

/// Characters that can start something in text, apart from autolink literals.
const MARKERS: [u8; 12] = [
    b'!',  // `label_start_image`
    b'$',  // `raw_text` (math (text))
    b'&',  // `character_reference`
    b'*',  // `attention` (emphasis, strong)
    b'<',  // `autolink`, `html_text`, `mdx_jsx_text`
    b'[',  // `label_start_link`, `gfm_label_start_footnote`
    b'\\', // `character_escape`, `hard_break_escape`
    b']',  // `label_end`
    b'_',  // `attention` (emphasis, strong)
    b'`',  // `raw_text` (code (text))
    b'{',  // `mdx_expression_text`
    b'~',  // `attention` (gfm strikethrough)
];

/// Characters that can start GFM autolink literals.
const LITERAL_MARKERS: [u8; 4] = [
    b'H', // `gfm_autolink_literal` (`protocol` kind)
    b'W', // `gfm_autolink_literal` (`www.` kind)
    b'h', // `gfm_autolink_literal` (`protocol` kind)
    b'w', // `gfm_autolink_literal` (`www.` kind)
];

/// Bytes that data stops at, without the letters of autolink literals.
const STOP_WITHOUT_LITERALS: ByteSet = with_bytes(LINE_STOP, &MARKERS);

/// Bytes that data stops at.
const STOP: ByteSet = with_bytes(STOP_WITHOUT_LITERALS, &LITERAL_MARKERS);

/// Whether an enabled construct can start at `byte`, as dispatched by `before`.
pub fn may_start(tokenizer: &Tokenizer, byte: u8) -> bool {
    let constructs = &tokenizer.parse_state.options.constructs;
    match byte {
        b'!' => constructs.label_start_image,
        b'$' => constructs.math_text,
        b'&' => constructs.character_reference,
        b'*' | b'_' => constructs.attention,
        b'<' => constructs.autolink || constructs.html_text || constructs.mdx_jsx_text,
        b'H' | b'h' => constructs.gfm_autolink_literal && protocol_may_start(tokenizer.previous),
        b'W' | b'w' => constructs.gfm_autolink_literal && www_may_start(tokenizer.previous),
        b'[' => constructs.label_start_link || constructs.gfm_label_start_footnote,
        b'\\' => constructs.character_escape || constructs.hard_break_escape,
        b']' => constructs.label_end,
        b'`' => constructs.code_text,
        b'{' => constructs.mdx_expression_text,
        b'~' => constructs.gfm_strikethrough,
        _ => false,
    }
}

/// Start of text.
///
/// There is a slightly weird case where task list items have their check at
/// the start of the first paragraph.
/// So we start by checking for that.
///
/// ```markdown
/// > | abc
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.markers = if tokenizer.parse_state.options.constructs.gfm_autolink_literal {
        &STOP
    } else {
        &STOP_WITHOUT_LITERALS
    };
    tokenizer.attempt(State::Next(StateName::TextBefore), State::Next(StateName::TextBefore));
    State::Retry(StateName::GfmTaskListItemCheckStart)
}

/// Before text.
///
/// ```markdown
/// > | abc
///     ^
/// ```
pub fn before(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None => {
            tokenizer.register_resolver(ResolveName::Data);
            tokenizer.register_resolver(ResolveName::Text);
            State::Ok
        }
        Some(b'!') => {
            tokenizer.attempt(
                State::Next(StateName::TextBefore),
                State::Next(StateName::TextBeforeData),
            );
            State::Retry(StateName::LabelStartImageStart)
        }
        // raw (text) (code (text), math (text))
        Some(b'$' | b'`') => {
            tokenizer.attempt(
                State::Next(StateName::TextBefore),
                State::Next(StateName::TextBeforeData),
            );
            State::Retry(StateName::RawTextStart)
        }
        Some(b'&') => {
            tokenizer.attempt(
                State::Next(StateName::TextBefore),
                State::Next(StateName::TextBeforeData),
            );
            State::Retry(StateName::CharacterReferenceStart)
        }
        // attention (emphasis, gfm strikethrough, strong)
        Some(b'*' | b'_' | b'~') => {
            tokenizer.attempt(
                State::Next(StateName::TextBefore),
                State::Next(StateName::TextBeforeData),
            );
            State::Retry(StateName::AttentionStart)
        }
        // `autolink`, `html_text` (order does not matter), `mdx_jsx_text` (order matters).
        Some(b'<') => {
            tokenizer.attempt(
                State::Next(StateName::TextBefore),
                State::Next(StateName::TextBeforeHtml),
            );
            State::Retry(StateName::AutolinkStart)
        }
        Some(b'H' | b'h') => {
            tokenizer.attempt(
                State::Next(StateName::TextBefore),
                State::Next(StateName::TextBeforeData),
            );
            State::Retry(StateName::GfmAutolinkLiteralProtocolStart)
        }
        Some(b'W' | b'w') => {
            tokenizer.attempt(
                State::Next(StateName::TextBefore),
                State::Next(StateName::TextBeforeData),
            );
            State::Retry(StateName::GfmAutolinkLiteralWwwStart)
        }
        Some(b'[') => {
            tokenizer.attempt(
                State::Next(StateName::TextBefore),
                State::Next(StateName::TextBeforeLabelStartLink),
            );
            State::Retry(StateName::GfmLabelStartFootnoteStart)
        }
        Some(b'\\') => {
            tokenizer.attempt(
                State::Next(StateName::TextBefore),
                State::Next(StateName::TextBeforeHardBreakEscape),
            );
            State::Retry(StateName::CharacterEscapeStart)
        }
        Some(b']') => {
            tokenizer.attempt(
                State::Next(StateName::TextBefore),
                State::Next(StateName::TextBeforeData),
            );
            State::Retry(StateName::LabelEndStart)
        }
        Some(b'{') => {
            tokenizer.attempt(
                State::Next(StateName::TextBefore),
                State::Next(StateName::TextBeforeData),
            );
            State::Retry(StateName::MdxExpressionTextStart)
        }
        _ => State::Retry(StateName::TextBeforeData),
    }
}

/// Before html (text).
///
/// At `<`, which wasn’t an autolink.
///
/// ```markdown
/// > | a <b>
///       ^
/// ```
pub fn before_html(tokenizer: &mut Tokenizer) -> State {
    tokenizer.attempt(
        State::Next(StateName::TextBefore),
        State::Next(StateName::TextBeforeMdxJsx),
    );
    State::Retry(StateName::HtmlTextStart)
}

/// Before mdx jsx (text).
///
/// At `<`, which wasn’t an autolink or html.
///
/// ```markdown
/// > | a <b>
///       ^
/// ```
pub fn before_mdx_jsx(tokenizer: &mut Tokenizer) -> State {
    tokenizer.attempt(
        State::Next(StateName::TextBefore),
        State::Next(StateName::TextBeforeData),
    );
    State::Retry(StateName::MdxJsxTextStart)
}

/// Before hard break escape.
///
/// At `\`, which wasn’t a character escape.
///
/// ```markdown
/// > | a \␊
///       ^
/// ```
pub fn before_hard_break_escape(tokenizer: &mut Tokenizer) -> State {
    tokenizer.attempt(
        State::Next(StateName::TextBefore),
        State::Next(StateName::TextBeforeData),
    );
    State::Retry(StateName::HardBreakEscapeStart)
}

/// Before label start (link).
///
/// At `[`, which wasn’t a GFM label start (footnote).
///
/// ```markdown
/// > | [a](b)
///     ^
/// ```
pub fn before_label_start_link(tokenizer: &mut Tokenizer) -> State {
    tokenizer.attempt(
        State::Next(StateName::TextBefore),
        State::Next(StateName::TextBeforeData),
    );
    State::Retry(StateName::LabelStartLinkStart)
}

/// Before data.
///
/// ```markdown
/// > | a
///     ^
/// ```
pub fn before_data(tokenizer: &mut Tokenizer) -> State {
    tokenizer.attempt(State::Next(StateName::TextBefore), State::Nok);
    State::Retry(StateName::DataStart)
}

/// Resolve whitespace.
pub fn resolve(tokenizer: &mut Tokenizer) -> Option<Subresult> {
    resolve_whitespace(
        tokenizer,
        tokenizer.parse_state.options.constructs.hard_break_trailing,
        true,
    );

    if tokenizer.parse_state.options.constructs.gfm_autolink_literal {
        resolve_gfm_autolink_literal(tokenizer);
    }

    tokenizer.map.consume(&mut tokenizer.events);
    None
}
