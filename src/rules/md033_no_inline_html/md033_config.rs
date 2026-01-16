use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// GFM security tags that are filtered/disallowed by default in GitHub Flavored Markdown.
/// These tags can execute scripts, load external content, or otherwise pose security risks.
///
/// Reference: <https://github.github.com/gfm/#disallowed-raw-html-extension->
pub const GFM_DISALLOWED_TAGS: &[&str] = &[
    "title",
    "textarea",
    "style",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "script",
    "plaintext",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MD033Config {
    #[serde(default, rename = "allowed-elements", alias = "allowed_elements", alias = "allowed")]
    pub allowed: Vec<String>,

    /// List of HTML tags that are explicitly disallowed.
    /// When set, only these tags will trigger warnings (allowlist mode is disabled).
    /// Use `"gfm"` as a special value to use GFM's security-filtered tags.
    #[serde(
        default,
        rename = "disallowed-elements",
        alias = "disallowed_elements",
        alias = "disallowed"
    )]
    pub disallowed: Vec<String>,
}

impl MD033Config {
    /// Convert allowed elements to HashSet for efficient lookup
    pub fn allowed_set(&self) -> HashSet<String> {
        self.allowed.iter().map(|s| s.to_lowercase()).collect()
    }

    /// Convert disallowed elements to HashSet for efficient lookup.
    /// If the list contains "gfm", expands to the GFM security tags.
    pub fn disallowed_set(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        for tag in &self.disallowed {
            let lower = tag.to_lowercase();
            if lower == "gfm" {
                // Expand "gfm" to all GFM security tags
                for gfm_tag in GFM_DISALLOWED_TAGS {
                    set.insert((*gfm_tag).to_string());
                }
            } else {
                set.insert(lower);
            }
        }
        set
    }

    /// Check if the rule is operating in disallowed-only mode
    pub fn is_disallowed_mode(&self) -> bool {
        !self.disallowed.is_empty()
    }
}

impl RuleConfig for MD033Config {
    const RULE_NAME: &'static str = "MD033";
}
