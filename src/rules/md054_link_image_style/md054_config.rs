use serde::{Deserialize, Serialize};
use crate::rule_config_serde::RuleConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MD054Config {
    #[serde(default = "default_true")]
    pub autolink: bool,
    #[serde(default = "default_true")]
    pub collapsed: bool,
    #[serde(default = "default_true")]
    pub full: bool,
    #[serde(default = "default_true")]
    pub inline: bool,
    #[serde(default = "default_true")]
    pub shortcut: bool,
    #[serde(default = "default_true")]
    pub url_inline: bool,
}

impl Default for MD054Config {
    fn default() -> Self {
        Self {
            autolink: true,
            collapsed: true,
            full: true,
            inline: true,
            shortcut: true,
            url_inline: true,
        }
    }
}

fn default_true() -> bool {
    true
}

impl RuleConfig for MD054Config {
    const RULE_NAME: &'static str = "MD054";
}