use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD022 (Headings should be surrounded by blank lines)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD022Config {
    /// Number of blank lines required above headings (default: 1)
    #[serde(default = "default_lines_above", alias = "lines_above")]
    pub lines_above: usize,

    /// Number of blank lines required below headings (default: 1)
    #[serde(default = "default_lines_below", alias = "lines_below")]
    pub lines_below: usize,

    /// Whether the first heading can be at the start of the document (default: true)
    #[serde(default = "default_allowed_at_start", alias = "allowed_at_start")]
    pub allowed_at_start: bool,
}

fn default_lines_above() -> usize {
    1
}

fn default_lines_below() -> usize {
    1
}

fn default_allowed_at_start() -> bool {
    true
}

impl Default for MD022Config {
    fn default() -> Self {
        Self {
            lines_above: default_lines_above(),
            lines_below: default_lines_below(),
            allowed_at_start: default_allowed_at_start(),
        }
    }
}

impl RuleConfig for MD022Config {
    const RULE_NAME: &'static str = "MD022";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_case_backwards_compatibility() {
        let toml_str = r#"
            lines_above = 2
            lines_below = 3
            allowed_at_start = false
        "#;
        let config: MD022Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.lines_above, 2);
        assert_eq!(config.lines_below, 3);
        assert!(!config.allowed_at_start);
    }

    #[test]
    fn test_kebab_case_canonical_format() {
        let toml_str = r#"
            lines-above = 2
            lines-below = 3
            allowed-at-start = false
        "#;
        let config: MD022Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.lines_above, 2);
        assert_eq!(config.lines_below, 3);
        assert!(!config.allowed_at_start);
    }
}
