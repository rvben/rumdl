use serde::{Deserialize, Serialize};
use crate::rule_config_serde::RuleConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD044Config {
    #[serde(default)]
    pub names: Vec<String>,
    
    #[serde(default = "default_code_blocks")]
    pub code_blocks: bool,
}

impl Default for MD044Config {
    fn default() -> Self {
        Self {
            names: Vec::new(),
            code_blocks: default_code_blocks(),
        }
    }
}

fn default_code_blocks() -> bool {
    true
}

impl RuleConfig for MD044Config {
    const RULE_NAME: &'static str = "MD044";
}