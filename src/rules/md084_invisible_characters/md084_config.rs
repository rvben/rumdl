use crate::{rule_config_serde::RuleConfig, utils::unicode};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub(super) struct MD084Config {
    /// When enabled, report any invisible character anywhere in the document.
    #[serde(default)]
    pub(super) strict: bool,

    /// Codepoint allow-list in `U+XXXX` / `U+XXXXX` / `U+XXXXXX` format.
    ///
    /// Any matched invisible character with a codepoint present in this list
    /// is ignored.
    #[serde(default, deserialize_with = "deserialize_allow")]
    pub(super) allow: HashSet<char>,
}

fn deserialize_allow<'de, D>(deserializer: D) -> Result<HashSet<char>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::<String>::deserialize(deserializer)?;
    let mut out = HashSet::with_capacity(values.len());

    for raw in values {
        out.insert(unicode::parse_codepoint(&raw).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "Invalid codepoint '{raw}': expected format U+XXXX or a single character"
            ))
        })?);
    }

    Ok(out)
}

impl RuleConfig for MD084Config {
    const RULE_NAME: &'static str = "MD084";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = MD084Config::default();
        assert!(!config.strict);
        assert!(config.allow.is_empty());
    }

    #[test]
    fn test_allow_deserialize_and_normalize() {
        let config: MD084Config = toml::from_str(
            r#"
            allow = ["u+200b", "U+1f3fb"]
            strict = true
            "#,
        )
        .unwrap();

        assert!(config.strict);
        assert_eq!(
            config.allow,
            vec!['\u{200B}', '\u{1F3FB}'].into_iter().collect::<HashSet<char>>()
        );
    }

    #[test]
    fn test_invalid_allow_codepoint_rejected() {
        let err = toml::from_str::<MD084Config>(
            r#"
            allow = ["200B"]
            "#,
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("expected format U+XXXX"));
    }
}
