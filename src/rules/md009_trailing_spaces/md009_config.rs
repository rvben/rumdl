use serde::{Deserialize, Serialize};
use crate::rule_config_serde::RuleConfig;

/// Configuration for MD009 (Trailing spaces)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD009Config {
    /// Number of spaces for line breaks (default: 2)
    #[serde(default = "default_br_spaces")]
    pub br_spaces: usize,
    
    /// Strict mode - remove all trailing spaces (default: false)
    #[serde(default)]
    pub strict: bool,
}

fn default_br_spaces() -> usize {
    2
}

impl Default for MD009Config {
    fn default() -> Self {
        Self {
            br_spaces: default_br_spaces(),
            strict: false,
        }
    }
}

impl RuleConfig for MD009Config {
    const RULE_NAME: &'static str = "MD009";
}