use crate::rule_config_serde::RuleConfig;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD059Config {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(
        default = "default_style",
        serialize_with = "serialize_style",
        deserialize_with = "deserialize_style"
    )]
    pub style: String,

    #[serde(default = "default_max_width")]
    pub max_width: Option<usize>,
}

impl Default for MD059Config {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            style: default_style(),
            max_width: default_max_width(),
        }
    }
}

fn default_enabled() -> bool {
    false
}

fn default_style() -> String {
    "aligned".to_string()
}

fn default_max_width() -> Option<usize> {
    Some(120)
}

fn serialize_style<S>(style: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(style)
}

fn deserialize_style<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;

    let valid_styles = ["aligned", "compact", "none"];

    if valid_styles.contains(&s.as_str()) {
        Ok(s)
    } else {
        Err(serde::de::Error::custom(format!(
            "Invalid table format style: {s}. Valid options: aligned, compact, none"
        )))
    }
}

impl RuleConfig for MD059Config {
    const RULE_NAME: &'static str = "MD059";
}
