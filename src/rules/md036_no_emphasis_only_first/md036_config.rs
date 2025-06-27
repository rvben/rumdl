use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD036 (Emphasis used instead of a heading)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD036Config {
    /// Punctuation characters to remove from the end of headings when converting from emphasis
    /// Default: ".,;:!?" - removes common trailing punctuation
    /// Set to empty string to preserve all punctuation
    #[serde(default = "default_punctuation")]
    pub punctuation: String,
}

fn default_punctuation() -> String {
    ".,;:!?".to_string()
}

impl Default for MD036Config {
    fn default() -> Self {
        Self {
            punctuation: default_punctuation(),
        }
    }
}

impl RuleConfig for MD036Config {
    const RULE_NAME: &'static str = "MD036";
}
