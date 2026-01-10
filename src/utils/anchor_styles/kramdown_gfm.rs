//! Kramdown with GFM input anchor generation
//!
//! This module implements the exact anchor generation algorithm used by kramdown
//! when configured with GFM input mode. This is the default configuration used by
//! Jekyll and GitHub Pages.
//!
//! Algorithm verified against official kramdown Ruby gem (2.5.1) with GFM input:
//! 1. Input validation and normalization
//! 2. Remove markdown formatting (emphasis, code, links - keep only link text)
//! 3. Symbol replacements (arrows with specific hyphen counts)
//! 4. Character filtering (ASCII letters/digits, common Unicode letters, spaces, hyphens, underscores)
//! 5. Trim to first letter (but preserve number-only headings)
//! 6. Convert spaces to hyphens, consolidate multiple hyphens
//! 7. Remove leading and trailing hyphens

use regex::Regex;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

use super::common::{
    CONTROL_CHARS, MAX_INPUT_SIZE_LARGE, UnicodeLetterMode, WHITESPACE_NORMALIZE, is_emoji_or_symbol,
    is_safe_unicode_letter,
};

// Improved markdown removal patterns with better nested handling
// Match emphasis patterns - asterisks and underscores
// Process multiple times to handle nested patterns
static ASTERISK_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*+([^*]*?)\*+").unwrap());
// Match emphasis underscores only at word boundaries, not in snake_case
static UNDERSCORE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b_+([^_\s][^_]*?)_+\b").unwrap());
static CODE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`+([^`]*?)`+").unwrap());
// Match images and links separately to handle nested brackets properly
static IMAGE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[([^\]]*)\]\([^)]*\)").unwrap());
static LINK_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[((?:[^\[\]]|\[[^\]]*\])*)\](?:\([^)]*\)|\[[^\]]*\])").unwrap());

/// Generate kramdown GFM style anchor fragment from heading text
///
/// This implementation matches kramdown's exact behavior when configured with
/// GFM input mode (used by Jekyll/GitHub Pages), verified against official
/// kramdown 2.5.1 Ruby gem.
///
/// # Security Features
/// - Input size limiting (1MB max)
/// - Unicode normalization (NFC)
/// - Control character filtering
/// - ReDoS protection through non-backtracking patterns
///
/// # Examples
/// ```
/// use rumdl_lib::utils::anchor_styles::kramdown_gfm;
///
/// assert_eq!(kramdown_gfm::heading_to_fragment("Hello World"), "hello-world");
/// assert_eq!(kramdown_gfm::heading_to_fragment("cbrown --> sbrown: --unsafe-paths"), "cbrown--sbrown-unsafe-paths");
/// assert_eq!(kramdown_gfm::heading_to_fragment("test_with_underscores"), "test_with_underscores");
/// ```
pub fn heading_to_fragment(heading: &str) -> String {
    // Step 1: Input validation and size limits
    if heading.is_empty() || heading.len() > MAX_INPUT_SIZE_LARGE {
        return if heading.is_empty() {
            String::new()
        } else {
            "section".to_string()
        };
    }

    // Step 2: Unicode normalization and security filtering
    let normalized: String = heading.nfc().collect();
    let text = CONTROL_CHARS.replace_all(&normalized, "");
    let text = WHITESPACE_NORMALIZE.replace_all(&text, " ");

    // Step 3: Symbol replacements - Jekyll/kramdown GFM replaces certain symbols
    // &, <, >, = become "--" ONLY when they have spaces around them
    // Without spaces, they're just removed during character filtering
    // This needs to happen BEFORE markdown removal so the symbols are still present
    let mut text = text.to_string();
    text = text
        .replace(" & ", " -- ")
        .replace(" < ", " -- ")
        .replace(" > ", " -- ")
        .replace(" = ", " -- ");

    // Step 4: Remove markdown formatting while preserving inner text
    // Process multiple times to handle nested patterns like **_text_**
    for _ in 0..3 {
        // Max 3 levels of nesting
        let prev = text.clone();
        text = ASTERISK_PATTERN.replace_all(&text, "$1").to_string();
        text = UNDERSCORE_PATTERN.replace_all(&text, "$1").to_string();
        if text == prev {
            break;
        } // No more changes
    }

    // Process code spans
    text = CODE_PATTERN.replace_all(&text, "$1").to_string();

    // Process images first, then links
    text = IMAGE_PATTERN.replace_all(&text, "$1").to_string();
    text = LINK_PATTERN.replace_all(&text, "$1").to_string();

    // DEBUG: Check text before filtering
    #[cfg(test)]
    if heading.contains('_') {
        eprintln!("DEBUG: Before character filtering:");
        eprintln!("  text: '{text}'");
        eprintln!("  contains underscores: {}", text.chars().any(|c| c == '_'));
    }

    // Step 5: Character filtering - keep safe letters, numbers, spaces, underscores, hyphens
    // Jekyll/GFM PRESERVES underscores (unlike pure kramdown) but removes other symbols
    // Track if we had leading emojis for special handling later
    let mut filtered = String::with_capacity(text.len());
    let mut had_leading_emoji = false;
    let mut seen_non_emoji = false;

    for c in text.chars() {
        if is_safe_unicode_letter(c, UnicodeLetterMode::Permissive)
            || c.is_ascii_digit()
            || c == ' '
            || c == '_'
            || c == '-'
        {
            filtered.push(c);
            seen_non_emoji = true;
        } else if is_emoji_or_symbol(c) {
            // Track if emoji appears before any other content
            if !seen_non_emoji && filtered.is_empty() {
                had_leading_emoji = true;
            }
            // Emojis get converted to nothing
        }
        // All other characters (punctuation, symbols, etc.) are removed
    }

    // DEBUG: Check filtered
    #[cfg(test)]
    if heading.contains("==>") {
        eprintln!("DEBUG: Processing '{heading}', after filtering: '{filtered}'");
    }

    // Step 6: Jekyll/GFM doesn't trim to first letter when there are leading digits
    // It preserves the entire string if it starts with numbers
    let mut start_pos = 0;
    let first_char = filtered.chars().next();

    // Only trim to first letter if the string starts with non-letter, non-digit characters
    if let Some(c) = first_char {
        if !c.is_ascii_digit() && !is_safe_unicode_letter(c, UnicodeLetterMode::Permissive) {
            // Find first letter or digit
            let mut found_alnum = false;
            for (i, ch) in filtered.char_indices() {
                if is_safe_unicode_letter(ch, UnicodeLetterMode::Permissive) || ch.is_ascii_digit() {
                    start_pos = i;
                    found_alnum = true;
                    break;
                }
            }
            if !found_alnum {
                return "section".to_string();
            }
        }
        // Otherwise keep the whole string (starts with letter or digit)
    } else {
        // Empty string after filtering - no valid characters
        return "section".to_string();
    }

    let trimmed = &filtered[start_pos..];

    // DEBUG: Check if underscores are present
    #[cfg(test)]
    if trimmed.contains('_') {
        eprintln!("DEBUG: After trimming, contains underscores: '{trimmed}'");
    }

    // DEBUG: Check trimmed BEFORE replacements
    #[cfg(test)]
    if heading.contains("==>") {
        eprintln!("DEBUG: Before smart typography, trimmed: '{trimmed}'");
    }

    // Step 7: Jekyll/kramdown GFM smart typography handling
    // In kramdown GFM mode:
    // - " --- " (with spaces on both sides) becomes "--" in ID
    // - " -- " (with spaces on both sides) becomes "--" in ID
    // - " == " (with spaces on both sides) becomes "--" in ID
    // - " - " (single hyphen with spaces) becomes "---" in ID (weird but true!)
    // - " --x" at word start (where x is not space/hyphen) becomes "-x" in ID
    // - Direct hyphens without spaces get consolidated by the n ≡ 1 (mod 3) pattern
    // Mark the patterns for special handling - ORDER MATTERS!
    let trimmed = trimmed
        .replace(" --- ", "§EMDASH§")     // Em-dash pattern (will become "--")
        .replace(" -- ", "§ENDASH§")      // En-dash pattern (will become "--")
        .replace(" == ", "§EQUALS§")      // Double equals pattern (will become "--")
        .replace(" - ", "§HYPHEN§"); // Single hyphen with spaces (will become "---")

    // DEBUG: Check trimmed after replacements
    #[cfg(test)]
    if heading.contains("==>") {
        eprintln!("DEBUG: After smart typography replacements, trimmed: '{trimmed}'");
    }

    // Now handle special hyphen and equals patterns:
    // The ` --word` pattern (space + -- attached to word) becomes ` -word` in kramdown
    // The ` ==word` pattern (space + == attached to word) becomes ` -word` too
    // But we DON'T want to then treat that ` -word` as needing doubling
    // So we'll use a special marker for the reduced hyphen
    let trimmed_chars: Vec<char> = trimmed.chars().collect();
    let mut result = String::new();
    let mut i = 0;

    while i < trimmed_chars.len() {
        let c = trimmed_chars[i];

        // Check for " --" or " ==" followed by a letter (not space or hyphen/equals)
        if i + 2 < trimmed_chars.len() && c == ' ' {
            let next1 = trimmed_chars[i + 1];
            let next2 = trimmed_chars[i + 2];

            if (next1 == '-' && next2 == '-') || (next1 == '=' && next2 == '=') {
                // Check if next char after -- or == is a letter
                if i + 3 < trimmed_chars.len() && trimmed_chars[i + 3].is_alphabetic() {
                    // This is " --word" or " ==word" pattern, mark it specially
                    result.push_str("§REDUCEHYPHEN§");
                    i += 3; // Skip space and both chars
                    continue;
                }
            }
        }

        // DEBUG: Track what happens to underscores
        #[cfg(test)]
        if c == '_' {
            eprintln!("DEBUG: Pushing underscore at position {i}");
        }

        result.push(c);
        i += 1;
    }
    let trimmed = result;

    // DEBUG: Check if underscores survived the loop
    #[cfg(test)]
    if heading.contains('_') && !trimmed.contains('_') {
        eprintln!("DEBUG: Underscores lost in smart typography loop!");
        eprintln!("  Original: '{heading}'");
        eprintln!("  After loop: '{trimmed}'");
    }

    // Step 8: Convert spaces to hyphens, lowercase letters
    // Process the string while PRESERVING our special markers
    let mut result = String::with_capacity(trimmed.len());

    // DEBUG
    #[cfg(test)]
    if heading.contains("==>") {
        eprintln!("DEBUG: Starting processing of: '{trimmed}'");
    }

    // Simple state machine for processing
    let mut i = 0;
    let chars: Vec<char> = trimmed.chars().collect();

    while i < chars.len() {
        let c = chars[i];

        // Check for our special markers - PRESERVE THEM for now
        if c == '§' {
            // Look for markers
            let remaining = &trimmed[trimmed.char_indices().nth(i).unwrap().0..];
            if remaining.starts_with("§ENDASH§") {
                result.push_str("§ENDASH§"); // Keep the marker
                i += "§ENDASH§".chars().count();
                continue;
            } else if remaining.starts_with("§EMDASH§") {
                result.push_str("§EMDASH§"); // Keep the marker
                i += "§EMDASH§".chars().count();
                continue;
            } else if remaining.starts_with("§HYPHEN§") {
                result.push_str("§HYPHEN§"); // Keep the marker
                i += "§HYPHEN§".chars().count();
                continue;
            } else if remaining.starts_with("§REDUCEHYPHEN§") {
                result.push_str("§REDUCEHYPHEN§"); // Keep the marker
                i += "§REDUCEHYPHEN§".chars().count();
                continue;
            } else if remaining.starts_with("§EQUALS§") {
                result.push_str("§EQUALS§"); // Keep the marker
                i += "§EQUALS§".chars().count();
                continue;
            }
        }

        // Normal character processing
        if is_safe_unicode_letter(c, UnicodeLetterMode::Permissive) {
            // Convert to lowercase
            for lowercase_c in c.to_lowercase() {
                result.push(lowercase_c);
            }
        } else if c.is_ascii_digit() || c == '-' || c == '_' {
            // Preserve digits, hyphens, and underscores
            // Jekyll GFM actually PRESERVES underscores (verified)
            result.push(c);
        } else if c == ' ' {
            // Convert spaces to hyphens - mark them to protect from consolidation
            result.push('§');
            result.push('S');
            result.push('§');
        }
        // All other characters are skipped

        i += 1;
    }

    // DEBUG
    #[cfg(test)]
    if heading.contains("==>") {
        eprintln!("DEBUG: After processing, result: '{result}'");
    }

    // Step 9: Apply kramdown GFM hyphen consolidation ONLY to natural hyphens
    // Pattern: n ≡ 1 (mod 3) AND n ≥ 4 → single hyphen (4,7,10,13...)
    // All other sequences of 2+ hyphens → removed
    // BUT: Smart typography markers are preserved and replaced AFTER consolidation
    static HYPHEN_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-{2,}").unwrap());

    let result = HYPHEN_PATTERN
        .replace_all(&result, |caps: &regex::Captures| {
            let hyphen_count = caps[0].len();
            // Kramdown GFM pattern: keep if n ≡ 1 (mod 3) AND n ≥ 4
            if hyphen_count >= 4 && (hyphen_count % 3) == 1 {
                "-".to_string()
            } else {
                // All other consecutive hyphen counts are removed
                "".to_string()
            }
        })
        .to_string();

    // Step 10: NOW replace the smart typography markers with their final form
    let mut result = result
        .replace("§ENDASH§", "--")         // En-dash marker becomes "--"
        .replace("§EMDASH§", "--")         // Em-dash marker becomes "--"
        .replace("§EQUALS§", "--")         // Double equals with spaces becomes "--"
        .replace("§HYPHEN§", "---")        // Single hyphen with spaces becomes "---" (weird Jekyll behavior)
        .replace("§REDUCEHYPHEN§", "-")    // Space + -- + word becomes single hyphen
        .replace("§S§", "-"); // Spaces become hyphens (protected from consolidation)

    // Step 11: Remove leading hyphens (but we'll add back emoji hyphen if needed)
    result = result.trim_start_matches('-').to_string();

    // Step 12: Handle leading emoji case - add hyphen if original had leading emoji
    // This happens AFTER trimming regular leading hyphens
    if had_leading_emoji && !result.is_empty() {
        result = format!("-{result}");
    }

    if result.is_empty() {
        "section".to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jekyll_basic_cases() {
        assert_eq!(heading_to_fragment("Hello World"), "hello-world");
        assert_eq!(heading_to_fragment("Test Case"), "test-case");
        assert_eq!(heading_to_fragment(""), "");
    }

    #[test]
    fn test_jekyll_underscores() {
        // Jekyll/GFM preserves underscores (verified against kramdown 2.5.1 with GFM)
        assert_eq!(heading_to_fragment("test_with_underscores"), "test_with_underscores");
        assert_eq!(heading_to_fragment("Update login_type"), "update-login_type");
        assert_eq!(heading_to_fragment("__dunder__"), "dunder");
    }

    #[test]
    fn test_jekyll_arrows_issue_39() {
        // Issue #39 cases - Jekyll/GFM specific arrow handling
        assert_eq!(
            heading_to_fragment("cbrown --> sbrown: --unsafe-paths"),
            "cbrown--sbrown-unsafe-paths"
        );
        assert_eq!(heading_to_fragment("cbrown -> sbrown"), "cbrown---sbrown");
        assert_eq!(heading_to_fragment("test-->more"), "testmore");
        assert_eq!(heading_to_fragment("test->more"), "test-more");
    }

    #[test]
    fn test_jekyll_character_filtering() {
        // Jekyll preserves Unicode letters and consolidates hyphens
        assert_eq!(heading_to_fragment("API::Response"), "apiresponse");
        assert_eq!(heading_to_fragment("Café René"), "café-rené");
        assert_eq!(heading_to_fragment("über uns"), "über-uns");
    }

    #[test]
    fn test_jekyll_symbol_replacements() {
        assert_eq!(heading_to_fragment("Testing & Coverage"), "testing--coverage");
        assert_eq!(heading_to_fragment("Compare > Results"), "compare--results");
        assert_eq!(heading_to_fragment("Arrow --> Test"), "arrow--test");
        assert_eq!(heading_to_fragment("Arrow ==> Test"), "arrow--test");
    }

    #[test]
    fn test_jekyll_hyphens() {
        // Jekyll/GFM removes consecutive hyphens but preserves single ones
        assert_eq!(heading_to_fragment("Double--Hyphen"), "doublehyphen");
        assert_eq!(heading_to_fragment("Pre-existing-hyphens"), "pre-existing-hyphens");
        assert_eq!(heading_to_fragment("Test---Multiple"), "testmultiple");
        assert_eq!(heading_to_fragment("Single-Hyphen"), "single-hyphen");
    }

    #[test]
    fn test_jekyll_leading_trailing_trimming() {
        // Jekyll removes leading and trailing hyphens
        assert_eq!(heading_to_fragment("---leading"), "leading");
        assert_eq!(heading_to_fragment("trailing---"), "trailing");
        assert_eq!(heading_to_fragment("---both---"), "both");
    }

    #[test]
    fn test_jekyll_numbers() {
        assert_eq!(heading_to_fragment("Step 1: Getting Started"), "step-1-getting-started");
        assert_eq!(heading_to_fragment("Version 2.1.0"), "version-210");
        assert_eq!(heading_to_fragment("123 Numbers"), "123-numbers");
        assert_eq!(heading_to_fragment("123"), "123"); // Numbers preserved
    }

    #[test]
    fn test_jekyll_markdown_removal() {
        assert_eq!(heading_to_fragment("*emphasized* text"), "emphasized-text");
        assert_eq!(heading_to_fragment("`code` in heading"), "code-in-heading");
        assert_eq!(heading_to_fragment("[link text](url)"), "link-text");
        // Test nested formatting
        assert_eq!(heading_to_fragment("**bold *italic* text**"), "bold-italic-text");
        assert_eq!(heading_to_fragment("_underline **bold** mix_"), "underline-bold-mix");
    }

    #[test]
    fn test_jekyll_emojis() {
        // Jekyll/GFM handles emojis by removing them, leaving leading hyphen
        assert_eq!(heading_to_fragment("🎉 emoji test"), "-emoji-test");
    }

    #[test]
    fn test_jekyll_comprehensive_verified() {
        // Test cases verified against actual Jekyll/kramdown Ruby gem with GFM
        let test_cases = [
            ("cbrown --> sbrown: --unsafe-paths", "cbrown--sbrown-unsafe-paths"),
            ("test_with_underscores", "test_with_underscores"),
            ("Update login_type", "update-login_type"),
            ("[link text](url)", "link-text"),
            ("trailing---", "trailing"),
            ("---both---", "both"),
            ("Double--Hyphen", "doublehyphen"),
            ("Test---Multiple", "testmultiple"),
            ("test-->more", "testmore"),
            ("123", "123"),
            ("🎉 emoji test", "-emoji-test"),
        ];

        for (input, expected) in test_cases {
            let actual = heading_to_fragment(input);
            assert_eq!(
                actual, expected,
                "Jekyll verified test failed for input: '{input}'\nExpected: '{expected}'\nActual: '{actual}'"
            );
        }
    }

    #[test]
    fn test_jekyll_edge_cases() {
        // Edge cases that caused issues in development
        assert_eq!(heading_to_fragment("123"), "123"); // Numbers preserved
        assert_eq!(heading_to_fragment("!!!"), "section"); // Punctuation only
        assert_eq!(heading_to_fragment("   "), "section"); // Whitespace only
        assert_eq!(heading_to_fragment("a"), "a"); // Single letter
        assert_eq!(heading_to_fragment("1a"), "1a"); // Number then letter (preserved)
    }

    #[test]
    fn test_security_features() {
        // Test input size limits
        let large_input = "a".repeat(MAX_INPUT_SIZE_LARGE + 1);
        assert_eq!(heading_to_fragment(&large_input), "section");

        // Test control character filtering
        assert_eq!(heading_to_fragment("Test\x00\x1F\x7FContent"), "testcontent");
        assert_eq!(heading_to_fragment("Test\u{200B}\u{FEFF}Content"), "testcontent");

        // Test Unicode normalization
        assert_eq!(heading_to_fragment("café"), "café");

        // Test whitespace normalization
        assert_eq!(heading_to_fragment("Test\tTab\u{00A0}Space"), "testtab-space");
    }

    #[test]
    fn test_unicode_safety() {
        // Test safe Unicode letter filtering
        assert_eq!(heading_to_fragment("Café"), "café");
        assert_eq!(heading_to_fragment("Naïve"), "naïve");
        assert_eq!(heading_to_fragment("Résumé"), "résumé");

        // Test rejection of mathematical symbols
        assert_eq!(heading_to_fragment("Test ∑ Math ∞ Symbols"), "test--math--symbols");

        // Test rejection of emoji and other Unicode
        assert_eq!(heading_to_fragment("Test 🚀 Emoji 💡 Content"), "test--emoji--content");

        // Test rejection of currency and other symbols
        assert_eq!(heading_to_fragment("Price €100 ¥200 $300"), "price-100-200-300");
    }
}
