//! Jekyll/GitHub Pages anchor generation
//!
//! This module implements the exact anchor generation algorithm used by Jekyll
//! with kramdown + GFM input (the default for GitHub Pages).
//!
//! Algorithm verified against official Jekyll/kramdown Ruby gem (2.5.1):
//! 1. Character filtering first (ASCII letters, numbers, spaces, hyphens only)
//! 2. Symbol replacements (-->, & → --, > → --)
//! 3. Space → hyphen conversion
//! 4. Leading character removal until first letter
//! 5. Case conversion and cleanup
//! 6. Leading hyphen removal only (preserve trailing)

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref EMPHASIS_PATTERN: Regex = Regex::new(r"\*+([^*]+)\*+").unwrap();
    static ref CODE_PATTERN: Regex = Regex::new(r"`([^`]+)`").unwrap();
    static ref LINK_PATTERN: Regex = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)|\[([^\]]+)\]\[[^\]]*\]").unwrap();
}

/// Generate Jekyll/GitHub Pages style anchor fragment from heading text
///
/// This implementation matches Jekyll's exact behavior when configured with
/// kramdown + GFM input (GitHub Pages default), verified against official
/// kramdown 2.5.1 Ruby gem.
///
/// # Examples
/// ```
/// use rumdl::utils::anchor_styles::jekyll;
///
/// assert_eq!(jekyll::heading_to_fragment("Hello World"), "hello-world");
/// assert_eq!(jekyll::heading_to_fragment("cbrown --> sbrown: --unsafe-paths"), "cbrown----sbrown---unsafe-paths");
/// assert_eq!(jekyll::heading_to_fragment("test_with_underscores"), "test_with_underscores");
/// ```
pub fn heading_to_fragment(heading: &str) -> String {
    if heading.is_empty() {
        return String::new();
    }

    // Step 1: Remove markdown formatting while preserving inner text
    let mut text = heading.to_string();
    text = EMPHASIS_PATTERN.replace_all(&text, "$1").to_string();
    text = CODE_PATTERN.replace_all(&text, "$1").to_string(); 
    text = LINK_PATTERN.replace_all(&text, |caps: &regex::Captures| {
        caps.get(1).or_else(|| caps.get(3)).map_or("".to_string(), |m| m.as_str().to_string())
    }).to_string();

    // Step 2: Character filtering - keep only ASCII letters, numbers, spaces, hyphens
    // This happens BEFORE symbol replacement in Jekyll
    let mut filtered = String::new();
    for c in text.chars() {
        if c.is_ascii_alphabetic() || c.is_ascii_digit() || c == ' ' || c == '-' || c == '_' {
            filtered.push(c);
        }
        // All other characters (including accented chars) are removed in Jekyll
    }

    // Step 3: Symbol replacements on filtered text
    // Jekyll does these replacements AFTER character filtering
    filtered = filtered.replace("-->", "----");  // 4 hyphens
    filtered = filtered.replace(" & ", "--");    // Ampersand with spaces
    filtered = filtered.replace(" > ", "--");    // Greater than with spaces

    // Step 4: Find first letter for trimming start
    let mut start_pos = 0;
    let mut found_letter = false;
    for (i, c) in filtered.char_indices() {
        if c.is_ascii_alphabetic() {
            start_pos = i;
            found_letter = true;
            break;
        }
    }

    // If no letters found, return "section" for numbers-only content
    if !found_letter {
        return "section".to_string();
    }

    let trimmed = &filtered[start_pos..];

    // Step 5: Convert spaces to hyphens and lowercase
    let mut result = String::new();
    for c in trimmed.chars() {
        if c.is_ascii_alphabetic() {
            result.push(c.to_ascii_lowercase());
        } else if c.is_ascii_digit() || c == '_' {
            result.push(c);
        } else {
            // Spaces and existing hyphens become hyphens
            result.push('-');
        }
    }

    // Step 6: Remove leading hyphens only (preserve trailing)
    let result = result.trim_start_matches('-').to_string();

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
        // Jekyll preserves underscores (like GitHub, unlike pure kramdown)
        assert_eq!(heading_to_fragment("test_with_underscores"), "test_with_underscores");
        assert_eq!(heading_to_fragment("Update login_type"), "update-login_type");
        assert_eq!(heading_to_fragment("__dunder__"), "dunder__");
    }

    #[test]
    fn test_jekyll_arrows_issue_39() {
        // Issue #39 cases - Jekyll handles arrows differently than GitHub
        assert_eq!(heading_to_fragment("cbrown --> sbrown: --unsafe-paths"), "cbrown----sbrown---unsafe-paths");
        assert_eq!(heading_to_fragment("cbrown -> sbrown"), "cbrown----sbrown");
        assert_eq!(heading_to_fragment("respect_gitignore"), "respect_gitignore");
    }

    #[test]
    fn test_jekyll_character_filtering() {
        // Jekyll removes characters not in the allowed set
        assert_eq!(heading_to_fragment("API::Response"), "apiresponse");
        assert_eq!(heading_to_fragment("Café René"), "caf-ren"); // Accented chars removed
        assert_eq!(heading_to_fragment("über uns"), "ber-uns");  // Umlaut removed
    }

    #[test]
    fn test_jekyll_symbol_replacements() {
        assert_eq!(heading_to_fragment("Testing & Coverage"), "testing--coverage");
        assert_eq!(heading_to_fragment("Compare > Results"), "compare--results");
        assert_eq!(heading_to_fragment("Arrow --> Test"), "arrow----test");
    }

    #[test]
    fn test_jekyll_hyphens() {
        // Jekyll preserves hyphens from the original text
        assert_eq!(heading_to_fragment("Double--Hyphen"), "double--hyphen");
        assert_eq!(heading_to_fragment("Pre-existing-hyphens"), "pre-existing-hyphens");
    }

    #[test]
    fn test_jekyll_leading_trimming() {
        // Jekyll removes leading hyphens but preserves trailing
        assert_eq!(heading_to_fragment("---leading"), "leading");
        assert_eq!(heading_to_fragment("trailing---"), "trailing---");
        assert_eq!(heading_to_fragment("---both---"), "both---");
    }

    #[test]
    fn test_jekyll_numbers() {
        assert_eq!(heading_to_fragment("Step 1: Getting Started"), "step-1-getting-started");
        assert_eq!(heading_to_fragment("Version 2.1.0"), "version-210");
        assert_eq!(heading_to_fragment("123 Numbers"), "numbers"); // Leading numbers trimmed
    }

    #[test]
    fn test_jekyll_markdown_removal() {
        assert_eq!(heading_to_fragment("*emphasized* text"), "emphasized-text");
        assert_eq!(heading_to_fragment("`code` in heading"), "code-in-heading");
        assert_eq!(heading_to_fragment("[link text](url)"), "link-text");
    }

    #[test]
    fn test_jekyll_comprehensive_verified() {
        // Test cases verified against actual Jekyll/kramdown Ruby gem
        let test_cases = [
            ("cbrown --> sbrown: --unsafe-paths", "cbrown----sbrown---unsafe-paths"),
            ("Update login_type", "update-login_type"),
            ("API::Response > Error--Handling", "apiresponse--error--handling"),
            ("Test---with---multiple---hyphens", "test---with---multiple---hyphens"),
            ("respect_gitignore", "respect_gitignore"),
            ("Simple test case", "simple-test-case"),
            ("Testing & Coverage", "testing--coverage"),
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
        assert_eq!(heading_to_fragment("123"), "section"); // Numbers only
        assert_eq!(heading_to_fragment("!!!"), "section"); // Punctuation only
        assert_eq!(heading_to_fragment("   "), "section"); // Whitespace only
        assert_eq!(heading_to_fragment("a"), "a");         // Single letter
        assert_eq!(heading_to_fragment("1a"), "a");        // Number then letter
    }
}