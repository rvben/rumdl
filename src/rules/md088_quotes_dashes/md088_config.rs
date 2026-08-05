use std::collections::HashSet;

use crate::rule_config_serde::RuleConfig;
use crate::utils::unicode;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) struct MD088Config {
    /// Enables linting and autofix for look-alike quotes.
    #[serde(default = "default_normalize_quotes")]
    pub(super) normalize_quotes: bool,

    /// Enables linting and autofix for look-alike dashes.
    #[serde(default = "default_normalize_dashes")]
    pub(super) normalize_dashes: bool,

    /// Unicode characters to allow as-is.
    ///
    /// Any look-alike character with a codepoint present in this list
    /// is ignored by MD088 and not auto-replaced.
    #[serde(default, deserialize_with = "deserialize_allow")]
    pub(super) allow: HashSet<char>,
}

fn default_normalize_quotes() -> bool {
    true
}
fn default_normalize_dashes() -> bool {
    false
}

impl Default for MD088Config {
    fn default() -> Self {
        Self {
            normalize_quotes: default_normalize_quotes(),
            normalize_dashes: default_normalize_dashes(),
            allow: HashSet::new(),
        }
    }
}

fn deserialize_allow<'de, D>(deserializer: D) -> Result<HashSet<char>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::<String>::deserialize(deserializer)?;
    let mut out = HashSet::with_capacity(values.len());

    for raw in values {
        out.insert(
            unicode::parse_single_char(&raw)
                .or_else(|| unicode::parse_codepoint(&raw))
                .ok_or_else(|| {
                    serde::de::Error::custom(format!(
                        "Invalid codepoint '{raw}': expected format U+XXXX or a single character"
                    ))
                })?,
        );
    }

    Ok(out)
}

impl RuleConfig for MD088Config {
    const RULE_NAME: &'static str = "MD088";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = MD088Config::default();
        assert!(config.normalize_quotes);
        assert!(!config.normalize_dashes);
        assert!(config.allow.is_empty());
    }

    #[test]
    fn test_enable_dashes_deserializes() {
        let config: MD088Config = toml::from_str(
            r#"
            normalize-dashes = true
            "#,
        )
        .unwrap();

        assert!(config.normalize_dashes);
    }

    #[test]
    fn test_allow_deserialize_and_normalize() {
        let config: MD088Config = toml::from_str(
            r#"
            allow = ["u+2019", "U+2033", "’", "″"]
            "#,
        )
        .unwrap();

        assert_eq!(config.allow, HashSet::from(['\u{2019}', '\u{2033}']));
    }

    #[test]
    fn test_invalid_allow_codepoint_rejected() {
        let err = toml::from_str::<MD088Config>(
            r#"
            allow = ["2019"]
            "#,
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("expected format U+XXXX"));
    }
}
