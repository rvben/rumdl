use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub(super) struct MD084Config {
    /// When enabled, report any invisible character anywhere in the document.
    #[serde(default)]
    pub(super) strict: bool,

    /// Codepoint allow-list in `U+XXXX` / `U+XXXXX` / `U+XXXXXX` format.
    ///
    /// Any matched invisible character with a codepoint present in this list
    /// is ignored.
    #[serde(default, deserialize_with = "deserialize_allow")]
    pub(super) allow: Vec<String>,
}

fn deserialize_allow<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::<String>::deserialize(deserializer)?;
    let mut out = Vec::with_capacity(values.len());

    for raw in values {
        out.push(normalize_codepoint_token(&raw).map_err(serde::de::Error::custom)?);
    }

    Ok(out)
}

fn normalize_codepoint_token(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    let Some(hex) = trimmed.strip_prefix("U+").or_else(|| trimmed.strip_prefix("u+")) else {
        return Err(format!("Invalid codepoint '{trimmed}': expected format U+XXXX"));
    };

    if !(4..=6).contains(&hex.len()) {
        return Err(format!("Invalid codepoint '{trimmed}': expected 4 to 6 hex digits"));
    }

    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("Invalid codepoint '{trimmed}': contains non-hex characters"));
    }

    let value = u32::from_str_radix(hex, 16).map_err(|_| format!("Invalid codepoint '{trimmed}': parse failed"))?;

    if value > 0x10FFFF || (0xD800..=0xDFFF).contains(&value) {
        return Err(format!("Invalid codepoint '{trimmed}': out of Unicode range"));
    }

    Ok(format!("U+{value:04X}"))
}

impl RuleConfig for MD084Config {
    const RULE_NAME: &'static str = "MD084";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = MD084Config::default();
        assert!(!config.strict);
        assert!(config.allow.is_empty());
    }

    #[test]
    fn test_allow_deserialize_and_normalize() {
        let config: MD084Config = toml::from_str(
            r#"
            allow = ["u+200b", "U+1f3fb"]
            strict = true
            "#,
        )
        .unwrap();

        assert!(config.strict);
        assert_eq!(config.allow, vec!["U+200B", "U+1F3FB"]);
    }

    #[test]
    fn test_invalid_allow_codepoint_rejected() {
        let err = toml::from_str::<MD084Config>(
            r#"
            allow = ["200B"]
            "#,
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("expected format U+XXXX"));
    }
}
