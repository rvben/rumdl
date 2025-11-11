use crate::rule_config_serde::RuleConfig;
use crate::types::HeadingLevel;
use serde::{Deserialize, Serialize};

/// Configuration for MD041 (First line heading)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD041Config {
    /// The required heading level (default: 1)
    #[serde(default)]
    pub level: HeadingLevel,

    /// Front matter title field to check (default: "title")
    /// Set to empty string to disable front matter title checking
    #[serde(default = "default_front_matter_title", alias = "front_matter_title")]
    pub front_matter_title: String,

    /// Optional regex pattern for front matter title field (default: None)
    /// If provided, checks for this pattern in front matter instead of "title:"
    #[serde(default, alias = "front_matter_title_pattern")]
    pub front_matter_title_pattern: Option<String>,
}

fn default_front_matter_title() -> String {
    "title".to_string()
}

impl Default for MD041Config {
    fn default() -> Self {
        Self {
            level: HeadingLevel::default(),
            front_matter_title: default_front_matter_title(),
            front_matter_title_pattern: None,
        }
    }
}

impl RuleConfig for MD041Config {
    const RULE_NAME: &'static str = "MD041";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MD041Config::default();
        assert_eq!(config.level.get(), 1);
        assert_eq!(config.front_matter_title, "title");
        assert!(config.front_matter_title_pattern.is_none());
    }

    #[test]
    fn test_config_deserialization_kebab_case() {
        let toml_str = r#"
            level = 2
            front-matter-title = "heading"
            front-matter-title-pattern = "^(title|header):"
        "#;
        let config: MD041Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.level.get(), 2);
        assert_eq!(config.front_matter_title, "heading");
        assert_eq!(config.front_matter_title_pattern, Some("^(title|header):".to_string()));
    }

    #[test]
    fn test_config_deserialization_snake_case_backwards_compat() {
        // Test backwards compatibility with snake_case (via serde's automatic alias)
        let toml_str = r#"
            level = 3
            front_matter_title = "mytitle"
        "#;
        let config: MD041Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.level.get(), 3);
        assert_eq!(config.front_matter_title, "mytitle");
    }

    #[test]
    fn test_config_serialization() {
        let config = MD041Config {
            level: HeadingLevel::new(2).unwrap(),
            front_matter_title: "header".to_string(),
            front_matter_title_pattern: Some("^heading:".to_string()),
        };

        let toml_str = toml::to_string(&config).unwrap();
        // Should serialize to kebab-case
        assert!(toml_str.contains("front-matter-title"));
        assert!(toml_str.contains("front-matter-title-pattern"));
        assert!(!toml_str.contains("front_matter_title"));
    }

    #[test]
    fn test_empty_front_matter_title() {
        let toml_str = r#"
            level = 1
            front-matter-title = ""
        "#;
        let config: MD041Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.front_matter_title, "");
    }
}
