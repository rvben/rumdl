use crate::rule_config_serde::RuleConfig;
use crate::types::BrSpaces;
use serde::{Deserialize, Serialize};

/// Configuration for MD009 (Trailing spaces)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct MD009Config {
    /// Number of trailing spaces kept as a hard line break (default: 2).
    /// A value below 2 turns the exception off and flags every trailing space.
    #[serde(default, alias = "br_spaces")]
    pub br_spaces: BrSpaces,

    /// Strict mode - also flag a line-break run where it renders no `<br>` (default: false)
    #[serde(default)]
    pub strict: bool,

    /// Allow trailing spaces in empty list item lines (default: false)
    #[serde(default, alias = "list_item_empty_lines")]
    pub list_item_empty_lines: bool,
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
        assert_eq!(config.br_spaces.get(), 3);
        assert!(config.list_item_empty_lines);
    }

    #[test]
    fn test_kebab_case_canonical_format() {
        let toml_str = r#"
            br-spaces = 3
            list-item-empty-lines = true
        "#;
        let config: MD009Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.br_spaces.get(), 3);
        assert!(config.list_item_empty_lines);
    }

    #[test]
    fn test_br_spaces_below_two_is_accepted_and_disables_the_exception() {
        for value in [0, 1] {
            let toml_str = format!("br-spaces = {value}");
            let config: MD009Config = toml::from_str(&toml_str).unwrap();
            assert_eq!(config.br_spaces.get(), value);
            assert_eq!(config.br_spaces.line_break(), None);
        }
    }
}
