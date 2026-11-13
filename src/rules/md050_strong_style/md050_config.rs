use crate::rule_config_serde::RuleConfig;
use crate::rules::strong_style::StrongStyle;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct MD050Config {
    #[serde(
        default = "default_style",
        serialize_with = "serialize_style",
        deserialize_with = "deserialize_style"
    )]
    pub style: StrongStyle,
}

impl Default for MD050Config {
    fn default() -> Self {
        Self { style: default_style() }
    }
}

fn default_style() -> StrongStyle {
    StrongStyle::Consistent
}

fn serialize_style<S>(style: &StrongStyle, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&style.to_string())
}

fn deserialize_style<'de, D>(deserializer: D) -> Result<StrongStyle, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.trim().to_ascii_lowercase().as_str() {
        "asterisk" => Ok(StrongStyle::Asterisk),
        "underscore" => Ok(StrongStyle::Underscore),
        "consistent" => Ok(StrongStyle::Consistent),
        _ => Err(serde::de::Error::custom(format!(
            "Invalid strong style: {s}. Valid options: asterisk, underscore, consistent"
        ))),
    }
}

impl RuleConfig for MD050Config {
    const RULE_NAME: &'static str = "MD050";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_is_case_insensitive() {
        let config: MD050Config = toml::from_str(r#"style = "Asterisk""#).unwrap();
        assert_eq!(config.style, StrongStyle::Asterisk);

        let config: MD050Config = toml::from_str(r#"style = "UNDERSCORE""#).unwrap();
        assert_eq!(config.style, StrongStyle::Underscore);
    }
}

#[cfg(test)]
mod schema_spelling_tests {
    use super::*;

    /// Every spelling the schema prints after "One of" has to deserialize.
    ///
    /// The style enum is parsed by this module rather than by a `serde` derive, so the
    /// schema spells it from the `#[schemars(rename)]` attributes and can drift from
    /// what `deserialize_style` accepts. Parsing the config struct is the check that
    /// can fail: loading a `rumdl.toml` cannot, because the loader only warns about a
    /// value it cannot parse and keeps the rule's default.
    #[test]
    fn schema_publishes_spellings_that_deserialize() {
        for spelling in crate::config::schema_gen::published_spellings("MD050", "style") {
            let section = format!("style = \"{spelling}\"\n");
            let parsed = toml::from_str::<MD050Config>(&section);
            assert!(
                parsed.is_ok(),
                "the schema offers MD050.style = \"{spelling}\" and `MD050Config` rejects it: {:?}",
                parsed.err()
            );
        }
    }
}
