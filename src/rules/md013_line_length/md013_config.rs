use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Reflow mode for MD013
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ReflowMode {
    /// Only reflow lines that exceed the line length limit (default behavior)
    #[default]
    Default,
    /// Normalize all paragraphs to use the full line length
    Normalize,
    /// One sentence per line - break at sentence boundaries
    SentencePerLine,
}

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
    #[serde(default, alias = "enable_reflow", alias = "enable-reflow")]
    pub reflow: bool,

    /// Reflow mode - how to handle reflowing (default: "long-lines")
    #[serde(default)]
    pub reflow_mode: ReflowMode,
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
            reflow_mode: ReflowMode::default(),
        }
    }
}

impl RuleConfig for MD013Config {
    const RULE_NAME: &'static str = "MD013";
}
