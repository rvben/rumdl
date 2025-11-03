use crate::rule_config_serde::RuleConfig;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD060Config {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(
        default = "default_style",
        serialize_with = "serialize_style",
        deserialize_with = "deserialize_style"
    )]
    pub style: String,
}

impl Default for MD060Config {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            style: default_style(),
        }
    }
}

fn default_enabled() -> bool {
    false
}

fn default_style() -> String {
    "any".to_string()
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

    let valid_styles = ["aligned", "compact", "tight", "any"];

    if valid_styles.contains(&s.as_str()) {
        Ok(s)
    } else {
        Err(serde::de::Error::custom(format!(
            "Invalid table format style: {s}. Valid options: aligned, compact, tight, any"
        )))
    }
}

impl RuleConfig for MD060Config {
    const RULE_NAME: &'static str = "MD060";
}
