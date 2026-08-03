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
    /// Fixed continuation indent relative to the list marker, e.g. `4` for
    /// MkDocs-style documents. When set, overrides the content-column-derived
    /// requirement (`content_col`, or `max(content_col, 4)` under the MkDocs
    /// flavor). `None` keeps the default content-column behavior.
    #[serde(default)]
    pub indent: Option<usize>,
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
}
