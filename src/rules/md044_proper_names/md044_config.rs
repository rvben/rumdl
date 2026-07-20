use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD044Config {
    #[serde(default)]
    pub names: Vec<String>,

    #[serde(default = "default_code_blocks", rename = "code-blocks", alias = "code_blocks")]
    pub code_blocks: bool,

    #[serde(default = "default_html_elements", rename = "html-elements", alias = "html_elements")]
    pub html_elements: bool,

    #[serde(default = "default_html_comments", rename = "html-comments", alias = "html_comments")]
    pub html_comments: bool,

    /// Top-level frontmatter keys whose values are not checked. Matched
    /// case-insensitively. `None` checks every field. A parent key excludes
    /// its whole subtree.
    #[serde(default, rename = "ignore-frontmatter-fields", alias = "ignore_frontmatter_fields")]
    pub ignore_frontmatter_fields: Option<Vec<String>>,
}

impl Default for MD044Config {
    fn default() -> Self {
        Self {
            names: Vec::new(),
            code_blocks: default_code_blocks(),
            html_elements: default_html_elements(),
            html_comments: default_html_comments(),
            ignore_frontmatter_fields: None,
        }
    }
}

fn default_code_blocks() -> bool {
    false
}

fn default_html_elements() -> bool {
    true
}

fn default_html_comments() -> bool {
    true
}

impl RuleConfig for MD044Config {
    const RULE_NAME: &'static str = "MD044";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kebab_case_canonical_format() {
        let toml_str = r#"
            names = ["JavaScript", "TypeScript"]
            code-blocks = false
            html-comments = false
        "#;
        let config: MD044Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.names, vec!["JavaScript", "TypeScript"]);
        assert!(!config.code_blocks);
        assert!(!config.html_comments);
    }

    #[test]
    fn test_snake_case_backwards_compatibility() {
        let toml_str = r#"
            names = ["Python", "Rust"]
            code_blocks = false
            html_comments = false
        "#;
        let config: MD044Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.names, vec!["Python", "Rust"]);
        assert!(!config.code_blocks);
        assert!(!config.html_comments);
    }

    #[test]
    fn test_mixed_formats() {
        // Test that kebab-case and snake_case can be mixed
        let toml_str = r#"
            names = ["Node.js"]
            code-blocks = true
            html_comments = false
        "#;
        let config: MD044Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.names, vec!["Node.js"]);
        assert!(config.code_blocks);
        assert!(!config.html_comments);
    }

    #[test]
    fn test_default_values() {
        let config = MD044Config::default();
        assert!(config.names.is_empty());
        assert!(!config.code_blocks);
        assert!(config.html_elements);
        assert!(config.html_comments);
    }

    #[test]
    fn test_ignore_frontmatter_fields_kebab_and_snake() {
        let kebab: MD044Config = toml::from_str(r#"ignore-frontmatter-fields = ["link"]"#).unwrap();
        assert_eq!(
            kebab.ignore_frontmatter_fields.as_deref(),
            Some(&["link".to_string()][..])
        );

        let snake: MD044Config = toml::from_str(r#"ignore_frontmatter_fields = ["link"]"#).unwrap();
        assert_eq!(
            snake.ignore_frontmatter_fields.as_deref(),
            Some(&["link".to_string()][..])
        );
    }

    #[test]
    fn test_ignore_frontmatter_fields_defaults_to_none() {
        assert!(MD044Config::default().ignore_frontmatter_fields.is_none());
    }
}
