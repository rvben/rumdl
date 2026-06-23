use super::ContinuationStyle;
use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Configuration for MD077 (List continuation content indentation)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct MD077Config {
    /// How strictly continuation-line indentation is enforced.
    #[serde(default)]
    pub style: ContinuationStyle,
}

impl RuleConfig for MD077Config {
    const RULE_NAME: &'static str = "MD077";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_any() {
        assert_eq!(MD077Config::default().style, ContinuationStyle::Any);
        let parsed: MD077Config = toml::from_str("").unwrap();
        assert_eq!(parsed.style, ContinuationStyle::Any);
    }

    #[test]
    fn parses_aligned() {
        let parsed: MD077Config = toml::from_str(r#"style = "aligned""#).unwrap();
        assert_eq!(parsed.style, ContinuationStyle::Aligned);
    }

    #[test]
    fn parses_any() {
        let parsed: MD077Config = toml::from_str(r#"style = "any""#).unwrap();
        assert_eq!(parsed.style, ContinuationStyle::Any);
    }
}
