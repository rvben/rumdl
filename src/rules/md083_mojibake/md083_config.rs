use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) struct MD083Config {
    /// Mojibake sequences that should not trigger MD083 findings.
    #[serde(default)]
    pub(super) ignore: Vec<String>,

    /// Whether to ignore mojibake findings inside fenced and indented code blocks.
    #[serde(default = "default_ignore_code_blocks", alias = "ignore_code_blocks")]
    pub(super) ignore_code_blocks: bool,
}

fn default_ignore_code_blocks() -> bool {
    true
}

impl Default for MD083Config {
    fn default() -> Self {
        Self {
            ignore: Vec::new(),
            ignore_code_blocks: default_ignore_code_blocks(),
        }
    }
}

impl RuleConfig for MD083Config {
    const RULE_NAME: &'static str = "MD083";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = MD083Config::default();
        assert!(config.ignore.is_empty());
        assert!(config.ignore_code_blocks);
    }

    #[test]
    fn test_deserialize_kebab_case() {
        let config: MD083Config = toml::from_str(
            r#"
            ignore = ["â€“", "â€™"]
            ignore-code-blocks = false
            "#,
        )
        .unwrap();

        assert_eq!(config.ignore, vec!["â€“", "â€™"]);
        assert!(!config.ignore_code_blocks);
    }

    #[test]
    fn test_deserialize_snake_case_alias() {
        let config: MD083Config = toml::from_str(
            r#"
            ignore_code_blocks = false
            "#,
        )
        .unwrap();

        assert!(!config.ignore_code_blocks);
    }
}
