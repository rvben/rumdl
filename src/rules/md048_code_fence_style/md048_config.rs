use crate::rule_config_serde::RuleConfig;
use crate::rules::code_fence_utils::CodeFenceStyle;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD048Config {
    #[serde(
        default = "default_style",
        serialize_with = "serialize_style",
        deserialize_with = "deserialize_style"
    )]
    pub style: CodeFenceStyle,
}

impl Default for MD048Config {
    fn default() -> Self {
        Self { style: default_style() }
    }
}

fn default_style() -> CodeFenceStyle {
    CodeFenceStyle::Consistent
}

fn serialize_style<S>(style: &CodeFenceStyle, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&style.to_string())
}

fn deserialize_style<'de, D>(deserializer: D) -> Result<CodeFenceStyle, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.as_str() {
        "backtick" => Ok(CodeFenceStyle::Backtick),
        "tilde" => Ok(CodeFenceStyle::Tilde),
        "consistent" => Ok(CodeFenceStyle::Consistent),
        _ => Err(serde::de::Error::custom(format!("Invalid code fence style: {s}"))),
    }
}

impl RuleConfig for MD048Config {
    const RULE_NAME: &'static str = "MD048";
}
