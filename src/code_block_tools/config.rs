//! Configuration types for code block tools.
//!
//! This module defines the configuration schema for per-language code block
//! linting and formatting using external tools.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Master configuration for code block tools.
///
/// This is disabled by default for safety - users must explicitly enable it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct CodeBlockToolsConfig {
    /// Master switch (default: false)
    #[serde(default)]
    pub enabled: bool,

    /// Language normalization strategy
    #[serde(default)]
    pub normalize_language: NormalizeLanguage,

    /// Global error handling strategy
    #[serde(default)]
    pub on_error: OnError,

    /// Behavior when a code block language has no tools configured for the current mode
    /// (e.g., no lint tools for `rumdl check`, no format tools for `rumdl check --fix`)
    #[serde(default)]
    pub on_missing_language_definition: OnMissing,

    /// Behavior when a configured tool's binary cannot be found (e.g., not in PATH)
    #[serde(default)]
    pub on_missing_tool_binary: OnMissing,

    /// Timeout per tool execution in milliseconds (default: 30000)
    #[serde(default = "default_timeout")]
    #[schemars(schema_with = "schema_timeout")]
    pub timeout: u64,

    /// Per-language tool configuration
    #[serde(default)]
    pub languages: HashMap<String, LanguageToolConfig>,

    /// User-defined language aliases (override built-in resolution)
    /// Example: { "py": "python", "bash": "shell" }
    #[serde(default)]
    pub language_aliases: HashMap<String, String>,

    /// Custom tool definitions (override built-ins)
    #[serde(default)]
    pub tools: HashMap<String, ToolDefinition>,
}

fn default_timeout() -> u64 {
    30_000
}

/// Generate a JSON Schema for timeout using standard integer type.
fn schema_timeout(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "integer",
        "minimum": 0
    })
}

impl Default for CodeBlockToolsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            normalize_language: NormalizeLanguage::default(),
            on_error: OnError::default(),
            on_missing_language_definition: OnMissing::default(),
            on_missing_tool_binary: OnMissing::default(),
            timeout: default_timeout(),
            languages: HashMap::new(),
            language_aliases: HashMap::new(),
            tools: HashMap::new(),
        }
    }
}

/// Language normalization strategy.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NormalizeLanguage {
    /// Resolve language aliases using GitHub Linguist data (e.g., "py" -> "python")
    #[default]
    Linguist,
    /// Use the language tag exactly as written in the code block
    Exact,
}

/// Error handling strategy for tool execution failures.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum OnError {
    /// Fail the lint/format operation (propagate error)
    #[default]
    Fail,
    /// Skip the code block and continue processing
    Skip,
    /// Log a warning but continue processing
    Warn,
}

/// Behavior when a language has no tools configured or a tool binary is missing.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum OnMissing {
    /// Silently skip and continue processing (default for backward compatibility)
    #[default]
    Ignore,
    /// Record an error for that block, continue processing, exit non-zero at the end
    Fail,
    /// Stop immediately on the first occurrence, exit non-zero
    FailFast,
}

/// Per-language tool configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct LanguageToolConfig {
    /// Tools to run in lint mode (rumdl check)
    #[serde(default)]
    pub lint: Vec<String>,

    /// Tools to run in format mode (rumdl check --fix / rumdl fmt)
    #[serde(default)]
    pub format: Vec<String>,

    /// Override global on-error setting for this language
    #[serde(default)]
    pub on_error: Option<OnError>,
}

/// Definition of an external tool.
///
/// This describes how to invoke a tool and how it communicates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ToolDefinition {
    /// Command to run (first element is the binary, rest are arguments)
    pub command: Vec<String>,

    /// Whether the tool reads from stdin (default: true)
    #[serde(default = "default_true")]
    pub stdin: bool,

    /// Whether the tool writes to stdout (default: true)
    #[serde(default = "default_true")]
    pub stdout: bool,

    /// Additional arguments for lint mode (appended to command)
    #[serde(default)]
    pub lint_args: Vec<String>,

    /// Additional arguments for format mode (appended to command)
    #[serde(default)]
    pub format_args: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for ToolDefinition {
    fn default() -> Self {
        Self {
            command: Vec::new(),
            stdin: true,
            stdout: true,
            lint_args: Vec::new(),
            format_args: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CodeBlockToolsConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.normalize_language, NormalizeLanguage::Linguist);
        assert_eq!(config.on_error, OnError::Fail);
        assert_eq!(config.on_missing_language_definition, OnMissing::Ignore);
        assert_eq!(config.on_missing_tool_binary, OnMissing::Ignore);
        assert_eq!(config.timeout, 30_000);
        assert!(config.languages.is_empty());
        assert!(config.language_aliases.is_empty());
        assert!(config.tools.is_empty());
    }

    #[test]
    fn test_deserialize_config() {
        let toml = r#"
enabled = true
normalize-language = "exact"
on-error = "skip"
timeout = 60000

[languages.python]
lint = ["ruff:check"]
format = ["ruff:format"]

[languages.json]
format = ["prettier"]
on-error = "warn"

[language-aliases]
py = "python"
bash = "shell"

[tools.custom-tool]
command = ["my-tool", "--format"]
stdin = true
stdout = true
"#;

        let config: CodeBlockToolsConfig = toml::from_str(toml).expect("Failed to parse TOML");

        assert!(config.enabled);
        assert_eq!(config.normalize_language, NormalizeLanguage::Exact);
        assert_eq!(config.on_error, OnError::Skip);
        assert_eq!(config.timeout, 60_000);

        let python = config.languages.get("python").expect("Missing python config");
        assert_eq!(python.lint, vec!["ruff:check"]);
        assert_eq!(python.format, vec!["ruff:format"]);
        assert_eq!(python.on_error, None);

        let json = config.languages.get("json").expect("Missing json config");
        assert!(json.lint.is_empty());
        assert_eq!(json.format, vec!["prettier"]);
        assert_eq!(json.on_error, Some(OnError::Warn));

        assert_eq!(config.language_aliases.get("py").map(String::as_str), Some("python"));
        assert_eq!(config.language_aliases.get("bash").map(String::as_str), Some("shell"));

        let tool = config.tools.get("custom-tool").expect("Missing custom tool");
        assert_eq!(tool.command, vec!["my-tool", "--format"]);
        assert!(tool.stdin);
        assert!(tool.stdout);
    }

    #[test]
    fn test_serialize_config() {
        let mut config = CodeBlockToolsConfig {
            enabled: true,
            ..Default::default()
        };
        config.languages.insert(
            "rust".to_string(),
            LanguageToolConfig {
                lint: vec![],
                format: vec!["rustfmt".to_string()],
                on_error: None,
            },
        );

        let toml = toml::to_string_pretty(&config).expect("Failed to serialize");
        assert!(toml.contains("enabled = true"));
        assert!(toml.contains("[languages.rust]"));
        assert!(toml.contains("rustfmt"));
    }

    #[test]
    fn test_on_missing_options() {
        let toml = r#"
enabled = true
on-missing-language-definition = "fail"
on-missing-tool-binary = "fail-fast"
"#;

        let config: CodeBlockToolsConfig = toml::from_str(toml).expect("Failed to parse TOML");

        assert_eq!(config.on_missing_language_definition, OnMissing::Fail);
        assert_eq!(config.on_missing_tool_binary, OnMissing::FailFast);
    }

    #[test]
    fn test_on_missing_default_ignore() {
        let toml = r#"
enabled = true
"#;

        let config: CodeBlockToolsConfig = toml::from_str(toml).expect("Failed to parse TOML");

        // Both should default to Ignore for backward compatibility
        assert_eq!(config.on_missing_language_definition, OnMissing::Ignore);
        assert_eq!(config.on_missing_tool_binary, OnMissing::Ignore);
    }

    #[test]
    fn test_on_missing_all_variants() {
        // Test all variants deserialize correctly
        for (input, expected) in [
            ("ignore", OnMissing::Ignore),
            ("fail", OnMissing::Fail),
            ("fail-fast", OnMissing::FailFast),
        ] {
            let toml = format!(
                r#"
enabled = true
on-missing-language-definition = "{input}"
"#
            );
            let config: CodeBlockToolsConfig = toml::from_str(&toml).expect("Failed to parse TOML");
            assert_eq!(
                config.on_missing_language_definition, expected,
                "Failed for variant: {input}"
            );
        }
    }
}
