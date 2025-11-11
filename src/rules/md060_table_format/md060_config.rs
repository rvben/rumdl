use crate::rule_config_serde::RuleConfig;
use crate::types::LineLength;
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

    /// Maximum table width before auto-switching to compact mode.
    ///
    /// - `0` (default): Inherit from MD013's `line-length` setting
    /// - Non-zero: Explicit max width threshold
    ///
    /// When a table's aligned width would exceed this limit, MD060 automatically
    /// uses compact formatting instead (minimal padding) to prevent excessively
    /// long lines. This matches the behavior of Prettier's table formatting.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [MD013]
    /// line-length = 100
    ///
    /// [MD060]
    /// style = "aligned"
    /// max-width = 0  # Uses MD013's line-length (100)
    /// ```
    ///
    /// ```toml
    /// [MD060]
    /// style = "aligned"
    /// max-width = 120  # Explicit threshold, independent of MD013
    /// ```
    #[serde(default = "default_max_width", rename = "max-width")]
    pub max_width: LineLength,
}

impl Default for MD060Config {
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
    "any".to_string()
}

fn default_max_width() -> LineLength {
    LineLength::from_const(0) // 0 = inherit from MD013
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
