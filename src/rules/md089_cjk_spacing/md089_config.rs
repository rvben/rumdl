//! Configuration for MD089 (CJK spacing).

use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Symbols that lead a Latin run and take a space after a CJK letter (`價格$5`).
fn default_symbols_after_cjk() -> String {
    "-+'\"([¥$".to_string()
}

/// Symbols that trail a Latin run and take a space before a CJK letter (`90°的`).
fn default_symbols_before_cjk() -> String {
    "-+;:'\"°%$)]".to_string()
}

/// Configuration for MD089. Each set is written as one string of characters;
/// whitespace in the string is ignored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) struct MD089Config {
    /// Symbols that lead a Latin run and take a space after a CJK letter.
    #[serde(default = "default_symbols_after_cjk")]
    pub(super) symbols_after_cjk: String,
    /// Symbols that trail a Latin run and take a space before a CJK letter.
    #[serde(default = "default_symbols_before_cjk")]
    pub(super) symbols_before_cjk: String,
}

impl Default for MD089Config {
    fn default() -> Self {
        Self {
            symbols_after_cjk: default_symbols_after_cjk(),
            symbols_before_cjk: default_symbols_before_cjk(),
        }
    }
}

impl RuleConfig for MD089Config {
    const RULE_NAME: &'static str = "MD089";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_mirror_the_reference_implementation() {
        let config = MD089Config::default();
        assert_eq!(config.symbols_after_cjk, "-+'\"([¥$");
        assert_eq!(config.symbols_before_cjk, "-+;:'\"°%$)]");
    }

    #[test]
    fn kebab_case_keys_deserialize() {
        let config: MD089Config = toml::from_str("symbols-after-cjk = \"$\"\nsymbols-before-cjk = \"\"\n").unwrap();
        assert_eq!(config.symbols_after_cjk, "$");
        assert_eq!(config.symbols_before_cjk, "");
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let config: MD089Config = toml::from_str("").unwrap();
        assert_eq!(config, MD089Config::default());
    }

    #[test]
    fn rule_name_is_md089() {
        assert_eq!(MD089Config::RULE_NAME, "MD089");
    }
}
