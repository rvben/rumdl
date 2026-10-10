//! Configuration types for code block tools.
//!
//! This module defines the configuration schema for per-language code block
//! linting and formatting using external tools.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

    /// Behavior when a recognized language has no tools configured in either mode.
    #[serde(default)]
    pub on_missing_language_definition: OnMissing,

    /// Behavior when a configured tool's binary is absent from the allowed lookup locations.
    /// Defaults to `warn`: the tools rumdl drives are installed separately from
    /// rumdl, so an absent one is common enough that silence about it is a trap.
    #[serde(default = "default_on_missing_tool_binary")]
    pub on_missing_tool_binary: OnMissing,

    /// Select already-installed project/PATH executables per underlying binary.
    #[serde(default, alias = "binary_preferences")]
    pub binary_preferences: BTreeMap<String, BinaryPreference>,

    /// Policy for unlabeled fenced code blocks.
    #[serde(default = "default_on_missing_tool_binary", alias = "on_missing_language_tag")]
    pub on_missing_language_tag: OnMissing,
    /// Policy for tags unknown to the resolver, configured aliases, or custom languages.
    #[serde(default = "default_on_missing_tool_binary", alias = "on_unknown_language_tag")]
    pub on_unknown_language_tag: OnMissing,
    /// Policy for a configured language missing tools for the active mode.
    #[serde(default, alias = "on_missing_mode_definition")]
    pub on_missing_mode_definition: OnMissing,
    /// Policy for semantically invalid tool definitions and references.
    #[serde(default = "default_on_missing_tool_binary", alias = "on_invalid_tool_definition")]
    pub on_invalid_tool_definition: OnMissing,
    /// Invocation-level policy when no tool or valid cached tool check was used.
    #[serde(default = "default_on_missing_tool_binary", alias = "on_no_tools_run")]
    pub on_no_tools_run: OnMissing,
    /// Invocation-owned accounting. Never part of user configuration or schema.
    #[serde(skip)]
    #[schemars(skip)]
    pub run_state: Option<std::sync::Arc<super::run_state::RunState>>,

    /// Timeout per tool execution in milliseconds (default: 30000)
    #[serde(default = "default_timeout")]
    #[schemars(schema_with = "schema_timeout")]
    pub timeout: u64,

    /// Per-language tool configuration
    #[serde(default)]
    pub languages: BTreeMap<String, LanguageToolConfig>,

    /// User-defined language aliases (override built-in resolution)
    /// Example: { "py": "python", "bash": "shell" }
    #[serde(default)]
    pub language_aliases: BTreeMap<String, String>,

    /// Custom tool definitions (override built-ins)
    #[serde(default)]
    pub tools: BTreeMap<String, ToolDefinition>,

    /// Whether this section came from a config file whose contents may not be quoted
    /// back (an `extends` target, whose path is arbitrary and whose text the extending
    /// project need not be able to read). The settings apply as written; only a message
    /// about one has to leave it out.
    ///
    /// Provenance rather than configuration, so it stays out of the serialized form and
    /// the JSON schema. The whole section is replaced as one value when configs merge,
    /// so the mark travels with the settings it describes.
    #[serde(skip)]
    #[schemars(skip)]
    pub values_withheld: bool,
}

fn default_timeout() -> u64 {
    30_000
}

fn default_on_missing_tool_binary() -> OnMissing {
    OnMissing::Warn
}

/// Generate a JSON Schema for timeout using standard integer type.
fn schema_timeout(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "integer",
        "minimum": 0
    })
}

impl CodeBlockToolsConfig {
    pub fn requires_fail_fast(&self) -> bool {
        self.enabled
            && [
                self.on_missing_language_tag,
                self.on_unknown_language_tag,
                self.on_missing_mode_definition,
                self.on_invalid_tool_definition,
                self.on_missing_language_definition,
                self.on_missing_tool_binary,
                self.on_no_tools_run,
            ]
            .contains(&OnMissing::FailFast)
    }
}

impl Default for CodeBlockToolsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            normalize_language: NormalizeLanguage::default(),
            on_error: OnError::default(),
            on_missing_language_definition: OnMissing::default(),
            on_missing_tool_binary: default_on_missing_tool_binary(),
            binary_preferences: BTreeMap::new(),
            on_missing_language_tag: OnMissing::Warn,
            on_unknown_language_tag: OnMissing::Warn,
            on_missing_mode_definition: OnMissing::Ignore,
            on_invalid_tool_definition: OnMissing::Warn,
            on_no_tools_run: OnMissing::Warn,
            run_state: None,
            timeout: default_timeout(),
            languages: BTreeMap::new(),
            language_aliases: BTreeMap::new(),
            tools: BTreeMap::new(),
            values_withheld: false,
        }
    }
}

/// Preference for resolving an external executable. Explicit paths bypass this.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BinaryPreference {
    /// Prefer the project's virtual environment or node_modules, then PATH.
    #[default]
    Project,
    /// Prefer PATH, then project-local installations.
    System,
    /// Require a project-local installation.
    OnlyProject,
    /// Preserve PATH-only lookup.
    OnlySystem,
}

/// How a language's `format` list is applied to a code block.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FormatMode {
    /// Try the formatters in order; the first one that succeeds supplies the
    /// block, even when it changes nothing
    #[default]
    Fallback,
    /// Run every formatter in order, each on the output of the last one that
    /// succeeded, and replace the block with the final result
    Pipeline,
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
    /// Continue with the next tool without a warning
    Skip,
    /// Log a warning but continue processing
    Warn,
}

/// Behavior when a language has no tools configured or a tool binary is missing.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum OnMissing {
    /// Silently skip and continue processing
    #[default]
    Ignore,
    /// Say once that the tool is missing, then continue as `ignore` does.
    ///
    /// A config warning rather than a finding: the run still exits 0, and
    /// `--deny-config-warnings` is what turns it into a failure. This is the
    /// default for a missing tool binary, because that is a fact about the
    /// machine rather than about the document, and silence there means a run
    /// that checked none of your code blocks reports success.
    Warn,
    /// Record an error for that block, continue processing, exit non-zero at the end
    Fail,
    /// Stop immediately on the first occurrence, exit non-zero
    FailFast,
}

impl OnMissing {
    /// Whether this setting leaves the block alone and reports nothing about it.
    ///
    /// `warn` reports, but once for the run rather than against a block, so from
    /// a block's point of view it behaves exactly as `ignore` does. The two are
    /// therefore equivalent for deciding whether a document has to be parsed at
    /// all.
    pub fn skips_the_block(self) -> bool {
        matches!(self, OnMissing::Ignore | OnMissing::Warn)
    }
}

/// Per-language tool configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct LanguageToolConfig {
    /// Whether code block tools are enabled for this language (default: true).
    /// Set to false to acknowledge a language without configuring tools.
    /// This satisfies strict mode (on-missing-language-definition) checks.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Tools to run in lint mode (rumdl check)
    #[serde(default)]
    pub lint: Vec<String>,

    /// Tools to run in format mode (rumdl check --fix / rumdl fmt)
    #[serde(default)]
    pub format: Vec<String>,

    /// How the `format` list is applied: as fallbacks (default) or as a pipeline
    #[serde(default)]
    pub format_mode: FormatMode,

    /// Override global on-error setting for this language
    #[serde(default)]
    pub on_error: Option<OnError>,
}

impl Default for LanguageToolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            lint: Vec::new(),
            format: Vec::new(),
            format_mode: FormatMode::default(),
            on_error: None,
        }
    }
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
        assert_eq!(config.on_missing_tool_binary, OnMissing::Warn);
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
                format: vec!["rustfmt".to_string()],
                ..Default::default()
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
    fn test_on_missing_defaults() {
        let toml = r#"
enabled = true
"#;

        let config: CodeBlockToolsConfig = toml::from_str(toml).expect("Failed to parse TOML");

        // A language a config never mentioned is not something rumdl has an
        // opinion about, so it stays silent.
        assert_eq!(config.on_missing_language_definition, OnMissing::Ignore);
        // A tool the config did name, and the machine does not have, is a gap
        // between the two that the run has to mention.
        assert_eq!(config.on_missing_tool_binary, OnMissing::Warn);
    }

    #[test]
    fn test_on_missing_all_variants() {
        // Test all variants deserialize correctly
        for (input, expected) in [
            ("ignore", OnMissing::Ignore),
            ("warn", OnMissing::Warn),
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

    #[test]
    fn test_language_config_enabled_defaults_to_true() {
        // Deserializing without `enabled` should default to true
        let toml = r#"
lint = ["ruff:check"]
"#;
        let config: LanguageToolConfig = toml::from_str(toml).expect("Failed to parse TOML");
        assert!(config.enabled);
        assert_eq!(config.lint, vec!["ruff:check"]);
        assert!(config.format.is_empty());
    }

    #[test]
    fn test_language_config_enabled_false() {
        // Explicitly set enabled = false
        let toml = r#"
enabled = false
"#;
        let config: LanguageToolConfig = toml::from_str(toml).expect("Failed to parse TOML");
        assert!(!config.enabled);
        assert!(config.lint.is_empty());
        assert!(config.format.is_empty());
    }

    #[test]
    fn test_language_config_enabled_false_with_tools() {
        // enabled=false should be respected even when tools are configured
        let toml = r#"
enabled = false
lint = ["ruff:check"]
format = ["ruff:format"]
"#;
        let config: LanguageToolConfig = toml::from_str(toml).expect("Failed to parse TOML");
        assert!(!config.enabled);
        assert_eq!(config.lint, vec!["ruff:check"]);
        assert_eq!(config.format, vec!["ruff:format"]);
    }

    #[test]
    fn test_language_config_enabled_in_full_config() {
        // Test enabled field within a full CodeBlockToolsConfig
        let toml = r#"
enabled = true
on-missing-language-definition = "fail"

[languages.python]
lint = ["ruff:check"]

[languages.plaintext]
enabled = false
"#;
        let config: CodeBlockToolsConfig = toml::from_str(toml).expect("Failed to parse TOML");

        let python = config.languages.get("python").expect("Missing python config");
        assert!(python.enabled);
        assert_eq!(python.lint, vec!["ruff:check"]);

        let plaintext = config.languages.get("plaintext").expect("Missing plaintext config");
        assert!(!plaintext.enabled);
        assert!(plaintext.lint.is_empty());
    }

    #[test]
    fn test_language_config_default_trait() {
        let config = LanguageToolConfig::default();
        assert!(config.enabled);
        assert!(config.lint.is_empty());
        assert!(config.format.is_empty());
        assert!(config.on_error.is_none());
    }

    #[test]
    fn test_format_mode_parses_and_defaults_to_fallback() {
        let toml = r#"
[languages]
json = { format = ["jq"] }
shell = { format = ["shuck:lint-fix", "shuck:format"], format-mode = "pipeline" }
python = { format = ["ruff:format", "black"], format-mode = "fallback" }
"#;
        let config: CodeBlockToolsConfig = toml::from_str(toml).expect("Failed to parse TOML");
        assert_eq!(config.languages["json"].format_mode, FormatMode::Fallback);
        assert_eq!(config.languages["shell"].format_mode, FormatMode::Pipeline);
        assert_eq!(config.languages["python"].format_mode, FormatMode::Fallback);

        let invalid = "[languages]\njson = { format = [\"jq\"], format-mode = \"chain\" }\n";
        let error = toml::from_str::<CodeBlockToolsConfig>(invalid).unwrap_err().to_string();
        assert!(
            error.contains("format-mode") || (error.contains("fallback") && error.contains("pipeline")),
            "{error}"
        );
    }

    #[test]
    fn test_format_mode_round_trips() {
        let config = LanguageToolConfig {
            format: vec!["jq".to_string()],
            format_mode: FormatMode::Pipeline,
            ..Default::default()
        };
        let toml = toml::to_string_pretty(&config).expect("Failed to serialize");
        assert!(toml.contains("format-mode = \"pipeline\""), "{toml}");
        assert_eq!(toml::from_str::<LanguageToolConfig>(&toml).unwrap(), config);
    }

    #[test]
    fn test_language_config_serialize_enabled_false() {
        let config = LanguageToolConfig {
            enabled: false,
            ..Default::default()
        };
        let toml = toml::to_string_pretty(&config).expect("Failed to serialize");
        assert!(toml.contains("enabled = false"));
    }
}
