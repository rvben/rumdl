use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Indentation style for unordered lists
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum IndentStyle {
    /// Text-aligned: Nested items align with parent's text content (rumdl default)
    #[default]
    #[serde(rename = "text-aligned", alias = "text_aligned")]
    TextAligned,
    /// Fixed: Use fixed multiples of indent size (markdownlint compatible)
    #[serde(rename = "fixed")]
    Fixed,
}

/// Configuration for MD007 (Unordered list indentation)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD007Config {
    /// Indentation size for nested unordered lists (default: 2)
    #[serde(default = "default_indent")]
    pub indent: usize,

    /// Allow first level lists to start indented (default: false)
    #[serde(default, alias = "start_indented")]
    pub start_indented: bool,

    /// Number of spaces for first level indent when start_indented is true (default: 2)
    #[serde(default = "default_start_indent", alias = "start_indent")]
    pub start_indent: usize,

    /// Indentation style: text-aligned (default) or fixed (markdownlint compatible)
    #[serde(default)]
    pub style: IndentStyle,
}

fn default_indent() -> usize {
    2
}

fn default_start_indent() -> usize {
    2
}

impl Default for MD007Config {
    fn default() -> Self {
        Self {
            indent: default_indent(),
            start_indented: false,
            start_indent: default_start_indent(),
            style: IndentStyle::default(),
        }
    }
}

impl RuleConfig for MD007Config {
    const RULE_NAME: &'static str = "MD007";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_case_backwards_compatibility() {
        // Test that snake_case field names work for backwards compatibility
        let toml_str = r#"
            start_indented = true
            start_indent = 4
        "#;
        let config: MD007Config = toml::from_str(toml_str).unwrap();
        assert!(config.start_indented);
        assert_eq!(config.start_indent, 4);
    }

    #[test]
    fn test_kebab_case_canonical_format() {
        // Test that kebab-case (canonical format) works
        let toml_str = r#"
            start-indented = true
            start-indent = 4
        "#;
        let config: MD007Config = toml::from_str(toml_str).unwrap();
        assert!(config.start_indented);
        assert_eq!(config.start_indent, 4);
    }
}
