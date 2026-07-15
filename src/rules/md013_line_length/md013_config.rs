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
    /// Semantic line breaks - cascading strategy:
    /// 1. Sentence boundaries (always)
    /// 2. Clause punctuation (when line > line-length)
    /// 3. English break-words (when line still > line-length)
    /// 4. Word wrap (fallback)
    #[serde(alias = "semantic_line_breaks")]
    SemanticLineBreaks,
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

    /// Check lines whose length comes from an inline code span (default: true).
    ///
    /// Inline code spans (`` `like this` ``) cannot be wrapped, so reflow cannot
    /// shorten a line whose excess length is one of them. When `false`, a line is
    /// not reported if it would fit within the limit once its inline code spans are
    /// excluded - useful with `reflow` so an otherwise-clean file is not failed by
    /// an unbreakable code incantation.
    #[serde(default = "default_code_spans", alias = "code_spans")]
    pub code_spans: bool,

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

    /// Check blockquote content for line length (default: true)
    /// When false, blockquote lines are not checked for line length.
    /// When paragraphs = false, blockquote content is also skipped
    /// since blockquote content is paragraph text.
    #[serde(default = "default_blockquotes")]
    pub blockquotes: bool,

    /// Strict mode - disables exceptions for URLs, etc. (default: false)
    #[serde(default)]
    pub strict: bool,

    /// Stern mode - like strict, but lines that consist of a single
    /// non-whitespace token (optionally prefixed by heading/blockquote
    /// markers) are still permitted. Mirrors markdownlint's `stern` option.
    /// Default: false.
    #[serde(default)]
    pub stern: bool,

    /// Whether to ignore inline link/image URLs when measuring line length
    /// (default: true).
    ///
    /// In non-strict mode, a line that exceeds the limit only because of the URL
    /// portion of an inline `[text](url)` / `![alt](url)` is forgiven (the URL
    /// cannot be shortened). Set to `false` to count those URLs toward the line
    /// length so the line is flagged. Combine with `stern` to flag a link line
    /// that has wrappable text around it while still exempting a line that is a
    /// single unbreakable token (a bare URL or a standalone link). Has no effect
    /// in `strict` mode, which already disables all forgiveness.
    ///
    /// Accepts the former `semantic-link-understanding` key as an alias.
    #[serde(
        default = "default_ignore_link_urls",
        alias = "ignore_link_urls",
        alias = "semantic-link-understanding",
        alias = "semantic_link_understanding"
    )]
    pub ignore_link_urls: bool,

    /// Per-context maximum line length for headings.
    ///
    /// `None` (unset) falls back to `line_length`. `Some(0)` means "no limit
    /// for headings". Mirrors markdownlint's `heading_line_length`.
    #[serde(default, alias = "heading_line_length")]
    pub heading_line_length: Option<LineLength>,

    /// Per-context maximum line length for code blocks (fenced or indented).
    ///
    /// `None` (unset) falls back to `line_length`. `Some(0)` means "no limit
    /// for code blocks". Mirrors markdownlint's `code_block_line_length`.
    #[serde(default, alias = "code_block_line_length")]
    pub code_block_line_length: Option<LineLength>,

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

    /// Whether to require uppercase after periods for sentence detection (default: true).
    /// When true, only "word. Capital" is treated as a sentence boundary.
    /// When false, "word. lowercase" is also treated as a sentence boundary.
    /// Does not affect ! and ? which are always treated as sentence boundaries.
    #[serde(
        default = "default_require_sentence_capital",
        alias = "require_sentence_capital",
        alias = "strict_sentences",
        alias = "strict-sentences"
    )]
    pub require_sentence_capital: bool,

    /// Whether to allow reflow breaking inside emphasis/strong/strikethrough spans.
    /// When true, emphasis text is wrapped word-by-word like normal text.
    /// When false (default), emphasis spans are treated as atomic units.
    #[serde(default = "default_emphasis_spans", alias = "emphasis_spans")]
    pub emphasis_spans: bool,
}

fn default_line_length() -> LineLength {
    LineLength::from_const(80)
}

fn default_code_blocks() -> bool {
    true
}

fn default_code_spans() -> bool {
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

fn default_blockquotes() -> bool {
    true
}

fn default_require_sentence_capital() -> bool {
    true
}

fn default_ignore_link_urls() -> bool {
    true
}

fn default_emphasis_spans() -> bool {
    false
}

impl Default for MD013Config {
    fn default() -> Self {
        Self {
            line_length: default_line_length(),
            code_blocks: default_code_blocks(),
            code_spans: default_code_spans(),
            tables: default_tables(),
            headings: default_headings(),
            paragraphs: default_paragraphs(),
            blockquotes: default_blockquotes(),
            strict: false,
            stern: false,
            ignore_link_urls: default_ignore_link_urls(),
            heading_line_length: None,
            code_block_line_length: None,
            reflow: false,
            reflow_mode: ReflowMode::default(),
            length_mode: LengthMode::default(),
            abbreviations: Vec::new(),
            require_sentence_capital: default_require_sentence_capital(),
            emphasis_spans: default_emphasis_spans(),
        }
    }
}

impl MD013Config {
    /// Effective line-length budget for heading lines.
    /// Falls back to `line_length` when `heading_line_length` is unset.
    pub fn effective_heading_line_length(&self) -> LineLength {
        self.heading_line_length.unwrap_or(self.line_length)
    }

    /// Effective line-length budget for fenced or indented code-block lines.
    /// Falls back to `line_length` when `code_block_line_length` is unset.
    pub fn effective_code_block_line_length(&self) -> LineLength {
        self.code_block_line_length.unwrap_or(self.line_length)
    }

    /// Smallest applicable line-length budget across all contexts. Used to
    /// pre-filter candidate lines: any line shorter than this can never
    /// violate, regardless of which context it falls under.
    pub fn min_effective_line_length(&self) -> LineLength {
        let mut limits: Vec<LineLength> = vec![self.line_length];
        if let Some(h) = self.heading_line_length {
            limits.push(h);
        }
        if let Some(c) = self.code_block_line_length {
            limits.push(c);
        }
        // "Unlimited" (0) is the laxest possible budget, so it must not win
        // the minimum unless all budgets are unlimited.
        let bounded: Vec<LineLength> = limits.iter().copied().filter(|l| !l.is_unlimited()).collect();
        if bounded.is_empty() {
            LineLength::from_const(0)
        } else {
            bounded.into_iter().min_by_key(|l| l.get()).unwrap()
        }
    }

    /// Convert abbreviations Vec to Option for ReflowOptions
    /// Empty Vec means "use defaults only" so it maps to None
    pub fn abbreviations_for_reflow(&self) -> Option<Vec<String>> {
        if self.abbreviations.is_empty() {
            None
        } else {
            Some(self.abbreviations.clone())
        }
    }

    /// Map the configured length mode to the reflow engine's length mode.
    pub(crate) fn reflow_length_mode(&self) -> crate::utils::text_reflow::ReflowLengthMode {
        match self.length_mode {
            LengthMode::Chars => crate::utils::text_reflow::ReflowLengthMode::Chars,
            LengthMode::Visual => crate::utils::text_reflow::ReflowLengthMode::Visual,
            LengthMode::Bytes => crate::utils::text_reflow::ReflowLengthMode::Bytes,
        }
    }

    /// Build a `ReflowOptions` from this configuration.
    ///
    /// Converts `reflow_mode`, `length_mode`, `abbreviations`, and `line_length`
    /// into the unified `ReflowOptions` type used by the reflow engine.
    pub fn to_reflow_options(&self) -> crate::utils::text_reflow::ReflowOptions {
        crate::utils::text_reflow::ReflowOptions {
            line_length: self.line_length.get(),
            break_on_sentences: true,
            preserve_breaks: false,
            sentence_per_line: self.reflow_mode == ReflowMode::SentencePerLine,
            semantic_line_breaks: self.reflow_mode == ReflowMode::SemanticLineBreaks,
            abbreviations: self.abbreviations_for_reflow(),
            length_mode: self.reflow_length_mode(),
            attr_lists: false,
            myst_roles: false,
            require_sentence_capital: self.require_sentence_capital,
            max_list_continuation_indent: None,
            // No document context here (config-only), so shortcut references
            // stay atomic. The rule's fix path supplies the defined labels.
            defined_references: None,
            emphasis_spans: self.emphasis_spans,
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

        let toml_str = r#"
            reflow-mode = "semantic-line-breaks"
        "#;
        let config: MD013Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.reflow_mode, ReflowMode::SemanticLineBreaks);
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

        let toml_str = r#"
            reflow-mode = "semantic_line_breaks"
        "#;
        let config: MD013Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.reflow_mode, ReflowMode::SemanticLineBreaks);
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
            code_spans: true,
            tables: true,
            headings: true,
            paragraphs: true,
            blockquotes: true,
            strict: false,
            stern: false,
            heading_line_length: None,
            code_block_line_length: None,
            reflow: true,
            reflow_mode: ReflowMode::SentencePerLine,
            length_mode: LengthMode::default(),
            abbreviations: Vec::new(),
            require_sentence_capital: true,
            ignore_link_urls: true,
            emphasis_spans: false,
        };

        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("sentence-per-line"));
        assert!(!toml_str.contains("sentence_per_line"));

        // Test serialization of SemanticLineBreaks
        let config = MD013Config {
            reflow_mode: ReflowMode::SemanticLineBreaks,
            ..config
        };
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("semantic-line-breaks"));
        assert!(!toml_str.contains("semantic_line_breaks"));
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
    fn test_code_spans_default_true_and_parses() {
        assert!(MD013Config::default().code_spans, "code-spans defaults to true");

        let config: MD013Config = toml::from_str("code-spans = false").unwrap();
        assert!(!config.code_spans);
        // snake_case alias also works.
        let config: MD013Config = toml::from_str("code_spans = false").unwrap();
        assert!(!config.code_spans);
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
