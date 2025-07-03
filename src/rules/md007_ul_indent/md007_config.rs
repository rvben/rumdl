use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD007 (Unordered list indentation)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD007Config {
    /// Indentation size for nested unordered lists (default: 2)
    #[serde(default = "default_indent")]
    pub indent: usize,

    /// Allow first level lists to start indented (default: false)
    #[serde(default)]
    pub start_indented: bool,

    /// Number of spaces for first level indent when start_indented is true (default: 2)
    #[serde(default = "default_start_indent")]
    pub start_indent: usize,
}

fn default_indent() -> usize {
    2
}

fn default_start_indent() -> usize {
    2
}

impl Default for MD007Config {
    fn default() -> Self {
        Self {
            indent: default_indent(),
            start_indented: false,
            start_indent: default_start_indent(),
        }
    }
}

impl RuleConfig for MD007Config {
    const RULE_NAME: &'static str = "MD007";
}
