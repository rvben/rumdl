//! Attention (emphasis, strong, optionally GFM strikethrough) occurs in the
//! [text][] content type.
//!
//! ## Grammar
//!
//! Attention sequences form with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! attention_sequence ::= 1*'*' | 1*'_'
//! gfm_attention_sequence ::= 1*'~'
//! ```
//!
//! Sequences are matched together to form attention based on which character
//! they contain, how long they are, and what character occurs before and after
//! each sequence.
//! Otherwise they are turned into data.
//!
//! ## HTML
//!
//! When asterisk/underscore sequences match, and two markers can be “taken”
//! from them, they together relate to the `<strong>` element in HTML.
//! When one marker can be taken, they relate to the `<em>` element.
//! See [*§ 4.5.2 The `em` element*][html-em] and
//! [*§ 4.5.3 The `strong` element*][html-strong] in the HTML spec for more
//! info.
//!
//! When tilde sequences match, they together relate to the `<del>` element in
//! HTML.
//! See [*§ 4.7.2 The `del` element*][html-del] in the HTML spec for more info.
//!
//! ## Recommendation
//!
//! It is recommended to use asterisks for emphasis/strong attention when
//! writing markdown.
//!
//! There are some small differences in whether sequences can open and/or close
//! based on whether they are formed with asterisks or underscores.
//! Because underscores also frequently occur in natural language inside words,
//! while asterisks typically never do, `CommonMark` prohibits underscore
//! sequences from opening or closing when *inside* a word.
//!
//! Because asterisks can be used to form the most markdown constructs, using
//! them has the added benefit of making it easier to gloss over markdown: you
//! can look for asterisks to find syntax while not worrying about other
//! characters.
//!
//! For strikethrough attention, it is recommended to use two markers.
//! While `github.com` allows single tildes too, it technically prohibits it in
//! their spec.
//!
//! ## Tokens
//!
//! * [`Emphasis`][Name::Emphasis]
//! * [`EmphasisSequence`][Name::EmphasisSequence]
//! * [`EmphasisText`][Name::EmphasisText]
//! * [`GfmStrikethrough`][Name::GfmStrikethrough]
//! * [`GfmStrikethroughSequence`][Name::GfmStrikethroughSequence]
//! * [`GfmStrikethroughText`][Name::GfmStrikethroughText]
//! * [`Strong`][Name::Strong]
//! * [`StrongSequence`][Name::StrongSequence]
//! * [`StrongText`][Name::StrongText]
//!
//! > 👉 **Note**: while parsing, [`AttentionSequence`][Name::AttentionSequence]
//! > is used, which is later compiled away.
//!
//! ## References
//!
//! * [`attention.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/attention.js)
//! * [`micromark-extension-gfm-strikethrough`](https://github.com/micromark/micromark-extension-gfm-strikethrough)
//! * [*§ 6.2 Emphasis and strong emphasis* in `CommonMark`](https://spec.commonmark.org/0.31/#emphasis-and-strong-emphasis)
//! * [*§ 6.5 Strikethrough (extension)* in `GFM`](https://github.github.com/gfm/#strikethrough-extension-)
//!
//! [text]: crate::mdx_parser::construct::text
//! [html-em]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-em-element
//! [html-strong]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-strong-element
//! [html-del]: https://html.spec.whatwg.org/multipage/edits.html#the-del-element

use crate::mdx_parser::event::{Event, Kind, Name, Point};
use crate::mdx_parser::resolve::Name as ResolveName;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::Subresult;
use crate::mdx_parser::tokenizer::Tokenizer;
use crate::mdx_parser::util::char::{
    Kind as CharacterKind, after_index as char_after_index, before_index as char_before_index, classify_opt,
};
use alloc::{vec, vec::Vec};

/// Attentention sequence that we can take markers from.
#[derive(Debug)]
struct Sequence {
    /// Marker as a byte (`u8`) used in this sequence.
    marker: u8,
    /// The innermost event that this sequence is in, so that one attention
    /// doesn’t start in say, one link, and end in another.
    scope: Option<usize>,
    /// How many events this sequence is in.
    depth: usize,
    /// The index into events where this sequence’s `Enter` currently resides.
    index: usize,
    /// The (shifted) point where this sequence starts.
    start_point: Point,
    /// The (shifted) point where this sequence end.
    end_point: Point,
    /// The number of markers we can still use.
    size: usize,
    /// The number of markers in the whole run, for the rule of 3.
    run_size: usize,
    /// Whether this sequence can open attention.
    open: bool,
    /// Whether this sequence can close attention.
    close: bool,
}

/// At start of attention.
///
/// ```markdown
/// > | **
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    // Emphasis/strong:
    if (tokenizer.parse_state.options.constructs.attention
        && matches!(tokenizer.current, Some(b'*' | b'_')))
        // GFM strikethrough:
        || (tokenizer.parse_state.options.constructs.gfm_strikethrough && tokenizer.current == Some(b'~'))
    {
        tokenizer.tokenize_state.marker = tokenizer.current.unwrap();
        tokenizer.enter(Name::AttentionSequence);
        State::Retry(StateName::AttentionInside)
    } else {
        State::Nok
    }
}

/// In sequence.
///
/// ```markdown
/// > | **
///     ^^
/// ```
pub fn inside(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.current == Some(tokenizer.tokenize_state.marker) {
        tokenizer.consume();
        State::Next(StateName::AttentionInside)
    } else {
        tokenizer.exit(Name::AttentionSequence);
        tokenizer.register_resolver(ResolveName::Attention);
        tokenizer.tokenize_state.marker = 0;
        State::Ok
    }
}

/// Resolve sequences.
pub fn resolve(tokenizer: &mut Tokenizer) -> Option<Subresult> {
    // Find all sequences, gather info about them.
    let mut sequences = get_sequences(tokenizer);

    // Now walk through them and match them.
    // Openers that can still match, nearest last, grouped by scope.
    let mut openers: Vec<usize> = vec![];
    let mut frames: Vec<Frame> = vec![];
    let mut close = 0;

    while close < sequences.len() {
        enter_scope(&mut frames, &mut openers, &sequences[close]);
        let frame = frames.last_mut().unwrap();

        if sequences[close].close {
            while let Some(kind) = category(tokenizer, &sequences[close]) {
                let found = (frame.bottom[kind]..openers.len())
                    .rev()
                    .find(|position| can_match(&sequences[openers[*position]], &sequences[close]));

                if let Some(position) = found {
                    let open = openers[position];
                    // Openers in between can no longer open, or attention
                    // would misnest.
                    openers.truncate(position + 1);
                    match_sequences(tokenizer, &mut sequences, open, close);

                    if sequences[open].size == 0 {
                        openers.pop();
                    }

                    // Keep bounds inside the surviving opener stack. Whole-run sizes
                    // do not change, so rejected openers remain rejected.
                    for bottom in &mut frame.bottom {
                        *bottom = (*bottom).min(openers.len());
                    }

                    if sequences[close].size == 0 {
                        break;
                    }
                } else {
                    frame.bottom[kind] = openers.len();
                    break;
                }
            }
        }

        if sequences[close].open && sequences[close].size > 0 {
            openers.push(close);
        }

        close += 1;
    }

    // Mark remaining sequences as data.
    for sequence in &sequences {
        if sequence.size > 0 {
            tokenizer.events[sequence.index].name = Name::Data;
            tokenizer.events[sequence.index + 1].name = Name::Data;
        }
    }

    tokenizer.map.consume(&mut tokenizer.events);
    None
}

/// Get sequences.
fn get_sequences(tokenizer: &mut Tokenizer) -> Vec<Sequence> {
    let mut index = 0;
    let mut stack = vec![];
    let mut sequences = vec![];

    while index < tokenizer.events.len() {
        let enter = &tokenizer.events[index];

        if enter.name == Name::AttentionSequence {
            if enter.kind == Kind::Enter {
                let end = index + 1;
                let exit = &tokenizer.events[end];

                let marker = tokenizer.parse_state.bytes[enter.point.index];
                let before_char = char_before_index(tokenizer.parse_state.bytes, enter.point.index);
                let before = classify_opt(before_char);
                let after_char = char_after_index(tokenizer.parse_state.bytes, exit.point.index);
                let after = classify_opt(after_char);
                let open = after == CharacterKind::Other
                    || (after == CharacterKind::Punctuation && before != CharacterKind::Other)
                    // GFM strikethrough loosens flanking for regular attention
                    // markers (not strikethrough) next to its `~`.
                    || (marker != b'~' && tokenizer.parse_state.options.constructs.gfm_strikethrough && matches!(after_char, Some('~')));
                let close = before == CharacterKind::Other
                    || (before == CharacterKind::Punctuation && after != CharacterKind::Other)
                    || (marker != b'~'
                        && tokenizer.parse_state.options.constructs.gfm_strikethrough
                        && matches!(before_char, Some('~')));

                sequences.push(Sequence {
                    index,
                    scope: stack.last().copied(),
                    depth: stack.len(),
                    start_point: enter.point.clone(),
                    end_point: exit.point.clone(),
                    size: exit.point.index - enter.point.index,
                    run_size: exit.point.index - enter.point.index,
                    open: if marker == b'_' {
                        open && (before != CharacterKind::Other || !close)
                    } else {
                        open
                    },
                    close: if marker == b'_' {
                        close && (after != CharacterKind::Other || !open)
                    } else {
                        close
                    },
                    marker,
                });
            }
        } else if enter.kind == Kind::Enter {
            stack.push(index);
        } else {
            stack.pop();
        }

        index += 1;
    }

    sequences
}

/// Closer categories: marker, can open, and size modulo 3; then `~` sizes.
const CATEGORIES: usize = 2 * 2 * 3 + 2;

/// Openers of one scope, and where searches for each closer category stop.
struct Frame {
    /// Scope of the sequences in this frame.
    scope: Option<usize>,
    /// Depth of the sequences in this frame.
    depth: usize,
    /// Where this frame’s openers start in the opener stack.
    start: usize,
    /// Per closer category: no opener below this stack position can match.
    bottom: [usize; CATEGORIES],
}

/// Use the frame for the scope of `sequence`, dropping frames of ended scopes.
fn enter_scope(frames: &mut Vec<Frame>, openers: &mut Vec<usize>, sequence: &Sequence) {
    while let Some(frame) = frames.last() {
        if frame.depth > sequence.depth || (frame.depth == sequence.depth && frame.scope != sequence.scope) {
            openers.truncate(frame.start);
            frames.pop();
        } else {
            break;
        }
    }

    if !matches!(frames.last(), Some(frame) if frame.depth == sequence.depth) {
        frames.push(Frame {
            scope: sequence.scope,
            depth: sequence.depth,
            start: openers.len(),
            bottom: [openers.len(); CATEGORIES],
        });
    }
}

/// Closers in one category match the same openers; `None` if none can.
fn category(tokenizer: &Tokenizer, close: &Sequence) -> Option<usize> {
    if close.marker == b'~' {
        match close.size {
            2 => Some(CATEGORIES - 1),
            1 if tokenizer.parse_state.options.gfm_strikethrough_single_tilde => Some(CATEGORIES - 2),
            _ => None,
        }
    } else {
        Some(usize::from(close.marker == b'_') * 6 + usize::from(close.open) * 3 + close.run_size % 3)
    }
}

/// Whether `open` can be closed by `close`, of the same scope.
fn can_match(open: &Sequence, close: &Sequence) -> bool {
    if open.marker != close.marker {
        return false;
    }

    // If the opening can close or the closing can open,
    // and the close size *is not* a multiple of three,
    // but the sum of the opening and closing size *is*
    // multiple of three, then **don’t** match.
    if (open.close || close.open) && close.run_size % 3 != 0 && (open.run_size + close.run_size) % 3 == 0 {
        return false;
    }

    // For GFM strikethrough, both sequences must have the same size.
    close.marker != b'~' || close.size == open.size
}

/// Match two sequences.
#[allow(clippy::too_many_lines)]
fn match_sequences(tokenizer: &mut Tokenizer, sequences: &mut [Sequence], open: usize, close: usize) {
    // Number of markers to use from the sequence.
    let take = if sequences[open].size > 1 && sequences[close].size > 1 {
        2
    } else {
        1
    };

    let (group_name, seq_name, text_name) = if sequences[open].marker == b'~' {
        (
            Name::GfmStrikethrough,
            Name::GfmStrikethroughSequence,
            Name::GfmStrikethroughText,
        )
    } else if take == 1 {
        (Name::Emphasis, Name::EmphasisSequence, Name::EmphasisText)
    } else {
        (Name::Strong, Name::StrongSequence, Name::StrongText)
    };
    let open_index = sequences[open].index;
    let close_index = sequences[close].index;
    let open_exit = sequences[open].end_point.clone();
    let close_enter = sequences[close].start_point.clone();

    // No need to worry about `VS`, because sequences are only actual characters.
    sequences[open].size -= take;
    sequences[close].size -= take;
    sequences[open].end_point.column -= take;
    sequences[open].end_point.index -= take;
    sequences[close].start_point.column += take;
    sequences[close].start_point.index += take;

    // Opening.
    tokenizer.map.add_before(
        // Add after the current sequence (it might remain).
        open_index + 2,
        0,
        vec![
            Event {
                kind: Kind::Enter,
                name: group_name.clone(),
                point: sequences[open].end_point.clone(),
                link: None,
            },
            Event {
                kind: Kind::Enter,
                name: seq_name.clone(),
                point: sequences[open].end_point.clone(),
                link: None,
            },
            Event {
                kind: Kind::Exit,
                name: seq_name.clone(),
                point: open_exit.clone(),
                link: None,
            },
            Event {
                kind: Kind::Enter,
                name: text_name.clone(),
                point: open_exit,
                link: None,
            },
        ],
    );
    // Closing.
    tokenizer.map.add(
        close_index,
        0,
        vec![
            Event {
                kind: Kind::Exit,
                name: text_name,
                point: close_enter.clone(),
                link: None,
            },
            Event {
                kind: Kind::Enter,
                name: seq_name.clone(),
                point: close_enter,
                link: None,
            },
            Event {
                kind: Kind::Exit,
                name: seq_name,
                point: sequences[close].start_point.clone(),
                link: None,
            },
            Event {
                kind: Kind::Exit,
                name: group_name,
                point: sequences[close].start_point.clone(),
                link: None,
            },
        ],
    );

    // Remove closing sequence if fully used.
    if sequences[close].size == 0 {
        tokenizer.map.add(close_index, 2, vec![]);
    } else {
        // Shift remaining closing sequence forward.
        // Do it here because a sequence can open and close different
        // other sequences, and the remainder can be on any side or
        // somewhere in the middle.
        tokenizer.events[close_index].point = sequences[close].start_point.clone();
    }

    if sequences[open].size == 0 {
        tokenizer.map.add(open_index, 2, vec![]);
    } else {
        tokenizer.events[open_index + 1].point = sequences[open].end_point.clone();
    }
}
