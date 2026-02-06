//! Python-Markdown anchor generation (used by MkDocs)
//!
//! This module implements the exact slugify algorithm from Python-Markdown's
//! `toc` extension, which is the default used by MkDocs.
//!
//! Algorithm (from markdown/extensions/toc.py):
//! 1. NFKD Unicode normalization, then encode to ASCII (ignore errors)
//! 2. Remove all characters except `\w`, `\s`, and `-`
//! 3. Lowercase and strip leading/trailing whitespace
//! 4. Collapse consecutive hyphens and whitespace into a single separator
//!
//! Verified against Python-Markdown 3.x via:
//! ```bash
//! uv run --with markdown python3 -c "
//! from markdown.extensions.toc import slugify
//! print(slugify('Your Heading', '-'))
//! "
//! ```

use regex::Regex;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

use super::common::MAX_INPUT_LENGTH;

static STRIP_NON_WORD: LazyLock<Regex> = LazyLock::new(|| {
    // Matches anything that is NOT a word character, whitespace, or hyphen.
    // `\w` in Rust's regex (with Unicode enabled) matches [a-zA-Z0-9_] plus Unicode letters/digits.
    // Python-Markdown operates on ASCII-only text at this point, but we replicate the
    // intent: keep alphanumeric, underscore, whitespace, and hyphen.
    Regex::new(r"[^\w\s-]").unwrap()
});

static COLLAPSE_SEPARATORS: LazyLock<Regex> = LazyLock::new(|| {
    // Collapse one or more hyphens/whitespace into the separator
    Regex::new(r"[-\s]+").unwrap()
});

/// Generate a Python-Markdown style anchor fragment from heading text.
///
/// This matches the behavior of `markdown.extensions.toc.slugify(value, '-')`.
///
/// # Examples
/// ```
/// use rumdl_lib::utils::anchor_styles::python_markdown;
///
/// assert_eq!(python_markdown::heading_to_fragment("Hello World"), "hello-world");
/// assert_eq!(
///     python_markdown::heading_to_fragment("Cross-references to other projects / inventories"),
///     "cross-references-to-other-projects-inventories"
/// );
/// assert_eq!(python_markdown::heading_to_fragment("test_with_underscores"), "test_with_underscores");
/// ```
pub fn heading_to_fragment(heading: &str) -> String {
    if heading.is_empty() {
        return String::new();
    }

    // Enforce input size limit
    let input = if heading.len() > MAX_INPUT_LENGTH {
        let mut end = MAX_INPUT_LENGTH;
        while end < heading.len() && !heading.is_char_boundary(end) {
            end += 1;
        }
        &heading[..end.min(heading.len())]
    } else {
        heading
    };

    // Step 1: NFKD normalization, then ASCII-only
    // Python-Markdown: unicodedata.normalize('NFKD', value).encode('ascii', 'ignore').decode()
    let nfkd: String = input.nfkd().collect();
    let ascii_only: String = nfkd.chars().filter(|c| c.is_ascii()).collect();

    // Step 2: Remove non-word, non-whitespace, non-hyphen characters
    // Python-Markdown: re.sub(r'[^\w\s-]', '', value)
    let cleaned = STRIP_NON_WORD.replace_all(&ascii_only, "");

    // Step 3: Lowercase and strip
    let lowered = cleaned.to_lowercase();
    let stripped = lowered.trim();

    // Step 4: Collapse consecutive hyphens and whitespace into single separator
    // Python-Markdown: re.sub(r'[-\s]+', separator, value)
    let result = COLLAPSE_SEPARATORS.replace_all(stripped, "-");

    result.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        assert_eq!(heading_to_fragment("Hello World"), "hello-world");
    }

    #[test]
    fn test_slash_collapsing() {
        // The `/` is removed, consecutive spaces collapsed into single `-`
        assert_eq!(
            heading_to_fragment("Cross-references to other projects / inventories"),
            "cross-references-to-other-projects-inventories"
        );
    }

    #[test]
    fn test_underscores_preserved() {
        assert_eq!(heading_to_fragment("test_with_underscores"), "test_with_underscores");
    }

    #[test]
    fn test_hyphens_preserved() {
        assert_eq!(heading_to_fragment("well-known"), "well-known");
    }

    #[test]
    fn test_consecutive_hyphens_collapsed() {
        // Unlike GitHub, Python-Markdown collapses consecutive hyphens
        assert_eq!(heading_to_fragment("test--double"), "test-double");
        assert_eq!(heading_to_fragment("test---triple"), "test-triple");
    }

    #[test]
    fn test_special_characters_removed() {
        assert_eq!(heading_to_fragment("Hello & World"), "hello-world");
        assert_eq!(heading_to_fragment("C++ Guide"), "c-guide");
        assert_eq!(heading_to_fragment("Q&A"), "qa");
    }

    #[test]
    fn test_unicode_decomposed() {
        // NFKD decomposes accented chars, then ASCII filter removes combining marks
        // é (U+00E9) → e + combining acute (U+0301), acute is non-ASCII → removed
        assert_eq!(heading_to_fragment("café"), "cafe");
        assert_eq!(heading_to_fragment("résumé"), "resume");
    }

    #[test]
    fn test_non_ascii_removed() {
        // Non-decomposable Unicode chars are removed entirely
        assert_eq!(heading_to_fragment("日本語 Test"), "test");
    }

    #[test]
    fn test_empty() {
        assert_eq!(heading_to_fragment(""), "");
    }

    #[test]
    fn test_whitespace_only() {
        assert_eq!(heading_to_fragment("   "), "");
    }

    #[test]
    fn test_numbers() {
        assert_eq!(heading_to_fragment("Step 1: Setup"), "step-1-setup");
    }

    #[test]
    fn test_arrows() {
        // Arrows are just punctuation to Python-Markdown, removed and collapsed
        assert_eq!(heading_to_fragment("A --> B"), "a-b");
        assert_eq!(heading_to_fragment("A -> B"), "a-b");
    }

    #[test]
    fn test_leading_trailing_stripped() {
        assert_eq!(heading_to_fragment("  Hello  "), "hello");
    }

    #[test]
    fn test_mixed_separators() {
        // Spaces, hyphens all collapsed into single `-`
        assert_eq!(heading_to_fragment("a - b - c"), "a-b-c");
        assert_eq!(heading_to_fragment("a  -  b"), "a-b");
    }
}
