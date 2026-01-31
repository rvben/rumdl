use crate::rule_config_serde::RuleConfig;
use crate::types::HeadingLevel;
use serde::{Deserialize, Serialize};

/// Heading style for auto-fix conversion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HeadingStyle {
    /// ATX style headings (## Heading)
    #[default]
    Atx,
}

/// Configuration for MD036 (Emphasis used instead of a heading)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD036Config {
    /// Punctuation characters that indicate emphasis is not being used as a heading.
    /// If the emphasized text ends with one of these characters, it won't be flagged.
    /// Default: ".,;:!?" - common trailing punctuation indicates a phrase, not a heading
    /// Set to empty string to flag all emphasis-only lines
    #[serde(default = "default_punctuation")]
    pub punctuation: String,

    /// Enable auto-fix to convert emphasis-as-heading to real headings.
    /// Default: false - auto-fix is opt-in to avoid unexpected document changes.
    /// When true, detected emphasis-only lines are converted to ATX headings.
    #[serde(default)]
    pub fix: bool,

    /// Heading style to use when auto-fixing.
    /// Default: "atx" (## Heading)
    #[serde(default, rename = "heading-style", alias = "heading_style")]
    pub heading_style: HeadingStyle,

    /// Heading level (1-6) to use when auto-fixing.
    /// Default: 2 (## Heading)
    /// Invalid values (0 or >6) produce a config validation error.
    #[serde(default = "default_heading_level", rename = "heading-level", alias = "heading_level")]
    pub heading_level: HeadingLevel,
}

fn default_punctuation() -> String {
    ".,;:!?".to_string()
}

fn default_heading_level() -> HeadingLevel {
    // Safe: 2 is always valid (1-6 range)
    HeadingLevel::new(2).unwrap()
}

impl Default for MD036Config {
    fn default() -> Self {
        Self {
            punctuation: default_punctuation(),
            fix: false,
            heading_style: HeadingStyle::default(),
            heading_level: default_heading_level(),
        }
    }
}

impl RuleConfig for MD036Config {
    const RULE_NAME: &'static str = "MD036";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = MD036Config::default();
        assert_eq!(config.punctuation, ".,;:!?");
        assert!(!config.fix);
        assert_eq!(config.heading_style, HeadingStyle::Atx);
        assert_eq!(config.heading_level.get(), 2);
    }

    #[test]
    fn test_kebab_case_config() {
        let toml_str = r#"
            punctuation = ".,;:"
            fix = true
            heading-style = "atx"
            heading-level = 3
        "#;
        let config: MD036Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.punctuation, ".,;:");
        assert!(config.fix);
        assert_eq!(config.heading_style, HeadingStyle::Atx);
        assert_eq!(config.heading_level.get(), 3);
    }

    #[test]
    fn test_snake_case_backwards_compatibility() {
        let toml_str = r#"
            punctuation = "."
            fix = true
            heading_style = "atx"
            heading_level = 4
        "#;
        let config: MD036Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.punctuation, ".");
        assert!(config.fix);
        assert_eq!(config.heading_style, HeadingStyle::Atx);
        assert_eq!(config.heading_level.get(), 4);
    }

    #[test]
    fn test_invalid_heading_level_rejected() {
        // Level 0 is invalid
        let toml_str = r#"
            heading-level = 0
        "#;
        let result: Result<MD036Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must be between 1 and 6"));

        // Level 7 is invalid
        let toml_str = r#"
            heading-level = 7
        "#;
        let result: Result<MD036Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must be between 1 and 6"));
    }

    #[test]
    fn test_all_valid_heading_levels() {
        for level in 1..=6 {
            let toml_str = format!("heading-level = {level}");
            let config: MD036Config = toml::from_str(&toml_str).unwrap();
            assert_eq!(config.heading_level.get(), level);
        }
    }
}
