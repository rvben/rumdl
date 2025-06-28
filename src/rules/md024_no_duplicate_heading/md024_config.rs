use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD024 (Multiple headings with the same content)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct MD024Config {
    /// Allow duplicate headings if they're nested at different levels (default: false)
    #[serde(default)]
    pub allow_different_nesting: bool,

    /// Only check siblings (same parent) for duplicates (default: false)
    #[serde(default)]
    pub siblings_only: bool,
}

impl RuleConfig for MD024Config {
    const RULE_NAME: &'static str = "MD024";
}
