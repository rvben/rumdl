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
//!
//! Common utilities are shared via the `common` module to avoid duplication.

pub mod common;
pub mod github;
pub mod kramdown;
pub mod kramdown_gfm; // Renamed from jekyll for clarity
pub mod python_markdown;

use serde::{Deserialize, Serialize};

/// Anchor generation style for heading fragments
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum AnchorStyle {
    /// GitHub/GFM style (default): preserves underscores, removes punctuation
    #[default]
    #[serde(rename = "github")]
    GitHub,
    /// Kramdown with GFM input: matches Jekyll/GitHub Pages behavior
    /// Accepts "kramdown-gfm", "kramdown_gfm", and "jekyll" (for backward compatibility)
    #[serde(rename = "kramdown-gfm", alias = "kramdown_gfm", alias = "jekyll")]
    KramdownGfm,
    /// Pure kramdown style: removes underscores and punctuation
    #[serde(rename = "kramdown")]
    Kramdown,
    /// Python-Markdown style: used by MkDocs (NFKD → ASCII, collapse separators)
    #[serde(rename = "python-markdown", alias = "python_markdown", alias = "mkdocs")]
    PythonMarkdown,
}

impl AnchorStyle {
    /// The anchor generation a flavor's renderer performs natively.
    ///
    /// Used when the user has not pinned `anchor-style`, so a document is
    /// checked against the anchors its own platform emits. `per-file-flavor`
    /// makes this a per-document answer, so resolve it from the flavor the file
    /// is parsed with rather than from the global one.
    pub fn for_flavor(flavor: crate::config::MarkdownFlavor) -> Self {
        match flavor {
            crate::config::MarkdownFlavor::MkDocs => AnchorStyle::PythonMarkdown,
            crate::config::MarkdownFlavor::Kramdown => AnchorStyle::KramdownGfm,
            _ => AnchorStyle::GitHub,
        }
    }

    /// Generate an anchor fragment using the specified style
    pub fn generate_fragment(&self, heading: &str) -> String {
        match self {
            AnchorStyle::GitHub => github::heading_to_fragment(heading),
            AnchorStyle::KramdownGfm => kramdown_gfm::heading_to_fragment(heading),
            AnchorStyle::Kramdown => kramdown::heading_to_fragment(heading),
            AnchorStyle::PythonMarkdown => python_markdown::heading_to_fragment(heading),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_whitespace_an_anchor_element_leaves_is_slugged_per_style() {
        // Slug text arrives with the whitespace an anchor element left at either
        // end (`## Alpha <a id="x"></a>` gives `Alpha `). GitHub and kramdown-GFM
        // slug every space to a hyphen and never trim; Python-Markdown trims and
        // collapses. Columns verified against GitHub.com, kramdown 2.5.2 and
        // Python-Markdown 3.10.3.
        let cases = [
            ("Alpha ", "alpha-", "alpha-", "alpha"),
            (" Beta", "-beta", "-beta", "beta"),
            ("Foo  Bar", "foo--bar", "foo--bar", "foo-bar"),
            ("Foo Bar", "foo-bar", "foo-bar", "foo-bar"),
            ("Gamma <!-- note -->", "gamma-", "gamma-", "gamma"),
            ("<!-- hidden --> Title", "-title", "-title", "title"),
        ];
        for (slug_text, github, kramdown_gfm, python_markdown) in cases {
            assert_eq!(
                AnchorStyle::GitHub.generate_fragment(slug_text),
                github,
                "github {slug_text:?}"
            );
            assert_eq!(
                AnchorStyle::KramdownGfm.generate_fragment(slug_text),
                kramdown_gfm,
                "kramdown-gfm {slug_text:?}"
            );
            assert_eq!(
                AnchorStyle::PythonMarkdown.generate_fragment(slug_text),
                python_markdown,
                "python-markdown {slug_text:?}"
            );
        }
    }

    #[test]
    fn test_anchor_style_serde() {
        // Test serialization (uses primary names)
        assert_eq!(serde_json::to_string(&AnchorStyle::GitHub).unwrap(), "\"github\"");
        assert_eq!(
            serde_json::to_string(&AnchorStyle::KramdownGfm).unwrap(),
            "\"kramdown-gfm\""
        );
        assert_eq!(serde_json::to_string(&AnchorStyle::Kramdown).unwrap(), "\"kramdown\"");
        assert_eq!(
            serde_json::to_string(&AnchorStyle::PythonMarkdown).unwrap(),
            "\"python-markdown\""
        );

        // Test deserialization with primary names (kebab-case)
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
        assert_eq!(
            serde_json::from_str::<AnchorStyle>("\"python-markdown\"").unwrap(),
            AnchorStyle::PythonMarkdown
        );

        // Test snake_case alias
        assert_eq!(
            serde_json::from_str::<AnchorStyle>("\"kramdown_gfm\"").unwrap(),
            AnchorStyle::KramdownGfm
        );
        assert_eq!(
            serde_json::from_str::<AnchorStyle>("\"python_markdown\"").unwrap(),
            AnchorStyle::PythonMarkdown
        );

        // Test backward compatibility aliases
        assert_eq!(
            serde_json::from_str::<AnchorStyle>("\"jekyll\"").unwrap(),
            AnchorStyle::KramdownGfm
        );
        assert_eq!(
            serde_json::from_str::<AnchorStyle>("\"mkdocs\"").unwrap(),
            AnchorStyle::PythonMarkdown
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
            let python_md = AnchorStyle::PythonMarkdown.generate_fragment(case);

            // Each style should produce a valid non-empty result
            assert!(!github.is_empty(), "GitHub style failed for: {case}");
            assert!(!kramdown_gfm.is_empty(), "KramdownGfm style failed for: {case}");
            assert!(!kramdown.is_empty(), "Kramdown style failed for: {case}");
            assert!(!python_md.is_empty(), "PythonMarkdown style failed for: {case}");
        }
    }
}
