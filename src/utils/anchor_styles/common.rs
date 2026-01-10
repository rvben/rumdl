//! Common utilities shared across anchor style implementations
//!
//! This module provides shared functionality for anchor generation,
//! including emoji detection, Unicode handling, and regex patterns.

use regex::Regex;
use std::sync::LazyLock;

// ============================================================================
// Shared Regex Patterns
// ============================================================================

/// Control character and dangerous Unicode filtering pattern
pub static CONTROL_CHARS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x00-\x1F\x7F-\x9F\u200B-\u200D\uFEFF]").unwrap());

/// Whitespace normalization pattern (tabs, Unicode spaces)
pub static WHITESPACE_NORMALIZE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\t\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]").unwrap());

/// Zero-width character pattern for security filtering
pub static ZERO_WIDTH_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\u200B-\u200D\u2060\uFEFF]").unwrap());

/// RTL override and dangerous Unicode control pattern
pub static DANGEROUS_UNICODE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\u202A-\u202E\u2066-\u2069\u061C\u200E\u200F]").unwrap());

// ============================================================================
// Emoji Detection
// ============================================================================

/// Check if a character is an emoji or symbol
///
/// This covers the most common emoji ranges used in headings.
/// Shared by all anchor styles.
#[inline]
pub fn is_emoji_or_symbol(c: char) -> bool {
    let code = c as u32;

    // Basic emoji ranges
    (0x1F600..=0x1F64F).contains(&code)     // Emoticons
        || (0x1F300..=0x1F5FF).contains(&code) // Miscellaneous Symbols and Pictographs
        || (0x1F680..=0x1F6FF).contains(&code) // Transport and Map Symbols
        || (0x1F900..=0x1F9FF).contains(&code) // Supplemental Symbols and Pictographs
        || (0x2600..=0x26FF).contains(&code)   // Miscellaneous Symbols
        || (0x2700..=0x27BF).contains(&code) // Dingbats
}

/// Extended emoji detection including country flags and keycaps
///
/// Used by GitHub style which has more comprehensive emoji handling.
#[inline]
pub fn is_emoji_or_symbol_extended(c: char) -> bool {
    let code = c as u32;

    // Start with basic ranges
    is_emoji_or_symbol(c)
        // Additional ranges for GitHub compatibility
        || (0x1F1E0..=0x1F1FF).contains(&code) // Regional indicator symbols (flags)
        || (0x1FA00..=0x1FA6F).contains(&code) // Chess symbols
        || (0x1FA70..=0x1FAFF).contains(&code) // Symbols and Pictographs Extended-A
        || (0x231A..=0x231B).contains(&code)   // Watch, Hourglass
        || (0x23E9..=0x23F3).contains(&code)   // Media control symbols
        || (0x23F8..=0x23FA).contains(&code)   // More media symbols
        || (0x25AA..=0x25AB).contains(&code)   // Small squares
        || code == 0x25B6                      // Play button
        || code == 0x25C0                      // Reverse button
        || (0x25FB..=0x25FE).contains(&code)   // Medium squares
        || (0x2614..=0x2615).contains(&code)   // Umbrella, Hot beverage
        || (0x2648..=0x2653).contains(&code)   // Zodiac symbols
        || code == 0x267F                      // Wheelchair symbol
        || code == 0x2693                      // Anchor
        || code == 0x26A1                      // High voltage
        || (0x26AA..=0x26AB).contains(&code)   // White/black circles
        || (0x26BD..=0x26BE).contains(&code)   // Sports balls
        || (0x26C4..=0x26C5).contains(&code)   // Snowman, Sun
        || code == 0x26CE                      // Ophiuchus
        || code == 0x26D4                      // No entry
        || code == 0x26EA                      // Church
        || (0x26F2..=0x26F3).contains(&code)   // Fountain, Golf
        || code == 0x26F5                      // Sailboat
        || code == 0x26FA                      // Tent
        || code == 0x26FD                      // Fuel pump
        || code == 0x2702                      // Scissors
        || code == 0x2705                      // Check mark
        || (0x2708..=0x270D).contains(&code)   // Airplane to writing hand
        || code == 0x270F                      // Pencil
        || code == 0x2712                      // Black nib
        || code == 0x2714                      // Heavy check
        || code == 0x2716                      // Heavy multiplication
        || code == 0x271D                      // Latin cross
        || code == 0x2721                      // Star of David
        || code == 0x2728                      // Sparkles
        || (0x2733..=0x2734).contains(&code)   // Eight spoked asterisk
        || code == 0x2744                      // Snowflake
        || code == 0x2747                      // Sparkle
        || code == 0x274C                      // Cross mark
        || code == 0x274E                      // Cross mark square
        || (0x2753..=0x2755).contains(&code)   // Question marks
        || code == 0x2757                      // Exclamation mark
        || (0x2763..=0x2764).contains(&code)   // Heart exclamation, heart
        || (0x2795..=0x2797).contains(&code)   // Plus, minus, divide
        || code == 0x27A1                      // Right arrow
        || code == 0x27B0                      // Curly loop
        || code == 0x27BF                      // Double curly loop
        || (0x2934..=0x2935).contains(&code)   // Arrows
        || (0x2B05..=0x2B07).contains(&code)   // Arrows
        || (0x2B1B..=0x2B1C).contains(&code)   // Squares
        || code == 0x2B50                      // Star
        || code == 0x2B55                      // Circle
        || code == 0x3030                      // Wavy dash
        || code == 0x303D                      // Part alternation mark
        || code == 0x3297                      // Circled Ideograph Congratulation
        || code == 0x3299                      // Circled Ideograph Secret
        || (0xFE00..=0xFE0F).contains(&code)   // Variation selectors (emoji modifiers)
        || code == 0x200D // Zero-width joiner (used in emoji sequences)
}

// ============================================================================
// Unicode Letter Detection
// ============================================================================

/// Mode for Unicode letter detection
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UnicodeLetterMode {
    /// Conservative: ASCII + common Latin extended only (Jekyll)
    Conservative,
    /// Permissive: All alphabetic except dangerous ranges (KramdownGfm)
    Permissive,
    /// Strict: ASCII only (pure Kramdown)
    AsciiOnly,
    /// GitHub: Explicit list of safe Unicode ranges with security filtering
    GitHub,
}

/// Check if a character is a safe Unicode letter based on the specified mode
#[inline]
pub fn is_safe_unicode_letter(c: char, mode: UnicodeLetterMode) -> bool {
    match mode {
        UnicodeLetterMode::AsciiOnly => c.is_ascii_alphabetic(),

        UnicodeLetterMode::Conservative => {
            // ASCII letters
            if c.is_ascii_alphabetic() {
                return true;
            }

            // Common Latin Extended characters (safe subset)
            match c as u32 {
                // Latin-1 Supplement letters (excluding symbols)
                0x00C0..=0x00D6 | 0x00D8..=0x00F6 | 0x00F8..=0x00FF => true,
                // Latin Extended-A (common European letters)
                0x0100..=0x017F => true,
                // Latin Extended Additional (common subset)
                0x1E00..=0x1EFF => true,
                _ => false,
            }
        }

        UnicodeLetterMode::Permissive => {
            // ASCII letters always allowed
            if c.is_ascii_alphabetic() {
                return true;
            }

            // Allow all alphabetic except dangerous ranges
            if c.is_alphabetic() {
                let code = c as u32;
                // Exclude dangerous ranges
                if (0xE000..=0xF8FF).contains(&code)    // Private Use Area
                    || (0xFE00..=0xFE0F).contains(&code)    // Variation Selectors
                    || (0x200B..=0x200D).contains(&code)    // Zero-width characters
                    || (0x202A..=0x202E).contains(&code)
                // Bidirectional overrides
                {
                    return false;
                }
                return true;
            }

            false
        }

        UnicodeLetterMode::GitHub => {
            let code = c as u32;

            // Exclude potentially dangerous ranges first
            if (0xE000..=0xF8FF).contains(&code)       // Private Use Area
                || (0xF0000..=0xFFFFD).contains(&code) // Supplementary Private Use Area-A
                || (0x100000..=0x10FFFD).contains(&code) // Supplementary Private Use Area-B
                || (0xFE00..=0xFE0F).contains(&code)   // Variation Selectors
                || (0xE0100..=0xE01EF).contains(&code)
            // Variation Selectors Supplement
            {
                return false;
            }

            // Allow explicit safe Unicode letter ranges
            (0x0000..=0x007F).contains(&code)    // Basic Latin
                || (0x0080..=0x00FF).contains(&code)    // Latin-1 Supplement
                || (0x0100..=0x017F).contains(&code)    // Latin Extended-A
                || (0x0180..=0x024F).contains(&code)    // Latin Extended-B
                || (0x0370..=0x03FF).contains(&code)    // Greek and Coptic
                || (0x0400..=0x04FF).contains(&code)    // Cyrillic
                || (0x0500..=0x052F).contains(&code)    // Cyrillic Supplement
                || (0x0590..=0x05FF).contains(&code)    // Hebrew
                || (0x0600..=0x06FF).contains(&code)    // Arabic
                || (0x0700..=0x074F).contains(&code)    // Syriac
                || (0x0750..=0x077F).contains(&code)    // Arabic Supplement
                || (0x1100..=0x11FF).contains(&code)    // Hangul Jamo
                || (0x3040..=0x309F).contains(&code)    // Hiragana
                || (0x30A0..=0x30FF).contains(&code)    // Katakana
                || (0x3130..=0x318F).contains(&code)    // Hangul Compatibility Jamo
                || (0x4E00..=0x9FFF).contains(&code)    // CJK Unified Ideographs
                || (0xAC00..=0xD7AF).contains(&code)    // Hangul Syllables (Korean)
                || (0xA000..=0xA48F).contains(&code)    // Yi Syllables
                || (0xA490..=0xA4CF).contains(&code) // Yi Radicals
        }
    }
}

// ============================================================================
// Input Validation
// ============================================================================

/// Maximum input length for security (10KB)
pub const MAX_INPUT_LENGTH: usize = 10240;

/// Maximum input size for permissive validation (1MB)
pub const MAX_INPUT_SIZE_LARGE: usize = 1024 * 1024;

/// Truncate input at a safe UTF-8 boundary
#[inline]
pub fn truncate_at_char_boundary(input: &str, max_len: usize) -> &str {
    if input.len() <= max_len {
        return input;
    }

    // Find the last valid char boundary before max_len
    for (byte_index, _) in input.char_indices() {
        if byte_index >= max_len {
            return &input[..byte_index];
        }
    }

    input
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_emoji_or_symbol() {
        // Basic emojis
        assert!(is_emoji_or_symbol('😀'));
        assert!(is_emoji_or_symbol('🎉'));
        assert!(is_emoji_or_symbol('❤'));

        // Not emojis
        assert!(!is_emoji_or_symbol('a'));
        assert!(!is_emoji_or_symbol('1'));
        assert!(!is_emoji_or_symbol(' '));
    }

    #[test]
    fn test_is_emoji_or_symbol_extended() {
        // Basic emojis
        assert!(is_emoji_or_symbol_extended('😀'));

        // Extended ranges
        assert!(is_emoji_or_symbol_extended('✅')); // 0x2705
        assert!(is_emoji_or_symbol_extended('⭐')); // 0x2B50

        // Not emojis
        assert!(!is_emoji_or_symbol_extended('a'));
    }

    #[test]
    fn test_is_safe_unicode_letter_modes() {
        // ASCII works in all modes
        assert!(is_safe_unicode_letter('a', UnicodeLetterMode::AsciiOnly));
        assert!(is_safe_unicode_letter('a', UnicodeLetterMode::Conservative));
        assert!(is_safe_unicode_letter('a', UnicodeLetterMode::Permissive));
        assert!(is_safe_unicode_letter('a', UnicodeLetterMode::GitHub));

        // Accented chars work in conservative, permissive, and github
        assert!(!is_safe_unicode_letter('é', UnicodeLetterMode::AsciiOnly));
        assert!(is_safe_unicode_letter('é', UnicodeLetterMode::Conservative));
        assert!(is_safe_unicode_letter('é', UnicodeLetterMode::Permissive));
        assert!(is_safe_unicode_letter('é', UnicodeLetterMode::GitHub));

        // CJK works in permissive and github modes
        assert!(!is_safe_unicode_letter('日', UnicodeLetterMode::AsciiOnly));
        assert!(!is_safe_unicode_letter('日', UnicodeLetterMode::Conservative));
        assert!(is_safe_unicode_letter('日', UnicodeLetterMode::Permissive));
        assert!(is_safe_unicode_letter('日', UnicodeLetterMode::GitHub));

        // Greek works in permissive and github modes
        assert!(!is_safe_unicode_letter('α', UnicodeLetterMode::AsciiOnly));
        assert!(!is_safe_unicode_letter('α', UnicodeLetterMode::Conservative));
        assert!(is_safe_unicode_letter('α', UnicodeLetterMode::Permissive));
        assert!(is_safe_unicode_letter('α', UnicodeLetterMode::GitHub));
    }

    #[test]
    fn test_truncate_at_char_boundary() {
        let input = "Hello, 世界!";

        // Within limit
        assert_eq!(truncate_at_char_boundary(input, 100), input);

        // Truncate at ASCII boundary
        assert_eq!(truncate_at_char_boundary(input, 5), "Hello");

        // Truncate doesn't split multi-byte chars
        let truncated = truncate_at_char_boundary(input, 8);
        assert!(truncated.is_char_boundary(truncated.len()));
    }
}
