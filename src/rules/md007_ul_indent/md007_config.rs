use crate::rule_config_serde::RuleConfig;
use crate::types::IndentSize;
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
    pub indent: IndentSize,

    /// Allow first level lists to start indented (default: false)
    #[serde(default, alias = "start_indented")]
    pub start_indented: bool,

    /// Number of spaces for first level indent when start_indented is true (default: 2)
    #[serde(default = "default_start_indent", alias = "start_indent")]
    pub start_indent: IndentSize,

    /// Indentation style: text-aligned (default) or fixed (markdownlint compatible)
    #[serde(default)]
    pub style: IndentStyle,

    /// Whether style was explicitly set in config (used for smart auto-detection)
    /// When false and indent != 2, we auto-select style based on document content:
    /// - Pure unordered lists → fixed style (markdownlint compatible)
    /// - Mixed ordered/unordered → text-aligned (avoids oscillation)
    #[serde(skip)]
    pub style_explicit: bool,

    /// Whether indent was explicitly set in config (used for "Do What I Mean" behavior)
    /// When indent is explicitly set but style is not, automatically use fixed style
    #[serde(skip)]
    pub indent_explicit: bool,
}

fn default_indent() -> IndentSize {
    IndentSize::from_const(2)
}

fn default_start_indent() -> IndentSize {
    IndentSize::from_const(2)
}

impl Default for MD007Config {
    fn default() -> Self {
        Self {
            indent: default_indent(),
            start_indented: false,
            start_indent: default_start_indent(),
            style: IndentStyle::default(),
            style_explicit: false,
            indent_explicit: false,
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
        assert_eq!(config.start_indent.get(), 4);
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
        assert_eq!(config.start_indent.get(), 4);
    }

    #[test]
    fn test_indent_validation() {
        // Test that invalid indent values are rejected
        let toml_str = r#"
            indent = 0
        "#;
        let result: Result<MD007Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must be between 1 and 8") || err.contains("got 0"));

        // Test that indent value above 8 is rejected
        let toml_str = r#"
            indent = 9
        "#;
        let result: Result<MD007Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must be between 1 and 8") || err.contains("got 9"));
    }
}
