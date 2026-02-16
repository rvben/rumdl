use crate::rule_config_serde::RuleConfig;
use crate::rules::strong_style::StrongStyle;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD050Config {
    #[serde(
        default = "default_style",
        serialize_with = "serialize_style",
        deserialize_with = "deserialize_style"
    )]
    pub style: StrongStyle,
}

impl Default for MD050Config {
    fn default() -> Self {
        Self { style: default_style() }
    }
}

fn default_style() -> StrongStyle {
    StrongStyle::Consistent
}

fn serialize_style<S>(style: &StrongStyle, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&style.to_string())
}

fn deserialize_style<'de, D>(deserializer: D) -> Result<StrongStyle, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.trim().to_ascii_lowercase().as_str() {
        "asterisk" => Ok(StrongStyle::Asterisk),
        "underscore" => Ok(StrongStyle::Underscore),
        "consistent" => Ok(StrongStyle::Consistent),
        _ => Err(serde::de::Error::custom(format!(
            "Invalid strong style: {s}. Valid options: asterisk, underscore, consistent"
        ))),
    }
}

impl RuleConfig for MD050Config {
    const RULE_NAME: &'static str = "MD050";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_is_case_insensitive() {
        let config: MD050Config = toml::from_str(r#"style = "Asterisk""#).unwrap();
        assert_eq!(config.style, StrongStyle::Asterisk);

        let config: MD050Config = toml::from_str(r#"style = "UNDERSCORE""#).unwrap();
        assert_eq!(config.style, StrongStyle::Underscore);
    }
}
