use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD010 (No hard tabs)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD010Config {
    /// Number of spaces per tab (default: 4)
    #[serde(default = "default_spaces_per_tab")]
    pub spaces_per_tab: usize,

    /// Check code blocks (default: true)
    #[serde(default = "default_code_blocks")]
    pub code_blocks: bool,

    /// List of code languages to ignore (e.g., ["makefile", "make", "Makefile"])
    #[serde(default)]
    pub ignore_code_languages: Vec<String>,
}

fn default_spaces_per_tab() -> usize {
    4
}

fn default_code_blocks() -> bool {
    true
}

impl Default for MD010Config {
    fn default() -> Self {
        Self {
            spaces_per_tab: default_spaces_per_tab(),
            code_blocks: default_code_blocks(),
            ignore_code_languages: Vec::new(),
        }
    }
}

impl RuleConfig for MD010Config {
    const RULE_NAME: &'static str = "MD010";
}
