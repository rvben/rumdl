use crate::rule_config_serde::RuleConfig;
use crate::types::HeadingLevel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct MD025Config {
    /// Heading level that counts as the document title: `1` (default) through `6`.
    #[serde(default)]
    #[schemars(description = "Heading level that counts as the document title: `1` (default) through `6`.")]
    pub level: HeadingLevel,

    /// Front matter key whose value is treated as the document title, so the body
    /// does not need a level-1 heading.
    #[serde(default = "default_front_matter_title", alias = "front_matter_title")]
    pub front_matter_title: String,

    /// Allows several sections of the document to each start a new level-1 heading.
    #[serde(default = "default_allow_document_sections", alias = "allow_document_sections")]
    pub allow_document_sections: bool,

    /// Allows a second level-1 heading when the first is followed by a separator.
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
