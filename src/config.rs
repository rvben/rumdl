//!
//! This module defines configuration structures, loading logic, and provenance tracking for rumdl.
//! Supports TOML, pyproject.toml, and markdownlint config formats, and provides merging and override logic.

use crate::rule::Rule;
use crate::rules;
use crate::types::LineLength;
use log;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::marker::PhantomData;
use std::path::Path;
use std::str::FromStr;
use toml_edit::DocumentMut;

// ============================================================================
// Typestate markers for configuration pipeline
// ============================================================================

/// Marker type for configuration that has been loaded but not yet validated.
/// This is the initial state after `load_with_discovery()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigLoaded;

/// Marker type for configuration that has been validated.
/// Only validated configs can be converted to `Config`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigValidated;

/// Markdown flavor/dialect enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkdownFlavor {
    /// Standard Markdown without flavor-specific adjustments
    #[serde(rename = "standard", alias = "none", alias = "")]
    #[default]
    Standard,
    /// MkDocs flavor with auto-reference support
    #[serde(rename = "mkdocs")]
    MkDocs,
    /// MDX flavor with JSX and ESM support (.mdx files)
    #[serde(rename = "mdx")]
    MDX,
    /// Quarto/RMarkdown flavor for scientific publishing (.qmd, .Rmd files)
    #[serde(rename = "quarto")]
    Quarto,
    // Future flavors can be added here when they have actual implementation differences
    // Planned: GFM (GitHub Flavored Markdown) - for GitHub-specific features like tables, strikethrough
    // Planned: CommonMark - for strict CommonMark compliance
}

impl fmt::Display for MarkdownFlavor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarkdownFlavor::Standard => write!(f, "standard"),
            MarkdownFlavor::MkDocs => write!(f, "mkdocs"),
            MarkdownFlavor::MDX => write!(f, "mdx"),
            MarkdownFlavor::Quarto => write!(f, "quarto"),
        }
    }
}

impl FromStr for MarkdownFlavor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "standard" | "" | "none" => Ok(MarkdownFlavor::Standard),
            "mkdocs" => Ok(MarkdownFlavor::MkDocs),
            "mdx" => Ok(MarkdownFlavor::MDX),
            "quarto" | "qmd" | "rmd" | "rmarkdown" => Ok(MarkdownFlavor::Quarto),
            // GFM and CommonMark are aliases for Standard since the base parser
            // (pulldown-cmark) already supports GFM extensions (tables, task lists,
            // strikethrough, autolinks, etc.) which are a superset of CommonMark
            "gfm" | "github" | "commonmark" => Ok(MarkdownFlavor::Standard),
            _ => Err(format!("Unknown markdown flavor: {s}")),
        }
    }
}

impl MarkdownFlavor {
    /// Detect flavor from file extension
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "mdx" => Self::MDX,
            "qmd" => Self::Quarto,
            "rmd" => Self::Quarto,
            _ => Self::Standard,
        }
    }

    /// Detect flavor from file path
    pub fn from_path(path: &std::path::Path) -> Self {
        path.extension()
            .and_then(|e| e.to_str())
            .map(Self::from_extension)
            .unwrap_or(Self::Standard)
    }

    /// Check if this flavor supports ESM imports/exports (MDX-specific)
    pub fn supports_esm_blocks(self) -> bool {
        matches!(self, Self::MDX)
    }

    /// Check if this flavor supports JSX components (MDX-specific)
    pub fn supports_jsx(self) -> bool {
        matches!(self, Self::MDX)
    }

    /// Check if this flavor supports auto-references (MkDocs-specific)
    pub fn supports_auto_references(self) -> bool {
        matches!(self, Self::MkDocs)
    }

    /// Get a human-readable name for this flavor
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::MkDocs => "MkDocs",
            Self::MDX => "MDX",
            Self::Quarto => "Quarto",
        }
    }
}

/// Normalizes configuration keys (rule names, option names) to lowercase kebab-case.
pub fn normalize_key(key: &str) -> String {
    // If the key looks like a rule name (e.g., MD013), uppercase it
    if key.len() == 5 && key.to_ascii_lowercase().starts_with("md") && key[2..].chars().all(|c| c.is_ascii_digit()) {
        key.to_ascii_uppercase()
    } else {
        key.replace('_', "-").to_ascii_lowercase()
    }
}

/// Represents a rule-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, schemars::JsonSchema)]
pub struct RuleConfig {
    /// Severity override for this rule (Error, Warning, or Info)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<crate::rule::Severity>,

    /// Configuration values for the rule
    #[serde(flatten)]
    #[schemars(schema_with = "arbitrary_value_schema")]
    pub values: BTreeMap<String, toml::Value>,
}

/// Generate a JSON schema for arbitrary configuration values
fn arbitrary_value_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": true
    })
}

/// Represents the complete configuration loaded from rumdl.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, schemars::JsonSchema)]
#[schemars(
    description = "rumdl configuration for linting Markdown files. Rules can be configured individually using [MD###] sections with rule-specific options."
)]
pub struct Config {
    /// Global configuration options
    #[serde(default)]
    pub global: GlobalConfig,

    /// Per-file rule ignores: maps file patterns to lists of rules to ignore
    /// Example: { "README.md": ["MD033"], "docs/**/*.md": ["MD013"] }
    #[serde(default, rename = "per-file-ignores")]
    pub per_file_ignores: HashMap<String, Vec<String>>,

    /// Rule-specific configurations (e.g., MD013, MD007, MD044)
    /// Each rule section can contain options specific to that rule.
    ///
    /// Common examples:
    /// - MD013: line_length, code_blocks, tables, headings
    /// - MD007: indent
    /// - MD003: style ("atx", "atx_closed", "setext")
    /// - MD044: names (array of proper names to check)
    ///
    /// See https://github.com/rvben/rumdl for full rule documentation.
    #[serde(flatten)]
    pub rules: BTreeMap<String, RuleConfig>,

    /// Project root directory, used for resolving relative paths in per-file-ignores
    #[serde(skip)]
    pub project_root: Option<std::path::PathBuf>,
}

impl Config {
    /// Check if the Markdown flavor is set to MkDocs
    pub fn is_mkdocs_flavor(&self) -> bool {
        self.global.flavor == MarkdownFlavor::MkDocs
    }

    // Future methods for when GFM and CommonMark are implemented:
    // pub fn is_gfm_flavor(&self) -> bool
    // pub fn is_commonmark_flavor(&self) -> bool

    /// Get the configured Markdown flavor
    pub fn markdown_flavor(&self) -> MarkdownFlavor {
        self.global.flavor
    }

    /// Legacy method for backwards compatibility - redirects to is_mkdocs_flavor
    pub fn is_mkdocs_project(&self) -> bool {
        self.is_mkdocs_flavor()
    }

    /// Get the severity override for a specific rule, if configured
    pub fn get_rule_severity(&self, rule_name: &str) -> Option<crate::rule::Severity> {
        self.rules.get(rule_name).and_then(|r| r.severity)
    }

    /// Get the set of rules that should be ignored for a specific file based on per-file-ignores configuration
    /// Returns a HashSet of rule names (uppercase, e.g., "MD033") that match the given file path
    pub fn get_ignored_rules_for_file(&self, file_path: &Path) -> HashSet<String> {
        use globset::{Glob, GlobSetBuilder};

        let mut ignored_rules = HashSet::new();

        if self.per_file_ignores.is_empty() {
            return ignored_rules;
        }

        // Normalize the file path to be relative to project_root for pattern matching
        // This ensures patterns like ".github/file.md" work with absolute paths
        let path_for_matching: std::borrow::Cow<'_, Path> = if let Some(ref root) = self.project_root {
            if let Ok(canonical_path) = file_path.canonicalize() {
                if let Ok(canonical_root) = root.canonicalize() {
                    if let Ok(relative) = canonical_path.strip_prefix(&canonical_root) {
                        std::borrow::Cow::Owned(relative.to_path_buf())
                    } else {
                        std::borrow::Cow::Borrowed(file_path)
                    }
                } else {
                    std::borrow::Cow::Borrowed(file_path)
                }
            } else {
                std::borrow::Cow::Borrowed(file_path)
            }
        } else {
            std::borrow::Cow::Borrowed(file_path)
        };

        // Build a globset for efficient matching
        let mut builder = GlobSetBuilder::new();
        let mut pattern_to_rules: Vec<(usize, &Vec<String>)> = Vec::new();

        for (idx, (pattern, rules)) in self.per_file_ignores.iter().enumerate() {
            if let Ok(glob) = Glob::new(pattern) {
                builder.add(glob);
                pattern_to_rules.push((idx, rules));
            } else {
                log::warn!("Invalid glob pattern in per-file-ignores: {pattern}");
            }
        }

        let globset = match builder.build() {
            Ok(gs) => gs,
            Err(e) => {
                log::error!("Failed to build globset for per-file-ignores: {e}");
                return ignored_rules;
            }
        };

        // Match the file path against all patterns
        for match_idx in globset.matches(path_for_matching.as_ref()) {
            if let Some((_, rules)) = pattern_to_rules.get(match_idx) {
                for rule in rules.iter() {
                    // Normalize rule names to uppercase (MD033, md033 -> MD033)
                    ignored_rules.insert(normalize_key(rule));
                }
            }
        }

        ignored_rules
    }
}

/// Global configuration options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(default, rename_all = "kebab-case")]
pub struct GlobalConfig {
    /// Enabled rules
    #[serde(default)]
    pub enable: Vec<String>,

    /// Disabled rules
    #[serde(default)]
    pub disable: Vec<String>,

    /// Files to exclude
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Files to include
    #[serde(default)]
    pub include: Vec<String>,

    /// Respect .gitignore files when scanning directories
    #[serde(default = "default_respect_gitignore", alias = "respect_gitignore")]
    pub respect_gitignore: bool,

    /// Global line length setting (used by MD013 and other rules if not overridden)
    #[serde(default, alias = "line_length")]
    pub line_length: LineLength,

    /// Output format for linting results (e.g., "text", "json", "pylint", etc.)
    #[serde(skip_serializing_if = "Option::is_none", alias = "output_format")]
    pub output_format: Option<String>,

    /// Rules that are allowed to be fixed when --fix is used
    /// If specified, only these rules will be fixed
    #[serde(default)]
    pub fixable: Vec<String>,

    /// Rules that should never be fixed, even when --fix is used
    /// Takes precedence over fixable
    #[serde(default)]
    pub unfixable: Vec<String>,

    /// Markdown flavor/dialect to use (mkdocs, gfm, commonmark, etc.)
    /// When set, adjusts parsing and validation rules for that specific Markdown variant
    #[serde(default)]
    pub flavor: MarkdownFlavor,

    /// [DEPRECATED] Whether to enforce exclude patterns for explicitly passed paths.
    /// This option is deprecated as of v0.0.156 and has no effect.
    /// Exclude patterns are now always respected, even for explicitly provided files.
    /// This prevents duplication between rumdl config and tool configs like pre-commit.
    #[serde(default, alias = "force_exclude")]
    #[deprecated(since = "0.0.156", note = "Exclude patterns are now always respected")]
    pub force_exclude: bool,

    /// Directory to store cache files (default: .rumdl_cache)
    /// Can also be set via --cache-dir CLI flag or RUMDL_CACHE_DIR environment variable
    #[serde(default, alias = "cache_dir", skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<String>,

    /// Whether caching is enabled (default: true)
    /// Can also be disabled via --no-cache CLI flag
    #[serde(default = "default_true")]
    pub cache: bool,
}

fn default_respect_gitignore() -> bool {
    true
}

fn default_true() -> bool {
    true
}

// Add the Default impl
impl Default for GlobalConfig {
    #[allow(deprecated)]
    fn default() -> Self {
        Self {
            enable: Vec::new(),
            disable: Vec::new(),
            exclude: Vec::new(),
            include: Vec::new(),
            respect_gitignore: true,
            line_length: LineLength::default(),
            output_format: None,
            fixable: Vec::new(),
            unfixable: Vec::new(),
            flavor: MarkdownFlavor::default(),
            force_exclude: false,
            cache_dir: None,
            cache: true,
        }
    }
}

const MARKDOWNLINT_CONFIG_FILES: &[&str] = &[
    ".markdownlint.json",
    ".markdownlint.jsonc",
    ".markdownlint.yaml",
    ".markdownlint.yml",
    "markdownlint.json",
    "markdownlint.jsonc",
    "markdownlint.yaml",
    "markdownlint.yml",
];

/// Create a default configuration file at the specified path
pub fn create_default_config(path: &str) -> Result<(), ConfigError> {
    // Check if file already exists
    if Path::new(path).exists() {
        return Err(ConfigError::FileExists { path: path.to_string() });
    }

    // Default configuration content
    let default_config = r#"# rumdl configuration file

# Global configuration options
[global]
# List of rules to disable (uncomment and modify as needed)
# disable = ["MD013", "MD033"]

# List of rules to enable exclusively (if provided, only these rules will run)
# enable = ["MD001", "MD003", "MD004"]

# List of file/directory patterns to include for linting (if provided, only these will be linted)
# include = [
#    "docs/*.md",
#    "src/**/*.md",
#    "README.md"
# ]

# List of file/directory patterns to exclude from linting
exclude = [
    # Common directories to exclude
    ".git",
    ".github",
    "node_modules",
    "vendor",
    "dist",
    "build",

    # Specific files or patterns
    "CHANGELOG.md",
    "LICENSE.md",
]

# Respect .gitignore files when scanning directories (default: true)
respect-gitignore = true

# Markdown flavor/dialect (uncomment to enable)
# Options: standard (default), gfm, commonmark, mkdocs, mdx, quarto
# flavor = "mkdocs"

# Rule-specific configurations (uncomment and modify as needed)

# [MD003]
# style = "atx"  # Heading style (atx, atx_closed, setext)

# [MD004]
# style = "asterisk"  # Unordered list style (asterisk, plus, dash, consistent)

# [MD007]
# indent = 4  # Unordered list indentation

# [MD013]
# line-length = 100  # Line length
# code-blocks = false  # Exclude code blocks from line length check
# tables = false  # Exclude tables from line length check
# headings = true  # Include headings in line length check

# [MD044]
# names = ["rumdl", "Markdown", "GitHub"]  # Proper names that should be capitalized correctly
# code-blocks = false  # Check code blocks for proper names (default: false, skips code blocks)
"#;

    // Write the default configuration to the file
    match fs::write(path, default_config) {
        Ok(_) => Ok(()),
        Err(err) => Err(ConfigError::IoError {
            source: err,
            path: path.to_string(),
        }),
    }
}

/// Errors that can occur when loading configuration
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read the configuration file
    #[error("Failed to read config file at {path}: {source}")]
    IoError { source: io::Error, path: String },

    /// Failed to parse the configuration content (TOML or JSON)
    #[error("Failed to parse config: {0}")]
    ParseError(String),

    /// Configuration file already exists
    #[error("Configuration file already exists at {path}")]
    FileExists { path: String },
}

/// Get a rule-specific configuration value
/// Automatically tries both the original key and normalized variants (kebab-case ↔ snake_case)
/// for better markdownlint compatibility
pub fn get_rule_config_value<T: serde::de::DeserializeOwned>(config: &Config, rule_name: &str, key: &str) -> Option<T> {
    let norm_rule_name = rule_name.to_ascii_uppercase(); // Use uppercase for lookup

    let rule_config = config.rules.get(&norm_rule_name)?;

    // Try multiple key variants to support both underscore and kebab-case formats
    let key_variants = [
        key.to_string(),       // Original key as provided
        normalize_key(key),    // Normalized key (lowercase, kebab-case)
        key.replace('-', "_"), // Convert kebab-case to snake_case
        key.replace('_', "-"), // Convert snake_case to kebab-case
    ];

    // Try each variant until we find a match
    for variant in &key_variants {
        if let Some(value) = rule_config.values.get(variant)
            && let Ok(result) = T::deserialize(value.clone())
        {
            return Some(result);
        }
    }

    None
}

/// Generate default rumdl configuration for pyproject.toml
pub fn generate_pyproject_config() -> String {
    let config_content = r#"
[tool.rumdl]
# Global configuration options
line-length = 100
disable = []
exclude = [
    # Common directories to exclude
    ".git",
    ".github",
    "node_modules",
    "vendor",
    "dist",
    "build",
]
respect-gitignore = true

# Rule-specific configurations (uncomment and modify as needed)

# [tool.rumdl.MD003]
# style = "atx"  # Heading style (atx, atx_closed, setext)

# [tool.rumdl.MD004]
# style = "asterisk"  # Unordered list style (asterisk, plus, dash, consistent)

# [tool.rumdl.MD007]
# indent = 4  # Unordered list indentation

# [tool.rumdl.MD013]
# line-length = 100  # Line length
# code-blocks = false  # Exclude code blocks from line length check
# tables = false  # Exclude tables from line length check
# headings = true  # Include headings in line length check

# [tool.rumdl.MD044]
# names = ["rumdl", "Markdown", "GitHub"]  # Proper names that should be capitalized correctly
# code-blocks = false  # Check code blocks for proper names (default: false, skips code blocks)
"#;

    config_content.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_flavor_loading() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_content = r#"
[global]
flavor = "mkdocs"
disable = ["MD001"]
"#;
        fs::write(&config_path, config_content).unwrap();

        // Load the config
        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // Check that flavor was loaded
        assert_eq!(config.global.flavor, MarkdownFlavor::MkDocs);
        assert!(config.is_mkdocs_flavor());
        assert!(config.is_mkdocs_project()); // Test backwards compatibility
        assert_eq!(config.global.disable, vec!["MD001".to_string()]);
    }

    #[test]
    fn test_pyproject_toml_root_level_config() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("pyproject.toml");

        // Create a test pyproject.toml with root-level configuration
        let content = r#"
[tool.rumdl]
line-length = 120
disable = ["MD033"]
enable = ["MD001", "MD004"]
include = ["docs/*.md"]
exclude = ["node_modules"]
respect-gitignore = true
        "#;

        fs::write(&config_path, content).unwrap();

        // Load the config with skip_auto_discovery to avoid environment config files
        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into(); // Convert to plain config for assertions

        // Check global settings
        assert_eq!(config.global.disable, vec!["MD033".to_string()]);
        assert_eq!(config.global.enable, vec!["MD001".to_string(), "MD004".to_string()]);
        // Should now contain only the configured pattern since auto-discovery is disabled
        assert_eq!(config.global.include, vec!["docs/*.md".to_string()]);
        assert_eq!(config.global.exclude, vec!["node_modules".to_string()]);
        assert!(config.global.respect_gitignore);

        // Check line-length was correctly added to MD013
        let line_length = get_rule_config_value::<usize>(&config, "MD013", "line-length");
        assert_eq!(line_length, Some(120));
    }

    #[test]
    fn test_pyproject_toml_snake_case_and_kebab_case() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("pyproject.toml");

        // Test with both kebab-case and snake_case variants
        let content = r#"
[tool.rumdl]
line-length = 150
respect_gitignore = true
        "#;

        fs::write(&config_path, content).unwrap();

        // Load the config with skip_auto_discovery to avoid environment config files
        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into(); // Convert to plain config for assertions

        // Check settings were correctly loaded
        assert!(config.global.respect_gitignore);
        let line_length = get_rule_config_value::<usize>(&config, "MD013", "line-length");
        assert_eq!(line_length, Some(150));
    }

    #[test]
    fn test_md013_key_normalization_in_rumdl_toml() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_content = r#"
[MD013]
line_length = 111
line-length = 222
"#;
        fs::write(&config_path, config_content).unwrap();
        // Load the config with skip_auto_discovery to avoid environment config files
        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let rule_cfg = sourced.rules.get("MD013").expect("MD013 rule config should exist");
        // Now we should only get the explicitly configured key
        let keys: Vec<_> = rule_cfg.values.keys().cloned().collect();
        assert_eq!(keys, vec!["line-length"]);
        let val = &rule_cfg.values["line-length"].value;
        assert_eq!(val.as_integer(), Some(222));
        // get_rule_config_value should retrieve the value for both snake_case and kebab-case
        let config: Config = sourced.clone().into_validated_unchecked().into();
        let v1 = get_rule_config_value::<usize>(&config, "MD013", "line_length");
        let v2 = get_rule_config_value::<usize>(&config, "MD013", "line-length");
        assert_eq!(v1, Some(222));
        assert_eq!(v2, Some(222));
    }

    #[test]
    fn test_md013_section_case_insensitivity() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_content = r#"
[md013]
line-length = 101

[Md013]
line-length = 102

[MD013]
line-length = 103
"#;
        fs::write(&config_path, config_content).unwrap();
        // Load the config with skip_auto_discovery to avoid environment config files
        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.clone().into_validated_unchecked().into();
        // Only the last section should win, and be present
        let rule_cfg = sourced.rules.get("MD013").expect("MD013 rule config should exist");
        let keys: Vec<_> = rule_cfg.values.keys().cloned().collect();
        assert_eq!(keys, vec!["line-length"]);
        let val = &rule_cfg.values["line-length"].value;
        assert_eq!(val.as_integer(), Some(103));
        let v = get_rule_config_value::<usize>(&config, "MD013", "line-length");
        assert_eq!(v, Some(103));
    }

    #[test]
    fn test_md013_key_snake_and_kebab_case() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_content = r#"
[MD013]
line_length = 201
line-length = 202
"#;
        fs::write(&config_path, config_content).unwrap();
        // Load the config with skip_auto_discovery to avoid environment config files
        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.clone().into_validated_unchecked().into();
        let rule_cfg = sourced.rules.get("MD013").expect("MD013 rule config should exist");
        let keys: Vec<_> = rule_cfg.values.keys().cloned().collect();
        assert_eq!(keys, vec!["line-length"]);
        let val = &rule_cfg.values["line-length"].value;
        assert_eq!(val.as_integer(), Some(202));
        let v1 = get_rule_config_value::<usize>(&config, "MD013", "line_length");
        let v2 = get_rule_config_value::<usize>(&config, "MD013", "line-length");
        assert_eq!(v1, Some(202));
        assert_eq!(v2, Some(202));
    }

    #[test]
    fn test_unknown_rule_section_is_ignored() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_content = r#"
[MD999]
foo = 1
bar = 2
[MD013]
line-length = 303
"#;
        fs::write(&config_path, config_content).unwrap();
        // Load the config with skip_auto_discovery to avoid environment config files
        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.clone().into_validated_unchecked().into();
        // MD999 should not be present
        assert!(!sourced.rules.contains_key("MD999"));
        // MD013 should be present and correct
        let v = get_rule_config_value::<usize>(&config, "MD013", "line-length");
        assert_eq!(v, Some(303));
    }

    #[test]
    fn test_invalid_toml_syntax() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        // Invalid TOML with unclosed string
        let config_content = r#"
[MD013]
line-length = "unclosed string
"#;
        fs::write(&config_path, config_content).unwrap();

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::ParseError(msg) => {
                // The actual error message from toml parser might vary
                assert!(msg.contains("expected") || msg.contains("invalid") || msg.contains("unterminated"));
            }
            _ => panic!("Expected ParseError"),
        }
    }

    #[test]
    fn test_wrong_type_for_config_value() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        // line-length should be a number, not a string
        let config_content = r#"
[MD013]
line-length = "not a number"
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // The value should be loaded as a string, not converted
        let rule_config = config.rules.get("MD013").unwrap();
        let value = rule_config.values.get("line-length").unwrap();
        assert!(matches!(value, toml::Value::String(_)));
    }

    #[test]
    fn test_empty_config_file() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        // Empty file
        fs::write(&config_path, "").unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // Should have default values
        assert_eq!(config.global.line_length.get(), 80);
        assert!(config.global.respect_gitignore);
        assert!(config.rules.is_empty());
    }

    #[test]
    fn test_malformed_pyproject_toml() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("pyproject.toml");

        // Missing closing bracket
        let content = r#"
[tool.rumdl
line-length = 120
"#;
        fs::write(&config_path, content).unwrap();

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_conflicting_config_values() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        // Both enable and disable the same rule - these need to be in a global section
        let config_content = r#"
[global]
enable = ["MD013"]
disable = ["MD013"]
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // Conflict resolution: enable wins over disable
        assert!(config.global.enable.contains(&"MD013".to_string()));
        assert!(!config.global.disable.contains(&"MD013".to_string()));
    }

    #[test]
    fn test_invalid_rule_names() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        let config_content = r#"
[global]
enable = ["MD001", "NOT_A_RULE", "md002", "12345"]
disable = ["MD-001", "MD_002"]
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // All values should be preserved as-is
        assert_eq!(config.global.enable.len(), 4);
        assert_eq!(config.global.disable.len(), 2);
    }

    #[test]
    fn test_deeply_nested_config() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        // This should be ignored as we don't support nested tables within rule configs
        let config_content = r#"
[MD013]
line-length = 100
[MD013.nested]
value = 42
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        let rule_config = config.rules.get("MD013").unwrap();
        assert_eq!(
            rule_config.values.get("line-length").unwrap(),
            &toml::Value::Integer(100)
        );
        // Nested table should not be present
        assert!(!rule_config.values.contains_key("nested"));
    }

    #[test]
    fn test_unicode_in_config() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        let config_content = r#"
[global]
include = ["文档/*.md", "ドキュメント/*.md"]
exclude = ["测试/*", "🚀/*"]

[MD013]
line-length = 80
message = "行太长了 🚨"
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        assert_eq!(config.global.include.len(), 2);
        assert_eq!(config.global.exclude.len(), 2);
        assert!(config.global.include[0].contains("文档"));
        assert!(config.global.exclude[1].contains("🚀"));

        let rule_config = config.rules.get("MD013").unwrap();
        let message = rule_config.values.get("message").unwrap();
        if let toml::Value::String(s) = message {
            assert!(s.contains("行太长了"));
            assert!(s.contains("🚨"));
        }
    }

    #[test]
    fn test_extremely_long_values() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        let long_string = "a".repeat(10000);
        let config_content = format!(
            r#"
[global]
exclude = ["{long_string}"]

[MD013]
line-length = 999999999
"#
        );

        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        assert_eq!(config.global.exclude[0].len(), 10000);
        let line_length = get_rule_config_value::<usize>(&config, "MD013", "line-length");
        assert_eq!(line_length, Some(999999999));
    }

    #[test]
    fn test_config_with_comments() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        let config_content = r#"
[global]
# This is a comment
enable = ["MD001"] # Enable MD001
# disable = ["MD002"] # This is commented out

[MD013] # Line length rule
line-length = 100 # Set to 100 characters
# ignored = true # This setting is commented out
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        assert_eq!(config.global.enable, vec!["MD001"]);
        assert!(config.global.disable.is_empty()); // Commented out

        let rule_config = config.rules.get("MD013").unwrap();
        assert_eq!(rule_config.values.len(), 1); // Only line-length
        assert!(!rule_config.values.contains_key("ignored"));
    }

    #[test]
    fn test_arrays_in_rule_config() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        let config_content = r#"
[MD003]
levels = [1, 2, 3]
tags = ["important", "critical"]
mixed = [1, "two", true]
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // Arrays should now be properly parsed
        let rule_config = config.rules.get("MD003").expect("MD003 config should exist");

        // Check that arrays are present and correctly parsed
        assert!(rule_config.values.contains_key("levels"));
        assert!(rule_config.values.contains_key("tags"));
        assert!(rule_config.values.contains_key("mixed"));

        // Verify array contents
        if let Some(toml::Value::Array(levels)) = rule_config.values.get("levels") {
            assert_eq!(levels.len(), 3);
            assert_eq!(levels[0], toml::Value::Integer(1));
            assert_eq!(levels[1], toml::Value::Integer(2));
            assert_eq!(levels[2], toml::Value::Integer(3));
        } else {
            panic!("levels should be an array");
        }

        if let Some(toml::Value::Array(tags)) = rule_config.values.get("tags") {
            assert_eq!(tags.len(), 2);
            assert_eq!(tags[0], toml::Value::String("important".to_string()));
            assert_eq!(tags[1], toml::Value::String("critical".to_string()));
        } else {
            panic!("tags should be an array");
        }

        if let Some(toml::Value::Array(mixed)) = rule_config.values.get("mixed") {
            assert_eq!(mixed.len(), 3);
            assert_eq!(mixed[0], toml::Value::Integer(1));
            assert_eq!(mixed[1], toml::Value::String("two".to_string()));
            assert_eq!(mixed[2], toml::Value::Boolean(true));
        } else {
            panic!("mixed should be an array");
        }
    }

    #[test]
    fn test_normalize_key_edge_cases() {
        // Rule names
        assert_eq!(normalize_key("MD001"), "MD001");
        assert_eq!(normalize_key("md001"), "MD001");
        assert_eq!(normalize_key("Md001"), "MD001");
        assert_eq!(normalize_key("mD001"), "MD001");

        // Non-rule names
        assert_eq!(normalize_key("line_length"), "line-length");
        assert_eq!(normalize_key("line-length"), "line-length");
        assert_eq!(normalize_key("LINE_LENGTH"), "line-length");
        assert_eq!(normalize_key("respect_gitignore"), "respect-gitignore");

        // Edge cases
        assert_eq!(normalize_key("MD"), "md"); // Too short to be a rule
        assert_eq!(normalize_key("MD00"), "md00"); // Too short
        assert_eq!(normalize_key("MD0001"), "md0001"); // Too long
        assert_eq!(normalize_key("MDabc"), "mdabc"); // Non-digit
        assert_eq!(normalize_key("MD00a"), "md00a"); // Partial digit
        assert_eq!(normalize_key(""), "");
        assert_eq!(normalize_key("_"), "-");
        assert_eq!(normalize_key("___"), "---");
    }

    #[test]
    fn test_missing_config_file() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("nonexistent.toml");

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::IoError { .. } => {}
            _ => panic!("Expected IoError for missing file"),
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_permission_denied_config() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        fs::write(&config_path, "enable = [\"MD001\"]").unwrap();

        // Remove read permissions
        let mut perms = fs::metadata(&config_path).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&config_path, perms).unwrap();

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);

        // Restore permissions for cleanup
        let mut perms = fs::metadata(&config_path).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&config_path, perms).unwrap();

        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::IoError { .. } => {}
            _ => panic!("Expected IoError for permission denied"),
        }
    }

    #[test]
    fn test_circular_reference_detection() {
        // This test is more conceptual since TOML doesn't support circular references
        // But we test that deeply nested structures don't cause stack overflow
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        let mut config_content = String::from("[MD001]\n");
        for i in 0..100 {
            config_content.push_str(&format!("key{i} = {i}\n"));
        }

        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        let rule_config = config.rules.get("MD001").unwrap();
        assert_eq!(rule_config.values.len(), 100);
    }

    #[test]
    fn test_special_toml_values() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        let config_content = r#"
[MD001]
infinity = inf
neg_infinity = -inf
not_a_number = nan
datetime = 1979-05-27T07:32:00Z
local_date = 1979-05-27
local_time = 07:32:00
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // Some values might not be parsed due to parser limitations
        if let Some(rule_config) = config.rules.get("MD001") {
            // Check special float values if present
            if let Some(toml::Value::Float(f)) = rule_config.values.get("infinity") {
                assert!(f.is_infinite() && f.is_sign_positive());
            }
            if let Some(toml::Value::Float(f)) = rule_config.values.get("neg_infinity") {
                assert!(f.is_infinite() && f.is_sign_negative());
            }
            if let Some(toml::Value::Float(f)) = rule_config.values.get("not_a_number") {
                assert!(f.is_nan());
            }

            // Check datetime values if present
            if let Some(val) = rule_config.values.get("datetime") {
                assert!(matches!(val, toml::Value::Datetime(_)));
            }
            // Note: local_date and local_time might not be parsed by the current implementation
        }
    }

    #[test]
    fn test_default_config_passes_validation() {
        use crate::rules;

        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_path_str = config_path.to_str().unwrap();

        // Create the default config using the same function that `rumdl init` uses
        create_default_config(config_path_str).unwrap();

        // Load it back as a SourcedConfig
        let sourced =
            SourcedConfig::load(Some(config_path_str), None).expect("Default config should load successfully");

        // Create the rule registry
        let all_rules = rules::all_rules(&Config::default());
        let registry = RuleRegistry::from_rules(&all_rules);

        // Validate the config
        let warnings = validate_config_sourced(&sourced, &registry);

        // The default config should have no warnings
        if !warnings.is_empty() {
            for warning in &warnings {
                eprintln!("Config validation warning: {}", warning.message);
                if let Some(rule) = &warning.rule {
                    eprintln!("  Rule: {rule}");
                }
                if let Some(key) = &warning.key {
                    eprintln!("  Key: {key}");
                }
            }
        }
        assert!(
            warnings.is_empty(),
            "Default config from rumdl init should pass validation without warnings"
        );
    }

    #[test]
    fn test_per_file_ignores_config_parsing() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_content = r#"
[per-file-ignores]
"README.md" = ["MD033"]
"docs/**/*.md" = ["MD013", "MD033"]
"test/*.md" = ["MD041"]
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // Verify per-file-ignores was loaded
        assert_eq!(config.per_file_ignores.len(), 3);
        assert_eq!(
            config.per_file_ignores.get("README.md"),
            Some(&vec!["MD033".to_string()])
        );
        assert_eq!(
            config.per_file_ignores.get("docs/**/*.md"),
            Some(&vec!["MD013".to_string(), "MD033".to_string()])
        );
        assert_eq!(
            config.per_file_ignores.get("test/*.md"),
            Some(&vec!["MD041".to_string()])
        );
    }

    #[test]
    fn test_per_file_ignores_glob_matching() {
        use std::path::PathBuf;

        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_content = r#"
[per-file-ignores]
"README.md" = ["MD033"]
"docs/**/*.md" = ["MD013"]
"**/test_*.md" = ["MD041"]
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // Test exact match
        let ignored = config.get_ignored_rules_for_file(&PathBuf::from("README.md"));
        assert!(ignored.contains("MD033"));
        assert_eq!(ignored.len(), 1);

        // Test glob pattern matching
        let ignored = config.get_ignored_rules_for_file(&PathBuf::from("docs/api/overview.md"));
        assert!(ignored.contains("MD013"));
        assert_eq!(ignored.len(), 1);

        // Test recursive glob pattern
        let ignored = config.get_ignored_rules_for_file(&PathBuf::from("tests/fixtures/test_example.md"));
        assert!(ignored.contains("MD041"));
        assert_eq!(ignored.len(), 1);

        // Test non-matching path
        let ignored = config.get_ignored_rules_for_file(&PathBuf::from("other/file.md"));
        assert!(ignored.is_empty());
    }

    #[test]
    fn test_per_file_ignores_pyproject_toml() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("pyproject.toml");
        let config_content = r#"
[tool.rumdl]
[tool.rumdl.per-file-ignores]
"README.md" = ["MD033", "MD013"]
"generated/*.md" = ["MD041"]
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // Verify per-file-ignores was loaded from pyproject.toml
        assert_eq!(config.per_file_ignores.len(), 2);
        assert_eq!(
            config.per_file_ignores.get("README.md"),
            Some(&vec!["MD033".to_string(), "MD013".to_string()])
        );
        assert_eq!(
            config.per_file_ignores.get("generated/*.md"),
            Some(&vec!["MD041".to_string()])
        );
    }

    #[test]
    fn test_per_file_ignores_multiple_patterns_match() {
        use std::path::PathBuf;

        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_content = r#"
[per-file-ignores]
"docs/**/*.md" = ["MD013"]
"**/api/*.md" = ["MD033"]
"docs/api/overview.md" = ["MD041"]
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // File matches multiple patterns - should get union of all rules
        let ignored = config.get_ignored_rules_for_file(&PathBuf::from("docs/api/overview.md"));
        assert_eq!(ignored.len(), 3);
        assert!(ignored.contains("MD013"));
        assert!(ignored.contains("MD033"));
        assert!(ignored.contains("MD041"));
    }

    #[test]
    fn test_per_file_ignores_rule_name_normalization() {
        use std::path::PathBuf;

        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_content = r#"
[per-file-ignores]
"README.md" = ["md033", "MD013", "Md041"]
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // All rule names should be normalized to uppercase
        let ignored = config.get_ignored_rules_for_file(&PathBuf::from("README.md"));
        assert_eq!(ignored.len(), 3);
        assert!(ignored.contains("MD033"));
        assert!(ignored.contains("MD013"));
        assert!(ignored.contains("MD041"));
    }

    #[test]
    fn test_per_file_ignores_invalid_glob_pattern() {
        use std::path::PathBuf;

        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_content = r#"
[per-file-ignores]
"[invalid" = ["MD033"]
"valid/*.md" = ["MD013"]
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // Invalid pattern should be skipped, valid pattern should work
        let ignored = config.get_ignored_rules_for_file(&PathBuf::from("valid/test.md"));
        assert!(ignored.contains("MD013"));

        // Invalid pattern should not cause issues
        let ignored2 = config.get_ignored_rules_for_file(&PathBuf::from("[invalid"));
        assert!(ignored2.is_empty());
    }

    #[test]
    fn test_per_file_ignores_empty_section() {
        use std::path::PathBuf;

        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_content = r#"
[global]
disable = ["MD001"]

[per-file-ignores]
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // Empty per-file-ignores should work fine
        assert_eq!(config.per_file_ignores.len(), 0);
        let ignored = config.get_ignored_rules_for_file(&PathBuf::from("README.md"));
        assert!(ignored.is_empty());
    }

    #[test]
    fn test_per_file_ignores_with_underscores_in_pyproject() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("pyproject.toml");
        let config_content = r#"
[tool.rumdl]
[tool.rumdl.per_file_ignores]
"README.md" = ["MD033"]
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // Should support both per-file-ignores and per_file_ignores
        assert_eq!(config.per_file_ignores.len(), 1);
        assert_eq!(
            config.per_file_ignores.get("README.md"),
            Some(&vec!["MD033".to_string()])
        );
    }

    #[test]
    fn test_per_file_ignores_absolute_path_matching() {
        // Regression test for issue #208: per-file-ignores should work with absolute paths
        // This is critical for GitHub Actions which uses absolute paths like $GITHUB_WORKSPACE
        use std::path::PathBuf;

        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");

        // Create a subdirectory and file to match against
        let github_dir = temp_dir.path().join(".github");
        fs::create_dir_all(&github_dir).unwrap();
        let test_file = github_dir.join("pull_request_template.md");
        fs::write(&test_file, "Test content").unwrap();

        let config_content = r#"
[per-file-ignores]
".github/pull_request_template.md" = ["MD041"]
"docs/**/*.md" = ["MD013"]
"#;
        fs::write(&config_path, config_content).unwrap();

        let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
        let config: Config = sourced.into_validated_unchecked().into();

        // Test with absolute path (like GitHub Actions would use)
        let absolute_path = test_file.canonicalize().unwrap();
        let ignored = config.get_ignored_rules_for_file(&absolute_path);
        assert!(
            ignored.contains("MD041"),
            "Should match absolute path {absolute_path:?} against relative pattern"
        );
        assert_eq!(ignored.len(), 1);

        // Also verify relative path still works
        let relative_path = PathBuf::from(".github/pull_request_template.md");
        let ignored = config.get_ignored_rules_for_file(&relative_path);
        assert!(ignored.contains("MD041"), "Should match relative path");
    }

    #[test]
    fn test_generate_json_schema() {
        use schemars::schema_for;
        use std::env;

        let schema = schema_for!(Config);
        let schema_json = serde_json::to_string_pretty(&schema).expect("Failed to serialize schema");

        // Write schema to file if RUMDL_UPDATE_SCHEMA env var is set
        if env::var("RUMDL_UPDATE_SCHEMA").is_ok() {
            let schema_path = env::current_dir().unwrap().join("rumdl.schema.json");
            fs::write(&schema_path, &schema_json).expect("Failed to write schema file");
            println!("Schema written to: {}", schema_path.display());
        }

        // Basic validation that schema was generated
        assert!(schema_json.contains("\"title\": \"Config\""));
        assert!(schema_json.contains("\"global\""));
        assert!(schema_json.contains("\"per-file-ignores\""));
    }

    #[test]
    fn test_project_config_is_standalone() {
        // Ruff model: Project config is standalone, user config is NOT merged
        // This ensures reproducibility across machines and CI/local consistency
        let temp_dir = tempdir().unwrap();

        // Create a fake user config directory
        // Note: user_configuration_path_impl adds /rumdl to the config dir
        let user_config_dir = temp_dir.path().join("user_config");
        let rumdl_config_dir = user_config_dir.join("rumdl");
        fs::create_dir_all(&rumdl_config_dir).unwrap();
        let user_config_path = rumdl_config_dir.join("rumdl.toml");

        // User config disables MD013 and MD041
        let user_config_content = r#"
[global]
disable = ["MD013", "MD041"]
line-length = 100
"#;
        fs::write(&user_config_path, user_config_content).unwrap();

        // Create a project config that enables MD001
        let project_config_path = temp_dir.path().join("project").join("pyproject.toml");
        fs::create_dir_all(project_config_path.parent().unwrap()).unwrap();
        let project_config_content = r#"
[tool.rumdl]
enable = ["MD001"]
"#;
        fs::write(&project_config_path, project_config_content).unwrap();

        // Load config with explicit project path, passing user_config_dir
        let sourced = SourcedConfig::load_with_discovery_impl(
            Some(project_config_path.to_str().unwrap()),
            None,
            false,
            Some(&user_config_dir),
        )
        .unwrap();

        let config: Config = sourced.into_validated_unchecked().into();

        // User config settings should NOT be present (Ruff model: project is standalone)
        assert!(
            !config.global.disable.contains(&"MD013".to_string()),
            "User config should NOT be merged with project config"
        );
        assert!(
            !config.global.disable.contains(&"MD041".to_string()),
            "User config should NOT be merged with project config"
        );

        // Project config settings should be applied
        assert!(
            config.global.enable.contains(&"MD001".to_string()),
            "Project config enabled rules should be applied"
        );
    }

    #[test]
    fn test_user_config_as_fallback_when_no_project_config() {
        // Ruff model: User config is used as fallback when no project config exists
        use std::env;

        let temp_dir = tempdir().unwrap();
        let original_dir = env::current_dir().unwrap();

        // Create a fake user config directory
        let user_config_dir = temp_dir.path().join("user_config");
        let rumdl_config_dir = user_config_dir.join("rumdl");
        fs::create_dir_all(&rumdl_config_dir).unwrap();
        let user_config_path = rumdl_config_dir.join("rumdl.toml");

        // User config with specific settings
        let user_config_content = r#"
[global]
disable = ["MD013", "MD041"]
line-length = 88
"#;
        fs::write(&user_config_path, user_config_content).unwrap();

        // Create a project directory WITHOUT any config
        let project_dir = temp_dir.path().join("project_no_config");
        fs::create_dir_all(&project_dir).unwrap();

        // Change to project directory
        env::set_current_dir(&project_dir).unwrap();

        // Load config - should use user config as fallback
        let sourced = SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir)).unwrap();

        let config: Config = sourced.into_validated_unchecked().into();

        // User config should be loaded as fallback
        assert!(
            config.global.disable.contains(&"MD013".to_string()),
            "User config should be loaded as fallback when no project config"
        );
        assert!(
            config.global.disable.contains(&"MD041".to_string()),
            "User config should be loaded as fallback when no project config"
        );
        assert_eq!(
            config.global.line_length.get(),
            88,
            "User config line-length should be loaded as fallback"
        );

        env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_typestate_validate_method() {
        use tempfile::tempdir;

        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let config_path = temp_dir.path().join("test.toml");

        // Create config with an unknown rule option to trigger a validation warning
        let config_content = r#"
[global]
enable = ["MD001"]

[MD013]
line_length = 80
unknown_option = true
"#;
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        // Load config - this returns SourcedConfig<ConfigLoaded>
        let loaded = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

        // Create a rule registry for validation
        let default_config = Config::default();
        let all_rules = crate::rules::all_rules(&default_config);
        let registry = RuleRegistry::from_rules(&all_rules);

        // Validate - this transitions to SourcedConfig<ConfigValidated>
        let validated = loaded.validate(&registry).expect("Should validate config");

        // Check that validation warnings were captured for the unknown option
        // Note: The validation checks rule options against the rule's schema
        let has_unknown_option_warning = validated
            .validation_warnings
            .iter()
            .any(|w| w.message.contains("unknown_option") || w.message.contains("Unknown option"));

        // Print warnings for debugging if assertion fails
        if !has_unknown_option_warning {
            for w in &validated.validation_warnings {
                eprintln!("Warning: {}", w.message);
            }
        }
        assert!(
            has_unknown_option_warning,
            "Should have warning for unknown option. Got {} warnings: {:?}",
            validated.validation_warnings.len(),
            validated
                .validation_warnings
                .iter()
                .map(|w| &w.message)
                .collect::<Vec<_>>()
        );

        // Now we can convert to Config (this would be a compile error with ConfigLoaded)
        let config: Config = validated.into();

        // Verify the config values are correct
        assert!(config.global.enable.contains(&"MD001".to_string()));
    }

    #[test]
    fn test_typestate_validate_into_convenience_method() {
        use tempfile::tempdir;

        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let config_path = temp_dir.path().join("test.toml");

        let config_content = r#"
[global]
enable = ["MD022"]

[MD022]
lines_above = 2
"#;
        std::fs::write(&config_path, config_content).expect("Failed to write config");

        let loaded = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

        let default_config = Config::default();
        let all_rules = crate::rules::all_rules(&default_config);
        let registry = RuleRegistry::from_rules(&all_rules);

        // Use the convenience method that validates and converts in one step
        let (config, warnings) = loaded.validate_into(&registry).expect("Should validate and convert");

        // Should have no warnings for valid config
        assert!(warnings.is_empty(), "Should have no warnings for valid config");

        // Config should be usable
        assert!(config.global.enable.contains(&"MD022".to_string()));
    }

    #[test]
    fn test_resolve_rule_name_canonical() {
        // Canonical IDs should resolve to themselves
        assert_eq!(resolve_rule_name("MD001"), "MD001");
        assert_eq!(resolve_rule_name("MD013"), "MD013");
        assert_eq!(resolve_rule_name("MD069"), "MD069");
    }

    #[test]
    fn test_resolve_rule_name_aliases() {
        // Aliases should resolve to canonical IDs
        assert_eq!(resolve_rule_name("heading-increment"), "MD001");
        assert_eq!(resolve_rule_name("line-length"), "MD013");
        assert_eq!(resolve_rule_name("no-bare-urls"), "MD034");
        assert_eq!(resolve_rule_name("ul-style"), "MD004");
    }

    #[test]
    fn test_resolve_rule_name_case_insensitive() {
        // Case should not matter
        assert_eq!(resolve_rule_name("HEADING-INCREMENT"), "MD001");
        assert_eq!(resolve_rule_name("Heading-Increment"), "MD001");
        assert_eq!(resolve_rule_name("md001"), "MD001");
        assert_eq!(resolve_rule_name("MD001"), "MD001");
    }

    #[test]
    fn test_resolve_rule_name_underscore_to_hyphen() {
        // Underscores should be converted to hyphens
        assert_eq!(resolve_rule_name("heading_increment"), "MD001");
        assert_eq!(resolve_rule_name("line_length"), "MD013");
        assert_eq!(resolve_rule_name("no_bare_urls"), "MD034");
    }

    #[test]
    fn test_resolve_rule_name_unknown() {
        // Unknown names should fall back to normalization
        assert_eq!(resolve_rule_name("custom-rule"), "custom-rule");
        assert_eq!(resolve_rule_name("CUSTOM_RULE"), "custom-rule");
        assert_eq!(resolve_rule_name("md999"), "MD999"); // Looks like an MD rule
    }

    #[test]
    fn test_resolve_rule_names_basic() {
        let result = resolve_rule_names("MD001,line-length,heading-increment");
        assert!(result.contains("MD001"));
        assert!(result.contains("MD013")); // line-length
        // Note: heading-increment also resolves to MD001, so set should contain MD001 and MD013
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_resolve_rule_names_with_whitespace() {
        let result = resolve_rule_names("  MD001 , line-length , MD034  ");
        assert!(result.contains("MD001"));
        assert!(result.contains("MD013"));
        assert!(result.contains("MD034"));
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_resolve_rule_names_empty_entries() {
        let result = resolve_rule_names("MD001,,MD013,");
        assert!(result.contains("MD001"));
        assert!(result.contains("MD013"));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_resolve_rule_names_empty_string() {
        let result = resolve_rule_names("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_resolve_rule_names_mixed() {
        // Mix of canonical IDs, aliases, and unknown
        let result = resolve_rule_names("MD001,line-length,custom-rule");
        assert!(result.contains("MD001"));
        assert!(result.contains("MD013"));
        assert!(result.contains("custom-rule"));
        assert_eq!(result.len(), 3);
    }
}

/// Configuration source with clear precedence hierarchy.
///
/// Precedence order (lower values override higher values):
/// - Default (0): Built-in defaults
/// - UserConfig (1): User-level ~/.config/rumdl/rumdl.toml
/// - PyprojectToml (2): Project-level pyproject.toml
/// - ProjectConfig (3): Project-level .rumdl.toml (most specific)
/// - Cli (4): Command-line flags (highest priority)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// Built-in default configuration
    Default,
    /// User-level configuration from ~/.config/rumdl/rumdl.toml
    UserConfig,
    /// Project-level configuration from pyproject.toml
    PyprojectToml,
    /// Project-level configuration from .rumdl.toml or rumdl.toml
    ProjectConfig,
    /// Command-line flags (highest precedence)
    Cli,
}

#[derive(Debug, Clone)]
pub struct ConfigOverride<T> {
    pub value: T,
    pub source: ConfigSource,
    pub file: Option<String>,
    pub line: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SourcedValue<T> {
    pub value: T,
    pub source: ConfigSource,
    pub overrides: Vec<ConfigOverride<T>>,
}

impl<T: Clone> SourcedValue<T> {
    pub fn new(value: T, source: ConfigSource) -> Self {
        Self {
            value: value.clone(),
            source,
            overrides: vec![ConfigOverride {
                value,
                source,
                file: None,
                line: None,
            }],
        }
    }

    /// Merges a new override into this SourcedValue based on source precedence.
    /// If the new source has higher or equal precedence, the value and source are updated,
    /// and the new override is added to the history.
    pub fn merge_override(
        &mut self,
        new_value: T,
        new_source: ConfigSource,
        new_file: Option<String>,
        new_line: Option<usize>,
    ) {
        // Helper function to get precedence, defined locally or globally
        fn source_precedence(src: ConfigSource) -> u8 {
            match src {
                ConfigSource::Default => 0,
                ConfigSource::UserConfig => 1,
                ConfigSource::PyprojectToml => 2,
                ConfigSource::ProjectConfig => 3,
                ConfigSource::Cli => 4,
            }
        }

        if source_precedence(new_source) >= source_precedence(self.source) {
            self.value = new_value.clone();
            self.source = new_source;
            self.overrides.push(ConfigOverride {
                value: new_value,
                source: new_source,
                file: new_file,
                line: new_line,
            });
        }
    }

    pub fn push_override(&mut self, value: T, source: ConfigSource, file: Option<String>, line: Option<usize>) {
        // This is essentially merge_override without the precedence check
        // We might consolidate these later, but keep separate for now during refactor
        self.value = value.clone();
        self.source = source;
        self.overrides.push(ConfigOverride {
            value,
            source,
            file,
            line,
        });
    }
}

impl<T: Clone + Eq + std::hash::Hash> SourcedValue<Vec<T>> {
    /// Merges a new value using union semantics (for arrays like `disable`)
    /// Values from both sources are combined, with deduplication
    pub fn merge_union(
        &mut self,
        new_value: Vec<T>,
        new_source: ConfigSource,
        new_file: Option<String>,
        new_line: Option<usize>,
    ) {
        fn source_precedence(src: ConfigSource) -> u8 {
            match src {
                ConfigSource::Default => 0,
                ConfigSource::UserConfig => 1,
                ConfigSource::PyprojectToml => 2,
                ConfigSource::ProjectConfig => 3,
                ConfigSource::Cli => 4,
            }
        }

        if source_precedence(new_source) >= source_precedence(self.source) {
            // Union: combine values from both sources with deduplication
            let mut combined = self.value.clone();
            for item in new_value.iter() {
                if !combined.contains(item) {
                    combined.push(item.clone());
                }
            }

            self.value = combined;
            self.source = new_source;
            self.overrides.push(ConfigOverride {
                value: new_value,
                source: new_source,
                file: new_file,
                line: new_line,
            });
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourcedGlobalConfig {
    pub enable: SourcedValue<Vec<String>>,
    pub disable: SourcedValue<Vec<String>>,
    pub exclude: SourcedValue<Vec<String>>,
    pub include: SourcedValue<Vec<String>>,
    pub respect_gitignore: SourcedValue<bool>,
    pub line_length: SourcedValue<LineLength>,
    pub output_format: Option<SourcedValue<String>>,
    pub fixable: SourcedValue<Vec<String>>,
    pub unfixable: SourcedValue<Vec<String>>,
    pub flavor: SourcedValue<MarkdownFlavor>,
    pub force_exclude: SourcedValue<bool>,
    pub cache_dir: Option<SourcedValue<String>>,
    pub cache: SourcedValue<bool>,
}

impl Default for SourcedGlobalConfig {
    fn default() -> Self {
        SourcedGlobalConfig {
            enable: SourcedValue::new(Vec::new(), ConfigSource::Default),
            disable: SourcedValue::new(Vec::new(), ConfigSource::Default),
            exclude: SourcedValue::new(Vec::new(), ConfigSource::Default),
            include: SourcedValue::new(Vec::new(), ConfigSource::Default),
            respect_gitignore: SourcedValue::new(true, ConfigSource::Default),
            line_length: SourcedValue::new(LineLength::default(), ConfigSource::Default),
            output_format: None,
            fixable: SourcedValue::new(Vec::new(), ConfigSource::Default),
            unfixable: SourcedValue::new(Vec::new(), ConfigSource::Default),
            flavor: SourcedValue::new(MarkdownFlavor::default(), ConfigSource::Default),
            force_exclude: SourcedValue::new(false, ConfigSource::Default),
            cache_dir: None,
            cache: SourcedValue::new(true, ConfigSource::Default),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SourcedRuleConfig {
    pub severity: Option<SourcedValue<crate::rule::Severity>>,
    pub values: BTreeMap<String, SourcedValue<toml::Value>>,
}

/// Represents configuration loaded from a single source file, with provenance.
/// Used as an intermediate step before merging into the final SourcedConfig.
#[derive(Debug, Clone)]
pub struct SourcedConfigFragment {
    pub global: SourcedGlobalConfig,
    pub per_file_ignores: SourcedValue<HashMap<String, Vec<String>>>,
    pub rules: BTreeMap<String, SourcedRuleConfig>,
    pub unknown_keys: Vec<(String, String, Option<String>)>, // (section, key, file_path)
                                                             // Note: loaded_files is tracked globally in SourcedConfig.
}

impl Default for SourcedConfigFragment {
    fn default() -> Self {
        Self {
            global: SourcedGlobalConfig::default(),
            per_file_ignores: SourcedValue::new(HashMap::new(), ConfigSource::Default),
            rules: BTreeMap::new(),
            unknown_keys: Vec::new(),
        }
    }
}

/// Configuration with provenance tracking for values.
///
/// The `State` type parameter encodes the validation state:
/// - `ConfigLoaded`: Config has been loaded but not validated
/// - `ConfigValidated`: Config has been validated and can be converted to `Config`
///
/// # Typestate Pattern
///
/// This uses the typestate pattern to ensure validation happens before conversion:
///
/// ```ignore
/// let loaded: SourcedConfig<ConfigLoaded> = SourcedConfig::load_with_discovery(...)?;
/// let validated: SourcedConfig<ConfigValidated> = loaded.validate(&registry)?;
/// let config: Config = validated.into();  // Only works on ConfigValidated!
/// ```
///
/// Attempting to convert a `ConfigLoaded` config directly to `Config` is a compile error.
#[derive(Debug, Clone)]
pub struct SourcedConfig<State = ConfigLoaded> {
    pub global: SourcedGlobalConfig,
    pub per_file_ignores: SourcedValue<HashMap<String, Vec<String>>>,
    pub rules: BTreeMap<String, SourcedRuleConfig>,
    pub loaded_files: Vec<String>,
    pub unknown_keys: Vec<(String, String, Option<String>)>, // (section, key, file_path)
    /// Project root directory (parent of config file), used for resolving relative paths
    pub project_root: Option<std::path::PathBuf>,
    /// Validation warnings (populated after validate() is called)
    pub validation_warnings: Vec<ConfigValidationWarning>,
    /// Phantom data for the state type parameter
    _state: PhantomData<State>,
}

impl Default for SourcedConfig<ConfigLoaded> {
    fn default() -> Self {
        Self {
            global: SourcedGlobalConfig::default(),
            per_file_ignores: SourcedValue::new(HashMap::new(), ConfigSource::Default),
            rules: BTreeMap::new(),
            loaded_files: Vec::new(),
            unknown_keys: Vec::new(),
            project_root: None,
            validation_warnings: Vec::new(),
            _state: PhantomData,
        }
    }
}

impl SourcedConfig<ConfigLoaded> {
    /// Merges another SourcedConfigFragment into this SourcedConfig.
    /// Uses source precedence to determine which values take effect.
    fn merge(&mut self, fragment: SourcedConfigFragment) {
        // Merge global config
        // Enable uses replace semantics (project can enforce rules)
        self.global.enable.merge_override(
            fragment.global.enable.value,
            fragment.global.enable.source,
            fragment.global.enable.overrides.first().and_then(|o| o.file.clone()),
            fragment.global.enable.overrides.first().and_then(|o| o.line),
        );

        // Disable uses union semantics (user can add to project disables)
        self.global.disable.merge_union(
            fragment.global.disable.value,
            fragment.global.disable.source,
            fragment.global.disable.overrides.first().and_then(|o| o.file.clone()),
            fragment.global.disable.overrides.first().and_then(|o| o.line),
        );

        // Conflict resolution: Enable overrides disable
        // Remove any rules from disable that appear in enable
        self.global
            .disable
            .value
            .retain(|rule| !self.global.enable.value.contains(rule));
        self.global.include.merge_override(
            fragment.global.include.value,
            fragment.global.include.source,
            fragment.global.include.overrides.first().and_then(|o| o.file.clone()),
            fragment.global.include.overrides.first().and_then(|o| o.line),
        );
        self.global.exclude.merge_override(
            fragment.global.exclude.value,
            fragment.global.exclude.source,
            fragment.global.exclude.overrides.first().and_then(|o| o.file.clone()),
            fragment.global.exclude.overrides.first().and_then(|o| o.line),
        );
        self.global.respect_gitignore.merge_override(
            fragment.global.respect_gitignore.value,
            fragment.global.respect_gitignore.source,
            fragment
                .global
                .respect_gitignore
                .overrides
                .first()
                .and_then(|o| o.file.clone()),
            fragment.global.respect_gitignore.overrides.first().and_then(|o| o.line),
        );
        self.global.line_length.merge_override(
            fragment.global.line_length.value,
            fragment.global.line_length.source,
            fragment
                .global
                .line_length
                .overrides
                .first()
                .and_then(|o| o.file.clone()),
            fragment.global.line_length.overrides.first().and_then(|o| o.line),
        );
        self.global.fixable.merge_override(
            fragment.global.fixable.value,
            fragment.global.fixable.source,
            fragment.global.fixable.overrides.first().and_then(|o| o.file.clone()),
            fragment.global.fixable.overrides.first().and_then(|o| o.line),
        );
        self.global.unfixable.merge_override(
            fragment.global.unfixable.value,
            fragment.global.unfixable.source,
            fragment.global.unfixable.overrides.first().and_then(|o| o.file.clone()),
            fragment.global.unfixable.overrides.first().and_then(|o| o.line),
        );

        // Merge flavor
        self.global.flavor.merge_override(
            fragment.global.flavor.value,
            fragment.global.flavor.source,
            fragment.global.flavor.overrides.first().and_then(|o| o.file.clone()),
            fragment.global.flavor.overrides.first().and_then(|o| o.line),
        );

        // Merge force_exclude
        self.global.force_exclude.merge_override(
            fragment.global.force_exclude.value,
            fragment.global.force_exclude.source,
            fragment
                .global
                .force_exclude
                .overrides
                .first()
                .and_then(|o| o.file.clone()),
            fragment.global.force_exclude.overrides.first().and_then(|o| o.line),
        );

        // Merge output_format if present
        if let Some(output_format_fragment) = fragment.global.output_format {
            if let Some(ref mut output_format) = self.global.output_format {
                output_format.merge_override(
                    output_format_fragment.value,
                    output_format_fragment.source,
                    output_format_fragment.overrides.first().and_then(|o| o.file.clone()),
                    output_format_fragment.overrides.first().and_then(|o| o.line),
                );
            } else {
                self.global.output_format = Some(output_format_fragment);
            }
        }

        // Merge cache_dir if present
        if let Some(cache_dir_fragment) = fragment.global.cache_dir {
            if let Some(ref mut cache_dir) = self.global.cache_dir {
                cache_dir.merge_override(
                    cache_dir_fragment.value,
                    cache_dir_fragment.source,
                    cache_dir_fragment.overrides.first().and_then(|o| o.file.clone()),
                    cache_dir_fragment.overrides.first().and_then(|o| o.line),
                );
            } else {
                self.global.cache_dir = Some(cache_dir_fragment);
            }
        }

        // Merge cache if not default (only override when explicitly set)
        if fragment.global.cache.source != ConfigSource::Default {
            self.global.cache.merge_override(
                fragment.global.cache.value,
                fragment.global.cache.source,
                fragment.global.cache.overrides.first().and_then(|o| o.file.clone()),
                fragment.global.cache.overrides.first().and_then(|o| o.line),
            );
        }

        // Merge per_file_ignores
        self.per_file_ignores.merge_override(
            fragment.per_file_ignores.value,
            fragment.per_file_ignores.source,
            fragment.per_file_ignores.overrides.first().and_then(|o| o.file.clone()),
            fragment.per_file_ignores.overrides.first().and_then(|o| o.line),
        );

        // Merge rule configs
        for (rule_name, rule_fragment) in fragment.rules {
            let norm_rule_name = rule_name.to_ascii_uppercase(); // Normalize to uppercase for case-insensitivity
            let rule_entry = self.rules.entry(norm_rule_name).or_default();

            // Merge severity if present in fragment
            if let Some(severity_fragment) = rule_fragment.severity {
                if let Some(ref mut existing_severity) = rule_entry.severity {
                    existing_severity.merge_override(
                        severity_fragment.value,
                        severity_fragment.source,
                        severity_fragment.overrides.first().and_then(|o| o.file.clone()),
                        severity_fragment.overrides.first().and_then(|o| o.line),
                    );
                } else {
                    rule_entry.severity = Some(severity_fragment);
                }
            }

            // Merge values
            for (key, sourced_value_fragment) in rule_fragment.values {
                let sv_entry = rule_entry
                    .values
                    .entry(key.clone())
                    .or_insert_with(|| SourcedValue::new(sourced_value_fragment.value.clone(), ConfigSource::Default));
                let file_from_fragment = sourced_value_fragment.overrides.first().and_then(|o| o.file.clone());
                let line_from_fragment = sourced_value_fragment.overrides.first().and_then(|o| o.line);
                sv_entry.merge_override(
                    sourced_value_fragment.value,  // Use the value from the fragment
                    sourced_value_fragment.source, // Use the source from the fragment
                    file_from_fragment,            // Pass the file path from the fragment override
                    line_from_fragment,            // Pass the line number from the fragment override
                );
            }
        }

        // Merge unknown_keys from fragment
        for (section, key, file_path) in fragment.unknown_keys {
            // Deduplicate: only add if not already present
            if !self.unknown_keys.iter().any(|(s, k, _)| s == &section && k == &key) {
                self.unknown_keys.push((section, key, file_path));
            }
        }
    }

    /// Load and merge configurations from files and CLI overrides.
    pub fn load(config_path: Option<&str>, cli_overrides: Option<&SourcedGlobalConfig>) -> Result<Self, ConfigError> {
        Self::load_with_discovery(config_path, cli_overrides, false)
    }

    /// Finds project root by walking up from start_dir looking for .git directory.
    /// Falls back to start_dir if no .git found.
    fn find_project_root_from(start_dir: &Path) -> std::path::PathBuf {
        let mut current = start_dir.to_path_buf();
        const MAX_DEPTH: usize = 100;

        for _ in 0..MAX_DEPTH {
            if current.join(".git").exists() {
                log::debug!("[rumdl-config] Found .git at: {}", current.display());
                return current;
            }

            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => break,
            }
        }

        // No .git found, use start_dir as project root
        log::debug!(
            "[rumdl-config] No .git found, using config location as project root: {}",
            start_dir.display()
        );
        start_dir.to_path_buf()
    }

    /// Discover configuration file by traversing up the directory tree.
    /// Returns the first configuration file found.
    /// Discovers config file and returns both the config path and project root.
    /// Returns: (config_file_path, project_root_path)
    /// Project root is the directory containing .git, or config parent as fallback.
    fn discover_config_upward() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        use std::env;

        const CONFIG_FILES: &[&str] = &[".rumdl.toml", "rumdl.toml", ".config/rumdl.toml", "pyproject.toml"];
        const MAX_DEPTH: usize = 100; // Prevent infinite traversal

        let start_dir = match env::current_dir() {
            Ok(dir) => dir,
            Err(e) => {
                log::debug!("[rumdl-config] Failed to get current directory: {e}");
                return None;
            }
        };

        let mut current_dir = start_dir.clone();
        let mut depth = 0;
        let mut found_config: Option<(std::path::PathBuf, std::path::PathBuf)> = None;

        loop {
            if depth >= MAX_DEPTH {
                log::debug!("[rumdl-config] Maximum traversal depth reached");
                break;
            }

            log::debug!("[rumdl-config] Searching for config in: {}", current_dir.display());

            // Check for config files in order of precedence (only if not already found)
            if found_config.is_none() {
                for config_name in CONFIG_FILES {
                    let config_path = current_dir.join(config_name);

                    if config_path.exists() {
                        // For pyproject.toml, verify it contains [tool.rumdl] section
                        if *config_name == "pyproject.toml" {
                            if let Ok(content) = std::fs::read_to_string(&config_path) {
                                if content.contains("[tool.rumdl]") || content.contains("tool.rumdl") {
                                    log::debug!("[rumdl-config] Found config file: {}", config_path.display());
                                    // Store config, but continue looking for .git
                                    found_config = Some((config_path.clone(), current_dir.clone()));
                                    break;
                                }
                                log::debug!("[rumdl-config] Found pyproject.toml but no [tool.rumdl] section");
                                continue;
                            }
                        } else {
                            log::debug!("[rumdl-config] Found config file: {}", config_path.display());
                            // Store config, but continue looking for .git
                            found_config = Some((config_path.clone(), current_dir.clone()));
                            break;
                        }
                    }
                }
            }

            // Check for .git directory (stop boundary)
            if current_dir.join(".git").exists() {
                log::debug!("[rumdl-config] Stopping at .git directory");
                break;
            }

            // Move to parent directory
            match current_dir.parent() {
                Some(parent) => {
                    current_dir = parent.to_owned();
                    depth += 1;
                }
                None => {
                    log::debug!("[rumdl-config] Reached filesystem root");
                    break;
                }
            }
        }

        // If config found, determine project root by walking up from config location
        if let Some((config_path, config_dir)) = found_config {
            let project_root = Self::find_project_root_from(&config_dir);
            return Some((config_path, project_root));
        }

        None
    }

    /// Discover markdownlint configuration file by traversing up the directory tree.
    /// Similar to discover_config_upward but for .markdownlint.yaml/json files.
    /// Returns the path to the config file if found.
    fn discover_markdownlint_config_upward() -> Option<std::path::PathBuf> {
        use std::env;

        const MAX_DEPTH: usize = 100;

        let start_dir = match env::current_dir() {
            Ok(dir) => dir,
            Err(e) => {
                log::debug!("[rumdl-config] Failed to get current directory for markdownlint discovery: {e}");
                return None;
            }
        };

        let mut current_dir = start_dir.clone();
        let mut depth = 0;

        loop {
            if depth >= MAX_DEPTH {
                log::debug!("[rumdl-config] Maximum traversal depth reached for markdownlint discovery");
                break;
            }

            log::debug!(
                "[rumdl-config] Searching for markdownlint config in: {}",
                current_dir.display()
            );

            // Check for markdownlint config files in order of precedence
            for config_name in MARKDOWNLINT_CONFIG_FILES {
                let config_path = current_dir.join(config_name);
                if config_path.exists() {
                    log::debug!("[rumdl-config] Found markdownlint config: {}", config_path.display());
                    return Some(config_path);
                }
            }

            // Check for .git directory (stop boundary)
            if current_dir.join(".git").exists() {
                log::debug!("[rumdl-config] Stopping markdownlint search at .git directory");
                break;
            }

            // Move to parent directory
            match current_dir.parent() {
                Some(parent) => {
                    current_dir = parent.to_owned();
                    depth += 1;
                }
                None => {
                    log::debug!("[rumdl-config] Reached filesystem root during markdownlint search");
                    break;
                }
            }
        }

        None
    }

    /// Internal implementation that accepts config directory for testing
    fn user_configuration_path_impl(config_dir: &Path) -> Option<std::path::PathBuf> {
        let config_dir = config_dir.join("rumdl");

        // Check for config files in precedence order (same as project discovery)
        const USER_CONFIG_FILES: &[&str] = &[".rumdl.toml", "rumdl.toml", "pyproject.toml"];

        log::debug!(
            "[rumdl-config] Checking for user configuration in: {}",
            config_dir.display()
        );

        for filename in USER_CONFIG_FILES {
            let config_path = config_dir.join(filename);

            if config_path.exists() {
                // For pyproject.toml, verify it contains [tool.rumdl] section
                if *filename == "pyproject.toml" {
                    if let Ok(content) = std::fs::read_to_string(&config_path) {
                        if content.contains("[tool.rumdl]") || content.contains("tool.rumdl") {
                            log::debug!("[rumdl-config] Found user configuration at: {}", config_path.display());
                            return Some(config_path);
                        }
                        log::debug!("[rumdl-config] Found user pyproject.toml but no [tool.rumdl] section");
                        continue;
                    }
                } else {
                    log::debug!("[rumdl-config] Found user configuration at: {}", config_path.display());
                    return Some(config_path);
                }
            }
        }

        log::debug!(
            "[rumdl-config] No user configuration found in: {}",
            config_dir.display()
        );
        None
    }

    /// Discover user-level configuration file from platform-specific config directory.
    /// Returns the first configuration file found in the user config directory.
    #[cfg(feature = "native")]
    fn user_configuration_path() -> Option<std::path::PathBuf> {
        use etcetera::{BaseStrategy, choose_base_strategy};

        match choose_base_strategy() {
            Ok(strategy) => {
                let config_dir = strategy.config_dir();
                Self::user_configuration_path_impl(&config_dir)
            }
            Err(e) => {
                log::debug!("[rumdl-config] Failed to determine user config directory: {e}");
                None
            }
        }
    }

    /// Stub for WASM builds - user config not supported
    #[cfg(not(feature = "native"))]
    fn user_configuration_path() -> Option<std::path::PathBuf> {
        None
    }

    /// Load an explicit config file (standalone, no user config merging)
    fn load_explicit_config(sourced_config: &mut Self, path: &str) -> Result<(), ConfigError> {
        let path_obj = Path::new(path);
        let filename = path_obj.file_name().and_then(|name| name.to_str()).unwrap_or("");
        let path_str = path.to_string();

        log::debug!("[rumdl-config] Loading explicit config file: {filename}");

        // Find project root by walking up from config location looking for .git
        if let Some(config_parent) = path_obj.parent() {
            let project_root = Self::find_project_root_from(config_parent);
            log::debug!(
                "[rumdl-config] Project root (from explicit config): {}",
                project_root.display()
            );
            sourced_config.project_root = Some(project_root);
        }

        // Known markdownlint config files
        const MARKDOWNLINT_FILENAMES: &[&str] = &[".markdownlint.json", ".markdownlint.yaml", ".markdownlint.yml"];

        if filename == "pyproject.toml" || filename == ".rumdl.toml" || filename == "rumdl.toml" {
            let content = std::fs::read_to_string(path).map_err(|e| ConfigError::IoError {
                source: e,
                path: path_str.clone(),
            })?;
            if filename == "pyproject.toml" {
                if let Some(fragment) = parse_pyproject_toml(&content, &path_str)? {
                    sourced_config.merge(fragment);
                    sourced_config.loaded_files.push(path_str);
                }
            } else {
                let fragment = parse_rumdl_toml(&content, &path_str, ConfigSource::ProjectConfig)?;
                sourced_config.merge(fragment);
                sourced_config.loaded_files.push(path_str);
            }
        } else if MARKDOWNLINT_FILENAMES.contains(&filename)
            || path_str.ends_with(".json")
            || path_str.ends_with(".jsonc")
            || path_str.ends_with(".yaml")
            || path_str.ends_with(".yml")
        {
            // Parse as markdownlint config (JSON/YAML)
            let fragment = load_from_markdownlint(&path_str)?;
            sourced_config.merge(fragment);
            sourced_config.loaded_files.push(path_str);
        } else {
            // Try TOML only
            let content = std::fs::read_to_string(path).map_err(|e| ConfigError::IoError {
                source: e,
                path: path_str.clone(),
            })?;
            let fragment = parse_rumdl_toml(&content, &path_str, ConfigSource::ProjectConfig)?;
            sourced_config.merge(fragment);
            sourced_config.loaded_files.push(path_str);
        }

        Ok(())
    }

    /// Load user config as fallback when no project config exists
    fn load_user_config_as_fallback(
        sourced_config: &mut Self,
        user_config_dir: Option<&Path>,
    ) -> Result<(), ConfigError> {
        let user_config_path = if let Some(dir) = user_config_dir {
            Self::user_configuration_path_impl(dir)
        } else {
            Self::user_configuration_path()
        };

        if let Some(user_config_path) = user_config_path {
            let path_str = user_config_path.display().to_string();
            let filename = user_config_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            log::debug!("[rumdl-config] Loading user config as fallback: {path_str}");

            if filename == "pyproject.toml" {
                let content = std::fs::read_to_string(&user_config_path).map_err(|e| ConfigError::IoError {
                    source: e,
                    path: path_str.clone(),
                })?;
                if let Some(fragment) = parse_pyproject_toml(&content, &path_str)? {
                    sourced_config.merge(fragment);
                    sourced_config.loaded_files.push(path_str);
                }
            } else {
                let content = std::fs::read_to_string(&user_config_path).map_err(|e| ConfigError::IoError {
                    source: e,
                    path: path_str.clone(),
                })?;
                let fragment = parse_rumdl_toml(&content, &path_str, ConfigSource::UserConfig)?;
                sourced_config.merge(fragment);
                sourced_config.loaded_files.push(path_str);
            }
        } else {
            log::debug!("[rumdl-config] No user configuration file found");
        }

        Ok(())
    }

    /// Internal implementation that accepts user config directory for testing
    #[doc(hidden)]
    pub fn load_with_discovery_impl(
        config_path: Option<&str>,
        cli_overrides: Option<&SourcedGlobalConfig>,
        skip_auto_discovery: bool,
        user_config_dir: Option<&Path>,
    ) -> Result<Self, ConfigError> {
        use std::env;
        log::debug!("[rumdl-config] Current working directory: {:?}", env::current_dir());

        let mut sourced_config = SourcedConfig::default();

        // Ruff model: Project config is standalone, user config is fallback only
        //
        // Priority order:
        // 1. If explicit config path provided → use ONLY that (standalone)
        // 2. Else if project config discovered → use ONLY that (standalone)
        // 3. Else if user config exists → use it as fallback
        // 4. CLI overrides always apply last
        //
        // This ensures project configs are reproducible across machines and
        // CI/local runs behave identically.

        // Explicit config path always takes precedence
        if let Some(path) = config_path {
            // Explicit config path provided - use ONLY this config (standalone)
            log::debug!("[rumdl-config] Explicit config_path provided: {path:?}");
            Self::load_explicit_config(&mut sourced_config, path)?;
        } else if skip_auto_discovery {
            log::debug!("[rumdl-config] Skipping config discovery due to --no-config/--isolated flag");
            // No config loading, just apply CLI overrides at the end
        } else {
            // No explicit path - try auto-discovery
            log::debug!("[rumdl-config] No explicit config_path, searching default locations");

            // Try to discover project config first
            if let Some((config_file, project_root)) = Self::discover_config_upward() {
                // Project config found - use ONLY this (standalone, no user config)
                let path_str = config_file.display().to_string();
                let filename = config_file.file_name().and_then(|n| n.to_str()).unwrap_or("");

                log::debug!("[rumdl-config] Found project config: {path_str}");
                log::debug!("[rumdl-config] Project root: {}", project_root.display());

                sourced_config.project_root = Some(project_root);

                if filename == "pyproject.toml" {
                    let content = std::fs::read_to_string(&config_file).map_err(|e| ConfigError::IoError {
                        source: e,
                        path: path_str.clone(),
                    })?;
                    if let Some(fragment) = parse_pyproject_toml(&content, &path_str)? {
                        sourced_config.merge(fragment);
                        sourced_config.loaded_files.push(path_str);
                    }
                } else if filename == ".rumdl.toml" || filename == "rumdl.toml" {
                    let content = std::fs::read_to_string(&config_file).map_err(|e| ConfigError::IoError {
                        source: e,
                        path: path_str.clone(),
                    })?;
                    let fragment = parse_rumdl_toml(&content, &path_str, ConfigSource::ProjectConfig)?;
                    sourced_config.merge(fragment);
                    sourced_config.loaded_files.push(path_str);
                }
            } else {
                // No rumdl project config - try markdownlint config
                log::debug!("[rumdl-config] No rumdl config found, checking markdownlint config");

                if let Some(markdownlint_path) = Self::discover_markdownlint_config_upward() {
                    let path_str = markdownlint_path.display().to_string();
                    log::debug!("[rumdl-config] Found markdownlint config: {path_str}");
                    match load_from_markdownlint(&path_str) {
                        Ok(fragment) => {
                            sourced_config.merge(fragment);
                            sourced_config.loaded_files.push(path_str);
                        }
                        Err(_e) => {
                            log::debug!("[rumdl-config] Failed to load markdownlint config, trying user config");
                            Self::load_user_config_as_fallback(&mut sourced_config, user_config_dir)?;
                        }
                    }
                } else {
                    // No project config at all - use user config as fallback
                    log::debug!("[rumdl-config] No project config found, using user config as fallback");
                    Self::load_user_config_as_fallback(&mut sourced_config, user_config_dir)?;
                }
            }
        }

        // Apply CLI overrides (highest precedence)
        if let Some(cli) = cli_overrides {
            sourced_config
                .global
                .enable
                .merge_override(cli.enable.value.clone(), ConfigSource::Cli, None, None);
            sourced_config
                .global
                .disable
                .merge_override(cli.disable.value.clone(), ConfigSource::Cli, None, None);
            sourced_config
                .global
                .exclude
                .merge_override(cli.exclude.value.clone(), ConfigSource::Cli, None, None);
            sourced_config
                .global
                .include
                .merge_override(cli.include.value.clone(), ConfigSource::Cli, None, None);
            sourced_config.global.respect_gitignore.merge_override(
                cli.respect_gitignore.value,
                ConfigSource::Cli,
                None,
                None,
            );
            sourced_config
                .global
                .fixable
                .merge_override(cli.fixable.value.clone(), ConfigSource::Cli, None, None);
            sourced_config
                .global
                .unfixable
                .merge_override(cli.unfixable.value.clone(), ConfigSource::Cli, None, None);
            // No rule-specific CLI overrides implemented yet
        }

        // Unknown keys are now collected during parsing and validated via validate_config_sourced()

        Ok(sourced_config)
    }

    /// Load and merge configurations from files and CLI overrides.
    /// If skip_auto_discovery is true, only explicit config paths are loaded.
    pub fn load_with_discovery(
        config_path: Option<&str>,
        cli_overrides: Option<&SourcedGlobalConfig>,
        skip_auto_discovery: bool,
    ) -> Result<Self, ConfigError> {
        Self::load_with_discovery_impl(config_path, cli_overrides, skip_auto_discovery, None)
    }

    /// Validate the configuration against a rule registry.
    ///
    /// This method transitions the config from `ConfigLoaded` to `ConfigValidated` state,
    /// enabling conversion to `Config`. Validation warnings are stored in the config
    /// and can be displayed to the user.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let loaded = SourcedConfig::load_with_discovery(path, None, false)?;
    /// let validated = loaded.validate(&registry)?;
    /// let config: Config = validated.into();
    /// ```
    pub fn validate(self, registry: &RuleRegistry) -> Result<SourcedConfig<ConfigValidated>, ConfigError> {
        let warnings = validate_config_sourced_internal(&self, registry);

        Ok(SourcedConfig {
            global: self.global,
            per_file_ignores: self.per_file_ignores,
            rules: self.rules,
            loaded_files: self.loaded_files,
            unknown_keys: self.unknown_keys,
            project_root: self.project_root,
            validation_warnings: warnings,
            _state: PhantomData,
        })
    }

    /// Validate and convert to Config in one step (convenience method).
    ///
    /// This combines `validate()` and `into()` for callers who want the
    /// validation warnings separately.
    pub fn validate_into(self, registry: &RuleRegistry) -> Result<(Config, Vec<ConfigValidationWarning>), ConfigError> {
        let validated = self.validate(registry)?;
        let warnings = validated.validation_warnings.clone();
        Ok((validated.into(), warnings))
    }

    /// Skip validation and convert directly to ConfigValidated state.
    ///
    /// # Safety
    ///
    /// This method bypasses validation. Use only when:
    /// - You've already validated via `validate_config_sourced()`
    /// - You're in test code that doesn't need validation
    /// - You're migrating legacy code and will add proper validation later
    ///
    /// Prefer `validate()` for new code.
    pub fn into_validated_unchecked(self) -> SourcedConfig<ConfigValidated> {
        SourcedConfig {
            global: self.global,
            per_file_ignores: self.per_file_ignores,
            rules: self.rules,
            loaded_files: self.loaded_files,
            unknown_keys: self.unknown_keys,
            project_root: self.project_root,
            validation_warnings: Vec::new(),
            _state: PhantomData,
        }
    }
}

/// Convert a validated configuration to the final Config type.
///
/// This implementation only exists for `SourcedConfig<ConfigValidated>`,
/// ensuring that validation must occur before conversion.
impl From<SourcedConfig<ConfigValidated>> for Config {
    fn from(sourced: SourcedConfig<ConfigValidated>) -> Self {
        let mut rules = BTreeMap::new();
        for (rule_name, sourced_rule_cfg) in sourced.rules {
            // Normalize rule name to uppercase for case-insensitive lookup
            let normalized_rule_name = rule_name.to_ascii_uppercase();
            let severity = sourced_rule_cfg.severity.map(|sv| sv.value);
            let mut values = BTreeMap::new();
            for (key, sourced_val) in sourced_rule_cfg.values {
                values.insert(key, sourced_val.value);
            }
            rules.insert(normalized_rule_name, RuleConfig { severity, values });
        }
        #[allow(deprecated)]
        let global = GlobalConfig {
            enable: sourced.global.enable.value,
            disable: sourced.global.disable.value,
            exclude: sourced.global.exclude.value,
            include: sourced.global.include.value,
            respect_gitignore: sourced.global.respect_gitignore.value,
            line_length: sourced.global.line_length.value,
            output_format: sourced.global.output_format.as_ref().map(|v| v.value.clone()),
            fixable: sourced.global.fixable.value,
            unfixable: sourced.global.unfixable.value,
            flavor: sourced.global.flavor.value,
            force_exclude: sourced.global.force_exclude.value,
            cache_dir: sourced.global.cache_dir.as_ref().map(|v| v.value.clone()),
            cache: sourced.global.cache.value,
        };
        Config {
            global,
            per_file_ignores: sourced.per_file_ignores.value,
            rules,
            project_root: sourced.project_root,
        }
    }
}

/// Registry of all known rules and their config schemas
pub struct RuleRegistry {
    /// Map of rule name (e.g. "MD013") to set of valid config keys and their TOML value types
    pub rule_schemas: std::collections::BTreeMap<String, toml::map::Map<String, toml::Value>>,
    /// Map of rule name to config key aliases
    pub rule_aliases: std::collections::BTreeMap<String, std::collections::HashMap<String, String>>,
}

impl RuleRegistry {
    /// Build a registry from a list of rules
    pub fn from_rules(rules: &[Box<dyn Rule>]) -> Self {
        let mut rule_schemas = std::collections::BTreeMap::new();
        let mut rule_aliases = std::collections::BTreeMap::new();

        for rule in rules {
            let norm_name = if let Some((name, toml::Value::Table(table))) = rule.default_config_section() {
                let norm_name = normalize_key(&name); // Normalize the name from default_config_section
                rule_schemas.insert(norm_name.clone(), table);
                norm_name
            } else {
                let norm_name = normalize_key(rule.name()); // Normalize the name from rule.name()
                rule_schemas.insert(norm_name.clone(), toml::map::Map::new());
                norm_name
            };

            // Store aliases if the rule provides them
            if let Some(aliases) = rule.config_aliases() {
                rule_aliases.insert(norm_name, aliases);
            }
        }

        RuleRegistry {
            rule_schemas,
            rule_aliases,
        }
    }

    /// Get all known rule names
    pub fn rule_names(&self) -> std::collections::BTreeSet<String> {
        self.rule_schemas.keys().cloned().collect()
    }

    /// Get the valid configuration keys for a rule, including both original and normalized variants
    pub fn config_keys_for(&self, rule: &str) -> Option<std::collections::BTreeSet<String>> {
        self.rule_schemas.get(rule).map(|schema| {
            let mut all_keys = std::collections::BTreeSet::new();

            // Always allow 'severity' for any rule
            all_keys.insert("severity".to_string());

            // Add original keys from schema
            for key in schema.keys() {
                all_keys.insert(key.clone());
            }

            // Add normalized variants for markdownlint compatibility
            for key in schema.keys() {
                // Add kebab-case variant
                all_keys.insert(key.replace('_', "-"));
                // Add snake_case variant
                all_keys.insert(key.replace('-', "_"));
                // Add normalized variant
                all_keys.insert(normalize_key(key));
            }

            // Add any aliases defined by the rule
            if let Some(aliases) = self.rule_aliases.get(rule) {
                for alias_key in aliases.keys() {
                    all_keys.insert(alias_key.clone());
                    // Also add normalized variants of the alias
                    all_keys.insert(alias_key.replace('_', "-"));
                    all_keys.insert(alias_key.replace('-', "_"));
                    all_keys.insert(normalize_key(alias_key));
                }
            }

            all_keys
        })
    }

    /// Get the expected value type for a rule's configuration key, trying variants
    pub fn expected_value_for(&self, rule: &str, key: &str) -> Option<&toml::Value> {
        if let Some(schema) = self.rule_schemas.get(rule) {
            // Check if this key is an alias
            if let Some(aliases) = self.rule_aliases.get(rule)
                && let Some(canonical_key) = aliases.get(key)
            {
                // Use the canonical key for schema lookup
                if let Some(value) = schema.get(canonical_key) {
                    return Some(value);
                }
            }

            // Try the original key
            if let Some(value) = schema.get(key) {
                return Some(value);
            }

            // Try key variants
            let key_variants = [
                key.replace('-', "_"), // Convert kebab-case to snake_case
                key.replace('_', "-"), // Convert snake_case to kebab-case
                normalize_key(key),    // Normalized key (lowercase, kebab-case)
            ];

            for variant in &key_variants {
                if let Some(value) = schema.get(variant) {
                    return Some(value);
                }
            }
        }
        None
    }

    /// Resolve any rule name (canonical or alias) to its canonical form
    /// Returns None if the rule name is not recognized
    ///
    /// Resolution order:
    /// 1. Direct canonical name match
    /// 2. Static aliases (built-in markdownlint aliases)
    pub fn resolve_rule_name(&self, name: &str) -> Option<String> {
        // Try normalized canonical name first
        let normalized = normalize_key(name);
        if self.rule_schemas.contains_key(&normalized) {
            return Some(normalized);
        }

        // Try static alias resolution (O(1) perfect hash lookup)
        resolve_rule_name_alias(name).map(|s| s.to_string())
    }
}

/// Compile-time perfect hash map for O(1) rule alias lookups
/// Uses phf for zero-cost abstraction - compiles to direct jumps
static RULE_ALIAS_MAP: phf::Map<&'static str, &'static str> = phf::phf_map! {
    // Canonical names (identity mapping for consistency)
    "MD001" => "MD001",
    "MD003" => "MD003",
    "MD004" => "MD004",
    "MD005" => "MD005",
    "MD007" => "MD007",
    "MD008" => "MD008",
    "MD009" => "MD009",
    "MD010" => "MD010",
    "MD011" => "MD011",
    "MD012" => "MD012",
    "MD013" => "MD013",
    "MD014" => "MD014",
    "MD015" => "MD015",
    "MD018" => "MD018",
    "MD019" => "MD019",
    "MD020" => "MD020",
    "MD021" => "MD021",
    "MD022" => "MD022",
    "MD023" => "MD023",
    "MD024" => "MD024",
    "MD025" => "MD025",
    "MD026" => "MD026",
    "MD027" => "MD027",
    "MD028" => "MD028",
    "MD029" => "MD029",
    "MD030" => "MD030",
    "MD031" => "MD031",
    "MD032" => "MD032",
    "MD033" => "MD033",
    "MD034" => "MD034",
    "MD035" => "MD035",
    "MD036" => "MD036",
    "MD037" => "MD037",
    "MD038" => "MD038",
    "MD039" => "MD039",
    "MD040" => "MD040",
    "MD041" => "MD041",
    "MD042" => "MD042",
    "MD043" => "MD043",
    "MD044" => "MD044",
    "MD045" => "MD045",
    "MD046" => "MD046",
    "MD047" => "MD047",
    "MD048" => "MD048",
    "MD049" => "MD049",
    "MD050" => "MD050",
    "MD051" => "MD051",
    "MD052" => "MD052",
    "MD053" => "MD053",
    "MD054" => "MD054",
    "MD055" => "MD055",
    "MD056" => "MD056",
    "MD057" => "MD057",
    "MD058" => "MD058",
    "MD059" => "MD059",
    "MD060" => "MD060",
    "MD061" => "MD061",
    "MD062" => "MD062",
    "MD063" => "MD063",
    "MD064" => "MD064",
    "MD065" => "MD065",
    "MD066" => "MD066",
    "MD067" => "MD067",
    "MD068" => "MD068",
    "MD069" => "MD069",

    // Aliases (hyphen format)
    "HEADING-INCREMENT" => "MD001",
    "HEADING-STYLE" => "MD003",
    "UL-STYLE" => "MD004",
    "LIST-INDENT" => "MD005",
    "UL-INDENT" => "MD007",
    "NO-TRAILING-SPACES" => "MD009",
    "NO-HARD-TABS" => "MD010",
    "NO-REVERSED-LINKS" => "MD011",
    "NO-MULTIPLE-BLANKS" => "MD012",
    "LINE-LENGTH" => "MD013",
    "COMMANDS-SHOW-OUTPUT" => "MD014",
    "NO-MISSING-SPACE-AFTER-LIST-MARKER" => "MD015",
    "NO-MISSING-SPACE-ATX" => "MD018",
    "NO-MULTIPLE-SPACE-ATX" => "MD019",
    "NO-MISSING-SPACE-CLOSED-ATX" => "MD020",
    "NO-MULTIPLE-SPACE-CLOSED-ATX" => "MD021",
    "BLANKS-AROUND-HEADINGS" => "MD022",
    "HEADING-START-LEFT" => "MD023",
    "NO-DUPLICATE-HEADING" => "MD024",
    "SINGLE-TITLE" => "MD025",
    "SINGLE-H1" => "MD025",
    "NO-TRAILING-PUNCTUATION" => "MD026",
    "NO-MULTIPLE-SPACE-BLOCKQUOTE" => "MD027",
    "NO-BLANKS-BLOCKQUOTE" => "MD028",
    "OL-PREFIX" => "MD029",
    "LIST-MARKER-SPACE" => "MD030",
    "BLANKS-AROUND-FENCES" => "MD031",
    "BLANKS-AROUND-LISTS" => "MD032",
    "NO-INLINE-HTML" => "MD033",
    "NO-BARE-URLS" => "MD034",
    "HR-STYLE" => "MD035",
    "NO-EMPHASIS-AS-HEADING" => "MD036",
    "NO-SPACE-IN-EMPHASIS" => "MD037",
    "NO-SPACE-IN-CODE" => "MD038",
    "NO-SPACE-IN-LINKS" => "MD039",
    "FENCED-CODE-LANGUAGE" => "MD040",
    "FIRST-LINE-HEADING" => "MD041",
    "FIRST-LINE-H1" => "MD041",
    "NO-EMPTY-LINKS" => "MD042",
    "REQUIRED-HEADINGS" => "MD043",
    "PROPER-NAMES" => "MD044",
    "NO-ALT-TEXT" => "MD045",
    "CODE-BLOCK-STYLE" => "MD046",
    "SINGLE-TRAILING-NEWLINE" => "MD047",
    "CODE-FENCE-STYLE" => "MD048",
    "EMPHASIS-STYLE" => "MD049",
    "STRONG-STYLE" => "MD050",
    "LINK-FRAGMENTS" => "MD051",
    "REFERENCE-LINKS-IMAGES" => "MD052",
    "LINK-IMAGE-REFERENCE-DEFINITIONS" => "MD053",
    "LINK-IMAGE-STYLE" => "MD054",
    "TABLE-PIPE-STYLE" => "MD055",
    "TABLE-COLUMN-COUNT" => "MD056",
    "EXISTING-RELATIVE-LINKS" => "MD057",
    "BLANKS-AROUND-TABLES" => "MD058",
    "TABLE-CELL-ALIGNMENT" => "MD059",
    "TABLE-FORMAT" => "MD060",
    "FORBIDDEN-TERMS" => "MD061",
    "LINK-DESTINATION-WHITESPACE" => "MD062",
    "HEADING-CAPITALIZATION" => "MD063",
    "NO-MULTIPLE-CONSECUTIVE-SPACES" => "MD064",
    "BLANKS-AROUND-HORIZONTAL-RULES" => "MD065",
    "FOOTNOTE-VALIDATION" => "MD066",
    "FOOTNOTE-DEFINITION-ORDER" => "MD067",
    "EMPTY-FOOTNOTE-DEFINITION" => "MD068",
    "NO-DUPLICATE-LIST-MARKERS" => "MD069",
};

/// Resolve a rule name alias to its canonical form with O(1) perfect hash lookup
/// Converts rule aliases (like "ul-style", "line-length") to canonical IDs (like "MD004", "MD013")
/// Returns None if the rule name is not recognized
pub fn resolve_rule_name_alias(key: &str) -> Option<&'static str> {
    // Normalize: uppercase and replace underscores with hyphens
    let normalized_key = key.to_ascii_uppercase().replace('_', "-");

    // O(1) perfect hash lookup
    RULE_ALIAS_MAP.get(normalized_key.as_str()).copied()
}

/// Resolves a rule name to its canonical ID, supporting both rule IDs and aliases.
/// Returns the canonical ID (e.g., "MD001") for any valid input:
/// - "MD001" → "MD001" (canonical)
/// - "heading-increment" → "MD001" (alias)
/// - "HEADING_INCREMENT" → "MD001" (case-insensitive, underscore variant)
///
/// For unknown names, falls back to normalization (uppercase for MDxxx pattern, otherwise kebab-case).
pub fn resolve_rule_name(name: &str) -> String {
    resolve_rule_name_alias(name)
        .map(|s| s.to_string())
        .unwrap_or_else(|| normalize_key(name))
}

/// Resolves a comma-separated list of rule names to canonical IDs.
/// Handles CLI input like "MD001,line-length,heading-increment".
/// Empty entries and whitespace are filtered out.
pub fn resolve_rule_names(input: &str) -> std::collections::HashSet<String> {
    input
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(resolve_rule_name)
        .collect()
}

/// Represents a config validation warning or error
#[derive(Debug, Clone)]
pub struct ConfigValidationWarning {
    pub message: String,
    pub rule: Option<String>,
    pub key: Option<String>,
}

/// Internal validation function that works with any SourcedConfig state.
/// This is used by both the public `validate_config_sourced` and the typestate `validate()` method.
fn validate_config_sourced_internal<S>(
    sourced: &SourcedConfig<S>,
    registry: &RuleRegistry,
) -> Vec<ConfigValidationWarning> {
    validate_config_sourced_impl(&sourced.rules, &sourced.unknown_keys, registry)
}

/// Core validation implementation that doesn't depend on SourcedConfig type parameter.
fn validate_config_sourced_impl(
    rules: &BTreeMap<String, SourcedRuleConfig>,
    unknown_keys: &[(String, String, Option<String>)],
    registry: &RuleRegistry,
) -> Vec<ConfigValidationWarning> {
    let mut warnings = Vec::new();
    let known_rules = registry.rule_names();
    // 1. Unknown rules
    for rule in rules.keys() {
        if !known_rules.contains(rule) {
            // Include both canonical names AND aliases for fuzzy matching
            let all_rule_names: Vec<String> = RULE_ALIAS_MAP.keys().map(|s| s.to_string()).collect();
            let message = if let Some(suggestion) = suggest_similar_key(rule, &all_rule_names) {
                // Convert alias suggestions to lowercase for better UX (MD001 stays uppercase, ul-style becomes lowercase)
                let formatted_suggestion = if suggestion.starts_with("MD") {
                    suggestion
                } else {
                    suggestion.to_lowercase()
                };
                format!("Unknown rule in config: {rule} (did you mean: {formatted_suggestion}?)")
            } else {
                format!("Unknown rule in config: {rule}")
            };
            warnings.push(ConfigValidationWarning {
                message,
                rule: Some(rule.clone()),
                key: None,
            });
        }
    }
    // 2. Unknown options and type mismatches
    for (rule, rule_cfg) in rules {
        if let Some(valid_keys) = registry.config_keys_for(rule) {
            for key in rule_cfg.values.keys() {
                if !valid_keys.contains(key) {
                    let valid_keys_vec: Vec<String> = valid_keys.iter().cloned().collect();
                    let message = if let Some(suggestion) = suggest_similar_key(key, &valid_keys_vec) {
                        format!("Unknown option for rule {rule}: {key} (did you mean: {suggestion}?)")
                    } else {
                        format!("Unknown option for rule {rule}: {key}")
                    };
                    warnings.push(ConfigValidationWarning {
                        message,
                        rule: Some(rule.clone()),
                        key: Some(key.clone()),
                    });
                } else {
                    // Type check: compare type of value to type of default
                    if let Some(expected) = registry.expected_value_for(rule, key) {
                        let actual = &rule_cfg.values[key].value;
                        if !toml_value_type_matches(expected, actual) {
                            warnings.push(ConfigValidationWarning {
                                message: format!(
                                    "Type mismatch for {}.{}: expected {}, got {}",
                                    rule,
                                    key,
                                    toml_type_name(expected),
                                    toml_type_name(actual)
                                ),
                                rule: Some(rule.clone()),
                                key: Some(key.clone()),
                            });
                        }
                    }
                }
            }
        }
    }
    // 3. Unknown global options (from unknown_keys)
    let known_global_keys = vec![
        "enable".to_string(),
        "disable".to_string(),
        "include".to_string(),
        "exclude".to_string(),
        "respect-gitignore".to_string(),
        "line-length".to_string(),
        "fixable".to_string(),
        "unfixable".to_string(),
        "flavor".to_string(),
        "force-exclude".to_string(),
        "output-format".to_string(),
        "cache-dir".to_string(),
        "cache".to_string(),
    ];

    for (section, key, file_path) in unknown_keys {
        if section.contains("[global]") || section.contains("[tool.rumdl]") {
            let message = if let Some(suggestion) = suggest_similar_key(key, &known_global_keys) {
                if let Some(path) = file_path {
                    format!("Unknown global option in {path}: {key} (did you mean: {suggestion}?)")
                } else {
                    format!("Unknown global option: {key} (did you mean: {suggestion}?)")
                }
            } else if let Some(path) = file_path {
                format!("Unknown global option in {path}: {key}")
            } else {
                format!("Unknown global option: {key}")
            };
            warnings.push(ConfigValidationWarning {
                message,
                rule: None,
                key: Some(key.clone()),
            });
        } else if !key.is_empty() {
            // This is an unknown rule section (key is empty means it's a section header)
            continue;
        } else {
            // Unknown rule section - suggest similar rule names
            let rule_name = section.trim_matches(|c| c == '[' || c == ']');
            let all_rule_names: Vec<String> = RULE_ALIAS_MAP.keys().map(|s| s.to_string()).collect();
            let message = if let Some(suggestion) = suggest_similar_key(rule_name, &all_rule_names) {
                // Convert alias suggestions to lowercase for better UX (MD001 stays uppercase, ul-style becomes lowercase)
                let formatted_suggestion = if suggestion.starts_with("MD") {
                    suggestion
                } else {
                    suggestion.to_lowercase()
                };
                if let Some(path) = file_path {
                    format!("Unknown rule in {path}: {rule_name} (did you mean: {formatted_suggestion}?)")
                } else {
                    format!("Unknown rule in config: {rule_name} (did you mean: {formatted_suggestion}?)")
                }
            } else if let Some(path) = file_path {
                format!("Unknown rule in {path}: {rule_name}")
            } else {
                format!("Unknown rule in config: {rule_name}")
            };
            warnings.push(ConfigValidationWarning {
                message,
                rule: None,
                key: None,
            });
        }
    }
    warnings
}

/// Validate a loaded config against the rule registry, using SourcedConfig for unknown key tracking.
///
/// This is the legacy API that works with `SourcedConfig<ConfigLoaded>`.
/// For new code, prefer using `sourced.validate(&registry)` which returns a
/// `SourcedConfig<ConfigValidated>` that can be converted to `Config`.
pub fn validate_config_sourced(
    sourced: &SourcedConfig<ConfigLoaded>,
    registry: &RuleRegistry,
) -> Vec<ConfigValidationWarning> {
    validate_config_sourced_internal(sourced, registry)
}

/// Validate a config that has already been validated (no-op, returns stored warnings).
///
/// This exists for API consistency - validated configs already have their warnings stored.
pub fn validate_config_sourced_validated(
    sourced: &SourcedConfig<ConfigValidated>,
    _registry: &RuleRegistry,
) -> Vec<ConfigValidationWarning> {
    sourced.validation_warnings.clone()
}

fn toml_type_name(val: &toml::Value) -> &'static str {
    match val {
        toml::Value::String(_) => "string",
        toml::Value::Integer(_) => "integer",
        toml::Value::Float(_) => "float",
        toml::Value::Boolean(_) => "boolean",
        toml::Value::Array(_) => "array",
        toml::Value::Table(_) => "table",
        toml::Value::Datetime(_) => "datetime",
    }
}

/// Calculate Levenshtein distance between two strings (simple implementation)
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.len();
    let len2 = s2.len();

    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();

    let mut prev_row: Vec<usize> = (0..=len2).collect();
    let mut curr_row = vec![0; len2 + 1];

    for i in 1..=len1 {
        curr_row[0] = i;
        for j in 1..=len2 {
            let cost = if s1_chars[i - 1] == s2_chars[j - 1] { 0 } else { 1 };
            curr_row[j] = (prev_row[j] + 1)          // deletion
                .min(curr_row[j - 1] + 1)            // insertion
                .min(prev_row[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[len2]
}

/// Suggest a similar key from a list of valid keys using fuzzy matching
fn suggest_similar_key(unknown: &str, valid_keys: &[String]) -> Option<String> {
    let unknown_lower = unknown.to_lowercase();
    let max_distance = 2.max(unknown.len() / 3); // Allow up to 2 edits or 30% of string length

    let mut best_match: Option<(String, usize)> = None;

    for valid in valid_keys {
        let valid_lower = valid.to_lowercase();
        let distance = levenshtein_distance(&unknown_lower, &valid_lower);

        if distance <= max_distance {
            if let Some((_, best_dist)) = &best_match {
                if distance < *best_dist {
                    best_match = Some((valid.clone(), distance));
                }
            } else {
                best_match = Some((valid.clone(), distance));
            }
        }
    }

    best_match.map(|(key, _)| key)
}

fn toml_value_type_matches(expected: &toml::Value, actual: &toml::Value) -> bool {
    use toml::Value::*;
    match (expected, actual) {
        (String(_), String(_)) => true,
        (Integer(_), Integer(_)) => true,
        (Float(_), Float(_)) => true,
        (Boolean(_), Boolean(_)) => true,
        (Array(_), Array(_)) => true,
        (Table(_), Table(_)) => true,
        (Datetime(_), Datetime(_)) => true,
        // Allow integer for float
        (Float(_), Integer(_)) => true,
        _ => false,
    }
}

/// Parses pyproject.toml content and extracts the [tool.rumdl] section if present.
fn parse_pyproject_toml(content: &str, path: &str) -> Result<Option<SourcedConfigFragment>, ConfigError> {
    let doc: toml::Value =
        toml::from_str(content).map_err(|e| ConfigError::ParseError(format!("{path}: Failed to parse TOML: {e}")))?;
    let mut fragment = SourcedConfigFragment::default();
    let source = ConfigSource::PyprojectToml;
    let file = Some(path.to_string());

    // Create rule registry for alias resolution
    let all_rules = rules::all_rules(&Config::default());
    let registry = RuleRegistry::from_rules(&all_rules);

    // 1. Handle [tool.rumdl] and [tool.rumdl.global] sections
    if let Some(rumdl_config) = doc.get("tool").and_then(|t| t.get("rumdl"))
        && let Some(rumdl_table) = rumdl_config.as_table()
    {
        // Helper function to extract global config from a table
        let extract_global_config = |fragment: &mut SourcedConfigFragment, table: &toml::value::Table| {
            // Extract global options from the given table
            if let Some(enable) = table.get("enable")
                && let Ok(values) = Vec::<String>::deserialize(enable.clone())
            {
                // Resolve rule name aliases (e.g., "ul-style" -> "MD004")
                let normalized_values = values
                    .into_iter()
                    .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
                    .collect();
                fragment
                    .global
                    .enable
                    .push_override(normalized_values, source, file.clone(), None);
            }

            if let Some(disable) = table.get("disable")
                && let Ok(values) = Vec::<String>::deserialize(disable.clone())
            {
                // Resolve rule name aliases
                let normalized_values: Vec<String> = values
                    .into_iter()
                    .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
                    .collect();
                fragment
                    .global
                    .disable
                    .push_override(normalized_values, source, file.clone(), None);
            }

            if let Some(include) = table.get("include")
                && let Ok(values) = Vec::<String>::deserialize(include.clone())
            {
                fragment
                    .global
                    .include
                    .push_override(values, source, file.clone(), None);
            }

            if let Some(exclude) = table.get("exclude")
                && let Ok(values) = Vec::<String>::deserialize(exclude.clone())
            {
                fragment
                    .global
                    .exclude
                    .push_override(values, source, file.clone(), None);
            }

            if let Some(respect_gitignore) = table
                .get("respect-gitignore")
                .or_else(|| table.get("respect_gitignore"))
                && let Ok(value) = bool::deserialize(respect_gitignore.clone())
            {
                fragment
                    .global
                    .respect_gitignore
                    .push_override(value, source, file.clone(), None);
            }

            if let Some(force_exclude) = table.get("force-exclude").or_else(|| table.get("force_exclude"))
                && let Ok(value) = bool::deserialize(force_exclude.clone())
            {
                fragment
                    .global
                    .force_exclude
                    .push_override(value, source, file.clone(), None);
            }

            if let Some(output_format) = table.get("output-format").or_else(|| table.get("output_format"))
                && let Ok(value) = String::deserialize(output_format.clone())
            {
                if fragment.global.output_format.is_none() {
                    fragment.global.output_format = Some(SourcedValue::new(value.clone(), source));
                } else {
                    fragment
                        .global
                        .output_format
                        .as_mut()
                        .unwrap()
                        .push_override(value, source, file.clone(), None);
                }
            }

            if let Some(fixable) = table.get("fixable")
                && let Ok(values) = Vec::<String>::deserialize(fixable.clone())
            {
                let normalized_values = values
                    .into_iter()
                    .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
                    .collect();
                fragment
                    .global
                    .fixable
                    .push_override(normalized_values, source, file.clone(), None);
            }

            if let Some(unfixable) = table.get("unfixable")
                && let Ok(values) = Vec::<String>::deserialize(unfixable.clone())
            {
                let normalized_values = values
                    .into_iter()
                    .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
                    .collect();
                fragment
                    .global
                    .unfixable
                    .push_override(normalized_values, source, file.clone(), None);
            }

            if let Some(flavor) = table.get("flavor")
                && let Ok(value) = MarkdownFlavor::deserialize(flavor.clone())
            {
                fragment.global.flavor.push_override(value, source, file.clone(), None);
            }

            // Handle line-length special case - this should set the global line_length
            if let Some(line_length) = table.get("line-length").or_else(|| table.get("line_length"))
                && let Ok(value) = u64::deserialize(line_length.clone())
            {
                fragment
                    .global
                    .line_length
                    .push_override(LineLength::new(value as usize), source, file.clone(), None);

                // Also add to MD013 rule config for backward compatibility
                let norm_md013_key = normalize_key("MD013");
                let rule_entry = fragment.rules.entry(norm_md013_key).or_default();
                let norm_line_length_key = normalize_key("line-length");
                let sv = rule_entry
                    .values
                    .entry(norm_line_length_key)
                    .or_insert_with(|| SourcedValue::new(line_length.clone(), ConfigSource::Default));
                sv.push_override(line_length.clone(), source, file.clone(), None);
            }

            if let Some(cache_dir) = table.get("cache-dir").or_else(|| table.get("cache_dir"))
                && let Ok(value) = String::deserialize(cache_dir.clone())
            {
                if fragment.global.cache_dir.is_none() {
                    fragment.global.cache_dir = Some(SourcedValue::new(value.clone(), source));
                } else {
                    fragment
                        .global
                        .cache_dir
                        .as_mut()
                        .unwrap()
                        .push_override(value, source, file.clone(), None);
                }
            }

            if let Some(cache) = table.get("cache")
                && let Ok(value) = bool::deserialize(cache.clone())
            {
                fragment.global.cache.push_override(value, source, file.clone(), None);
            }
        };

        // First, check for [tool.rumdl.global] section
        if let Some(global_table) = rumdl_table.get("global").and_then(|g| g.as_table()) {
            extract_global_config(&mut fragment, global_table);
        }

        // Also extract global options from [tool.rumdl] directly (for flat structure)
        extract_global_config(&mut fragment, rumdl_table);

        // --- Extract per-file-ignores configurations ---
        // Check both hyphenated and underscored versions for compatibility
        let per_file_ignores_key = rumdl_table
            .get("per-file-ignores")
            .or_else(|| rumdl_table.get("per_file_ignores"));

        if let Some(per_file_ignores_value) = per_file_ignores_key
            && let Some(per_file_table) = per_file_ignores_value.as_table()
        {
            let mut per_file_map = HashMap::new();
            for (pattern, rules_value) in per_file_table {
                if let Ok(rules) = Vec::<String>::deserialize(rules_value.clone()) {
                    let normalized_rules = rules
                        .into_iter()
                        .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
                        .collect();
                    per_file_map.insert(pattern.clone(), normalized_rules);
                } else {
                    log::warn!(
                        "[WARN] Expected array for per-file-ignores pattern '{pattern}' in {path}, found {rules_value:?}"
                    );
                }
            }
            fragment
                .per_file_ignores
                .push_override(per_file_map, source, file.clone(), None);
        }

        // --- Extract rule-specific configurations ---
        for (key, value) in rumdl_table {
            let norm_rule_key = normalize_key(key);

            // Skip keys already handled as global or special cases
            // Note: Only skip these if they're NOT tables (rule sections are tables)
            let is_global_key = [
                "enable",
                "disable",
                "include",
                "exclude",
                "respect_gitignore",
                "respect-gitignore",
                "force_exclude",
                "force-exclude",
                "output_format",
                "output-format",
                "fixable",
                "unfixable",
                "per-file-ignores",
                "per_file_ignores",
                "global",
                "flavor",
                "cache_dir",
                "cache-dir",
                "cache",
            ]
            .contains(&norm_rule_key.as_str());

            // Special handling for line-length: could be global config OR rule section
            let is_line_length_global =
                (norm_rule_key == "line-length" || norm_rule_key == "line_length") && !value.is_table();

            if is_global_key || is_line_length_global {
                continue;
            }

            // Try to resolve as a rule name (handles both canonical names and aliases)
            if let Some(resolved_rule_name) = registry.resolve_rule_name(key)
                && value.is_table()
                && let Some(rule_config_table) = value.as_table()
            {
                let rule_entry = fragment.rules.entry(resolved_rule_name.clone()).or_default();
                for (rk, rv) in rule_config_table {
                    let norm_rk = normalize_key(rk);

                    // Special handling for severity - store in rule_entry.severity
                    if norm_rk == "severity" {
                        if let Ok(severity) = crate::rule::Severity::deserialize(rv.clone()) {
                            if rule_entry.severity.is_none() {
                                rule_entry.severity = Some(SourcedValue::new(severity, source));
                            } else {
                                rule_entry.severity.as_mut().unwrap().push_override(
                                    severity,
                                    source,
                                    file.clone(),
                                    None,
                                );
                            }
                        }
                        continue; // Skip regular value processing for severity
                    }

                    let toml_val = rv.clone();

                    let sv = rule_entry
                        .values
                        .entry(norm_rk.clone())
                        .or_insert_with(|| SourcedValue::new(toml_val.clone(), ConfigSource::Default));
                    sv.push_override(toml_val, source, file.clone(), None);
                }
            } else if registry.resolve_rule_name(key).is_none() {
                // Key is not a global/special key and not a recognized rule name
                // Track unknown keys under [tool.rumdl] for validation
                fragment
                    .unknown_keys
                    .push(("[tool.rumdl]".to_string(), key.to_string(), Some(path.to_string())));
            }
        }
    }

    // 2. Handle [tool.rumdl.MDxxx] sections as rule-specific config (nested under [tool])
    if let Some(tool_table) = doc.get("tool").and_then(|t| t.as_table()) {
        for (key, value) in tool_table.iter() {
            if let Some(rule_name) = key.strip_prefix("rumdl.") {
                // Try to resolve as a rule name (handles both canonical names and aliases)
                if let Some(resolved_rule_name) = registry.resolve_rule_name(rule_name) {
                    if let Some(rule_table) = value.as_table() {
                        let rule_entry = fragment.rules.entry(resolved_rule_name.clone()).or_default();
                        for (rk, rv) in rule_table {
                            let norm_rk = normalize_key(rk);

                            // Special handling for severity - store in rule_entry.severity
                            if norm_rk == "severity" {
                                if let Ok(severity) = crate::rule::Severity::deserialize(rv.clone()) {
                                    if rule_entry.severity.is_none() {
                                        rule_entry.severity = Some(SourcedValue::new(severity, source));
                                    } else {
                                        rule_entry.severity.as_mut().unwrap().push_override(
                                            severity,
                                            source,
                                            file.clone(),
                                            None,
                                        );
                                    }
                                }
                                continue; // Skip regular value processing for severity
                            }

                            let toml_val = rv.clone();
                            let sv = rule_entry
                                .values
                                .entry(norm_rk.clone())
                                .or_insert_with(|| SourcedValue::new(toml_val.clone(), source));
                            sv.push_override(toml_val, source, file.clone(), None);
                        }
                    }
                } else if rule_name.to_ascii_uppercase().starts_with("MD")
                    || rule_name.chars().any(|c| c.is_alphabetic())
                {
                    // Track unknown rule sections like [tool.rumdl.MD999] or [tool.rumdl.unknown-rule]
                    fragment.unknown_keys.push((
                        format!("[tool.rumdl.{rule_name}]"),
                        String::new(),
                        Some(path.to_string()),
                    ));
                }
            }
        }
    }

    // 3. Handle [tool.rumdl.MDxxx] sections as top-level keys (e.g., [tool.rumdl.MD007] or [tool.rumdl.line-length])
    if let Some(doc_table) = doc.as_table() {
        for (key, value) in doc_table.iter() {
            if let Some(rule_name) = key.strip_prefix("tool.rumdl.") {
                // Try to resolve as a rule name (handles both canonical names and aliases)
                if let Some(resolved_rule_name) = registry.resolve_rule_name(rule_name) {
                    if let Some(rule_table) = value.as_table() {
                        let rule_entry = fragment.rules.entry(resolved_rule_name.clone()).or_default();
                        for (rk, rv) in rule_table {
                            let norm_rk = normalize_key(rk);

                            // Special handling for severity - store in rule_entry.severity
                            if norm_rk == "severity" {
                                if let Ok(severity) = crate::rule::Severity::deserialize(rv.clone()) {
                                    if rule_entry.severity.is_none() {
                                        rule_entry.severity = Some(SourcedValue::new(severity, source));
                                    } else {
                                        rule_entry.severity.as_mut().unwrap().push_override(
                                            severity,
                                            source,
                                            file.clone(),
                                            None,
                                        );
                                    }
                                }
                                continue; // Skip regular value processing for severity
                            }

                            let toml_val = rv.clone();
                            let sv = rule_entry
                                .values
                                .entry(norm_rk.clone())
                                .or_insert_with(|| SourcedValue::new(toml_val.clone(), source));
                            sv.push_override(toml_val, source, file.clone(), None);
                        }
                    }
                } else if rule_name.to_ascii_uppercase().starts_with("MD")
                    || rule_name.chars().any(|c| c.is_alphabetic())
                {
                    // Track unknown rule sections like [tool.rumdl.MD999] or [tool.rumdl.unknown-rule]
                    fragment.unknown_keys.push((
                        format!("[tool.rumdl.{rule_name}]"),
                        String::new(),
                        Some(path.to_string()),
                    ));
                }
            }
        }
    }

    // Only return Some(fragment) if any config was found
    let has_any = !fragment.global.enable.value.is_empty()
        || !fragment.global.disable.value.is_empty()
        || !fragment.global.include.value.is_empty()
        || !fragment.global.exclude.value.is_empty()
        || !fragment.global.fixable.value.is_empty()
        || !fragment.global.unfixable.value.is_empty()
        || fragment.global.output_format.is_some()
        || fragment.global.cache_dir.is_some()
        || !fragment.global.cache.value
        || !fragment.per_file_ignores.value.is_empty()
        || !fragment.rules.is_empty();
    if has_any { Ok(Some(fragment)) } else { Ok(None) }
}

/// Parses rumdl.toml / .rumdl.toml content.
fn parse_rumdl_toml(content: &str, path: &str, source: ConfigSource) -> Result<SourcedConfigFragment, ConfigError> {
    let doc = content
        .parse::<DocumentMut>()
        .map_err(|e| ConfigError::ParseError(format!("{path}: Failed to parse TOML: {e}")))?;
    let mut fragment = SourcedConfigFragment::default();
    // source parameter provided by caller
    let file = Some(path.to_string());

    // Define known rules before the loop
    let all_rules = rules::all_rules(&Config::default());
    let registry = RuleRegistry::from_rules(&all_rules);

    // Handle [global] section
    if let Some(global_item) = doc.get("global")
        && let Some(global_table) = global_item.as_table()
    {
        for (key, value_item) in global_table.iter() {
            let norm_key = normalize_key(key);
            match norm_key.as_str() {
                "enable" | "disable" | "include" | "exclude" => {
                    if let Some(toml_edit::Value::Array(formatted_array)) = value_item.as_value() {
                        // Corrected: Iterate directly over the Formatted<Array>
                        let values: Vec<String> = formatted_array
                                .iter()
                                .filter_map(|item| item.as_str()) // Extract strings
                                .map(|s| s.to_string())
                                .collect();

                        // Resolve rule name aliases for enable/disable (e.g., "ul-style" -> "MD004")
                        let final_values = if norm_key == "enable" || norm_key == "disable" {
                            values
                                .into_iter()
                                .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
                                .collect()
                        } else {
                            values
                        };

                        match norm_key.as_str() {
                            "enable" => fragment
                                .global
                                .enable
                                .push_override(final_values, source, file.clone(), None),
                            "disable" => {
                                fragment
                                    .global
                                    .disable
                                    .push_override(final_values, source, file.clone(), None)
                            }
                            "include" => {
                                fragment
                                    .global
                                    .include
                                    .push_override(final_values, source, file.clone(), None)
                            }
                            "exclude" => {
                                fragment
                                    .global
                                    .exclude
                                    .push_override(final_values, source, file.clone(), None)
                            }
                            _ => unreachable!("Outer match guarantees only enable/disable/include/exclude"),
                        }
                    } else {
                        log::warn!(
                            "[WARN] Expected array for global key '{}' in {}, found {}",
                            key,
                            path,
                            value_item.type_name()
                        );
                    }
                }
                "respect_gitignore" | "respect-gitignore" => {
                    // Handle both cases
                    if let Some(toml_edit::Value::Boolean(formatted_bool)) = value_item.as_value() {
                        let val = *formatted_bool.value();
                        fragment
                            .global
                            .respect_gitignore
                            .push_override(val, source, file.clone(), None);
                    } else {
                        log::warn!(
                            "[WARN] Expected boolean for global key '{}' in {}, found {}",
                            key,
                            path,
                            value_item.type_name()
                        );
                    }
                }
                "force_exclude" | "force-exclude" => {
                    // Handle both cases
                    if let Some(toml_edit::Value::Boolean(formatted_bool)) = value_item.as_value() {
                        let val = *formatted_bool.value();
                        fragment
                            .global
                            .force_exclude
                            .push_override(val, source, file.clone(), None);
                    } else {
                        log::warn!(
                            "[WARN] Expected boolean for global key '{}' in {}, found {}",
                            key,
                            path,
                            value_item.type_name()
                        );
                    }
                }
                "line_length" | "line-length" => {
                    // Handle both cases
                    if let Some(toml_edit::Value::Integer(formatted_int)) = value_item.as_value() {
                        let val = LineLength::new(*formatted_int.value() as usize);
                        fragment
                            .global
                            .line_length
                            .push_override(val, source, file.clone(), None);
                    } else {
                        log::warn!(
                            "[WARN] Expected integer for global key '{}' in {}, found {}",
                            key,
                            path,
                            value_item.type_name()
                        );
                    }
                }
                "output_format" | "output-format" => {
                    // Handle both cases
                    if let Some(toml_edit::Value::String(formatted_string)) = value_item.as_value() {
                        let val = formatted_string.value().clone();
                        if fragment.global.output_format.is_none() {
                            fragment.global.output_format = Some(SourcedValue::new(val.clone(), source));
                        } else {
                            fragment.global.output_format.as_mut().unwrap().push_override(
                                val,
                                source,
                                file.clone(),
                                None,
                            );
                        }
                    } else {
                        log::warn!(
                            "[WARN] Expected string for global key '{}' in {}, found {}",
                            key,
                            path,
                            value_item.type_name()
                        );
                    }
                }
                "cache_dir" | "cache-dir" => {
                    // Handle both cases
                    if let Some(toml_edit::Value::String(formatted_string)) = value_item.as_value() {
                        let val = formatted_string.value().clone();
                        if fragment.global.cache_dir.is_none() {
                            fragment.global.cache_dir = Some(SourcedValue::new(val.clone(), source));
                        } else {
                            fragment
                                .global
                                .cache_dir
                                .as_mut()
                                .unwrap()
                                .push_override(val, source, file.clone(), None);
                        }
                    } else {
                        log::warn!(
                            "[WARN] Expected string for global key '{}' in {}, found {}",
                            key,
                            path,
                            value_item.type_name()
                        );
                    }
                }
                "cache" => {
                    if let Some(toml_edit::Value::Boolean(b)) = value_item.as_value() {
                        let val = *b.value();
                        fragment.global.cache.push_override(val, source, file.clone(), None);
                    } else {
                        log::warn!(
                            "[WARN] Expected boolean for global key '{}' in {}, found {}",
                            key,
                            path,
                            value_item.type_name()
                        );
                    }
                }
                "fixable" => {
                    if let Some(toml_edit::Value::Array(formatted_array)) = value_item.as_value() {
                        let values: Vec<String> = formatted_array
                            .iter()
                            .filter_map(|item| item.as_str())
                            .map(normalize_key)
                            .collect();
                        fragment
                            .global
                            .fixable
                            .push_override(values, source, file.clone(), None);
                    } else {
                        log::warn!(
                            "[WARN] Expected array for global key '{}' in {}, found {}",
                            key,
                            path,
                            value_item.type_name()
                        );
                    }
                }
                "unfixable" => {
                    if let Some(toml_edit::Value::Array(formatted_array)) = value_item.as_value() {
                        let values: Vec<String> = formatted_array
                            .iter()
                            .filter_map(|item| item.as_str())
                            .map(|s| registry.resolve_rule_name(s).unwrap_or_else(|| normalize_key(s)))
                            .collect();
                        fragment
                            .global
                            .unfixable
                            .push_override(values, source, file.clone(), None);
                    } else {
                        log::warn!(
                            "[WARN] Expected array for global key '{}' in {}, found {}",
                            key,
                            path,
                            value_item.type_name()
                        );
                    }
                }
                "flavor" => {
                    if let Some(toml_edit::Value::String(formatted_string)) = value_item.as_value() {
                        let val = formatted_string.value();
                        if let Ok(flavor) = MarkdownFlavor::from_str(val) {
                            fragment.global.flavor.push_override(flavor, source, file.clone(), None);
                        } else {
                            log::warn!("[WARN] Unknown markdown flavor '{val}' in {path}");
                        }
                    } else {
                        log::warn!(
                            "[WARN] Expected string for global key '{}' in {}, found {}",
                            key,
                            path,
                            value_item.type_name()
                        );
                    }
                }
                _ => {
                    // Track unknown global keys for validation
                    fragment
                        .unknown_keys
                        .push(("[global]".to_string(), key.to_string(), Some(path.to_string())));
                    log::warn!("[WARN] Unknown key in [global] section of {path}: {key}");
                }
            }
        }
    }

    // Handle [per-file-ignores] section
    if let Some(per_file_item) = doc.get("per-file-ignores")
        && let Some(per_file_table) = per_file_item.as_table()
    {
        let mut per_file_map = HashMap::new();
        for (pattern, value_item) in per_file_table.iter() {
            if let Some(toml_edit::Value::Array(formatted_array)) = value_item.as_value() {
                let rules: Vec<String> = formatted_array
                    .iter()
                    .filter_map(|item| item.as_str())
                    .map(|s| registry.resolve_rule_name(s).unwrap_or_else(|| normalize_key(s)))
                    .collect();
                per_file_map.insert(pattern.to_string(), rules);
            } else {
                let type_name = value_item.type_name();
                log::warn!(
                    "[WARN] Expected array for per-file-ignores pattern '{pattern}' in {path}, found {type_name}"
                );
            }
        }
        fragment
            .per_file_ignores
            .push_override(per_file_map, source, file.clone(), None);
    }

    // Rule-specific: all other top-level tables
    for (key, item) in doc.iter() {
        // Skip known special sections
        if key == "global" || key == "per-file-ignores" {
            continue;
        }

        // Resolve rule name (handles both canonical names like "MD004" and aliases like "ul-style")
        let norm_rule_name = if let Some(resolved) = registry.resolve_rule_name(key) {
            resolved
        } else {
            // Unknown rule - always track it for validation and suggestions
            fragment
                .unknown_keys
                .push((format!("[{key}]"), String::new(), Some(path.to_string())));
            continue;
        };

        if let Some(tbl) = item.as_table() {
            let rule_entry = fragment.rules.entry(norm_rule_name.clone()).or_default();
            for (rk, rv_item) in tbl.iter() {
                let norm_rk = normalize_key(rk);

                // Special handling for severity - store in rule_entry.severity
                if norm_rk == "severity" {
                    if let Some(toml_edit::Value::String(formatted_string)) = rv_item.as_value() {
                        let severity_str = formatted_string.value();
                        match crate::rule::Severity::deserialize(toml::Value::String(severity_str.to_string())) {
                            Ok(severity) => {
                                if rule_entry.severity.is_none() {
                                    rule_entry.severity = Some(SourcedValue::new(severity, source));
                                } else {
                                    rule_entry.severity.as_mut().unwrap().push_override(
                                        severity,
                                        source,
                                        file.clone(),
                                        None,
                                    );
                                }
                            }
                            Err(_) => {
                                log::warn!(
                                    "[WARN] Invalid severity '{severity_str}' for rule {norm_rule_name} in {path}. Valid values: error, warning"
                                );
                            }
                        }
                    }
                    continue; // Skip regular value processing for severity
                }

                let maybe_toml_val: Option<toml::Value> = match rv_item.as_value() {
                    Some(toml_edit::Value::String(formatted)) => Some(toml::Value::String(formatted.value().clone())),
                    Some(toml_edit::Value::Integer(formatted)) => Some(toml::Value::Integer(*formatted.value())),
                    Some(toml_edit::Value::Float(formatted)) => Some(toml::Value::Float(*formatted.value())),
                    Some(toml_edit::Value::Boolean(formatted)) => Some(toml::Value::Boolean(*formatted.value())),
                    Some(toml_edit::Value::Datetime(formatted)) => Some(toml::Value::Datetime(*formatted.value())),
                    Some(toml_edit::Value::Array(formatted_array)) => {
                        // Convert toml_edit Array to toml::Value::Array
                        let mut values = Vec::new();
                        for item in formatted_array.iter() {
                            match item {
                                toml_edit::Value::String(formatted) => {
                                    values.push(toml::Value::String(formatted.value().clone()))
                                }
                                toml_edit::Value::Integer(formatted) => {
                                    values.push(toml::Value::Integer(*formatted.value()))
                                }
                                toml_edit::Value::Float(formatted) => {
                                    values.push(toml::Value::Float(*formatted.value()))
                                }
                                toml_edit::Value::Boolean(formatted) => {
                                    values.push(toml::Value::Boolean(*formatted.value()))
                                }
                                toml_edit::Value::Datetime(formatted) => {
                                    values.push(toml::Value::Datetime(*formatted.value()))
                                }
                                _ => {
                                    log::warn!(
                                        "[WARN] Skipping unsupported array element type in key '{norm_rule_name}.{norm_rk}' in {path}"
                                    );
                                }
                            }
                        }
                        Some(toml::Value::Array(values))
                    }
                    Some(toml_edit::Value::InlineTable(_)) => {
                        log::warn!(
                            "[WARN] Skipping inline table value for key '{norm_rule_name}.{norm_rk}' in {path}. Table conversion not yet fully implemented in parser."
                        );
                        None
                    }
                    None => {
                        log::warn!(
                            "[WARN] Skipping non-value item for key '{norm_rule_name}.{norm_rk}' in {path}. Expected simple value."
                        );
                        None
                    }
                };
                if let Some(toml_val) = maybe_toml_val {
                    let sv = rule_entry
                        .values
                        .entry(norm_rk.clone())
                        .or_insert_with(|| SourcedValue::new(toml_val.clone(), ConfigSource::Default));
                    sv.push_override(toml_val, source, file.clone(), None);
                }
            }
        } else if item.is_value() {
            log::warn!("[WARN] Ignoring top-level value key in {path}: '{key}'. Expected a table like [{key}].");
        }
    }

    Ok(fragment)
}

/// Loads and converts a markdownlint config file (.json or .yaml) into a SourcedConfigFragment.
fn load_from_markdownlint(path: &str) -> Result<SourcedConfigFragment, ConfigError> {
    // Use the unified loader from markdownlint_config.rs
    let ml_config = crate::markdownlint_config::load_markdownlint_config(path)
        .map_err(|e| ConfigError::ParseError(format!("{path}: {e}")))?;
    Ok(ml_config.map_to_sourced_rumdl_config_fragment(Some(path)))
}

#[cfg(test)]
#[path = "config_intelligent_merge_tests.rs"]
mod config_intelligent_merge_tests;
