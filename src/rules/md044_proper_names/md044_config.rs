use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD044Config {
    #[serde(default)]
    pub names: Vec<String>,

    #[serde(default = "default_code_blocks")]
    pub code_blocks: bool,

    #[serde(default = "default_html_comments")]
    pub html_comments: bool,
}

impl Default for MD044Config {
    fn default() -> Self {
        Self {
            names: Vec::new(),
            code_blocks: default_code_blocks(),
            html_comments: default_html_comments(),
        }
    }
}

fn default_code_blocks() -> bool {
    true
}

fn default_html_comments() -> bool {
    true
}

impl RuleConfig for MD044Config {
    const RULE_NAME: &'static str = "MD044";
}
