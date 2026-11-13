use crate::rule_config_serde::RuleConfig;
use crate::rules::emphasis_style::EmphasisStyle;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_is_case_insensitive() {
        let config: MD049Config = toml::from_str(r#"style = "Asterisk""#).unwrap();
        assert_eq!(config.style, EmphasisStyle::Asterisk);

        let config: MD049Config = toml::from_str(r#"style = "UNDERSCORE""#).unwrap();
        assert_eq!(config.style, EmphasisStyle::Underscore);
    }
}

#[cfg(test)]
mod schema_spelling_tests {
    use super::*;

    /// Every spelling the schema prints after "One of" has to deserialize.
    ///
    /// The style enum is parsed by this module, not by a serde derive, so the schema
    /// spells it from the #[schemars(rename)] attributes and can drift from what
    /// deserialize_style accepts. Parsing the config struct is the check that can
    /// fail: loading a rumdl.toml cannot, because the loader only warns about a value
    /// it cannot parse and keeps the rule's default.
    #[test]
    fn schema_publishes_spellings_that_deserialize() {
        let schema = crate::config::schema_gen::config_schema_json();
        let description = schema["$defs"]["MD049"]["properties"]["style"]["description"]
            .as_str()
            .expect("MD049.style describes its spellings");
        let (_, spellings) = description
            .rsplit_once("One of ")
            .expect("MD049.style names its spellings after \"One of\"");

        for spelling in spellings.trim_end_matches('.').split(", ") {
            let spelling = spelling.trim().trim_matches('\u{60}');
            let section = format!("style = \"{spelling}\"\n");
            let parsed = toml::from_str::<MD049Config>(&section);
            assert!(
                parsed.is_ok(),
                "the schema offers style = \"{spelling}\" and MD049Config rejects it: {:?}",
                parsed.err()
            );
        }
    }
}
