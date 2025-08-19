//! Anchor generation styles for different Markdown platforms
//!
//! This module provides different anchor generation implementations that match
//! the behavior of various Markdown platforms:
//!
//! - **GitHub**: GitHub.com's official anchor generation algorithm
//! - **KramdownGfm**: Kramdown with GFM input (used by Jekyll/GitHub Pages)
//! - **Kramdown**: Pure kramdown without GFM extensions
//!
//! Each style is implemented in a separate module with comprehensive tests
//! verified against the official tools/platforms.

pub mod github;
pub mod kramdown;
pub mod kramdown_gfm; // Renamed from jekyll for clarity

use serde::{Deserialize, Serialize};

/// Anchor generation style for heading fragments
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum AnchorStyle {
    /// GitHub/GFM style (default): preserves underscores, removes punctuation
    #[default]
    #[serde(rename = "github")]
    GitHub,
    /// Kramdown with GFM input: matches Jekyll/GitHub Pages behavior
    /// Accepts both "kramdown-gfm" and "jekyll" (for backward compatibility)
    #[serde(rename = "kramdown-gfm", alias = "jekyll")]
    KramdownGfm,
    /// Pure kramdown style: removes underscores and punctuation
    #[serde(rename = "kramdown")]
    Kramdown,
}

impl AnchorStyle {
    /// Generate an anchor fragment using the specified style
    pub fn generate_fragment(&self, heading: &str) -> String {
        match self {
            AnchorStyle::GitHub => github::heading_to_fragment(heading),
            AnchorStyle::KramdownGfm => kramdown_gfm::heading_to_fragment(heading),
            AnchorStyle::Kramdown => kramdown::heading_to_fragment(heading),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anchor_style_serde() {
        // Test serialization (uses primary names)
        assert_eq!(serde_json::to_string(&AnchorStyle::GitHub).unwrap(), "\"github\"");
        assert_eq!(
            serde_json::to_string(&AnchorStyle::KramdownGfm).unwrap(),
            "\"kramdown-gfm\""
        );
        assert_eq!(serde_json::to_string(&AnchorStyle::Kramdown).unwrap(), "\"kramdown\"");

        // Test deserialization with primary names
        assert_eq!(
            serde_json::from_str::<AnchorStyle>("\"github\"").unwrap(),
            AnchorStyle::GitHub
        );
        assert_eq!(
            serde_json::from_str::<AnchorStyle>("\"kramdown-gfm\"").unwrap(),
            AnchorStyle::KramdownGfm
        );
        assert_eq!(
            serde_json::from_str::<AnchorStyle>("\"kramdown\"").unwrap(),
            AnchorStyle::Kramdown
        );

        // Test backward compatibility: "jekyll" alias still works
        assert_eq!(
            serde_json::from_str::<AnchorStyle>("\"jekyll\"").unwrap(),
            AnchorStyle::KramdownGfm
        );
    }

    #[test]
    fn test_anchor_style_differences() {
        let test_cases = [
            "cbrown --> sbrown: --unsafe-paths",
            "Update login_type",
            "Test---with---multiple---hyphens",
            "API::Response > Error--Handling",
        ];

        for case in test_cases {
            let github = AnchorStyle::GitHub.generate_fragment(case);
            let kramdown_gfm = AnchorStyle::KramdownGfm.generate_fragment(case);
            let kramdown = AnchorStyle::Kramdown.generate_fragment(case);

            // Each style should produce a valid non-empty result
            assert!(!github.is_empty(), "GitHub style failed for: {case}");
            assert!(!kramdown_gfm.is_empty(), "KramdownGfm style failed for: {case}");
            assert!(!kramdown.is_empty(), "Kramdown style failed for: {case}");
        }
    }
}
