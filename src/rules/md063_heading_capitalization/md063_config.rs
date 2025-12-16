use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Capitalization style for headings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HeadingCapStyle {
    /// Title Case - capitalize major words (default)
    #[default]
    TitleCase,
    /// Sentence case - only first word capitalized
    SentenceCase,
    /// ALL CAPS - all letters uppercase
    AllCaps,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD063Config {
    /// Whether this rule is enabled (default: false - opt-in rule)
    #[serde(default)]
    pub enabled: bool,

    /// Capitalization style to enforce
    #[serde(default)]
    pub style: HeadingCapStyle,

    /// Words that should always be lowercase in title case
    /// (articles, prepositions, conjunctions)
    #[serde(
        default = "default_lowercase_words",
        rename = "lowercase-words",
        alias = "lowercase_words"
    )]
    pub lowercase_words: Vec<String>,

    /// Words to preserve exactly as specified (brand names like iPhone, macOS)
    #[serde(default, rename = "ignore-words", alias = "ignore_words")]
    pub ignore_words: Vec<String>,

    /// Preserve existing mixed-case words even if not in ignore_words
    #[serde(
        default = "default_preserve_cased_words",
        rename = "preserve-cased-words",
        alias = "preserve_cased_words"
    )]
    pub preserve_cased_words: bool,

    /// Minimum heading level to check (1-6)
    #[serde(default = "default_min_level", rename = "min-level", alias = "min_level")]
    pub min_level: u8,

    /// Maximum heading level to check (1-6)
    #[serde(default = "default_max_level", rename = "max-level", alias = "max_level")]
    pub max_level: u8,
}

fn default_lowercase_words() -> Vec<String> {
    // Standard title case lowercase words (Chicago Manual of Style inspired)
    vec![
        "a", "an", "and", "as", "at", "but", "by", "for", "from", "in", "into", "nor", "of", "off", "on", "or", "per",
        "so", "the", "to", "up", "via", "with", "yet",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn default_preserve_cased_words() -> bool {
    true
}

fn default_min_level() -> u8 {
    1
}

fn default_max_level() -> u8 {
    6
}

impl Default for MD063Config {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default - opt-in rule
            style: HeadingCapStyle::default(),
            lowercase_words: default_lowercase_words(),
            ignore_words: Vec::new(),
            preserve_cased_words: default_preserve_cased_words(),
            min_level: default_min_level(),
            max_level: default_max_level(),
        }
    }
}

impl RuleConfig for MD063Config {
    const RULE_NAME: &'static str = "MD063";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = MD063Config::default();
        assert!(!config.enabled); // Disabled by default
        assert_eq!(config.style, HeadingCapStyle::TitleCase);
        assert!(!config.lowercase_words.is_empty());
        assert!(config.lowercase_words.contains(&"the".to_string()));
        assert!(config.ignore_words.is_empty());
        assert!(config.preserve_cased_words);
        assert_eq!(config.min_level, 1);
        assert_eq!(config.max_level, 6);
    }

    #[test]
    fn test_kebab_case_config() {
        let toml_str = r#"
            style = "title_case"
            lowercase-words = ["a", "an", "the"]
            ignore-words = ["iPhone", "macOS"]
            preserve-cased-words = true
            min-level = 1
            max-level = 3
        "#;
        let config: MD063Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.style, HeadingCapStyle::TitleCase);
        assert_eq!(config.lowercase_words, vec!["a", "an", "the"]);
        assert_eq!(config.ignore_words, vec!["iPhone", "macOS"]);
        assert!(config.preserve_cased_words);
        assert_eq!(config.min_level, 1);
        assert_eq!(config.max_level, 3);
    }

    #[test]
    fn test_snake_case_backwards_compatibility() {
        let toml_str = r#"
            style = "sentence_case"
            lowercase_words = ["a", "the"]
            ignore_words = ["GitHub"]
            preserve_cased_words = false
            min_level = 2
            max_level = 4
        "#;
        let config: MD063Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.style, HeadingCapStyle::SentenceCase);
        assert_eq!(config.lowercase_words, vec!["a", "the"]);
        assert_eq!(config.ignore_words, vec!["GitHub"]);
        assert!(!config.preserve_cased_words);
        assert_eq!(config.min_level, 2);
        assert_eq!(config.max_level, 4);
    }

    #[test]
    fn test_all_caps_style() {
        let toml_str = r#"
            style = "all_caps"
        "#;
        let config: MD063Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.style, HeadingCapStyle::AllCaps);
    }

    #[test]
    fn test_style_serialization() {
        assert_eq!(
            serde_json::to_string(&HeadingCapStyle::TitleCase).unwrap(),
            "\"title_case\""
        );
        assert_eq!(
            serde_json::to_string(&HeadingCapStyle::SentenceCase).unwrap(),
            "\"sentence_case\""
        );
        assert_eq!(
            serde_json::to_string(&HeadingCapStyle::AllCaps).unwrap(),
            "\"all_caps\""
        );
    }
}
