use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD024 (Multiple headings with the same content)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD024Config {
    /// Allow duplicate headings if they're nested at different levels (default: false)
    #[serde(default, alias = "allow_different_nesting")]
    pub allow_different_nesting: bool,

    /// Only check siblings (same parent) for duplicates (default: true)
    ///
    /// Unlike markdownlint, rumdl defaults this to true to reduce false positives
    /// in common documentation patterns like CHANGELOGs.
    ///
    /// Note: This may cause duplicate anchor IDs when linking. Most renderers
    /// (GitHub, GitLab, etc.) handle this by adding numeric suffixes.
    #[serde(default = "default_siblings_only", alias = "siblings_only")]
    pub siblings_only: bool,

    /// Treat headings with different custom link anchors (e.g. `{#custom-id}`) as distinct (default: true)
    ///
    /// When true, headings that share the same visible text but carry different `{#id}` suffixes
    /// produce distinct deduplication keys and are not flagged as duplicates. This matches the
    /// effective behavior of markdownlint, which compares raw heading text (retaining the suffix).
    ///
    /// Set to false to restore the previous behavior where `{#id}` suffixes are ignored during
    /// deduplication.
    #[serde(
        default = "default_allow_different_link_anchors",
        alias = "allow_different_link_anchors"
    )]
    pub allow_different_link_anchors: bool,
}

fn default_siblings_only() -> bool {
    true
}

fn default_allow_different_link_anchors() -> bool {
    true
}

impl Default for MD024Config {
    fn default() -> Self {
        Self {
            allow_different_nesting: false,
            siblings_only: true,
            allow_different_link_anchors: true,
        }
    }
}

impl RuleConfig for MD024Config {
    const RULE_NAME: &'static str = "MD024";
}
