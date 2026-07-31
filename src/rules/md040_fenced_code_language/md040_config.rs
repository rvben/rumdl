use crate::linguist_data::{get_aliases, is_valid_alias, resolve_canonical};
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

impl MD040Config {
    /// Why `preferred-aliases` cannot make `alias` stand for `language`, when it cannot.
    ///
    /// This is the single definition of a usable entry: it both phrases the
    /// configuration error and decides whether the entry is honored, so a label
    /// the user is told is invalid is never one that gets suggested or written.
    pub fn preferred_alias_problem(&self, language: &str, alias: &str) -> Option<String> {
        let Some(canonical) = resolve_canonical(language) else {
            return Some(format!(
                "Unknown language '{language}' in preferred-aliases. Use GitHub Linguist canonical names."
            ));
        };
        if is_valid_alias(canonical, alias) {
            return None;
        }
        let examples = get_aliases(canonical).map_or_else(String::new, |valid_aliases| {
            let valid_str = valid_aliases
                .iter()
                .take(5)
                .map(|s| format!("'{s}'"))
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if valid_aliases.len() > 5 { ", ..." } else { "" };
            format!(" Valid aliases include: {valid_str}{suffix}")
        });
        Some(format!("Invalid alias '{alias}' for language '{canonical}'.{examples}"))
    }

    /// The label `preferred-aliases` sets for `language`, when it sets a usable one.
    pub fn preferred_label(&self, language: &str) -> Option<&str> {
        self.preferred_aliases
            .iter()
            .find(|(configured, _)| configured.eq_ignore_ascii_case(language))
            .map(|(_, alias)| alias.as_str())
            .filter(|alias| self.preferred_alias_problem(language, alias).is_none())
    }
}

impl RuleConfig for MD040Config {
    const RULE_NAME: &'static str = "MD040";
}
