use super::ContinuationStyle;
use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Deserializer, Serialize};

/// Configuration for MD077 (List continuation content indentation)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct MD077Config {
    /// How strictly continuation-line indentation is enforced.
    #[serde(default)]
    pub style: ContinuationStyle,
    /// Fixed continuation indent relative to the list marker, e.g. `4` for
    /// MkDocs-style documents. When set, it replaces the content-column-derived
    /// requirement, except that under a flavor requiring strict list indentation
    /// it may only raise the 4-space minimum, never lower it. `None` keeps the
    /// default content-column behavior.
    #[serde(default, deserialize_with = "deserialize_indent")]
    pub indent: Option<usize>,
}

/// Rejects `indent = 0`. A continuation at the marker column is not continuation
/// content at all: it ends the list item, so requiring it would make `fix`
/// dissolve the list it is meant to keep consistent.
fn deserialize_indent<'de, D>(deserializer: D) -> Result<Option<usize>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<usize>::deserialize(deserializer)? {
        Some(0) => Err(serde::de::Error::custom(
            "Invalid indent 0: continuation content at the list marker's own column \
             leaves the list item. Use 1 or greater.",
        )),
        other => Ok(other),
    }
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
        assert_eq!(MD077Config::default().indent, None);
        let parsed: MD077Config = toml::from_str("").unwrap();
        assert_eq!(parsed.style, ContinuationStyle::Any);
        assert_eq!(parsed.indent, None);
    }

    #[test]
    fn parses_aligned() {
        let parsed: MD077Config = toml::from_str(r#"style = "aligned""#).unwrap();
        assert_eq!(parsed.style, ContinuationStyle::Aligned);
        assert_eq!(parsed.indent, None);
    }

    #[test]
    fn parses_any() {
        let parsed: MD077Config = toml::from_str(r#"style = "any""#).unwrap();
        assert_eq!(parsed.style, ContinuationStyle::Any);
    }

    #[test]
    fn parses_fixed_indent() {
        let parsed: MD077Config = toml::from_str("indent = 4").unwrap();
        assert_eq!(parsed.indent, Some(4));
        assert_eq!(parsed.style, ContinuationStyle::Any);
    }

    #[test]
    fn parses_style_and_indent_together() {
        let parsed: MD077Config = toml::from_str("style = \"aligned\"\nindent = 4").unwrap();
        assert_eq!(parsed.style, ContinuationStyle::Aligned);
        assert_eq!(parsed.indent, Some(4));
    }

    #[test]
    fn rejects_zero_indent() {
        let err = toml::from_str::<MD077Config>("indent = 0").unwrap_err().to_string();
        assert!(
            err.contains("leaves the list item"),
            "zero must be rejected with an explanation, got: {err}"
        );
        // Control: the neighbouring value parses, so the rejection is about 0
        // and not about the key being unreadable.
        assert_eq!(toml::from_str::<MD077Config>("indent = 1").unwrap().indent, Some(1));
    }

    #[test]
    fn rejects_negative_indent() {
        assert!(toml::from_str::<MD077Config>("indent = -1").is_err());
    }
}
