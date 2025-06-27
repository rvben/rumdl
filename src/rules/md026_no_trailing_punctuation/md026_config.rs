use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD026Config {
    #[serde(default = "default_punctuation")]
    pub punctuation: String,
}

impl Default for MD026Config {
    fn default() -> Self {
        Self {
            punctuation: default_punctuation(),
        }
    }
}

fn default_punctuation() -> String {
    ".,;".to_string()
}

impl RuleConfig for MD026Config {
    const RULE_NAME: &'static str = "MD026";
}
