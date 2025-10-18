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
}

fn default_siblings_only() -> bool {
    true
}

impl Default for MD024Config {
    fn default() -> Self {
        Self {
            allow_different_nesting: false,
            siblings_only: true,
        }
    }
}

impl RuleConfig for MD024Config {
    const RULE_NAME: &'static str = "MD024";
}
