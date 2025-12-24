use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD032 (Lists should be surrounded by blank lines)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD032Config {
    /// Allow lazy continuation of list items (default: true)
    ///
    /// When true (default), text following a list item without indentation is treated
    /// as lazy continuation per CommonMark spec and no warning is generated.
    ///
    /// When false, warns when unindented text follows a list item without a blank line.
    /// This helps catch cases where text was intended to be a separate paragraph.
    ///
    /// Example with `allow_lazy_continuation = false`:
    /// ```markdown
    /// 1. List item
    /// Some text.    <- Warning: should have blank line or indentation
    /// ```
    #[serde(default = "default_allow_lazy_continuation", alias = "allow_lazy_continuation")]
    pub allow_lazy_continuation: bool,
}

fn default_allow_lazy_continuation() -> bool {
    true
}

impl Default for MD032Config {
    fn default() -> Self {
        Self {
            allow_lazy_continuation: default_allow_lazy_continuation(),
        }
    }
}

impl RuleConfig for MD032Config {
    const RULE_NAME: &'static str = "MD032";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MD032Config::default();
        assert!(config.allow_lazy_continuation);
    }

    #[test]
    fn test_kebab_case_config() {
        let toml_str = r#"
            allow-lazy-continuation = false
        "#;
        let config: MD032Config = toml::from_str(toml_str).unwrap();
        assert!(!config.allow_lazy_continuation);
    }

    #[test]
    fn test_snake_case_backwards_compatibility() {
        let toml_str = r#"
            allow_lazy_continuation = false
        "#;
        let config: MD032Config = toml::from_str(toml_str).unwrap();
        assert!(!config.allow_lazy_continuation);
    }
}
