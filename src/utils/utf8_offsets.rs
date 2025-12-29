//! UTF-8 byte offset to character offset conversion utilities.
//!
//! JavaScript uses UTF-16 code units for string indexing, while Rust uses bytes.
//! This module provides functions to convert between byte offsets and character
//! offsets for proper interoperability with JavaScript/WASM environments.

/// Convert a byte offset to a character offset in a UTF-8 string.
///
/// JavaScript uses UTF-16 code units for string indexing, while Rust uses bytes.
/// For most characters this is the same, but multi-byte UTF-8 characters
/// (like `æ` = 2 bytes, emoji = 4 bytes) need conversion.
///
/// # Arguments
/// * `content` - The UTF-8 string
/// * `byte_offset` - The byte offset to convert
///
/// # Returns
/// The corresponding character offset
///
/// # Examples
/// ```
/// use rumdl::utils::utf8_offsets::byte_offset_to_char_offset;
///
/// // ASCII: bytes == characters
/// assert_eq!(byte_offset_to_char_offset("Hello", 5), 5);
///
/// // Norwegian æ is 2 bytes in UTF-8, 1 character
/// assert_eq!(byte_offset_to_char_offset("æ", 2), 1);
///
/// // Mixed content
/// let content = "Hello æ"; // 6 bytes + 2 bytes = 8 bytes, 7 characters
/// assert_eq!(byte_offset_to_char_offset(content, 8), 7);
/// ```
/// Convert a 1-indexed byte column to a 1-indexed character column within a line.
///
/// This is used to convert column positions in warnings from byte offsets
/// to character offsets for JavaScript compatibility.
///
/// # Arguments
/// * `line_content` - The content of the specific line
/// * `byte_column` - The 1-indexed byte column within the line
///
/// # Returns
/// The corresponding 1-indexed character column
pub fn byte_column_to_char_column(line_content: &str, byte_column: usize) -> usize {
    if byte_column <= 1 {
        return 1;
    }

    // Convert to 0-indexed byte offset
    let byte_offset = byte_column - 1;

    // Convert byte offset to character offset
    let char_offset = byte_offset_to_char_offset(line_content, byte_offset);

    // Convert back to 1-indexed
    char_offset + 1
}

/// Get the content of a specific line (1-indexed) from the full content.
pub fn get_line_content(content: &str, line_number: usize) -> Option<&str> {
    if line_number == 0 {
        return None;
    }
    content.lines().nth(line_number - 1)
}

pub fn byte_offset_to_char_offset(content: &str, byte_offset: usize) -> usize {
    // Handle edge cases
    if byte_offset == 0 {
        return 0;
    }

    if byte_offset >= content.len() {
        return content.chars().count();
    }

    // Count characters up to the byte offset
    content
        .char_indices()
        .take_while(|(byte_idx, _)| *byte_idx < byte_offset)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_offset_to_char_offset_empty() {
        assert_eq!(byte_offset_to_char_offset("", 0), 0);
        assert_eq!(byte_offset_to_char_offset("", 1), 0);
    }

    #[test]
    fn test_byte_offset_to_char_offset_ascii() {
        // ASCII-only string: bytes == characters
        let content = "Hello World";
        assert_eq!(byte_offset_to_char_offset(content, 0), 0);
        assert_eq!(byte_offset_to_char_offset(content, 5), 5);
        assert_eq!(byte_offset_to_char_offset(content, 11), 11);
        // Beyond end
        assert_eq!(byte_offset_to_char_offset(content, 100), 11);
    }

    #[test]
    fn test_byte_offset_to_char_offset_norwegian() {
        // Norwegian æ is 2 bytes in UTF-8
        let content = "æ"; // 2 bytes, 1 character
        assert_eq!(content.len(), 2); // 2 bytes
        assert_eq!(content.chars().count(), 1); // 1 character
        assert_eq!(byte_offset_to_char_offset(content, 0), 0);
        assert_eq!(byte_offset_to_char_offset(content, 2), 1); // End of string
    }

    #[test]
    fn test_byte_offset_to_char_offset_mixed() {
        // Mixed ASCII and multi-byte: "Hello æ world"
        let content = "Hello æ world";
        // Bytes: H(1) e(1) l(1) l(1) o(1) ' '(1) æ(2) ' '(1) w(1) o(1) r(1) l(1) d(1) = 14 bytes
        // Chars: H   e    l    l    o    ' '   æ    ' '   w    o    r    l    d    = 13 chars
        assert_eq!(content.len(), 14); // 14 bytes
        assert_eq!(content.chars().count(), 13); // 13 characters

        // Before æ
        assert_eq!(byte_offset_to_char_offset(content, 6), 6); // Space before æ
        // After æ (byte 8 = char 7)
        assert_eq!(byte_offset_to_char_offset(content, 8), 7); // Space after æ
        // End of string (byte 14 = char 13)
        assert_eq!(byte_offset_to_char_offset(content, 14), 13);
    }

    #[test]
    fn test_byte_offset_to_char_offset_emoji() {
        // Emoji is 4 bytes in UTF-8
        let content = "Hi 👋"; // "Hi " (3 bytes) + wave (4 bytes) = 7 bytes, 4 chars
        assert_eq!(content.len(), 7);
        assert_eq!(content.chars().count(), 4);
        assert_eq!(byte_offset_to_char_offset(content, 3), 3); // Before emoji
        assert_eq!(byte_offset_to_char_offset(content, 7), 4); // End of string
    }

    #[test]
    fn test_byte_offset_to_char_offset_norwegian_sentence() {
        // This is the exact bug case: Norwegian letter at end of file
        let content = "# Heading\n\nContent with Norwegian letter \"æ\".";
        assert_eq!(content.len(), 46); // 46 bytes (æ is 2 bytes)
        assert_eq!(content.chars().count(), 45); // 45 characters (æ is 1 char)

        // End of file: byte offset 46 should convert to character offset 45
        assert_eq!(byte_offset_to_char_offset(content, 46), 45);
    }

    #[test]
    fn test_byte_offset_to_char_offset_multiple_multibyte() {
        // String with multiple multi-byte characters
        let content = "café résumé"; // c(1) a(1) f(1) é(2) ' '(1) r(1) é(2) s(1) u(1) m(1) é(2) = 14 bytes, 11 chars
        assert_eq!(content.len(), 14);
        assert_eq!(content.chars().count(), 11);

        assert_eq!(byte_offset_to_char_offset(content, 0), 0);
        assert_eq!(byte_offset_to_char_offset(content, 3), 3); // Before first é
        assert_eq!(byte_offset_to_char_offset(content, 5), 4); // After first é
        assert_eq!(byte_offset_to_char_offset(content, 14), 11); // End
    }

    #[test]
    fn test_byte_column_to_char_column() {
        // Line with Norwegian æ
        let line = "Content with Norwegian letter \"æ\".";
        // Bytes: 35 (æ is 2 bytes)
        // Chars: 34 (æ is 1 char)
        assert_eq!(line.len(), 35);
        assert_eq!(line.chars().count(), 34);

        // Column 1 stays 1
        assert_eq!(byte_column_to_char_column(line, 1), 1);

        // Before æ: columns are the same (all ASCII so far)
        assert_eq!(byte_column_to_char_column(line, 30), 30);

        // At æ position: byte column 32 = char column 32 (æ is at char index 31, column 32)
        assert_eq!(byte_column_to_char_column(line, 32), 32);

        // After æ: byte column 34 = char column 33 (quote after æ is at char index 32)
        assert_eq!(byte_column_to_char_column(line, 34), 33);

        // End of line: byte column 36 = char column 35 (1 past end)
        assert_eq!(byte_column_to_char_column(line, 36), 35);
    }

    #[test]
    fn test_byte_column_to_char_column_edge_cases() {
        // Empty string
        assert_eq!(byte_column_to_char_column("", 1), 1);
        assert_eq!(byte_column_to_char_column("", 0), 1);

        // ASCII only - no conversion needed
        let ascii = "Hello World";
        assert_eq!(byte_column_to_char_column(ascii, 1), 1);
        assert_eq!(byte_column_to_char_column(ascii, 6), 6);
        assert_eq!(byte_column_to_char_column(ascii, 12), 12); // Past end

        // Multiple multi-byte characters in sequence
        let multi = "æøå"; // 6 bytes, 3 chars
        assert_eq!(multi.len(), 6);
        assert_eq!(multi.chars().count(), 3);
        assert_eq!(byte_column_to_char_column(multi, 1), 1); // Start of æ
        assert_eq!(byte_column_to_char_column(multi, 3), 2); // Start of ø
        assert_eq!(byte_column_to_char_column(multi, 5), 3); // Start of å
        assert_eq!(byte_column_to_char_column(multi, 7), 4); // Past end

        // Emoji (4 bytes)
        let emoji = "Hi 👋!"; // 3 + 4 + 1 = 8 bytes, 5 chars
        assert_eq!(emoji.len(), 8);
        assert_eq!(emoji.chars().count(), 5);
        assert_eq!(byte_column_to_char_column(emoji, 4), 4); // Start of emoji
        assert_eq!(byte_column_to_char_column(emoji, 8), 5); // The "!"
        assert_eq!(byte_column_to_char_column(emoji, 9), 6); // Past end

        // Line with only multi-byte characters
        let only_multi = "日本語"; // 9 bytes (3 chars × 3 bytes each)
        assert_eq!(only_multi.len(), 9);
        assert_eq!(only_multi.chars().count(), 3);
        assert_eq!(byte_column_to_char_column(only_multi, 1), 1);
        assert_eq!(byte_column_to_char_column(only_multi, 4), 2);
        assert_eq!(byte_column_to_char_column(only_multi, 7), 3);
        assert_eq!(byte_column_to_char_column(only_multi, 10), 4);
    }

    #[test]
    fn test_byte_column_to_char_column_bug_scenario() {
        // This tests the exact scenario from issue #4:
        // A warning at the end of a line containing Norwegian letter æ
        // MD047 reports column 36 (byte-based) which should be column 35 (char-based)
        let line = "Content with Norwegian letter \"æ\".";

        // The byte position after the last character (the period)
        // Byte offset: 35 (0-indexed: 34), so byte column 36
        // Char offset: 34 (0-indexed: 33), so char column 35
        let byte_column_at_end = line.len() + 1; // 36
        let expected_char_column = line.chars().count() + 1; // 35

        assert_eq!(
            byte_column_to_char_column(line, byte_column_at_end),
            expected_char_column,
            "End-of-line column should be converted from byte {byte_column_at_end} to char {expected_char_column}"
        );

        // Also verify that when combined with line.from, we get the correct position
        // In the full document "# Heading\n\nContent with Norwegian letter \"æ\"."
        // Line 3 starts at character position 11 (after "# Heading\n\n")
        // The fix should apply at position 45 (11 + 34), not 46 (11 + 35)
        let line_from = 11_usize;
        let from_position = line_from + (expected_char_column - 1);
        assert_eq!(from_position, 45, "Fix position should be 45, not 46");
    }

    #[test]
    fn test_get_line_content() {
        let content = "# Heading\n\nContent with Norwegian letter \"æ\".";

        assert_eq!(get_line_content(content, 1), Some("# Heading"));
        assert_eq!(get_line_content(content, 2), Some(""));
        assert_eq!(
            get_line_content(content, 3),
            Some("Content with Norwegian letter \"æ\".")
        );
        assert_eq!(get_line_content(content, 4), None);
        assert_eq!(get_line_content(content, 0), None);
    }

    #[test]
    fn test_get_line_content_edge_cases() {
        // Empty content
        assert_eq!(get_line_content("", 1), None);
        assert_eq!(get_line_content("", 0), None);

        // Single line without newline
        assert_eq!(get_line_content("Hello", 1), Some("Hello"));
        assert_eq!(get_line_content("Hello", 2), None);

        // Multiple empty lines
        let content = "\n\n\n";
        assert_eq!(get_line_content(content, 1), Some(""));
        assert_eq!(get_line_content(content, 2), Some(""));
        assert_eq!(get_line_content(content, 3), Some(""));
        assert_eq!(get_line_content(content, 4), None);

        // Lines with various multi-byte characters
        let content = "Line 1\næøå\n日本語\n👋🎉";
        assert_eq!(get_line_content(content, 1), Some("Line 1"));
        assert_eq!(get_line_content(content, 2), Some("æøå"));
        assert_eq!(get_line_content(content, 3), Some("日本語"));
        assert_eq!(get_line_content(content, 4), Some("👋🎉"));
        assert_eq!(get_line_content(content, 5), None);
    }
}
