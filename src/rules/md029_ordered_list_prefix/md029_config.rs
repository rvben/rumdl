use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Represents the style for ordered lists
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ListStyle {
    One, // Use '1.' for all items
    #[serde(rename = "one-one", alias = "one_one")]
    OneOne, // All ones (1. 1. 1.)
    Ordered, // Sequential (1. 2. 3.)
    #[serde(rename = "ordered0")]
    Ordered0, // Zero-based (0. 1. 2.)
    #[default]
    #[serde(rename = "one-or-ordered", alias = "one_or_ordered")]
    OneOrOrdered, // Either all ones OR sequential per-list (markdownlint default)
    Consistent, // Document-wide: use most prevalent style across all lists
}

/// Configuration for MD029 (Ordered list item prefix)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct MD029Config {
    /// Style for ordered list numbering (default: "one-or-ordered" - matches markdownlint)
    #[serde(default)]
    pub style: ListStyle,
}

impl RuleConfig for MD029Config {
    const RULE_NAME: &'static str = "MD029";
}
