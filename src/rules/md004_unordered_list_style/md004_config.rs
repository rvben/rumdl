use super::UnorderedListStyle;
use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD004 (Unordered list style)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(super) struct MD004Config {
    /// The style for unordered list markers
    #[serde(default)]
    pub style: UnorderedListStyle,
}

impl RuleConfig for MD004Config {
    const RULE_NAME: &'static str = "MD004";
}
