use crate::rule_config_serde::RuleConfig;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct MD055Config {
    /// Pipe style every row must use: `consistent` (the most common style in each
    /// table), `leading-and-trailing`, `no-leading-or-trailing`, `trailing-only` or
    /// `leading-only`. Kebab-case and snake_case are both read.
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
    // Normalize both kebab-case and snake_case to snake_case, which is what
    // the rule's match arms and determine_pipe_style() use internally.
    let normalized = s.trim().to_ascii_lowercase().replace('-', "_");

    let valid_styles = [
        "consistent",
        "leading_and_trailing",
        "no_leading_or_trailing",
        "leading_only",
        "trailing_only",
    ];

    if valid_styles.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(serde::de::Error::custom(format!(
            "Invalid table pipe style: {s}. Valid options: consistent, \
             leading-and-trailing, no-leading-or-trailing, leading-only, trailing-only \
             (hyphens and underscores are interchangeable; value is case-insensitive)"
        )))
    }
}

impl RuleConfig for MD055Config {
    const RULE_NAME: &'static str = "MD055";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deserialize(toml_value: &str) -> Result<MD055Config, toml::de::Error> {
        toml::from_str(&format!("style = \"{toml_value}\""))
    }

    #[test]
    fn test_kebab_case_styles_are_accepted() {
        // Every documented kebab-case variant must deserialize without error
        assert!(deserialize("consistent").is_ok());
        assert!(deserialize("leading-and-trailing").is_ok());
        assert!(deserialize("no-leading-or-trailing").is_ok());
        assert!(deserialize("leading-only").is_ok());
        assert!(deserialize("trailing-only").is_ok());
    }

    #[test]
    fn test_snake_case_styles_are_accepted() {
        assert!(deserialize("consistent").is_ok());
        assert!(deserialize("leading_and_trailing").is_ok());
        assert!(deserialize("no_leading_or_trailing").is_ok());
        assert!(deserialize("leading_only").is_ok());
        assert!(deserialize("trailing_only").is_ok());
    }

    #[test]
    fn test_stored_value_is_always_snake_case() {
        // Every input form must produce the specific snake_case internal value.
        // Verifying the stored form (not just equality between two inputs) ensures
        // the rule's match arms always see the expected value.
        let cases: &[(&str, &str)] = &[
            ("consistent", "consistent"),
            ("leading-and-trailing", "leading_and_trailing"),
            ("LEADING-AND-TRAILING", "leading_and_trailing"),
            ("no-leading-or-trailing", "no_leading_or_trailing"),
            ("NO_LEADING_OR_TRAILING", "no_leading_or_trailing"),
            ("leading-only", "leading_only"),
            ("LEADING_ONLY", "leading_only"),
            ("trailing-only", "trailing_only"),
            ("TRAILING_ONLY", "trailing_only"),
        ];
        for (input, expected) in cases {
            let config = deserialize(input).unwrap();
            assert_eq!(
                config.style, *expected,
                "'{input}' must store as '{expected}', got '{}'",
                config.style
            );
        }
    }

    #[test]
    fn test_kebab_and_snake_case_normalize_to_same_internal_value() {
        // Both spelling variants must produce the same stored value so that
        // the rule's match arms (which use snake_case) see a consistent value.
        let pairs = [
            ("leading-and-trailing", "leading_and_trailing"),
            ("no-leading-or-trailing", "no_leading_or_trailing"),
            ("leading-only", "leading_only"),
            ("trailing-only", "trailing_only"),
        ];
        for (kebab, snake) in pairs {
            let from_kebab = deserialize(kebab).unwrap();
            let from_snake = deserialize(snake).unwrap();
            assert_eq!(
                from_kebab.style, from_snake.style,
                "kebab '{kebab}' and snake '{snake}' must store the same value"
            );
        }
    }

    #[test]
    fn test_uppercase_styles_are_accepted_and_lowercased() {
        // Case-insensitive: all-caps variants must work and normalize to lowercase snake_case.
        let cases: &[(&str, &str)] = &[
            ("CONSISTENT", "consistent"),
            ("LEADING-AND-TRAILING", "leading_and_trailing"),
            ("NO-LEADING-OR-TRAILING", "no_leading_or_trailing"),
            ("LEADING_AND_TRAILING", "leading_and_trailing"),
            ("TRAILING_ONLY", "trailing_only"),
        ];
        for (input, expected) in cases {
            let config = deserialize(input).unwrap();
            assert_eq!(
                config.style, *expected,
                "'{input}' must normalize to lowercase '{expected}'"
            );
        }
    }

    #[test]
    fn test_default_style_is_consistent() {
        // When no style key is present in the config, the default must be "consistent".
        let config: MD055Config = toml::from_str("").unwrap();
        assert_eq!(config.style, "consistent");

        let default = MD055Config::default();
        assert_eq!(default.style, "consistent");
    }

    #[test]
    fn test_serde_roundtrip_preserves_snake_case() {
        // A config deserialized from TOML must re-serialize to the snake_case form
        // so that `rumdl config get` shows a clean, internally consistent value.
        let cases = [
            ("leading-and-trailing", "leading_and_trailing"),
            ("no-leading-or-trailing", "no_leading_or_trailing"),
            ("leading-only", "leading_only"),
            ("trailing-only", "trailing_only"),
        ];
        for (input, expected_snake) in cases {
            let config = deserialize(input).unwrap();
            let serialized = toml::to_string(&config).unwrap();
            // The serialized TOML must contain the snake_case form, never kebab-case.
            assert!(
                serialized.contains(expected_snake),
                "'{input}' → serialized TOML should contain '{expected_snake}', got: {serialized:?}"
            );
            assert!(
                !serialized.contains(input),
                "'{input}' → serialized TOML must not contain the original kebab form, got: {serialized:?}"
            );
        }
    }

    #[test]
    fn test_invalid_style_is_rejected() {
        assert!(deserialize("both").is_err());
        assert!(deserialize("none").is_err());
        assert!(deserialize("leading-or-trailing").is_err());
    }
}
