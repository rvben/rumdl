use crate::rule_config_serde::RuleConfig;
use crate::rules::heading_utils::HeadingStyle;
use serde::{Deserialize, Serialize};

/// Configuration for MD003 (Heading style)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD003Config {
    /// The heading style to enforce (default: "consistent")
    #[serde(
        default = "default_style",
        serialize_with = "serialize_style",
        deserialize_with = "deserialize_style"
    )]
    pub style: HeadingStyle,
}

fn default_style() -> HeadingStyle {
    HeadingStyle::Consistent
}

fn serialize_style<S>(style: &HeadingStyle, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&style.to_string())
}

fn deserialize_style<'de, D>(deserializer: D) -> Result<HeadingStyle, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    std::str::FromStr::from_str(&s).map_err(|_| serde::de::Error::custom(format!("Invalid heading style: {s}")))
}

impl Default for MD003Config {
    fn default() -> Self {
        Self { style: default_style() }
    }
}

impl RuleConfig for MD003Config {
    const RULE_NAME: &'static str = "MD003";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_accepts_kebab_case_variants() {
        let config: MD003Config = toml::from_str(r#"style = "atx-closed""#).unwrap();
        assert_eq!(config.style, HeadingStyle::AtxClosed);

        let config: MD003Config = toml::from_str(r#"style = "setext-with-atx""#).unwrap();
        assert_eq!(config.style, HeadingStyle::SetextWithAtx);

        let config: MD003Config = toml::from_str(r#"style = "setext-with-atx-closed""#).unwrap();
        assert_eq!(config.style, HeadingStyle::SetextWithAtxClosed);
    }

    #[test]
    fn test_style_is_case_insensitive() {
        let config: MD003Config = toml::from_str(r#"style = "ATX_CLOSED""#).unwrap();
        assert_eq!(config.style, HeadingStyle::AtxClosed);

        let config: MD003Config = toml::from_str(r#"style = "SETEXT-WITH-ATX""#).unwrap();
        assert_eq!(config.style, HeadingStyle::SetextWithAtx);
    }
}
