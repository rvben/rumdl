use super::UnorderedListStyle;
use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Configuration for MD004 (Unordered list style)
#[derive(Debug, Clone, PartialEq)]
pub struct MD004Config {
    /// The style for unordered list markers
    pub style: UnorderedListStyle,
    /// Number of spaces after marker (not currently exposed in config)
    pub after_marker: usize,
}

// Manual Serialize implementation for MD004Config
impl Serialize for MD004Config {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("MD004Config", 1)?;
        let style_str = match self.style {
            UnorderedListStyle::Asterisk => "asterisk",
            UnorderedListStyle::Dash => "dash",
            UnorderedListStyle::Plus => "plus",
            UnorderedListStyle::Consistent => "consistent",
            UnorderedListStyle::Sublist => "sublist",
        };
        state.serialize_field("style", style_str)?;
        state.end()
    }
}

// Manual Deserialize implementation for MD004Config
impl<'de> Deserialize<'de> for MD004Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct Helper {
            #[serde(default = "default_style_str")]
            style: String,
        }

        let helper = Helper::deserialize(deserializer)?;
        let style = match helper.style.as_str() {
            "asterisk" => UnorderedListStyle::Asterisk,
            "dash" => UnorderedListStyle::Dash,
            "plus" => UnorderedListStyle::Plus,
            "consistent" => UnorderedListStyle::Consistent,
            "sublist" => UnorderedListStyle::Sublist,
            _ => UnorderedListStyle::Consistent,
        };

        Ok(MD004Config { style, after_marker: 1 })
    }
}

fn default_style_str() -> String {
    "consistent".to_string()
}

impl Default for MD004Config {
    fn default() -> Self {
        Self {
            style: UnorderedListStyle::default(),
            after_marker: 1,
        }
    }
}

impl RuleConfig for MD004Config {
    const RULE_NAME: &'static str = "MD004";
}
