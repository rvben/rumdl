use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD002 (First heading should be top level)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD002Config {
    /// The heading level required for the first heading (default: 1)
    #[serde(default = "default_level")]
    pub level: u32,
}

fn default_level() -> u32 {
    1
}

impl Default for MD002Config {
    fn default() -> Self {
        Self { level: default_level() }
    }
}

impl RuleConfig for MD002Config {
    const RULE_NAME: &'static str = "MD002";
}
