use crate::rule_config_serde::RuleConfig;
use crate::types::PositiveUsize;
use serde::{Deserialize, Serialize};

/// Configuration for MD030 (Spaces after list markers)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD030Config {
    /// Spaces for single-line unordered list items (default: 1)
    #[serde(default = "default_spaces", alias = "ul_single")]
    pub ul_single: PositiveUsize,

    /// Spaces for multi-line unordered list items (default: 1)
    #[serde(default = "default_spaces", alias = "ul_multi")]
    pub ul_multi: PositiveUsize,

    /// Spaces for single-line ordered list items (default: 1)
    #[serde(default = "default_spaces", alias = "ol_single")]
    pub ol_single: PositiveUsize,

    /// Spaces for multi-line ordered list items (default: 1)
    #[serde(default = "default_spaces", alias = "ol_multi")]
    pub ol_multi: PositiveUsize,
}

fn default_spaces() -> PositiveUsize {
    PositiveUsize::from_const(1)
}

impl Default for MD030Config {
    fn default() -> Self {
        Self {
            ul_single: default_spaces(),
            ul_multi: default_spaces(),
            ol_single: default_spaces(),
            ol_multi: default_spaces(),
        }
    }
}

impl RuleConfig for MD030Config {
    const RULE_NAME: &'static str = "MD030";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_case_backwards_compatibility() {
        let toml_str = r#"
            ul_single = 2
            ol_single = 3
            ul_multi = 4
            ol_multi = 5
        "#;
        let config: MD030Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.ul_single.get(), 2);
        assert_eq!(config.ol_single.get(), 3);
        assert_eq!(config.ul_multi.get(), 4);
        assert_eq!(config.ol_multi.get(), 5);
    }

    #[test]
    fn test_kebab_case_canonical_format() {
        let toml_str = r#"
            ul-single = 2
            ol-single = 3
            ul-multi = 4
            ol-multi = 5
        "#;
        let config: MD030Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.ul_single.get(), 2);
        assert_eq!(config.ol_single.get(), 3);
        assert_eq!(config.ul_multi.get(), 4);
        assert_eq!(config.ol_multi.get(), 5);
    }
}
