use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD010 (No hard tabs)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD010Config {
    /// Number of spaces per tab (default: 4)
    #[serde(default = "default_spaces_per_tab")]
    pub spaces_per_tab: usize,
}

fn default_spaces_per_tab() -> usize {
    4
}

impl Default for MD010Config {
    fn default() -> Self {
        Self {
            spaces_per_tab: default_spaces_per_tab(),
        }
    }
}

impl RuleConfig for MD010Config {
    const RULE_NAME: &'static str = "MD010";
}
