use crate::rule_config_serde::RuleConfig;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use std::fmt;

/// The style for code blocks (MD046)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum CodeBlockStyle {
    /// Consistent with the first code block style found
    #[default]
    Consistent,
    /// Indented code blocks (4 spaces)
    Indented,
    /// Fenced code blocks (``` or ~~~)
    Fenced,
}

impl fmt::Display for CodeBlockStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeBlockStyle::Fenced => write!(f, "fenced"),
            CodeBlockStyle::Indented => write!(f, "indented"),
            CodeBlockStyle::Consistent => write!(f, "consistent"),
        }
    }
}

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
