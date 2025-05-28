//!
//! Shared utilities for rumdl, including document structure analysis, code block handling, regex helpers, and string extensions.
//! Provides reusable traits and functions for rule implementations and core linter logic.

pub mod ast_utils;
pub mod code_block_utils;
pub mod document_structure;
pub mod early_returns;
pub mod element_cache;
pub mod markdown_elements;
pub mod range_utils;
pub mod regex_cache;
pub mod string_interner;
pub mod table_utils;

pub use ast_utils::AstCache;
pub use code_block_utils::CodeBlockUtils;
pub use document_structure::DocumentStructure;
pub use markdown_elements::{ElementQuality, ElementType, MarkdownElement, MarkdownElements};
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
        let mut result = String::with_capacity(
            non_space_len + replacement.len() + if ends_with_newline { 1 } else { 0 },
        );
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
