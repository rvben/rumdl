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
}
