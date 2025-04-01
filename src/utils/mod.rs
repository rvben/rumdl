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
        self.len() - self.trim_end().len()
    }
    
    fn replace_trailing_spaces(&self, replacement: &str) -> String {
        let non_space_len = self.trim_end().len();
        format!("{}{}", &self[..non_space_len], replacement)
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
