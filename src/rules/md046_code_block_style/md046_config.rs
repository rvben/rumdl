use crate::rule_config_serde::RuleConfig;
use crate::rules::code_block_utils::CodeBlockStyle;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD046Config {
    #[serde(
        default = "default_style",
        serialize_with = "serialize_style",
        deserialize_with = "deserialize_style"
    )]
    pub style: CodeBlockStyle,
}

impl Default for MD046Config {
    fn default() -> Self {
        Self { style: default_style() }
    }
}

fn default_style() -> CodeBlockStyle {
    CodeBlockStyle::Consistent
}

fn deserialize_style<'de, D>(deserializer: D) -> Result<CodeBlockStyle, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.trim().to_ascii_lowercase().as_str() {
        "fenced" => Ok(CodeBlockStyle::Fenced),
        "indented" => Ok(CodeBlockStyle::Indented),
        "consistent" => Ok(CodeBlockStyle::Consistent),
        _ => Err(serde::de::Error::custom(format!(
            "Invalid code block style: {s}. Valid options: fenced, indented, consistent"
        ))),
    }
}

fn serialize_style<S>(style: &CodeBlockStyle, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&style.to_string())
}

impl RuleConfig for MD046Config {
    const RULE_NAME: &'static str = "MD046";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_is_case_insensitive() {
        let config: MD046Config = toml::from_str(r#"style = "Fenced""#).unwrap();
        assert_eq!(config.style, CodeBlockStyle::Fenced);

        let config: MD046Config = toml::from_str(r#"style = "INDENTED""#).unwrap();
        assert_eq!(config.style, CodeBlockStyle::Indented);

        let config: MD046Config = toml::from_str(r#"style = "Consistent""#).unwrap();
        assert_eq!(config.style, CodeBlockStyle::Consistent);
    }
}
