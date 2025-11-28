use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD061Config {
    #[serde(default)]
    pub terms: Vec<String>,

    #[serde(default, alias = "case_sensitive")]
    pub case_sensitive: bool,
}

impl Default for MD061Config {
    fn default() -> Self {
        Self {
            terms: Vec::new(),
            case_sensitive: true,
        }
    }
}

impl RuleConfig for MD061Config {
    const RULE_NAME: &'static str = "MD061";
}
