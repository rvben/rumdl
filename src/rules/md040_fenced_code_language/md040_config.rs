use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Style for language label normalization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LanguageStyle {
    /// No normalization, only check for missing language (default)
    #[default]
    Disabled,
    /// Normalize to most prevalent alias per canonical language
    Consistent,
}

/// Action to take for unknown language labels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnknownLanguageAction {
    /// Silently ignore unknown languages (default)
    #[default]
    Ignore,
    /// Emit a warning for unknown languages
    Warn,
    /// Treat unknown languages as errors
    Error,
}

/// Configuration for MD040 (Fenced code language)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct MD040Config {
    /// Language normalization style
    #[serde(default)]
    pub style: LanguageStyle,

    /// Override preferred label for specific languages
    /// Keys: Linguist canonical names (case-insensitive), Values: preferred alias
    #[serde(default, alias = "preferred_aliases")]
    pub preferred_aliases: HashMap<String, String>,

    /// Only allow these languages (empty = allow all)
    /// Uses Linguist canonical language names (case-insensitive)
    #[serde(default, alias = "allowed_languages")]
    pub allowed_languages: Vec<String>,

    /// Block these languages (ignored if allowed_languages is non-empty)
    /// Uses Linguist canonical language names (case-insensitive)
    #[serde(default, alias = "disallowed_languages")]
    pub disallowed_languages: Vec<String>,

    /// Action for unknown language labels not in Linguist
    #[serde(default, alias = "unknown_language_action")]
    pub unknown_language_action: UnknownLanguageAction,
}

impl RuleConfig for MD040Config {
    const RULE_NAME: &'static str = "MD040";
}
