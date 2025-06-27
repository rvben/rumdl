use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MD057Config {
    #[serde(default = "default_skip_media_files")]
    pub skip_media_files: bool,
}

impl Default for MD057Config {
    fn default() -> Self {
        Self { skip_media_files: true }
    }
}

fn default_skip_media_files() -> bool {
    true
}

impl RuleConfig for MD057Config {
    const RULE_NAME: &'static str = "MD057";
}
