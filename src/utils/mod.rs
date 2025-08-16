//!
//! Shared utilities for rumdl, including document structure analysis, code block handling, regex helpers, and string extensions.
//! Provides reusable traits and functions for rule implementations and core linter logic.

pub mod anchor_styles;
pub mod ast_utils;
pub mod code_block_utils;
pub mod document_structure;
pub mod early_returns;
pub mod element_cache;
pub mod emphasis_utils;
pub mod fix_utils;
pub mod header_id_utils;
pub mod kramdown_utils;
pub mod markdown_elements;
pub mod range_utils;
pub mod regex_cache;
pub mod skip_context;
pub mod string_interner;
pub mod table_utils;
pub mod text_reflow;

pub use ast_utils::AstCache;
pub use code_block_utils::CodeBlockUtils;
pub use document_structure::DocumentStructure;
pub use markdown_elements::{ElementQuality, ElementType, MarkdownElement, MarkdownElements};

/// Detect the predominant line ending style in content
pub fn detect_line_ending(content: &str) -> &'static str {
    let crlf_count = content.matches("\r\n").count();
    let lf_count = content.matches('\n').count() - crlf_count;

    if crlf_count > lf_count { "\r\n" } else { "\n" }
}
pub use range_utils::LineIndex;

/// Trait for string-related extensions
pub trait StrExt {
    /// Replace trailing spaces with a specified replacement string
    fn replace_trailing_spaces(&self, replacement: &str) -> String;

    /// Check if the string has trailing whitespace
    fn has_trailing_spaces(&self) -> bool;

    /// Count the number of trailing spaces in the string
    fn trailing_spaces(&self) -> usize;
}

impl StrExt for str {
    fn replace_trailing_spaces(&self, replacement: &str) -> String {
        // Custom implementation to handle both newlines and tabs specially

        // Check if string ends with newline
        let (content, ends_with_newline) = if let Some(stripped) = self.strip_suffix('\n') {
            (stripped, true)
        } else {
            (self, false)
        };

        // Find where the trailing spaces begin
        let mut non_space_len = content.len();
        for c in content.chars().rev() {
            if c == ' ' {
                non_space_len -= 1;
            } else {
                break;
            }
        }

        // Build the final string
        let mut result =
            String::with_capacity(non_space_len + replacement.len() + if ends_with_newline { 1 } else { 0 });
        result.push_str(&content[..non_space_len]);
        result.push_str(replacement);
        if ends_with_newline {
            result.push('\n');
        }

        result
    }

    fn has_trailing_spaces(&self) -> bool {
        self.trailing_spaces() > 0
    }

    fn trailing_spaces(&self) -> usize {
        // Custom implementation to handle both newlines and tabs specially

        // Prepare the string without newline if it ends with one
        let content = self.strip_suffix('\n').unwrap_or(self);

        // Count only trailing spaces at the end, not tabs
        let mut space_count = 0;
        for c in content.chars().rev() {
            if c == ' ' {
                space_count += 1;
            } else {
                break;
            }
        }

        space_count
    }
}

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Fast hash function for string content
///
/// This utility function provides a quick way to generate a hash from string content
/// for use in caching mechanisms. It uses Rust's built-in DefaultHasher.
///
/// # Arguments
///
/// * `content` - The string content to hash
///
/// # Returns
///
/// A 64-bit hash value derived from the content
pub fn fast_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_line_ending_pure_lf() {
        // Test content with only LF line endings
        let content = "First line\nSecond line\nThird line\n";
        assert_eq!(detect_line_ending(content), "\n");
    }

    #[test]
    fn test_detect_line_ending_pure_crlf() {
        // Test content with only CRLF line endings
        let content = "First line\r\nSecond line\r\nThird line\r\n";
        assert_eq!(detect_line_ending(content), "\r\n");
    }

    #[test]
    fn test_detect_line_ending_mixed_more_lf() {
        // Test content with mixed line endings where LF is more common
        let content = "First line\nSecond line\r\nThird line\nFourth line\n";
        assert_eq!(detect_line_ending(content), "\n");
    }

    #[test]
    fn test_detect_line_ending_mixed_more_crlf() {
        // Test content with mixed line endings where CRLF is more common
        let content = "First line\r\nSecond line\r\nThird line\nFourth line\r\n";
        assert_eq!(detect_line_ending(content), "\r\n");
    }

    #[test]
    fn test_detect_line_ending_empty_string() {
        // Test empty string - should default to LF
        let content = "";
        assert_eq!(detect_line_ending(content), "\n");
    }

    #[test]
    fn test_detect_line_ending_single_line_no_ending() {
        // Test single line without any line endings - should default to LF
        let content = "This is a single line with no line ending";
        assert_eq!(detect_line_ending(content), "\n");
    }

    #[test]
    fn test_detect_line_ending_equal_lf_and_crlf() {
        // Test edge case with equal number of CRLF and LF
        // Since LF count is calculated as total '\n' minus CRLF count,
        // and the algorithm uses > (not >=), it should default to LF
        let content = "Line 1\r\nLine 2\nLine 3\r\nLine 4\n";
        assert_eq!(detect_line_ending(content), "\n");
    }

    #[test]
    fn test_detect_line_ending_single_lf() {
        // Test with just a single LF
        let content = "Line 1\n";
        assert_eq!(detect_line_ending(content), "\n");
    }

    #[test]
    fn test_detect_line_ending_single_crlf() {
        // Test with just a single CRLF
        let content = "Line 1\r\n";
        assert_eq!(detect_line_ending(content), "\r\n");
    }

    #[test]
    fn test_detect_line_ending_embedded_cr() {
        // Test with CR characters that are not part of CRLF
        // These should not affect the count
        let content = "Line 1\rLine 2\nLine 3\r\nLine 4\n";
        // This has 1 CRLF and 2 LF (after subtracting the CRLF)
        assert_eq!(detect_line_ending(content), "\n");
    }
}
