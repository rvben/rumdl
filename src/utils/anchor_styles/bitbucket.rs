//! Bitbucket anchor generation
//!
//! This module implements Bitbucket's anchor generation algorithm, which is
//! similar to GitHub but adds a 'markdown-header-' prefix to all anchors.
//!
//! Algorithm:
//! 1. Apply GitHub-style anchor generation
//! 2. Add 'markdown-header-' prefix
//! 3. Handle edge cases for empty results

/// Generate Bitbucket style anchor fragment from heading text
///
/// Bitbucket uses GitHub-style generation with a 'markdown-header-' prefix.
///
/// # Examples
/// ```
/// use rumdl::utils::anchor_styles::bitbucket;
///
/// assert_eq!(bitbucket::heading_to_fragment("Hello World"), "markdown-header-hello-world");
/// assert_eq!(bitbucket::heading_to_fragment("test_with_underscores"), "markdown-header-test_with_underscores");
/// ```
pub fn heading_to_fragment(heading: &str) -> String {
    if heading.is_empty() {
        return "markdown-header-".to_string();
    }

    // Use GitHub-style generation as the base
    let github_fragment = super::github::heading_to_fragment(heading);
    
    // Add Bitbucket prefix
    if github_fragment.is_empty() {
        "markdown-header-".to_string()
    } else {
        format!("markdown-header-{}", github_fragment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitbucket_basic_cases() {
        assert_eq!(heading_to_fragment("Hello World"), "markdown-header-hello-world");
        assert_eq!(heading_to_fragment("Test Case"), "markdown-header-test-case");
        assert_eq!(heading_to_fragment(""), "markdown-header-");
    }

    #[test]
    fn test_bitbucket_underscores() {
        // Bitbucket preserves underscores (like GitHub)
        assert_eq!(heading_to_fragment("test_with_underscores"), "markdown-header-test_with_underscores");
        assert_eq!(heading_to_fragment("Update login_type"), "markdown-header-update-login_type");
        assert_eq!(heading_to_fragment("__dunder__"), "markdown-header-__dunder__");
    }

    #[test]
    fn test_bitbucket_arrows_issue_39() {
        // Bitbucket uses GitHub behavior for arrows
        assert_eq!(heading_to_fragment("cbrown --> sbrown: --unsafe-paths"), "markdown-header-cbrown----sbrown---unsafe-paths");
        assert_eq!(heading_to_fragment("cbrown -> sbrown"), "markdown-header-cbrown---sbrown");
    }

    #[test]
    fn test_bitbucket_hyphens() {
        // Bitbucket preserves consecutive hyphens (like GitHub)
        assert_eq!(heading_to_fragment("Double--Hyphen"), "markdown-header-double--hyphen");
        assert_eq!(heading_to_fragment("Triple---Dash"), "markdown-header-triple---dash");
    }

    #[test]
    fn test_bitbucket_special_symbols() {
        assert_eq!(heading_to_fragment("Testing & Coverage"), "markdown-header-testing--coverage");
        assert_eq!(heading_to_fragment("API::Response > Error--Handling"), "markdown-header-apiresponse--error--handling");
    }

    #[test]
    fn test_bitbucket_unicode() {
        // Bitbucket preserves Unicode (like GitHub)
        assert_eq!(heading_to_fragment("Café René"), "markdown-header-café-rené");
        assert_eq!(heading_to_fragment("über uns"), "markdown-header-über-uns");
    }

    #[test]
    fn test_bitbucket_edge_cases() {
        assert_eq!(heading_to_fragment("123"), "markdown-header-123");
        assert_eq!(heading_to_fragment("!!!"), "markdown-header-");
        assert_eq!(heading_to_fragment("   "), "markdown-header-");
    }

    #[test]
    fn test_bitbucket_comprehensive() {
        // Test cases showing Bitbucket prefix behavior
        let test_cases = [
            ("Hello World", "markdown-header-hello-world"),
            ("test_with_underscores", "markdown-header-test_with_underscores"),
            ("Double--Hyphen", "markdown-header-double--hyphen"),
            ("Triple---Dash", "markdown-header-triple---dash"),
            ("A - B - C", "markdown-header-a---b---c"),
            ("Café au Lait", "markdown-header-café-au-lait"),
            ("123 Numbers", "markdown-header-123-numbers"),
            ("Version 2.1.0", "markdown-header-version-2-1-0"),
            ("Pre-existing-hyphens", "markdown-header-pre-existing-hyphens"),
            ("Simple-Hyphen", "markdown-header-simple-hyphen"),
        ];

        for (input, expected) in test_cases {
            let actual = heading_to_fragment(input);
            assert_eq!(
                actual, expected,
                "Bitbucket test failed for input: '{input}'\nExpected: '{expected}'\nActual: '{actual}'"
            );
        }
    }

    #[test]
    fn test_bitbucket_prefix_always_present() {
        // Ensure the prefix is always present, even for edge cases
        let edge_cases = ["", "   ", "!!!", "123", "---"];
        
        for case in edge_cases {
            let result = heading_to_fragment(case);
            assert!(result.starts_with("markdown-header-"), 
                "Bitbucket result should always start with prefix for input: '{case}', got: '{result}'");
        }
    }
}