//! Position encoding for the language server.
//!
//! LSP positions are measured in **UTF-16 code units** (LSP 3.17
//! `PositionEncodingKind::UTF16`), which rumdl does not renegotiate at
//! initialize time. rumdl itself works in bytes (parser offsets, regex
//! matches) and in characters (`LintWarning::column`). Every position that
//! crosses the protocol boundary goes through this module, so a non-BMP
//! codepoint - an emoji, a supplementary-plane CJK ideograph - counts as the
//! two code units the client expects rather than one byte, char, or column.

use tower_lsp::lsp_types::{Position, Range};

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

/// Convert a byte range into an LSP `Range`.
///
/// A range at or past the end of the text resolves to the end position rather
/// than failing, so a fix that deletes trailing content still has somewhere to
/// point.
pub(super) fn byte_range_to_lsp_range(text: &str, byte_range: std::ops::Range<usize>) -> Option<Range> {
    let mut line = 0u32;
    let mut character = 0u32;
    let mut byte_pos = 0;

    let mut start_pos = None;
    let mut end_pos = None;

    for ch in text.chars() {
        if byte_pos == byte_range.start {
            start_pos = Some(Position { line, character });
        }
        if byte_pos == byte_range.end {
            end_pos = Some(Position { line, character });
            break;
        }

        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }

        byte_pos += ch.len_utf8();
    }

    if start_pos.is_none() && byte_pos >= byte_range.start {
        start_pos = Some(Position { line, character });
    }
    if end_pos.is_none() && byte_pos >= byte_range.end {
        end_pos = Some(Position { line, character });
    }

    match (start_pos, end_pos) {
        (Some(start), Some(end)) => Some(Range { start, end }),
        _ => {
            log::warn!(
                "Failed to convert byte range {:?} to LSP range for text of length {}",
                byte_range,
                text.len()
            );
            None
        }
    }
}

/// The position just past the last character of the text.
pub(super) fn end_of_text(text: &str) -> Position {
    let last_line = text.rsplit('\n').next().unwrap_or("");
    Position {
        line: text.matches('\n').count() as u32,
        character: utf16_len(last_line),
    }
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
