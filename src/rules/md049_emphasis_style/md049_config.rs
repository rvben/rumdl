use crate::rule_config_serde::RuleConfig;
use crate::rules::emphasis_style::EmphasisStyle;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD049Config {
    #[serde(
        default = "default_style",
        serialize_with = "serialize_style",
        deserialize_with = "deserialize_style"
    )]
    pub style: EmphasisStyle,
}

impl Default for MD049Config {
    fn default() -> Self {
        Self { style: default_style() }
    }
}

fn default_style() -> EmphasisStyle {
    EmphasisStyle::Consistent
}

fn serialize_style<S>(style: &EmphasisStyle, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&style.to_string())
}

fn deserialize_style<'de, D>(deserializer: D) -> Result<EmphasisStyle, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(EmphasisStyle::from(s.as_str()))
}

impl RuleConfig for MD049Config {
    const RULE_NAME: &'static str = "MD049";
}
