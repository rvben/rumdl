use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};

/// Reflow mode for MD013
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ReflowMode {
    /// Only reflow lines that exceed the line length limit (default behavior)
    #[default]
    Default,
    /// Normalize all paragraphs to use the full line length
    Normalize,
    /// One sentence per line - break at sentence boundaries
    #[serde(alias = "sentence_per_line")]
    SentencePerLine,
}

/// Configuration for MD013 (Line length)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD013Config {
    /// Maximum line length (default: 80)
    #[serde(default = "default_line_length", alias = "line_length")]
    pub line_length: usize,

    /// Check code blocks for line length (default: true)
    #[serde(default = "default_code_blocks", alias = "code_blocks")]
    pub code_blocks: bool,

    /// Check tables for line length (default: true)
    #[serde(default = "default_tables")]
    pub tables: bool,

    /// Check headings for line length (default: true)
    #[serde(default = "default_headings")]
    pub headings: bool,

    /// Strict mode - disables exceptions for URLs, etc. (default: false)
    #[serde(default)]
    pub strict: bool,

    /// Enable text reflow to wrap long lines (default: false)
    #[serde(default, alias = "enable_reflow", alias = "enable-reflow")]
    pub reflow: bool,

    /// Reflow mode - how to handle reflowing (default: "long-lines")
    #[serde(default, alias = "reflow_mode")]
    pub reflow_mode: ReflowMode,
}

fn default_line_length() -> usize {
    80
}

fn default_code_blocks() -> bool {
    true
}

fn default_tables() -> bool {
    true
}

fn default_headings() -> bool {
    true
}

impl Default for MD013Config {
    fn default() -> Self {
        Self {
            line_length: default_line_length(),
            code_blocks: default_code_blocks(),
            tables: default_tables(),
            headings: default_headings(),
            strict: false,
            reflow: false,
            reflow_mode: ReflowMode::default(),
        }
    }
}

impl RuleConfig for MD013Config {
    const RULE_NAME: &'static str = "MD013";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reflow_mode_deserialization_kebab_case() {
        // Test that kebab-case (official format) works
        // Note: field name is reflow-mode (kebab) due to struct-level rename_all
        let toml_str = r#"
            reflow-mode = "sentence-per-line"
        "#;
        let config: MD013Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.reflow_mode, ReflowMode::SentencePerLine);

        let toml_str = r#"
            reflow-mode = "default"
        "#;
        let config: MD013Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.reflow_mode, ReflowMode::Default);

        let toml_str = r#"
            reflow-mode = "normalize"
        "#;
        let config: MD013Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.reflow_mode, ReflowMode::Normalize);
    }

    #[test]
    fn test_reflow_mode_deserialization_snake_case_alias() {
        // Test that snake_case (alias for backwards compatibility) works
        // Both for the enum value AND potentially for the field name
        let toml_str = r#"
            reflow-mode = "sentence_per_line"
        "#;
        let config: MD013Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.reflow_mode, ReflowMode::SentencePerLine);
    }

    #[test]
    fn test_field_name_backwards_compatibility() {
        // Test that snake_case field names work (for backwards compatibility)
        // even though docs show kebab-case (like Ruff)
        let toml_str = r#"
            line_length = 100
            code_blocks = false
            reflow_mode = "sentence_per_line"
        "#;
        let config: MD013Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.line_length, 100);
        assert!(!config.code_blocks);
        assert_eq!(config.reflow_mode, ReflowMode::SentencePerLine);

        // Also test mixed format (should work)
        let toml_str = r#"
            line-length = 100
            code_blocks = false
            reflow-mode = "normalize"
        "#;
        let config: MD013Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.line_length, 100);
        assert!(!config.code_blocks);
        assert_eq!(config.reflow_mode, ReflowMode::Normalize);
    }

    #[test]
    fn test_reflow_mode_serialization() {
        // Test that serialization always uses kebab-case (primary format)
        let config = MD013Config {
            line_length: 80,
            code_blocks: true,
            tables: true,
            headings: true,
            strict: false,
            reflow: true,
            reflow_mode: ReflowMode::SentencePerLine,
        };

        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("sentence-per-line"));
        assert!(!toml_str.contains("sentence_per_line"));
    }

    #[test]
    fn test_reflow_mode_invalid_value() {
        // Test that invalid values fail deserialization
        let toml_str = r#"
            reflow-mode = "invalid_mode"
        "#;
        let result = toml::from_str::<MD013Config>(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_full_config_with_reflow_mode() {
        let toml_str = r#"
            line-length = 100
            code-blocks = false
            tables = false
            headings = true
            strict = true
            reflow = true
            reflow-mode = "sentence-per-line"
        "#;
        let config: MD013Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.line_length, 100);
        assert!(!config.code_blocks);
        assert!(!config.tables);
        assert!(config.headings);
        assert!(config.strict);
        assert!(config.reflow);
        assert_eq!(config.reflow_mode, ReflowMode::SentencePerLine);
    }
}
