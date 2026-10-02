use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Capitalization style for headings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeadingCapStyle {
    /// Title Case - capitalize major words (default)
    #[default]
    TitleCase,
    /// Sentence case - only first word capitalized
    SentenceCase,
    /// ALL CAPS - all letters uppercase
    AllCaps,
    /// Consistent - detect the first heading's style and enforce it throughout
    Consistent,
}

impl Serialize for HeadingCapStyle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            HeadingCapStyle::TitleCase => serializer.serialize_str("title-case"),
            HeadingCapStyle::SentenceCase => serializer.serialize_str("sentence-case"),
            HeadingCapStyle::AllCaps => serializer.serialize_str("all-caps"),
            HeadingCapStyle::Consistent => serializer.serialize_str("consistent"),
        }
    }
}

impl<'de> Deserialize<'de> for HeadingCapStyle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let normalized = s.trim().to_ascii_lowercase().replace('-', "_");
        match normalized.as_str() {
            "title_case" => Ok(HeadingCapStyle::TitleCase),
            "sentence_case" => Ok(HeadingCapStyle::SentenceCase),
            "all_caps" => Ok(HeadingCapStyle::AllCaps),
            "consistent" => Ok(HeadingCapStyle::Consistent),
            _ => Err(serde::de::Error::custom(format!(
                "Invalid heading capitalization style: {s}. Valid options: title-case, sentence-case, all-caps, consistent"
            ))),
        }
    }
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

    /// Punctuation that starts a new sentence in sentence case, so the word after it
    /// is capitalized like the first word of the heading. Matched against the end of a
    /// word, so the boundary is only recognized where whitespace follows it.
    /// Empty by default, which capitalizes only the heading's first word.
    #[serde(
        default,
        rename = "sentence-case-restart-after",
        alias = "sentence_case_restart_after"
    )]
    pub sentence_case_restart_after: Vec<String>,

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
            sentence_case_restart_after: Vec::new(),
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
        assert!(config.sentence_case_restart_after.is_empty());
        assert_eq!(config.min_level, 1);
        assert_eq!(config.max_level, 6);
    }

    #[test]
    fn test_sentence_case_restart_after_reads_kebab_case() {
        // Config keys reach serde already lowercase-kebab, so this is the spelling
        // that has to work.
        let config: MD063Config = toml::from_str(r#"sentence-case-restart-after = [":", ";"]"#).unwrap();
        assert_eq!(config.sentence_case_restart_after, vec![":", ";"]);

        let toml_str = toml::to_string(&MD063Config::default()).unwrap();
        assert!(toml_str.contains("sentence-case-restart-after"));
        assert!(!toml_str.contains("sentence_case_restart_after"));
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
    fn test_style_accepts_kebab_case_aliases() {
        let title_case: MD063Config = toml::from_str(r#"style = "title-case""#).unwrap();
        assert_eq!(title_case.style, HeadingCapStyle::TitleCase);

        let sentence_case: MD063Config = toml::from_str(r#"style = "sentence-case""#).unwrap();
        assert_eq!(sentence_case.style, HeadingCapStyle::SentenceCase);

        let all_caps: MD063Config = toml::from_str(r#"style = "all-caps""#).unwrap();
        assert_eq!(all_caps.style, HeadingCapStyle::AllCaps);
    }

    #[test]
    fn test_style_serialization() {
        assert_eq!(
            serde_json::to_string(&HeadingCapStyle::TitleCase).unwrap(),
            "\"title-case\""
        );
        assert_eq!(
            serde_json::to_string(&HeadingCapStyle::SentenceCase).unwrap(),
            "\"sentence-case\""
        );
        assert_eq!(
            serde_json::to_string(&HeadingCapStyle::AllCaps).unwrap(),
            "\"all-caps\""
        );
        assert_eq!(
            serde_json::to_string(&HeadingCapStyle::Consistent).unwrap(),
            "\"consistent\""
        );
    }

    #[test]
    fn test_consistent_style_deserialization() {
        let config: MD063Config = toml::from_str("style = \"consistent\"").unwrap();
        assert_eq!(config.style, HeadingCapStyle::Consistent);

        // Hyphenated form must also parse.
        let config: MD063Config = toml::from_str("style = \"consistent\"").unwrap();
        assert_eq!(config.style, HeadingCapStyle::Consistent);

        // Invalid value is rejected with a helpful message.
        let err = toml::from_str::<MD063Config>("style = \"bogus\"").unwrap_err();
        assert!(
            err.to_string().contains("consistent"),
            "Error should mention valid options: {err}"
        );
    }
}
