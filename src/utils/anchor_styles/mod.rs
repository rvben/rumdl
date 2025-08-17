//! Anchor generation styles for different Markdown platforms
//!
//! This module provides different anchor generation implementations that match
//! the behavior of various Markdown platforms:
//!
//! - **GitHub**: GitHub.com's official anchor generation algorithm
//! - **Jekyll**: Jekyll/GitHub Pages with kramdown + GFM input
//! - **Kramdown**: Pure kramdown without GFM extensions
//!
//! Each style is implemented in a separate module with comprehensive tests
//! verified against the official tools/platforms.

pub mod github;
pub mod jekyll; 
pub mod kramdown;

use serde::{Deserialize, Serialize};

/// Anchor generation style for heading fragments
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnchorStyle {
    /// GitHub/GFM style (default): preserves underscores, removes punctuation
    GitHub,
    /// Jekyll/kramdown with GFM style: matches Jekyll/GitHub Pages behavior
    Jekyll,
    /// Pure kramdown style: removes underscores and punctuation
    Kramdown,
}

impl Default for AnchorStyle {
    fn default() -> Self {
        AnchorStyle::GitHub
    }
}

impl AnchorStyle {
    /// Generate an anchor fragment using the specified style
    pub fn generate_fragment(&self, heading: &str) -> String {
        match self {
            AnchorStyle::GitHub => github::heading_to_fragment(heading),
            AnchorStyle::Jekyll => jekyll::heading_to_fragment(heading),
            AnchorStyle::Kramdown => kramdown::heading_to_fragment(heading),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anchor_style_serde() {
        // Test serialization
        assert_eq!(serde_json::to_string(&AnchorStyle::GitHub).unwrap(), "\"github\"");
        assert_eq!(serde_json::to_string(&AnchorStyle::Jekyll).unwrap(), "\"jekyll\"");
        assert_eq!(serde_json::to_string(&AnchorStyle::Kramdown).unwrap(), "\"kramdown\"");

        // Test deserialization
        assert_eq!(serde_json::from_str::<AnchorStyle>("\"github\"").unwrap(), AnchorStyle::GitHub);
        assert_eq!(serde_json::from_str::<AnchorStyle>("\"jekyll\"").unwrap(), AnchorStyle::Jekyll);
        assert_eq!(serde_json::from_str::<AnchorStyle>("\"kramdown\"").unwrap(), AnchorStyle::Kramdown);
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
            let jekyll = AnchorStyle::Jekyll.generate_fragment(case);
            let kramdown = AnchorStyle::Kramdown.generate_fragment(case);

            // Each style should produce a valid non-empty result
            assert!(!github.is_empty(), "GitHub style failed for: {}", case);
            assert!(!jekyll.is_empty(), "Jekyll style failed for: {}", case);
            assert!(!kramdown.is_empty(), "Kramdown style failed for: {}", case);
        }
    }
}