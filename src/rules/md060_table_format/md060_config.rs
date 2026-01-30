use crate::rule_config_serde::RuleConfig;
use crate::types::LineLength;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

/// Controls how cell text is aligned within padded columns.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ColumnAlign {
    /// Use alignment indicators from delimiter row (`:---`, `:---:`, `---:`)
    #[default]
    Auto,
    /// Force all columns to left-align text
    Left,
    /// Force all columns to center text
    Center,
    /// Force all columns to right-align text
    Right,
}

impl Serialize for ColumnAlign {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ColumnAlign::Auto => serializer.serialize_str("auto"),
            ColumnAlign::Left => serializer.serialize_str("left"),
            ColumnAlign::Center => serializer.serialize_str("center"),
            ColumnAlign::Right => serializer.serialize_str("right"),
        }
    }
}

impl<'de> Deserialize<'de> for ColumnAlign {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "auto" => Ok(ColumnAlign::Auto),
            "left" => Ok(ColumnAlign::Left),
            "center" => Ok(ColumnAlign::Center),
            "right" => Ok(ColumnAlign::Right),
            _ => Err(serde::de::Error::custom(format!(
                "Invalid column-align value: {s}. Valid options: auto, left, center, right"
            ))),
        }
    }
}

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

    /// Controls how cell text is aligned within the padded column width.
    ///
    /// - `auto` (default): Use alignment indicators from delimiter row (`:---`, `:---:`, `---:`)
    /// - `left`: Force all columns to left-align text
    /// - `center`: Force all columns to center text
    /// - `right`: Force all columns to right-align text
    ///
    /// Only applies when `style = "aligned"` or `style = "aligned-no-space"`.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [MD060]
    /// style = "aligned"
    /// column-align = "center"  # Center all cell text
    /// ```
    #[serde(default, rename = "column-align")]
    pub column_align: ColumnAlign,

    /// Controls alignment specifically for the header row.
    ///
    /// When set, overrides `column-align` for the header row only.
    /// If not set, falls back to `column-align`.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [MD060]
    /// style = "aligned"
    /// column-align-header = "center"  # Center header text
    /// column-align-body = "left"      # Left-align body text
    /// ```
    #[serde(default, rename = "column-align-header")]
    pub column_align_header: Option<ColumnAlign>,

    /// Controls alignment specifically for body rows (non-header, non-delimiter).
    ///
    /// When set, overrides `column-align` for body rows only.
    /// If not set, falls back to `column-align`.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [MD060]
    /// style = "aligned"
    /// column-align-header = "center"  # Center header text
    /// column-align-body = "left"      # Left-align body text
    /// ```
    #[serde(default, rename = "column-align-body")]
    pub column_align_body: Option<ColumnAlign>,

    /// When enabled, the last column in body rows is not padded to match the header width.
    ///
    /// This is useful for tables where the last column contains descriptions or other
    /// variable-length content. The header and delimiter rows remain fully aligned,
    /// but body rows can have shorter or longer last columns.
    ///
    /// Only applies when `style = "aligned"` or `style = "aligned-no-space"`.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [MD060]
    /// style = "aligned"
    /// loose-last-column = true
    /// ```
    ///
    /// Result:
    /// ```markdown
    /// | Name   | Status   | Description |
    /// |--------|----------|-------------|
    /// | Foo    | Enabled  | Short |
    /// | Bar    | Disabled | A much longer description that would waste space if padded |
    /// ```
    #[serde(default, rename = "loose-last-column")]
    pub loose_last_column: bool,
}

impl Default for MD060Config {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            style: default_style(),
            max_width: default_max_width(),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
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

    let valid_styles = ["aligned", "aligned-no-space", "compact", "tight", "any"];

    if valid_styles.contains(&s.as_str()) {
        Ok(s)
    } else {
        Err(serde::de::Error::custom(format!(
            "Invalid table format style: {s}. Valid options: aligned, aligned-no-space, compact, tight, any"
        )))
    }
}

impl RuleConfig for MD060Config {
    const RULE_NAME: &'static str = "MD060";
}
