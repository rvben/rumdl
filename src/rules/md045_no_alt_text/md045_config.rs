use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// MD045 is diagnostic-only and has no configurable options.
/// The struct accepts (and ignores) the legacy `placeholder-text` field
/// for backward compatibility with existing config files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD045Config {
    #[serde(
        default,
        rename = "placeholder-text",
        alias = "placeholder_text",
        skip_serializing
    )]
    _placeholder_text: Option<String>,
}

impl Default for MD045Config {
    fn default() -> Self {
        Self {
            _placeholder_text: None,
        }
    }
}

impl RuleConfig for MD045Config {
    const RULE_NAME: &'static str = "MD045";
}
