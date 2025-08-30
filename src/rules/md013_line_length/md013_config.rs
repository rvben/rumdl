use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD013 (Line length)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD013Config {
    /// Maximum line length (default: 80)
    #[serde(default = "default_line_length")]
    pub line_length: usize,

    /// Check code blocks for line length (default: true)
    #[serde(default = "default_code_blocks")]
    pub code_blocks: bool,

    /// Check tables for line length (default: true)
    #[serde(default = "default_tables")]
    pub tables: bool,

    /// Check headings for line length (default: true)
    #[serde(default = "default_headings")]
    pub headings: bool,

    /// Strict mode - disables exceptions for URLs, etc. (default: false)
    #[serde(default)]
    pub strict: bool,

    /// Enable text reflow to wrap long lines (default: false)
    #[serde(default, rename = "reflow", alias = "enable_reflow")]
    pub reflow: bool,
}

fn default_line_length() -> usize {
    80
}

fn default_code_blocks() -> bool {
    true
}

fn default_tables() -> bool {
    true
}

fn default_headings() -> bool {
    true
}

impl Default for MD013Config {
    fn default() -> Self {
        Self {
            line_length: default_line_length(),
            code_blocks: default_code_blocks(),
            tables: default_tables(),
            headings: default_headings(),
            strict: false,
            reflow: false,
        }
    }
}

impl RuleConfig for MD013Config {
    const RULE_NAME: &'static str = "MD013";
}
