use crate::rule_config_serde::RuleConfig;
use crate::types::PositiveUsize;
use serde::{Deserialize, Serialize};

/// Configuration for MD010 (No hard tabs)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD010Config {
    /// Number of spaces per tab (default: 4)
    #[serde(default = "default_spaces_per_tab", alias = "spaces_per_tab")]
    pub spaces_per_tab: PositiveUsize,
}

fn default_spaces_per_tab() -> PositiveUsize {
    PositiveUsize::from_const(4)
}

impl Default for MD010Config {
    fn default() -> Self {
        Self {
            spaces_per_tab: default_spaces_per_tab(),
        }
    }
}

impl RuleConfig for MD010Config {
    const RULE_NAME: &'static str = "MD010";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MD010Config::default();
        assert_eq!(config.spaces_per_tab.get(), 4);
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
            spaces-per-tab = 2
        "#;
        let config: MD010Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.spaces_per_tab.get(), 2);
    }

    #[test]
    fn test_snake_case_backwards_compatibility() {
        let toml_str = r#"
            spaces_per_tab = 8
        "#;
        let config: MD010Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.spaces_per_tab.get(), 8);
    }

    #[test]
    fn test_validation_error() {
        // Test that 0 is rejected
        let toml_str = r#"
            spaces-per-tab = 0
        "#;
        let result: Result<MD010Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must be at least 1") || err.contains("got 0"));
    }
}
