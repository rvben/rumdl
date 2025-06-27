use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD030 (Spaces after list markers)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD030Config {
    /// Spaces for single-line unordered list items (default: 1)
    #[serde(default = "default_spaces")]
    pub ul_single: usize,

    /// Spaces for multi-line unordered list items (default: 1)
    #[serde(default = "default_spaces")]
    pub ul_multi: usize,

    /// Spaces for single-line ordered list items (default: 1)
    #[serde(default = "default_spaces")]
    pub ol_single: usize,

    /// Spaces for multi-line ordered list items (default: 1)
    #[serde(default = "default_spaces")]
    pub ol_multi: usize,
}

fn default_spaces() -> usize {
    1
}

impl Default for MD030Config {
    fn default() -> Self {
        Self {
            ul_single: default_spaces(),
            ul_multi: default_spaces(),
            ol_single: default_spaces(),
            ol_multi: default_spaces(),
        }
    }
}

impl RuleConfig for MD030Config {
    const RULE_NAME: &'static str = "MD030";
}
