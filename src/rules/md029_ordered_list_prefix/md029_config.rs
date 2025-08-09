use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Represents the style for ordered lists
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ListStyle {
    One, // Use '1.' for all items
    #[serde(rename = "one-one", alias = "one_one")]
    OneOne, // All ones (1. 1. 1.)
    #[default]
    Ordered, // Sequential (1. 2. 3.)
    #[serde(rename = "ordered0")]
    Ordered0, // Zero-based (0. 1. 2.)
}

/// Configuration for MD029 (Ordered list item prefix)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct MD029Config {
    /// Style for ordered list numbering (default: "ordered")
    #[serde(default)]
    pub style: ListStyle,
}

impl RuleConfig for MD029Config {
    const RULE_NAME: &'static str = "MD029";
}
