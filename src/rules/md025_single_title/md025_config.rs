use crate::rule_config_serde::RuleConfig;
use crate::types::HeadingLevel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD025Config {
    #[serde(default)]
    pub level: HeadingLevel,

    #[serde(default = "default_front_matter_title", alias = "front_matter_title")]
    pub front_matter_title: String,

    #[serde(default = "default_allow_document_sections", alias = "allow_document_sections")]
    pub allow_document_sections: bool,

    #[serde(default = "default_allow_with_separators", alias = "allow_with_separators")]
    pub allow_with_separators: bool,
}

impl Default for MD025Config {
    fn default() -> Self {
        Self {
            level: HeadingLevel::default(),
            front_matter_title: default_front_matter_title(),
            allow_document_sections: default_allow_document_sections(),
            allow_with_separators: default_allow_with_separators(),
        }
    }
}

fn default_front_matter_title() -> String {
    "title".to_string()
}

fn default_allow_document_sections() -> bool {
    false // Changed to false for markdownlint compatibility
}

fn default_allow_with_separators() -> bool {
    false // Changed to false for markdownlint compatibility
}

impl RuleConfig for MD025Config {
    const RULE_NAME: &'static str = "MD025";
}
