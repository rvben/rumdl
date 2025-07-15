use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD013 (Line length)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD013Config {
    /// Maximum line length (default: 80)
    #[serde(default = "default_line_length")]
    pub line_length: usize,

    /// Apply rule to code blocks (default: true)
    #[serde(default = "default_code_blocks")]
    pub code_blocks: bool,

    /// Apply rule to tables (default: true)
    #[serde(default = "default_tables")]
    pub tables: bool,

    /// Apply rule to headings (default: true)
    #[serde(default = "default_headings")]
    pub headings: bool,

    /// Strict mode - disables exceptions for URLs, etc. (default: false)
    #[serde(default)]
    pub strict: bool,

    /// Maximum line length for headings (default: None, uses line_length)
    #[serde(default)]
    pub heading_line_length: Option<usize>,

    /// Maximum line length for code blocks (default: None, uses line_length)
    #[serde(default)]
    pub code_block_line_length: Option<usize>,

    /// Stern mode - stricter checking without exceptions (default: false)
    #[serde(default)]
    pub stern: bool,

    /// Enable text reflow to wrap long lines (default: false)
    #[serde(default)]
    pub enable_reflow: bool,
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
            heading_line_length: None,
            code_block_line_length: None,
            stern: false,
            enable_reflow: false,
        }
    }
}

impl RuleConfig for MD013Config {
    const RULE_NAME: &'static str = "MD013";
}
