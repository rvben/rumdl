pub mod range_utils;

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
