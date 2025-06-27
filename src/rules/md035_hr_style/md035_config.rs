use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD035 (Horizontal rule style)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD035Config {
    /// The style for horizontal rules (default: "---")
    /// Can be "---", "***", "___", "- - -", "* * *", "_ _ _", or "consistent"
    #[serde(default = "default_style")]
    pub style: String,
}

fn default_style() -> String {
    "---".to_string()
}

impl Default for MD035Config {
    fn default() -> Self {
        Self { style: default_style() }
    }
}

impl RuleConfig for MD035Config {
    const RULE_NAME: &'static str = "MD035";
}
