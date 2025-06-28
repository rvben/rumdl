use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MD033Config {
    #[serde(default, rename = "allowed_elements")]
    pub allowed: Vec<String>,
}

impl MD033Config {
    /// Convert allowed elements to HashSet for efficient lookup
    pub fn allowed_set(&self) -> HashSet<String> {
        self.allowed.iter().map(|s| s.to_lowercase()).collect()
    }
}

impl RuleConfig for MD033Config {
    const RULE_NAME: &'static str = "MD033";
}
