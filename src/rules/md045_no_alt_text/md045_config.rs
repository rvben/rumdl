use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD045Config {
    #[serde(default = "default_placeholder_text")]
    pub placeholder_text: String,
}

impl Default for MD045Config {
    fn default() -> Self {
        Self {
            placeholder_text: default_placeholder_text(),
        }
    }
}

fn default_placeholder_text() -> String {
    "TODO: Add image description".to_string()
}

impl RuleConfig for MD045Config {
    const RULE_NAME: &'static str = "MD045";
}
