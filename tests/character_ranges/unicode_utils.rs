//! Unicode and Text Handling Utilities for Character Range Testing
//!
//! This module provides utilities for handling Unicode characters, multi-line text,
//! and complex text scenarios in character range tests.

use unicode_width::UnicodeWidthStr;

/// Unicode-aware character counting utilities
pub struct UnicodeUtils;

impl UnicodeUtils {
    /// Count Unicode grapheme clusters (user-perceived characters)
    pub fn grapheme_count(text: &str) -> usize {
        use unicode_segmentation::UnicodeSegmentation;
        text.graphemes(true).count()
    }

    /// Get the display width of text (accounting for wide characters)
    pub fn display_width(text: &str) -> usize {
        text.width()
    }

    /// Check if text contains emoji (simplified check)
    pub fn has_emoji(text: &str) -> bool {
        text.chars().any(|c| {
            // Simple emoji detection based on Unicode ranges
            let code = c as u32;
            (0x1F600..=0x1F64F).contains(&code) || // Emoticons
            (0x1F300..=0x1F5FF).contains(&code) || // Misc Symbols and Pictographs
            (0x1F680..=0x1F6FF).contains(&code) || // Transport and Map
            (0x2600..=0x26FF).contains(&code) // Misc symbols
        })
    }

    /// Check if text contains RTL (right-to-left) characters (simplified)
    pub fn has_rtl_characters(text: &str) -> bool {
        text.chars().any(|c| {
            let code = c as u32;
            // Hebrew: U+0590–U+05FF, Arabic: U+0600–U+06FF
            (0x0590..=0x05FF).contains(&code) || (0x0600..=0x06FF).contains(&code)
        })
    }
}

/// Multi-line text extraction helpers
pub struct MultiLineUtils;

impl MultiLineUtils {
    /// Extract text range across multiple lines with proper Unicode handling
    pub fn extract_range_unicode_aware(
        content: &str,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        let lines: Vec<&str> = content.lines().collect();

        if start_line == end_line {
            // Single line extraction
            if let Some(line) = lines.get(start_line - 1) {
                use unicode_segmentation::UnicodeSegmentation;
                let graphemes: Vec<&str> = line.graphemes(true).collect();
                let start_idx = (start_col - 1).min(graphemes.len());
                let end_idx = (end_col - 1).min(graphemes.len());
                return graphemes[start_idx..end_idx].join("");
            }
        } else {
            // Multi-line extraction
            let mut result = String::new();

            for line_num in start_line..=end_line {
                if let Some(line) = lines.get(line_num - 1) {
                    use unicode_segmentation::UnicodeSegmentation;
                    let graphemes: Vec<&str> = line.graphemes(true).collect();

                    if line_num == start_line {
                        // First line: from start column to end
                        let start_idx = (start_col - 1).min(graphemes.len());
                        result.push_str(&graphemes[start_idx..].join(""));
                    } else if line_num == end_line {
                        // Last line: from start to end column
                        if !result.is_empty() {
                            result.push('\n');
                        }
                        let end_idx = (end_col - 1).min(graphemes.len());
                        result.push_str(&graphemes[..end_idx].join(""));
                    } else {
                        // Middle lines: entire line
                        if !result.is_empty() {
                            result.push('\n');
                        }
                        result.push_str(line);
                    }
                }
            }

            return result;
        }

        String::new()
    }

    /// Split content into lines preserving line ending information
    pub fn split_with_endings(content: &str) -> Vec<(String, LineEnding)> {
        let mut result = Vec::new();
        let mut current_line = String::new();
        let mut chars = content.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next(); // consume \n
                        result.push((current_line.clone(), LineEnding::CRLF));
                    } else {
                        result.push((current_line.clone(), LineEnding::CR));
                    }
                    current_line.clear();
                }
                '\n' => {
                    result.push((current_line.clone(), LineEnding::LF));
                    current_line.clear();
                }
                _ => {
                    current_line.push(ch);
                }
            }
        }

        // Add final line if it doesn't end with a line ending
        if !current_line.is_empty() {
            result.push((current_line, LineEnding::None));
        }

        result
    }
}

/// Line ending types
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineEnding {
    LF,   // Unix (\n)
    CRLF, // Windows (\r\n)
    CR,   // Classic Mac (\r)
    None, // No line ending (last line)
}

/// Zero-width character handling utilities
pub struct ZeroWidthUtils;

impl ZeroWidthUtils {
    /// Check if character is zero-width
    pub fn is_zero_width(ch: char) -> bool {
        matches!(
            ch,
            '\u{200B}' | // Zero Width Space
            '\u{200C}' | // Zero Width Non-Joiner
            '\u{200D}' | // Zero Width Joiner
            '\u{2060}' | // Word Joiner
            '\u{FEFF}' // Zero Width No-Break Space (BOM)
        ) || unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1) == 0
    }

    /// Remove zero-width characters from text
    pub fn remove_zero_width(text: &str) -> String {
        text.chars()
            .filter(|&ch| !Self::is_zero_width(ch))
            .collect()
    }

    /// Count visible characters (excluding zero-width)
    pub fn visible_char_count(text: &str) -> usize {
        text.chars().filter(|&ch| !Self::is_zero_width(ch)).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unicode_utils_grapheme_count() {
        assert_eq!(UnicodeUtils::grapheme_count("café"), 4);
        assert_eq!(UnicodeUtils::grapheme_count("👨‍👩‍👧‍👦"), 1); // Family emoji
    }

    #[test]
    fn test_unicode_utils_display_width() {
        assert_eq!(UnicodeUtils::display_width("hello"), 5);
        assert_eq!(UnicodeUtils::display_width("こんにちは"), 10); // Wide characters
    }

    #[test]
    fn test_unicode_utils_has_emoji() {
        assert!(UnicodeUtils::has_emoji("Hello 🎉"));
        assert!(!UnicodeUtils::has_emoji("Hello world"));
    }

    #[test]
    fn test_unicode_utils_has_rtl() {
        assert!(UnicodeUtils::has_rtl_characters("Hello עברית"));
        assert!(!UnicodeUtils::has_rtl_characters("Hello world"));
    }

    #[test]
    fn test_zero_width_utils() {
        assert!(ZeroWidthUtils::is_zero_width('\u{200B}'));
        assert!(!ZeroWidthUtils::is_zero_width('a'));

        let text_with_zw = "hello\u{200B}world";
        assert_eq!(
            ZeroWidthUtils::remove_zero_width(text_with_zw),
            "helloworld"
        );
        assert_eq!(ZeroWidthUtils::visible_char_count(text_with_zw), 10);
    }

    #[test]
    fn test_multiline_utils_extract_range() {
        let content = "Line 1\nLine 2\nLine 3";
        let result = MultiLineUtils::extract_range_unicode_aware(content, 1, 6, 2, 5);
        assert_eq!(result, "1\nLine"); // Fixed expectation
    }

    #[test]
    fn test_multiline_utils_split_with_endings() {
        let content = "Line 1\r\nLine 2\nLine 3\r";
        let lines = MultiLineUtils::split_with_endings(content);

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], ("Line 1".to_string(), LineEnding::CRLF));
        assert_eq!(lines[1], ("Line 2".to_string(), LineEnding::LF));
        assert_eq!(lines[2], ("Line 3".to_string(), LineEnding::CR));
    }
}
