use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD014 (Commands in code blocks should show output)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD014Config {
    /// Whether commands should show output (default: true)
    #[serde(default = "default_show_output")]
    pub show_output: bool,
}

fn default_show_output() -> bool {
    true
}

impl Default for MD014Config {
    fn default() -> Self {
        Self {
            show_output: default_show_output(),
        }
    }
}

impl RuleConfig for MD014Config {
    const RULE_NAME: &'static str = "MD014";
}
