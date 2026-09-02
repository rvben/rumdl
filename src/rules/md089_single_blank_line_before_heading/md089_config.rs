//! Configuration for rule MD089.
use serde::{Deserialize, Serialize};

/// Heading styles MD089 checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MD089HeadingStyle {
    /// ATX headings (`# Heading`).
    Atx,
    /// Setext headings (`Heading\n======`).
    Setext,
}

/// Configuration for the MD089 rule.
///
/// The rule enforces that every heading is preceded by exactly one blank
/// line, except the first heading in the document (configurable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct MD089Config {
    /// Heading styles the rule checks. Defaults to both ATX and Setext.
    #[serde(default = "default_heading_styles")]
    pub heading_styles: Vec<MD089HeadingStyle>,
    /// Whether the first heading in the document is exempt from the
    /// blank-line requirement. Defaults to `true`.
    #[serde(default = "default_first_heading_exempt")]
    pub first_heading_exempt: bool,
}

fn default_heading_styles() -> Vec<MD089HeadingStyle> {
    vec![MD089HeadingStyle::Atx, MD089HeadingStyle::Setext]
}

fn default_first_heading_exempt() -> bool {
    true
}

impl Default for MD089Config {
    fn default() -> Self {
        Self {
            heading_styles: default_heading_styles(),
            first_heading_exempt: default_first_heading_exempt(),
        }
    }
}

impl crate::rule_config_serde::RuleConfig for MD089Config {
    const RULE_NAME: &'static str = "MD089";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule_config_serde::load_rule_config;
    use std::collections::BTreeMap;

    #[test]
    fn test_default_config() {
        let config = MD089Config::default();
        assert!(config.first_heading_exempt);
        assert_eq!(config.heading_styles.len(), 2);
    }

    #[test]
    fn test_parse_config_from_toml() {
        let toml_str = r#"
            heading-styles = ["atx"]
            first-heading-exempt = false
        "#;
        let config: MD089Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.heading_styles, vec![MD089HeadingStyle::Atx]);
        assert!(!config.first_heading_exempt);
    }

    #[test]
    fn test_load_from_config() {
        let mut config = crate::config::Config::default();
        let mut values = BTreeMap::new();
        values.insert(
            "heading-styles".to_string(),
            toml::Value::Array(vec![toml::Value::String("setext".to_string())]),
        );
        config.rules.insert(
            "MD089".to_string(),
            crate::config::RuleConfig { severity: None, values },
        );
        let loaded: MD089Config = load_rule_config(&config);
        assert_eq!(loaded.heading_styles, vec![MD089HeadingStyle::Setext]);
        assert!(loaded.first_heading_exempt, "default should be true");
    }
}
