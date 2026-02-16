use crate::rule_config_serde::RuleConfig;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD055Config {
    #[serde(
        default = "default_style",
        serialize_with = "serialize_style",
        deserialize_with = "deserialize_style"
    )]
    pub style: String,
}

impl Default for MD055Config {
    fn default() -> Self {
        Self { style: default_style() }
    }
}

fn default_style() -> String {
    "consistent".to_string()
}

fn serialize_style<S>(style: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // Just serialize the string as-is
    serializer.serialize_str(style)
}

fn deserialize_style<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let normalized = s.trim().to_ascii_lowercase().replace('_', "-");

    let valid_styles = [
        "consistent",
        "leading-and-trailing",
        "no-leading-or-trailing",
        "leading-only",
        "trailing-only",
    ];

    if valid_styles.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(serde::de::Error::custom(format!(
            "Invalid table pipe style: {s}. Valid options: consistent, leading-and-trailing, \
             no-leading-or-trailing, leading-only, trailing-only"
        )))
    }
}

impl RuleConfig for MD055Config {
    const RULE_NAME: &'static str = "MD055";
}
