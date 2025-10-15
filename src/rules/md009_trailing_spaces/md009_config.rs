use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD009 (Trailing spaces)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD009Config {
    /// Number of spaces for line breaks (default: 2)
    #[serde(default = "default_br_spaces", alias = "br_spaces")]
    pub br_spaces: usize,

    /// Strict mode - remove all trailing spaces (default: false)
    #[serde(default)]
    pub strict: bool,

    /// Allow trailing spaces in empty list item lines (default: false)
    #[serde(default, alias = "list_item_empty_lines")]
    pub list_item_empty_lines: bool,
}

fn default_br_spaces() -> usize {
    2
}

impl Default for MD009Config {
    fn default() -> Self {
        Self {
            br_spaces: default_br_spaces(),
            strict: false,
            list_item_empty_lines: false,
        }
    }
}

impl RuleConfig for MD009Config {
    const RULE_NAME: &'static str = "MD009";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_case_backwards_compatibility() {
        let toml_str = r#"
            br_spaces = 3
            list_item_empty_lines = true
        "#;
        let config: MD009Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.br_spaces, 3);
        assert!(config.list_item_empty_lines);
    }

    #[test]
    fn test_kebab_case_canonical_format() {
        let toml_str = r#"
            br-spaces = 3
            list-item-empty-lines = true
        "#;
        let config: MD009Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.br_spaces, 3);
        assert!(config.list_item_empty_lines);
    }
}
