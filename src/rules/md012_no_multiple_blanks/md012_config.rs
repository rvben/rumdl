use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD012 (No multiple consecutive blank lines)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD012Config {
    /// Maximum number of consecutive blank lines allowed (default: 1)
    #[serde(default = "default_maximum")]
    pub maximum: usize,
}

fn default_maximum() -> usize {
    1
}

impl Default for MD012Config {
    fn default() -> Self {
        Self {
            maximum: default_maximum(),
        }
    }
}

impl RuleConfig for MD012Config {
    const RULE_NAME: &'static str = "MD012";
}
