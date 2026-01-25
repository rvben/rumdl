use crate::rule_config_serde::RuleConfig;
use crate::types::LineLength;
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

/// Length calculation mode for MD013
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LengthMode {
    /// Count Unicode characters (grapheme clusters)
    /// Use this only if you need backward compatibility with character-based counting
    #[serde(alias = "chars", alias = "characters")]
    Chars,
    /// Count visual display width (CJK characters = 2 columns, emoji = 2, etc.) - default
    /// This is semantically correct: line-length = 80 means "80 columns on screen"
    #[default]
    #[serde(alias = "display", alias = "visual_width")]
    Visual,
    /// Count raw bytes (legacy mode, not recommended for Unicode text)
    Bytes,
}

/// Configuration for MD013 (Line length)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD013Config {
    /// Maximum line length (default: 80, 0 means no limit)
    #[serde(default = "default_line_length", alias = "line_length")]
    pub line_length: LineLength,

    /// Check code blocks for line length (default: true)
    #[serde(default = "default_code_blocks", alias = "code_blocks")]
    pub code_blocks: bool,

    /// Check tables for line length (default: false)
    ///
    /// Note: markdownlint defaults to true, but rumdl defaults to false to avoid
    /// conflicts with MD060 (table formatting). Tables often require specific widths
    /// for alignment, which can conflict with line length limits.
    #[serde(default = "default_tables")]
    pub tables: bool,

    /// Check headings for line length (default: true)
    #[serde(default = "default_headings")]
    pub headings: bool,

    /// Check paragraph/text line length (default: true)
    /// When false, line length violations in regular text are not reported,
    /// but reflow can still be used to format paragraphs.
    #[serde(default = "default_paragraphs")]
    pub paragraphs: bool,

    /// Strict mode - disables exceptions for URLs, etc. (default: false)
    #[serde(default)]
    pub strict: bool,

    /// Enable text reflow to wrap long lines (default: false)
    #[serde(default, alias = "enable_reflow", alias = "enable-reflow")]
    pub reflow: bool,

    /// Reflow mode - how to handle reflowing (default: "long-lines")
    #[serde(default, alias = "reflow_mode")]
    pub reflow_mode: ReflowMode,

    /// Length calculation mode (default: "chars")
    /// - "chars": Count Unicode characters (emoji = 1, CJK = 1)
    /// - "visual": Count visual display width (emoji = 2, CJK = 2)
    /// - "bytes": Count raw bytes (not recommended for Unicode)
    #[serde(default, alias = "length_mode")]
    pub length_mode: LengthMode,

    /// Custom abbreviations for sentence-per-line mode
    /// Periods are optional - both "Dr" and "Dr." work the same
    /// Inherited from global config, can be overridden per-rule
    /// Custom abbreviations are always added to the built-in defaults
    #[serde(default)]
    pub abbreviations: Vec<String>,
}

fn default_line_length() -> LineLength {
    LineLength::from_const(80)
}

fn default_code_blocks() -> bool {
    true
}

fn default_tables() -> bool {
    false
}

fn default_headings() -> bool {
    true
}

fn default_paragraphs() -> bool {
    true
}

impl Default for MD013Config {
    fn default() -> Self {
        Self {
            line_length: default_line_length(),
            code_blocks: default_code_blocks(),
            tables: default_tables(),
            headings: default_headings(),
            paragraphs: default_paragraphs(),
            strict: false,
            reflow: false,
            reflow_mode: ReflowMode::default(),
            length_mode: LengthMode::default(),
            abbreviations: Vec::new(),
        }
    }
}

impl MD013Config {
    /// Convert abbreviations Vec to Option for ReflowOptions
    /// Empty Vec means "use defaults only" so it maps to None
    pub fn abbreviations_for_reflow(&self) -> Option<Vec<String>> {
        if self.abbreviations.is_empty() {
            None
        } else {
            Some(self.abbreviations.clone())
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
        assert_eq!(config.line_length.get(), 100);
        assert!(!config.code_blocks);
        assert_eq!(config.reflow_mode, ReflowMode::SentencePerLine);

        // Also test mixed format (should work)
        let toml_str = r#"
            line-length = 100
            code_blocks = false
            reflow-mode = "normalize"
        "#;
        let config: MD013Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.line_length.get(), 100);
        assert!(!config.code_blocks);
        assert_eq!(config.reflow_mode, ReflowMode::Normalize);
    }

    #[test]
    fn test_reflow_mode_serialization() {
        // Test that serialization always uses kebab-case (primary format)
        let config = MD013Config {
            line_length: LineLength::from_const(80),
            code_blocks: true,
            tables: true,
            headings: true,
            paragraphs: true,
            strict: false,
            reflow: true,
            reflow_mode: ReflowMode::SentencePerLine,
            length_mode: LengthMode::default(),
            abbreviations: Vec::new(),
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
        assert_eq!(config.line_length.get(), 100);
        assert!(!config.code_blocks);
        assert!(!config.tables);
        assert!(config.headings);
        assert!(config.strict);
        assert!(config.reflow);
        assert_eq!(config.reflow_mode, ReflowMode::SentencePerLine);
    }

    #[test]
    fn test_paragraphs_default_true() {
        // Test that paragraphs defaults to true
        let config = MD013Config::default();
        assert!(config.paragraphs, "paragraphs should default to true");
    }

    #[test]
    fn test_paragraphs_deserialization_kebab_case() {
        // Test kebab-case (canonical format)
        let toml_str = r#"
            paragraphs = false
        "#;
        let config: MD013Config = toml::from_str(toml_str).unwrap();
        assert!(!config.paragraphs);
    }

    #[test]
    fn test_paragraphs_full_config() {
        // Test paragraphs in a full configuration with issue #121 use case
        let toml_str = r#"
            line-length = 80
            code-blocks = true
            tables = true
            headings = false
            paragraphs = false
            reflow = true
            reflow-mode = "sentence-per-line"
        "#;
        let config: MD013Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.line_length.get(), 80);
        assert!(config.code_blocks, "code-blocks should be true");
        assert!(config.tables, "tables should be true");
        assert!(!config.headings, "headings should be false");
        assert!(!config.paragraphs, "paragraphs should be false");
        assert!(config.reflow, "reflow should be true");
        assert_eq!(config.reflow_mode, ReflowMode::SentencePerLine);
    }

    #[test]
    fn test_abbreviations_for_reflow_empty_vec() {
        // Empty vec means "use defaults only" -> returns None
        let config = MD013Config {
            abbreviations: Vec::new(),
            ..Default::default()
        };
        assert!(
            config.abbreviations_for_reflow().is_none(),
            "Empty abbreviations should return None for reflow"
        );
    }

    #[test]
    fn test_abbreviations_for_reflow_with_custom() {
        // Non-empty vec means "use these custom abbreviations" -> returns Some
        let config = MD013Config {
            abbreviations: vec!["Corp".to_string(), "Inc".to_string()],
            ..Default::default()
        };
        let result = config.abbreviations_for_reflow();
        assert!(result.is_some(), "Custom abbreviations should return Some");
        let abbrevs = result.unwrap();
        assert_eq!(abbrevs.len(), 2);
        assert!(abbrevs.contains(&"Corp".to_string()));
        assert!(abbrevs.contains(&"Inc".to_string()));
    }
}
