//! Position encoding for the language server.
//!
//! LSP positions are measured in **UTF-16 code units** (LSP 3.17
//! `PositionEncodingKind::UTF16`), which rumdl does not renegotiate at
//! initialize time. rumdl itself works in bytes (parser offsets, regex
//! matches) and in characters (`LintWarning::column`). Every position that
//! crosses the protocol boundary goes through this module, so a non-BMP
//! codepoint - an emoji, a supplementary-plane CJK ideograph - counts as the
//! two code units the client expects rather than one byte, char, or column.

use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent};

/// Convert a UTF-16 code unit offset to the corresponding byte offset in a UTF-8 string.
///
/// Returns `None` if `utf16_offset` is beyond the end of the string.
pub(super) fn utf16_to_byte_offset(s: &str, utf16_offset: usize) -> Option<usize> {
    let mut byte_pos = 0;
    let mut utf16_pos = 0;
    for ch in s.chars() {
        if utf16_pos >= utf16_offset {
            return Some(byte_pos);
        }
        byte_pos += ch.len_utf8();
        utf16_pos += ch.len_utf16();
    }
    // Cursor at the very end of the string is valid.
    if utf16_pos >= utf16_offset {
        Some(byte_pos)
    } else {
        None
    }
}

/// Convert a byte offset to the corresponding UTF-16 code unit offset in a UTF-8 string.
///
/// Panics if `byte_offset` is not on a character boundary.
pub(super) fn byte_to_utf16_offset(s: &str, byte_offset: usize) -> u32 {
    s[..byte_offset].chars().map(|c| c.len_utf16() as u32).sum()
}

/// The length of a line in UTF-16 code units, which is the LSP character
/// position just past its last character.
pub(super) fn utf16_len(s: &str) -> u32 {
    s.chars().map(|c| c.len_utf16() as u32).sum()
}

/// Convert a 1-indexed character column into an LSP character position.
///
/// `LintWarning` columns count characters, so a column is only the same number
/// as an LSP position while the line stays inside the Basic Multilingual Plane.
/// `line_text` is the line the column refers to; a column beyond its end (an
/// exclusive end column is one such) keeps the overshoot, and a line the
/// document does not have leaves the column as-is, which is the best available
/// answer for a warning that points outside the text.
pub(super) fn char_column_to_utf16(line_text: Option<&str>, column: usize) -> u32 {
    let char_index = column.saturating_sub(1);
    let Some(line_text) = line_text else {
        return char_index as u32;
    };
    let mut counted = 0;
    let mut utf16 = 0u32;
    for ch in line_text.chars().take(char_index) {
        counted += 1;
        utf16 += ch.len_utf16() as u32;
    }
    utf16 + (char_index - counted) as u32
}

/// Convert a byte range of analysis text into an LSP `Range` of `text`.
///
/// Analysis text is `text` without its leading byte order mark, which the client
/// still counts as a character of the first line. The range is shifted past the
/// mark before it is converted.
pub(super) fn analysis_range_to_lsp_range(text: &str, byte_range: std::ops::Range<usize>) -> Option<Range> {
    let shift = crate::utils::bom::bom_len(text);
    byte_range_to_lsp_range(text, byte_range.start + shift..byte_range.end + shift)
}

/// Convert a byte range of `text` itself into an LSP `Range`.
///
/// A range at or past the end of the text resolves to the end position rather
/// than failing, so a fix that deletes trailing content still has somewhere to
/// point.
pub(super) fn byte_range_to_lsp_range(text: &str, byte_range: std::ops::Range<usize>) -> Option<Range> {
    byte_ranges_to_lsp_ranges(text, std::slice::from_ref(&byte_range))?.pop()
}

/// Convert byte ranges of `text` itself into LSP `Range`s, in the order given.
///
/// The text is read once however many ranges there are, so a fix that carries
/// an edit for every line of a long paragraph costs one pass rather than one
/// pass per edit. A range that reaches past the end of the text, or that ends
/// before it starts, has no LSP form, and then none of the ranges is answered.
pub(super) fn byte_ranges_to_lsp_ranges(text: &str, byte_ranges: &[std::ops::Range<usize>]) -> Option<Vec<Range>> {
    if let Some(unanswerable) = byte_ranges
        .iter()
        .find(|range| range.start > range.end || range.end > text.len())
    {
        log::warn!(
            "Failed to convert byte range {:?} to LSP range for text of length {}",
            unanswerable,
            text.len()
        );
        return None;
    }

    let mut offsets: Vec<(usize, usize)> = byte_ranges
        .iter()
        .enumerate()
        .flat_map(|(index, range)| [(range.start, index * 2), (range.end, index * 2 + 1)])
        .collect();
    offsets.sort_unstable();

    let mut positions = vec![Position::default(); offsets.len()];
    let mut pending = offsets.iter().peekable();
    let mut position = Position::default();
    let mut byte_pos = 0;

    for ch in text.chars() {
        while let Some(&(_, slot)) = pending.next_if(|&&(offset, _)| offset <= byte_pos) {
            positions[slot] = position;
        }
        if pending.peek().is_none() {
            break;
        }
        if ch == '\n' {
            position.line += 1;
            position.character = 0;
        } else {
            position.character += ch.len_utf16() as u32;
        }
        byte_pos += ch.len_utf8();
    }
    for &(_, slot) in pending {
        positions[slot] = position;
    }

    Some(
        positions
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[start, end]| Range { start, end })
            .collect(),
    )
}

/// The position just past the last character of the text.
pub(super) fn end_of_text(text: &str) -> Position {
    let last_line = text.rsplit('\n').next().unwrap_or("");
    Position {
        line: text.matches('\n').count() as u32,
        character: utf16_len(last_line),
    }
}

/// Convert an LSP position into a byte offset in `text`.
///
/// Lines end at `\n`, `\r\n` or `\r`, as the protocol defines them. A
/// position past the end of its line resolves to the line's end, and a line
/// past the end of the document resolves to the end of the text, which is how
/// the specification says a client's out-of-range position is read.
pub(super) fn lsp_position_to_byte_offset(text: &str, position: Position) -> usize {
    let bytes = text.as_bytes();
    let mut line_start = 0;
    for _ in 0..position.line {
        let Some(terminator) = bytes[line_start..].iter().position(|&b| b == b'\n' || b == b'\r') else {
            return text.len();
        };
        let terminator = line_start + terminator;
        line_start = if bytes[terminator] == b'\r' && bytes.get(terminator + 1) == Some(&b'\n') {
            terminator + 2
        } else {
            terminator + 1
        };
    }
    let line_end = bytes[line_start..]
        .iter()
        .position(|&b| b == b'\n' || b == b'\r')
        .map_or(text.len(), |offset| line_start + offset);
    let line = &text[line_start..line_end];
    line_start + utf16_to_byte_offset(line, position.character as usize).unwrap_or(line.len())
}

/// Apply one `textDocument/didChange` notification's content changes to
/// `text`, in order: each change's range refers to the document as the
/// changes before it left it, and a change without a range replaces the
/// whole document.
pub(super) fn apply_content_changes(
    mut text: String,
    changes: impl IntoIterator<Item = TextDocumentContentChangeEvent>,
) -> String {
    for change in changes {
        match change.range {
            None => text = change.text,
            Some(range) => {
                let start = lsp_position_to_byte_offset(&text, range.start);
                let end = lsp_position_to_byte_offset(&text, range.end).max(start);
                text.replace_range(start..end, &change.text);
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// U+1F389 PARTY POPPER: 4 UTF-8 bytes, 1 char, 2 UTF-16 code units.
    const EMOJI: &str = "🎉";

    #[test]
    fn byte_offset_past_a_non_bmp_codepoint_counts_both_code_units() {
        let line = format!("a{EMOJI}b");
        assert_eq!(byte_to_utf16_offset(&line, 0), 0);
        assert_eq!(byte_to_utf16_offset(&line, 1), 1);
        assert_eq!(byte_to_utf16_offset(&line, 5), 3);
        assert_eq!(byte_to_utf16_offset(&line, 6), 4);
    }

    #[test]
    fn utf16_offset_round_trips_back_to_the_byte_offset() {
        let line = format!("a{EMOJI}b");
        for byte_offset in [0, 1, 5, 6] {
            let utf16 = byte_to_utf16_offset(&line, byte_offset);
            assert_eq!(utf16_to_byte_offset(&line, utf16 as usize), Some(byte_offset));
        }
    }

    #[test]
    fn utf16_len_is_the_position_past_the_last_character() {
        assert_eq!(utf16_len(""), 0);
        assert_eq!(utf16_len("abc"), 3);
        assert_eq!(utf16_len("héllo"), 5);
        assert_eq!(utf16_len(&format!("a{EMOJI}b")), 4);
    }

    #[test]
    fn a_character_column_after_a_non_bmp_codepoint_shifts_by_one() {
        let line = format!("a{EMOJI}b");
        // Column 3 is 'b': one character for 'a', one for the emoji.
        assert_eq!(char_column_to_utf16(Some(&line), 3), 3);
        // The exclusive end of 'b' is one past the line's last character.
        assert_eq!(char_column_to_utf16(Some(&line), 4), 4);
    }

    #[test]
    fn a_column_past_the_end_of_the_line_keeps_its_overshoot() {
        assert_eq!(char_column_to_utf16(Some("abc"), 4), 3);
        assert_eq!(char_column_to_utf16(Some("abc"), 6), 5);
        assert_eq!(char_column_to_utf16(None, 4), 3);
    }

    #[test]
    fn a_column_of_zero_clamps_to_the_start_of_the_line() {
        assert_eq!(char_column_to_utf16(Some("abc"), 0), 0);
        assert_eq!(char_column_to_utf16(Some("abc"), 1), 0);
    }

    #[test]
    fn analysis_range_to_lsp_range_skips_the_byte_order_mark() {
        // "Hello" starts after the mark: one code unit on the client's line.
        let range = analysis_range_to_lsp_range("\u{feff}Hello\nWorld", 0..5).unwrap();
        assert_eq!((range.start.character, range.end.character), (1, 6));
        let range = analysis_range_to_lsp_range("\u{feff}Hello\nWorld", 6..11).unwrap();
        assert_eq!(
            (range.start.line, range.start.character, range.end.character),
            (1, 0, 5)
        );
        // Without a mark it is the plain conversion; a second mark is text.
        let range = analysis_range_to_lsp_range("Hello", 0..5).unwrap();
        assert_eq!((range.start.character, range.end.character), (0, 5));
        let range = analysis_range_to_lsp_range("\u{feff}\u{feff}Hi", 0..5).unwrap();
        // One unit for the stripped mark, then the second mark and "Hi".
        assert_eq!((range.start.character, range.end.character), (1, 4));
    }

    #[test]
    fn byte_ranges_are_answered_in_the_order_given_whatever_their_order_in_the_text() {
        let text = format!("a{EMOJI}b\nWorld\nTest");
        let ranges = [12..16, 0..1, 5..6, 7..12, 16..17];
        let together = byte_ranges_to_lsp_ranges(&text, &ranges).unwrap();
        let alone: Vec<Range> = ranges
            .iter()
            .map(|range| byte_range_to_lsp_range(&text, range.clone()).unwrap())
            .collect();
        assert_eq!(together, alone);
        assert_eq!(together[0].start, Position { line: 1, character: 5 });
        assert_eq!(together[0].end, Position { line: 2, character: 3 });
        assert_eq!(together[2].start, Position { line: 0, character: 3 });
        assert_eq!(together[4].end, Position { line: 2, character: 4 });
    }

    /// The time to convert one range on each of `lines` lines, as the fastest
    /// of five runs.
    fn one_range_a_line_time(lines: usize) -> std::time::Duration {
        let line = "a line of some paragraph\n";
        let text = line.repeat(lines);
        let ranges: Vec<_> = (0..lines)
            .map(|index| index * line.len()..index * line.len() + 1)
            .collect();
        (0..5)
            .map(|_| {
                let start = std::time::Instant::now();
                let converted = byte_ranges_to_lsp_ranges(&text, &ranges).unwrap();
                let elapsed = start.elapsed();
                assert_eq!(converted.len(), lines);
                assert_eq!(converted[lines - 1].start.line as usize, lines - 1);
                elapsed
            })
            .min()
            .unwrap()
    }

    #[test]
    fn byte_ranges_to_lsp_ranges_linear_complexity() {
        // A fix can carry an edit for every line of a paragraph. Reading the
        // text once for all of them costs the same per line at any length;
        // reading it from the start for each costs the square of the length,
        // sixteen times as much for four times the lines.
        let short = one_range_a_line_time(10_000);
        let long = one_range_a_line_time(40_000);
        let ratio = long.as_secs_f64() / short.as_secs_f64();
        println!("convert: {short:?} for 10000 lines, {long:?} for 40000 ({ratio:.2}x)");
        assert!(
            ratio < 8.0,
            "converting a range a line grows faster than the text: {short:?} for 10000 lines, {long:?} for 40000 \
             ({ratio:.2}x)"
        );
    }

    #[test]
    fn one_range_without_an_lsp_form_leaves_none_answered() {
        let reversed = std::ops::Range { start: 3, end: 1 };
        assert_eq!(byte_range_to_lsp_range("Hello", reversed.clone()), None);
        assert_eq!(byte_ranges_to_lsp_ranges("Hello", &[0..1, reversed]), None);
        assert_eq!(byte_ranges_to_lsp_ranges("Hello", &[0..1, 5..6]), None);
    }

    #[test]
    fn byte_range_to_lsp_range_maps_a_span_within_one_line() {
        let range = byte_range_to_lsp_range("Hello\nWorld", 0..5).unwrap();
        assert_eq!(range.start, Position { line: 0, character: 0 });
        assert_eq!(range.end, Position { line: 0, character: 5 });
    }

    #[test]
    fn byte_range_to_lsp_range_counts_lines_from_the_newlines_it_passes() {
        let range = byte_range_to_lsp_range("Hello\nWorld\nTest", 6..11).unwrap();
        assert_eq!(range.start, Position { line: 1, character: 0 });
        assert_eq!(range.end, Position { line: 1, character: 5 });
    }

    #[test]
    fn byte_range_to_lsp_range_counts_a_bmp_codepoint_as_one_code_unit() {
        // Each of the two ideographs is three UTF-8 bytes and one code unit.
        let range = byte_range_to_lsp_range("Hello 世界\nTest", 6..12).unwrap();
        assert_eq!(range.start, Position { line: 0, character: 6 });
        assert_eq!(range.end, Position { line: 0, character: 8 });
    }

    #[test]
    fn byte_range_to_lsp_range_counts_a_non_bmp_codepoint_as_a_surrogate_pair() {
        // Byte 5 is 'b': one byte for 'a' and four for the emoji.
        let range = byte_range_to_lsp_range(&format!("a{EMOJI}b"), 5..6).unwrap();
        assert_eq!(range.start, Position { line: 0, character: 3 });
        assert_eq!(range.end, Position { line: 0, character: 4 });
    }

    #[test]
    fn byte_range_to_lsp_range_answers_an_empty_range_at_the_end_of_the_text() {
        let text = "Hello\nWorld";
        let range = byte_range_to_lsp_range(text, text.len()..text.len()).unwrap();
        assert_eq!(range.start, Position { line: 1, character: 5 });
        assert_eq!(range.end, range.start);

        let text = "Hello\nWorld\n";
        let range = byte_range_to_lsp_range(text, text.len()..text.len()).unwrap();
        assert_eq!(range.start, Position { line: 2, character: 0 });
        assert_eq!(range.end, range.start);
    }

    #[test]
    fn byte_range_to_lsp_range_spans_a_trailing_blank_line() {
        let range = byte_range_to_lsp_range("line1\nline2\n\n", 12..13).unwrap();
        assert_eq!(range.start, Position { line: 2, character: 0 });
        assert_eq!(range.end, Position { line: 3, character: 0 });
    }

    #[test]
    fn byte_range_to_lsp_range_rejects_a_range_past_the_end_of_the_text() {
        assert_eq!(byte_range_to_lsp_range("Hello", 10..15), None);
    }

    #[test]
    fn end_of_text_is_measured_in_code_units() {
        assert_eq!(end_of_text(""), Position { line: 0, character: 0 });
        assert_eq!(end_of_text("abc"), Position { line: 0, character: 3 });
        assert_eq!(end_of_text("abc\n"), Position { line: 1, character: 0 });
        assert_eq!(
            end_of_text(&format!("one\na{EMOJI}b")),
            Position { line: 1, character: 4 }
        );
    }
}
