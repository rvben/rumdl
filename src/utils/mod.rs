pub mod range_utils;
pub mod code_block_utils;
pub mod markdown_elements;

pub use range_utils::LineIndex;
pub use code_block_utils::CodeBlockUtils;
pub use markdown_elements::{MarkdownElements, MarkdownElement, ElementType, ElementQuality};

/// Trait for string-related extensions
pub trait StrExt {
    /// Count the number of trailing spaces in a string
    fn trailing_spaces(&self) -> usize;
    
    /// Replace trailing spaces with a given replacement string
    fn replace_trailing_spaces(&self, replacement: &str) -> String;
}

impl StrExt for str {
    fn trailing_spaces(&self) -> usize {
        // Custom implementation to handle both newlines and tabs specially
        
        // Prepare the string without newline if it ends with one
        let content = if self.ends_with('\n') {
            &self[..self.len() - 1]
        } else {
            self
        };
        
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
    
    fn replace_trailing_spaces(&self, replacement: &str) -> String {
        // Custom implementation to handle both newlines and tabs specially
        
        // Check if string ends with newline
        let ends_with_newline = self.ends_with('\n');
        
        // Prepare the string without newline if needed
        let content = if ends_with_newline {
            &self[..self.len() - 1]
        } else {
            self
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
        let mut result = String::with_capacity(non_space_len + replacement.len() + if ends_with_newline { 1 } else { 0 });
        result.push_str(&content[..non_space_len]);
        result.push_str(replacement);
        if ends_with_newline {
            result.push('\n');
        }
        
        result
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
