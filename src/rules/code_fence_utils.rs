use lazy_static::lazy_static;
use regex::Regex;
use std::fmt;

/// The style for code fence markers (MD048)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default, Hash)]
pub enum CodeFenceStyle {
    /// Consistent with the first code fence style found
    #[default]
    Consistent,
    /// Backtick style (```)
    Backtick,
    /// Tilde style (~~~)
    Tilde,
}

impl fmt::Display for CodeFenceStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeFenceStyle::Backtick => write!(f, "backtick"),
            CodeFenceStyle::Tilde => write!(f, "tilde"),
            CodeFenceStyle::Consistent => write!(f, "consistent"),
        }
    }
}

/// Get regex pattern for finding code fence markers
pub fn get_code_fence_pattern() -> &'static Regex {
    lazy_static! {
        static ref CODE_FENCE_PATTERN: Regex = Regex::new(r"^(```|~~~)").unwrap();
    }
    &CODE_FENCE_PATTERN
}

/// Determine the code fence style from a marker
pub fn get_fence_style(marker: &str) -> Option<CodeFenceStyle> {
    match marker {
        "```" => Some(CodeFenceStyle::Backtick),
        "~~~" => Some(CodeFenceStyle::Tilde),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_fence_style_default() {
        let style = CodeFenceStyle::default();
        assert_eq!(style, CodeFenceStyle::Consistent);
    }

    #[test]
    fn test_code_fence_style_equality() {
        assert_eq!(CodeFenceStyle::Backtick, CodeFenceStyle::Backtick);
        assert_eq!(CodeFenceStyle::Tilde, CodeFenceStyle::Tilde);
        assert_eq!(CodeFenceStyle::Consistent, CodeFenceStyle::Consistent);

        assert_ne!(CodeFenceStyle::Backtick, CodeFenceStyle::Tilde);
        assert_ne!(CodeFenceStyle::Backtick, CodeFenceStyle::Consistent);
        assert_ne!(CodeFenceStyle::Tilde, CodeFenceStyle::Consistent);
    }

    #[test]
    fn test_code_fence_style_display() {
        assert_eq!(format!("{}", CodeFenceStyle::Backtick), "backtick");
        assert_eq!(format!("{}", CodeFenceStyle::Tilde), "tilde");
        assert_eq!(format!("{}", CodeFenceStyle::Consistent), "consistent");
    }

    #[test]
    fn test_code_fence_style_debug() {
        assert_eq!(format!("{:?}", CodeFenceStyle::Backtick), "Backtick");
        assert_eq!(format!("{:?}", CodeFenceStyle::Tilde), "Tilde");
        assert_eq!(format!("{:?}", CodeFenceStyle::Consistent), "Consistent");
    }

    #[test]
    fn test_code_fence_style_clone() {
        let style1 = CodeFenceStyle::Backtick;
        let style2 = style1;
        assert_eq!(style1, style2);
    }

    #[test]
    fn test_get_code_fence_pattern() {
        let pattern = get_code_fence_pattern();

        // Test matching backtick fences
        assert!(pattern.is_match("```"));
        assert!(pattern.is_match("```rust"));
        assert!(pattern.is_match("```\n"));

        // Test matching tilde fences
        assert!(pattern.is_match("~~~"));
        assert!(pattern.is_match("~~~python"));
        assert!(pattern.is_match("~~~\n"));

        // Test non-matching cases
        assert!(!pattern.is_match("  ```")); // Indented
        assert!(!pattern.is_match("text```")); // Not at start
        assert!(!pattern.is_match("``")); // Too few markers
        assert!(pattern.is_match("````")); // Four backticks still matches (first 3)

        // Test that it only matches at start of line
        let captures = pattern.captures("```rust");
        assert!(captures.is_some());
        assert_eq!(captures.unwrap().get(1).unwrap().as_str(), "```");

        let captures = pattern.captures("~~~yaml");
        assert!(captures.is_some());
        assert_eq!(captures.unwrap().get(1).unwrap().as_str(), "~~~");
    }

    #[test]
    fn test_get_fence_style() {
        // Valid styles
        assert_eq!(get_fence_style("```"), Some(CodeFenceStyle::Backtick));
        assert_eq!(get_fence_style("~~~"), Some(CodeFenceStyle::Tilde));

        // Invalid inputs
        assert_eq!(get_fence_style("``"), None);
        assert_eq!(get_fence_style("````"), None);
        assert_eq!(get_fence_style("~~"), None);
        assert_eq!(get_fence_style("~~~~"), None);
        assert_eq!(get_fence_style(""), None);
        assert_eq!(get_fence_style("random"), None);
        assert_eq!(get_fence_style("```rust"), None); // Full fence line
        assert_eq!(get_fence_style("~~~yaml"), None); // Full fence line
    }

    #[test]
    fn test_pattern_singleton() {
        // Ensure the lazy_static pattern is the same instance
        let pattern1 = get_code_fence_pattern();
        let pattern2 = get_code_fence_pattern();

        // Compare pointers
        assert_eq!(pattern1 as *const _, pattern2 as *const _);
    }

    #[test]
    fn test_edge_cases() {
        let pattern = get_code_fence_pattern();

        // Empty string
        assert!(!pattern.is_match(""));

        // Unicode in fence
        assert!(pattern.is_match("```中文"));
        assert!(pattern.is_match("~~~émoji🦀"));

        // Tabs and spaces after fence
        assert!(pattern.is_match("```\t"));
        assert!(pattern.is_match("~~~   "));

        // Mixed markers (should match first set)
        let captures = pattern.captures("```~~~");
        assert!(captures.is_some());
        assert_eq!(captures.unwrap().get(1).unwrap().as_str(), "```");
    }

    #[test]
    fn test_code_fence_style_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(CodeFenceStyle::Backtick);
        set.insert(CodeFenceStyle::Tilde);
        set.insert(CodeFenceStyle::Consistent);

        assert_eq!(set.len(), 3);
        assert!(set.contains(&CodeFenceStyle::Backtick));
        assert!(set.contains(&CodeFenceStyle::Tilde));
        assert!(set.contains(&CodeFenceStyle::Consistent));
    }

    #[test]
    fn test_pattern_usage_examples() {
        let pattern = get_code_fence_pattern();

        // Typical markdown code fence lines
        let test_cases = vec![
            ("```rust", true, "```"),
            ("```", true, "```"),
            ("~~~python", true, "~~~"),
            ("~~~", true, "~~~"),
            ("```json\n", true, "```"),
            ("~~~yaml\n", true, "~~~"),
            ("    ```", false, ""),       // Indented code fence
            ("Some text ```", false, ""), // Not at start
        ];

        for (input, should_match, expected_capture) in test_cases {
            let is_match = pattern.is_match(input);
            assert_eq!(is_match, should_match, "Failed for input: {input}");

            if should_match {
                let captures = pattern.captures(input).unwrap();
                assert_eq!(
                    captures.get(1).unwrap().as_str(),
                    expected_capture,
                    "Failed capture for input: {input}"
                );
            }
        }
    }
}
