use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD025Config {
    #[serde(default = "default_level")]
    pub level: usize,

    #[serde(default = "default_front_matter_title")]
    pub front_matter_title: String,

    #[serde(default = "default_allow_document_sections")]
    pub allow_document_sections: bool,

    #[serde(default = "default_allow_with_separators")]
    pub allow_with_separators: bool,
}

impl Default for MD025Config {
    fn default() -> Self {
        Self {
            level: default_level(),
            front_matter_title: default_front_matter_title(),
            allow_document_sections: default_allow_document_sections(),
            allow_with_separators: default_allow_with_separators(),
        }
    }
}

fn default_level() -> usize {
    1
}

fn default_front_matter_title() -> String {
    "title".to_string()
}

fn default_allow_document_sections() -> bool {
    true
}

fn default_allow_with_separators() -> bool {
    true
}

impl RuleConfig for MD025Config {
    const RULE_NAME: &'static str = "MD025";
}
