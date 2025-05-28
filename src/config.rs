//!
//! This module defines configuration structures, loading logic, and provenance tracking for rumdl.
//! Supports TOML, pyproject.toml, and markdownlint config formats, and provides merging and override logic.

use crate::rule::Rule;
use crate::rules;
use lazy_static::lazy_static;
use log;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::Path;
use toml_edit::DocumentMut;

lazy_static! {
    // Map common markdownlint config keys to rumdl rule names
    static ref MARKDOWNLINT_KEY_MAP: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        // Add mappings based on common markdownlint config names
        // From https://github.com/DavidAnson/markdownlint/blob/main/schema/.markdownlint.jsonc
        m.insert("ul-style", "md004");
        m.insert("code-block-style", "md046");
        m.insert("ul-indent", "md007"); // Example
        m.insert("line-length", "md013"); // Example of a common one that might be top-level
        // Add more mappings as needed based on markdownlint schema or observed usage
        m
    };
}

/// Normalizes configuration keys (rule names, option names) to lowercase kebab-case.
pub fn normalize_key(key: &str) -> String {
    // If the key looks like a rule name (e.g., MD013), uppercase it
    if key.len() == 5
        && key.to_ascii_lowercase().starts_with("md")
        && key[2..].chars().all(|c| c.is_ascii_digit())
    {
        key.to_ascii_uppercase()
    } else {
        key.replace('_', "-").to_ascii_lowercase()
    }
}

/// Represents a rule-specific configuration
#[derive(Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct RuleConfig {
    /// Configuration values for the rule
    #[serde(flatten)]
    pub values: BTreeMap<String, toml::Value>,
}

/// Represents the complete configuration loaded from rumdl.toml
#[derive(Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Config {
    /// Global configuration options
    #[serde(default)]
    pub global: GlobalConfig,

    /// Rule-specific configurations
    #[serde(flatten)]
    pub rules: BTreeMap<String, RuleConfig>,
}

/// Global configuration options
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
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
    #[serde(default = "default_respect_gitignore")]
    pub respect_gitignore: bool,
}

fn default_respect_gitignore() -> bool {
    true
}

// Add the Default impl
impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            enable: Vec::new(),
            disable: Vec::new(),
            exclude: Vec::new(),
            include: Vec::new(),
            respect_gitignore: true,
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
        return Err(ConfigError::FileExists {
            path: path.to_string(),
        });
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
respect_gitignore = true

# Rule-specific configurations (uncomment and modify as needed)

# [MD003]
# style = "atx"  # Heading style (atx, atx_closed, setext)

# [MD004]
# style = "asterisk"  # Unordered list style (asterisk, plus, dash, consistent)

# [MD007]
# indent = 4  # Unordered list indentation

# [MD013]
# line_length = 100  # Line length
# code_blocks = false  # Exclude code blocks from line length check
# tables = false  # Exclude tables from line length check
# headings = true  # Include headings in line length check

# [MD044]
# names = ["rumdl", "Markdown", "GitHub"]  # Proper names that should be capitalized correctly
# code_blocks_excluded = true  # Exclude code blocks from proper name check
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
pub fn get_rule_config_value<T: serde::de::DeserializeOwned>(
    config: &Config,
    rule_name: &str,
    key: &str,
) -> Option<T> {
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
        if let Some(value) = rule_config.values.get(variant) {
            if let Ok(result) = T::deserialize(value.clone()) {
                return Some(result);
            }
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
# line_length = 100  # Line length
# code_blocks = false  # Exclude code blocks from line length check
# tables = false  # Exclude tables from line length check
# headings = true  # Include headings in line length check

# [tool.rumdl.MD044]
# names = ["rumdl", "Markdown", "GitHub"]  # Proper names that should be capitalized correctly
# code_blocks_excluded = true  # Exclude code blocks from proper name check
"#;

    config_content.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

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
        let sourced =
            SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
                .unwrap();
        let config: Config = sourced.into(); // Convert to plain config for assertions

        // Check global settings
        assert_eq!(config.global.disable, vec!["MD033".to_string()]);
        assert_eq!(
            config.global.enable,
            vec!["MD001".to_string(), "MD004".to_string()]
        );
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
        let sourced =
            SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
                .unwrap();
        let config: Config = sourced.into(); // Convert to plain config for assertions

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
        let sourced =
            SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
                .unwrap();
        // DEBUG: Print all rule keys and their value keys
        log::debug!(
            "[DEBUG] All rules loaded: {:?}",
            sourced.rules.keys().collect::<Vec<_>>()
        );
        for (rule, cfg) in &sourced.rules {
            log::debug!(
                "[DEBUG] Rule '{}' value keys: {:?}",
                rule,
                cfg.values.keys().collect::<Vec<_>>()
            );
        }
        let rule_cfg = sourced
            .rules
            .get("MD013")
            .expect("MD013 rule config should exist");
        // Now we should only get the explicitly configured key
        let keys: Vec<_> = rule_cfg.values.keys().cloned().collect();
        assert_eq!(keys, vec!["line-length"]);
        let val = &rule_cfg.values["line-length"].value;
        assert_eq!(val.as_integer(), Some(222));
        // get_rule_config_value should retrieve the value for both snake_case and kebab-case
        let config: Config = sourced.clone().into();
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
        let sourced =
            SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
                .unwrap();
        let config: Config = sourced.clone().into();
        // Only the last section should win, and be present
        let rule_cfg = sourced
            .rules
            .get("MD013")
            .expect("MD013 rule config should exist");
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
        let sourced =
            SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
                .unwrap();
        let config: Config = sourced.clone().into();
        let rule_cfg = sourced
            .rules
            .get("MD013")
            .expect("MD013 rule config should exist");
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
        let sourced =
            SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
                .unwrap();
        let config: Config = sourced.clone().into();
        // MD999 should not be present
        assert!(!sourced.rules.contains_key("MD999"));
        // MD013 should be present and correct
        let v = get_rule_config_value::<usize>(&config, "MD013", "line-length");
        assert_eq!(v, Some(303));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    Default,
    RumdlToml,
    PyprojectToml,
    Cli,
    /// Value was loaded from a markdownlint config file (e.g. .markdownlint.json, .markdownlint.yaml)
    Markdownlint,
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
        // TODO: Add new_line Option<usize> if needed later
    ) {
        // Helper function to get precedence, defined locally or globally
        fn source_precedence(src: ConfigSource) -> u8 {
            match src {
                ConfigSource::Default => 0,
                ConfigSource::PyprojectToml => 1,
                ConfigSource::Markdownlint => 2,
                ConfigSource::RumdlToml => 3,
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
                line: None, // Placeholder for now
            });
        }
    }

    pub fn push_override(
        &mut self,
        value: T,
        source: ConfigSource,
        file: Option<String>,
        line: Option<usize>,
    ) {
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

#[derive(Debug, Clone)]
pub struct SourcedGlobalConfig {
    pub enable: SourcedValue<Vec<String>>,
    pub disable: SourcedValue<Vec<String>>,
    pub exclude: SourcedValue<Vec<String>>,
    pub include: SourcedValue<Vec<String>>,
    pub respect_gitignore: SourcedValue<bool>,
}

impl Default for SourcedGlobalConfig {
    fn default() -> Self {
        SourcedGlobalConfig {
            enable: SourcedValue::new(Vec::new(), ConfigSource::Default),
            disable: SourcedValue::new(Vec::new(), ConfigSource::Default),
            exclude: SourcedValue::new(Vec::new(), ConfigSource::Default),
            include: SourcedValue::new(Vec::new(), ConfigSource::Default),
            respect_gitignore: SourcedValue::new(true, ConfigSource::Default),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SourcedRuleConfig {
    pub values: BTreeMap<String, SourcedValue<toml::Value>>,
}

/// Represents configuration loaded from a single source file, with provenance.
/// Used as an intermediate step before merging into the final SourcedConfig.
#[derive(Debug, Default, Clone)]
pub struct SourcedConfigFragment {
    pub global: SourcedGlobalConfig,
    pub rules: BTreeMap<String, SourcedRuleConfig>,
    // Note: Does not include loaded_files or unknown_keys, as those are tracked globally.
}

#[derive(Debug, Default, Clone)]
pub struct SourcedConfig {
    pub global: SourcedGlobalConfig,
    pub rules: BTreeMap<String, SourcedRuleConfig>,
    pub loaded_files: Vec<String>,
    pub unknown_keys: Vec<(String, String)>, // (section, key)
}

impl SourcedConfig {
    /// Merges another SourcedConfigFragment into this SourcedConfig.
    /// Uses source precedence to determine which values take effect.
    fn merge(&mut self, fragment: SourcedConfigFragment) {
        // Merge global config
        self.global.enable.merge_override(
            fragment.global.enable.value,
            fragment.global.enable.source,
            fragment
                .global
                .enable
                .overrides
                .first()
                .and_then(|o| o.file.clone()),
        );
        self.global.disable.merge_override(
            fragment.global.disable.value,
            fragment.global.disable.source,
            fragment
                .global
                .disable
                .overrides
                .first()
                .and_then(|o| o.file.clone()),
        );
        self.global.include.merge_override(
            fragment.global.include.value,
            fragment.global.include.source,
            fragment
                .global
                .include
                .overrides
                .first()
                .and_then(|o| o.file.clone()),
        );
        self.global.exclude.merge_override(
            fragment.global.exclude.value,
            fragment.global.exclude.source,
            fragment
                .global
                .exclude
                .overrides
                .first()
                .and_then(|o| o.file.clone()),
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
        );

        // Merge rule configs
        for (rule_name, rule_fragment) in fragment.rules {
            let norm_rule_name = rule_name.to_ascii_uppercase(); // Normalize to uppercase for case-insensitivity
            let rule_entry = self.rules.entry(norm_rule_name).or_default();
            for (key, sourced_value_fragment) in rule_fragment.values {
                let sv_entry = rule_entry.values.entry(key.clone()).or_insert_with(|| {
                    SourcedValue::new(sourced_value_fragment.value.clone(), ConfigSource::Default)
                });
                let file_from_fragment = sourced_value_fragment
                    .overrides
                    .first()
                    .and_then(|o| o.file.clone());
                sv_entry.merge_override(
                    sourced_value_fragment.value,  // Use the value from the fragment
                    sourced_value_fragment.source, // Use the source from the fragment
                    file_from_fragment,            // Pass the file path from the fragment override
                );
            }
        }
    }

    /// Load and merge configurations from files and CLI overrides.
    pub fn load(
        config_path: Option<&str>,
        cli_overrides: Option<&SourcedGlobalConfig>,
    ) -> Result<Self, ConfigError> {
        Self::load_with_discovery(config_path, cli_overrides, false)
    }

    /// Load and merge configurations from files and CLI overrides.
    /// If skip_auto_discovery is true, only explicit config paths are loaded.
    pub fn load_with_discovery(
        config_path: Option<&str>,
        cli_overrides: Option<&SourcedGlobalConfig>,
        skip_auto_discovery: bool,
    ) -> Result<Self, ConfigError> {
        use std::env;
        log::debug!(
            "[rumdl-config] Current working directory: {:?}",
            env::current_dir()
        );
        if config_path.is_none() {
            if skip_auto_discovery {
                log::debug!("[rumdl-config] Skipping auto-discovery due to --no-config flag");
            } else {
                log::debug!("[rumdl-config] No explicit config_path provided, will search default locations");
            }
        } else {
            log::debug!(
                "[rumdl-config] Explicit config_path provided: {:?}",
                config_path
            );
        }
        let mut sourced_config = SourcedConfig::default();
        let mut loaded_toml_or_pyproject = false;

        // 1. Load explicit config path if provided
        if let Some(path) = config_path {
            let path_obj = Path::new(path);
            let filename = path_obj
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            log::debug!("[rumdl-config] Trying to load config file: {}", filename);
            let path_str = path.to_string();

            // Known markdownlint config files
            const MARKDOWNLINT_FILENAMES: &[&str] = &[
                ".markdownlint.json",
                ".markdownlint.yaml",
                ".markdownlint.yml",
            ];

            if filename == "pyproject.toml" || filename == ".rumdl.toml" || filename == "rumdl.toml"
            {
                let content = std::fs::read_to_string(path).map_err(|e| ConfigError::IoError {
                    source: e,
                    path: path_str.clone(),
                })?;
                if filename == "pyproject.toml" {
                    if let Some(fragment) = parse_pyproject_toml(&content, &path_str)? {
                        sourced_config.merge(fragment);
                        sourced_config.loaded_files.push(path_str.clone());
                        loaded_toml_or_pyproject = true;
                    }
                } else {
                    let fragment = parse_rumdl_toml(&content, &path_str)?;
                    sourced_config.merge(fragment);
                    sourced_config.loaded_files.push(path_str.clone());
                    loaded_toml_or_pyproject = true;
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
                sourced_config.loaded_files.push(path_str.clone());
                // Do NOT set loaded_toml_or_pyproject = true; markdownlint is fallback only
            } else {
                // Try TOML only
                let content = std::fs::read_to_string(path).map_err(|e| ConfigError::IoError {
                    source: e,
                    path: path_str.clone(),
                })?;
                let fragment = parse_rumdl_toml(&content, &path_str)?;
                sourced_config.merge(fragment);
                sourced_config.loaded_files.push(path_str.clone());
                loaded_toml_or_pyproject = true;
            }
        }

        // Only perform auto-discovery if not skipped AND no explicit config path provided
        if !skip_auto_discovery && config_path.is_none() {
            // 2. Discover and load default files: pyproject.toml first
            if std::path::Path::new("pyproject.toml").exists() {
                log::debug!("[rumdl-config] Found pyproject.toml in current directory");
                let content = std::fs::read_to_string("pyproject.toml").map_err(|e| {
                    ConfigError::IoError {
                        source: e,
                        path: "pyproject.toml".to_string(),
                    }
                })?;
                if let Some(fragment) = parse_pyproject_toml(&content, "pyproject.toml")? {
                    sourced_config.merge(fragment);
                    sourced_config
                        .loaded_files
                        .push("pyproject.toml".to_string());
                    loaded_toml_or_pyproject = true;
                } else {
                    // pyproject.toml exists, but no [tool.rumdl]. Flag should remain false.
                }
            }

            // 3. Discover and load .rumdl.toml / rumdl.toml (overrides pyproject)
            for filename in [".rumdl.toml", "rumdl.toml"] {
                if std::path::Path::new(filename).exists() {
                    log::debug!("[rumdl-config] Found {} in current directory", filename);
                    let content =
                        std::fs::read_to_string(filename).map_err(|e| ConfigError::IoError {
                            source: e,
                            path: filename.to_string(),
                        })?;
                    let fragment = parse_rumdl_toml(&content, filename)?;
                    sourced_config.merge(fragment);
                    sourced_config.loaded_files.push(filename.to_string());
                    loaded_toml_or_pyproject = true;
                    break; // Load only the first one found
                } else {
                    log::debug!("[rumdl-config] {} not found in current directory", filename);
                }
            }

            // 4. Markdownlint config fallback if no TOML/pyproject config was loaded
            if !loaded_toml_or_pyproject {
                for filename in MARKDOWNLINT_CONFIG_FILES {
                    if std::path::Path::new(filename).exists() {
                        match load_from_markdownlint(filename) {
                            Ok(fragment) => {
                                sourced_config.merge(fragment);
                                sourced_config.loaded_files.push(filename.to_string());
                                break; // Load only the first one found
                            }
                            Err(_e) => {
                                // Log error but continue (it's just a fallback)
                            }
                        }
                    }
                }
            }
        }

        // 5. Apply CLI overrides (highest precedence)
        if let Some(cli) = cli_overrides {
            sourced_config.global.enable.merge_override(
                cli.enable.value.clone(),
                ConfigSource::Cli,
                None,
            );
            sourced_config.global.disable.merge_override(
                cli.disable.value.clone(),
                ConfigSource::Cli,
                None,
            );
            sourced_config.global.exclude.merge_override(
                cli.exclude.value.clone(),
                ConfigSource::Cli,
                None,
            );
            sourced_config.global.include.merge_override(
                cli.include.value.clone(),
                ConfigSource::Cli,
                None,
            );
            sourced_config.global.respect_gitignore.merge_override(
                cli.respect_gitignore.value,
                ConfigSource::Cli,
                None,
            );
            // No rule-specific CLI overrides implemented yet
        }

        // TODO: Handle unknown keys collected during parsing/merging

        Ok(sourced_config)
    }
}

impl From<SourcedConfig> for Config {
    fn from(sourced: SourcedConfig) -> Self {
        let mut rules = BTreeMap::new();
        for (rule_name, sourced_rule_cfg) in sourced.rules {
            // Normalize rule name to uppercase for case-insensitive lookup
            let normalized_rule_name = rule_name.to_ascii_uppercase();
            let mut values = BTreeMap::new();
            for (key, sourced_val) in sourced_rule_cfg.values {
                values.insert(key, sourced_val.value);
            }
            rules.insert(normalized_rule_name, RuleConfig { values });
        }
        let global = GlobalConfig {
            enable: sourced.global.enable.value,
            disable: sourced.global.disable.value,
            exclude: sourced.global.exclude.value,
            include: sourced.global.include.value,
            respect_gitignore: sourced.global.respect_gitignore.value,
        };
        Config { global, rules }
    }
}

/// Registry of all known rules and their config schemas
pub struct RuleRegistry {
    /// Map of rule name (e.g. "MD013") to set of valid config keys and their TOML value types
    pub rule_schemas: std::collections::BTreeMap<String, toml::map::Map<String, toml::Value>>,
}

impl RuleRegistry {
    /// Build a registry from a list of rules
    pub fn from_rules(rules: &[Box<dyn Rule>]) -> Self {
        let mut rule_schemas = std::collections::BTreeMap::new();
        for rule in rules {
            if let Some((name, toml::Value::Table(table))) = rule.default_config_section() {
                let norm_name = normalize_key(&name); // Normalize the name from default_config_section
                rule_schemas.insert(norm_name, table);
            } else {
                let norm_name = normalize_key(rule.name()); // Normalize the name from rule.name()
                rule_schemas.insert(norm_name, toml::map::Map::new());
            }
        }
        RuleRegistry { rule_schemas }
    }

    /// Get all known rule names
    pub fn rule_names(&self) -> std::collections::BTreeSet<String> {
        self.rule_schemas.keys().cloned().collect()
    }

    /// Get the valid configuration keys for a rule, including both original and normalized variants
    pub fn config_keys_for(&self, rule: &str) -> Option<std::collections::BTreeSet<String>> {
        self.rule_schemas.get(rule).map(|schema| {
            let mut all_keys = std::collections::BTreeSet::new();

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

            all_keys
        })
    }

    /// Get the expected value type for a rule's configuration key, trying variants
    pub fn expected_value_for(&self, rule: &str, key: &str) -> Option<&toml::Value> {
        if let Some(schema) = self.rule_schemas.get(rule) {
            // Try the original key first
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
}

/// Represents a config validation warning or error
#[derive(Debug, Clone)]
pub struct ConfigValidationWarning {
    pub message: String,
    pub rule: Option<String>,
    pub key: Option<String>,
}

/// Validate a loaded config against the rule registry, using SourcedConfig for unknown key tracking
pub fn validate_config_sourced(
    sourced: &SourcedConfig,
    registry: &RuleRegistry,
) -> Vec<ConfigValidationWarning> {
    let mut warnings = Vec::new();
    let known_rules = registry.rule_names();
    // 1. Unknown rules
    for rule in sourced.rules.keys() {
        if !known_rules.contains(rule) {
            warnings.push(ConfigValidationWarning {
                message: format!("Unknown rule in config: {}", rule),
                rule: Some(rule.clone()),
                key: None,
            });
        }
    }
    // 2. Unknown options and type mismatches
    for (rule, rule_cfg) in &sourced.rules {
        if let Some(valid_keys) = registry.config_keys_for(rule) {
            for key in rule_cfg.values.keys() {
                if !valid_keys.contains(key) {
                    warnings.push(ConfigValidationWarning {
                        message: format!("Unknown option for rule {}: {}", rule, key),
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
    for (section, key) in &sourced.unknown_keys {
        if section.contains("[global]") {
            warnings.push(ConfigValidationWarning {
                message: format!("Unknown global option: {}", key),
                rule: None,
                key: Some(key.clone()),
            });
        }
    }
    warnings
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
fn parse_pyproject_toml(
    content: &str,
    path: &str,
) -> Result<Option<SourcedConfigFragment>, ConfigError> {
    let doc: toml::Value = toml::from_str(content)
        .map_err(|e| ConfigError::ParseError(format!("{}: Failed to parse TOML: {}", path, e)))?;
    let mut fragment = SourcedConfigFragment::default();
    let source = ConfigSource::PyprojectToml;
    let file = Some(path.to_string());

    // 1. Handle [tool.rumdl] as before
    if let Some(rumdl_config) = doc.get("tool").and_then(|t| t.get("rumdl")) {
        if let Some(rumdl_table) = rumdl_config.as_table() {
            // --- Extract global options ---
            if let Some(enable) = rumdl_table.get("enable") {
                if let Ok(values) = Vec::<String>::deserialize(enable.clone()) {
                    // Normalize rule names in the list
                    let normalized_values = values.into_iter().map(|s| normalize_key(&s)).collect();
                    fragment.global.enable.push_override(
                        normalized_values,
                        source,
                        file.clone(),
                        None,
                    );
                }
            }
            if let Some(disable) = rumdl_table.get("disable") {
                if let Ok(values) = Vec::<String>::deserialize(disable.clone()) {
                    // Re-enable normalization
                    let normalized_values: Vec<String> =
                        values.into_iter().map(|s| normalize_key(&s)).collect();
                    fragment.global.disable.push_override(
                        normalized_values,
                        source,
                        file.clone(),
                        None,
                    );
                }
            }
            if let Some(include) = rumdl_table.get("include") {
                if let Ok(values) = Vec::<String>::deserialize(include.clone()) {
                    fragment
                        .global
                        .include
                        .push_override(values, source, file.clone(), None);
                }
            }
            if let Some(exclude) = rumdl_table.get("exclude") {
                if let Ok(values) = Vec::<String>::deserialize(exclude.clone()) {
                    fragment
                        .global
                        .exclude
                        .push_override(values, source, file.clone(), None);
                }
            }
            if let Some(respect_gitignore) = rumdl_table
                .get("respect-gitignore")
                .or_else(|| rumdl_table.get("respect_gitignore"))
            {
                if let Ok(value) = bool::deserialize(respect_gitignore.clone()) {
                    fragment.global.respect_gitignore.push_override(
                        value,
                        source,
                        file.clone(),
                        None,
                    );
                }
            }

            // --- Re-introduce special line-length handling ---
            let mut found_line_length_val: Option<toml::Value> = None;
            for key in ["line-length", "line_length"].iter() {
                if let Some(val) = rumdl_table.get(*key) {
                    // Ensure the value is actually an integer before cloning
                    if val.is_integer() {
                        found_line_length_val = Some(val.clone());
                        break;
                    } else {
                        // Optional: Warn about wrong type for line-length?
                    }
                }
            }
            if let Some(line_length_val) = found_line_length_val {
                let norm_md013_key = normalize_key("MD013"); // Normalize to "md013"
                let rule_entry = fragment.rules.entry(norm_md013_key).or_default();
                let norm_line_length_key = normalize_key("line-length"); // Ensure "line-length"
                let sv = rule_entry
                    .values
                    .entry(norm_line_length_key)
                    .or_insert_with(|| {
                        SourcedValue::new(line_length_val.clone(), ConfigSource::Default)
                    });
                sv.push_override(line_length_val, source, file.clone(), None);
            }

            // --- Extract rule-specific configurations ---
            for (key, value) in rumdl_table {
                let norm_rule_key = normalize_key(key);

                // Skip keys already handled as global or special cases
                if [
                    "enable",
                    "disable",
                    "include",
                    "exclude",
                    "respect_gitignore",
                    "respect-gitignore", // Added kebab-case here too
                    "line_length",
                    "line-length",
                ]
                .contains(&norm_rule_key.as_str())
                {
                    continue;
                }

                // Explicitly check if the key looks like a rule name (e.g., starts with 'md')
                // AND if the value is actually a TOML table before processing as rule config.
                // This prevents misinterpreting other top-level keys under [tool.rumdl]
                let norm_rule_key_upper = norm_rule_key.to_ascii_uppercase();
                if norm_rule_key_upper.len() == 5
                    && norm_rule_key_upper.starts_with("MD")
                    && norm_rule_key_upper[2..].chars().all(|c| c.is_ascii_digit())
                    && value.is_table()
                {
                    if let Some(rule_config_table) = value.as_table() {
                        // Get the entry for this rule (e.g., "md013")
                        let rule_entry = fragment.rules.entry(norm_rule_key_upper).or_default();
                        for (rk, rv) in rule_config_table {
                            let norm_rk = normalize_key(rk); // Normalize the config key itself

                            let toml_val = rv.clone();

                            let sv =
                                rule_entry.values.entry(norm_rk.clone()).or_insert_with(|| {
                                    SourcedValue::new(toml_val.clone(), ConfigSource::Default)
                                });
                            sv.push_override(toml_val, source, file.clone(), None);
                        }
                    }
                } else {
                    // Key is not a global/special key, doesn't start with 'md', or isn't a table.
                    // TODO: Track unknown keys/sections if necessary for validation later.
                    // eprintln!("[DEBUG parse_pyproject] Skipping key '{}' as it's not a recognized rule table.", key);
                }
            }
        }
    }

    // 2. Handle [tool.rumdl.MDxxx] sections as rule-specific config (nested under [tool])
    if let Some(tool_table) = doc.get("tool").and_then(|t| t.as_table()) {
        for (key, value) in tool_table.iter() {
            if let Some(rule_name) = key.strip_prefix("rumdl.") {
                let norm_rule_name = normalize_key(rule_name);
                if norm_rule_name.len() == 5
                    && norm_rule_name.to_ascii_uppercase().starts_with("MD")
                    && norm_rule_name[2..].chars().all(|c| c.is_ascii_digit())
                {
                    if let Some(rule_table) = value.as_table() {
                        let rule_entry = fragment
                            .rules
                            .entry(norm_rule_name.to_ascii_uppercase())
                            .or_default();
                        for (rk, rv) in rule_table {
                            let norm_rk = normalize_key(rk);
                            let toml_val = rv.clone();
                            let sv = rule_entry
                                .values
                                .entry(norm_rk.clone())
                                .or_insert_with(|| SourcedValue::new(toml_val.clone(), source));
                            sv.push_override(toml_val, source, file.clone(), None);
                        }
                    }
                }
            }
        }
    }

    // 3. Handle [tool.rumdl.MDxxx] sections as top-level keys (e.g., [tool.rumdl.MD007])
    if let Some(doc_table) = doc.as_table() {
        for (key, value) in doc_table.iter() {
            if let Some(rule_name) = key.strip_prefix("tool.rumdl.") {
                let norm_rule_name = normalize_key(rule_name);
                if norm_rule_name.len() == 5
                    && norm_rule_name.to_ascii_uppercase().starts_with("MD")
                    && norm_rule_name[2..].chars().all(|c| c.is_ascii_digit())
                {
                    if let Some(rule_table) = value.as_table() {
                        let rule_entry = fragment
                            .rules
                            .entry(norm_rule_name.to_ascii_uppercase())
                            .or_default();
                        for (rk, rv) in rule_table {
                            let norm_rk = normalize_key(rk);
                            let toml_val = rv.clone();
                            let sv = rule_entry
                                .values
                                .entry(norm_rk.clone())
                                .or_insert_with(|| SourcedValue::new(toml_val.clone(), source));
                            sv.push_override(toml_val, source, file.clone(), None);
                        }
                    }
                }
            }
        }
    }

    // Only return Some(fragment) if any config was found
    let has_any = !fragment.global.enable.value.is_empty()
        || !fragment.global.disable.value.is_empty()
        || !fragment.global.include.value.is_empty()
        || !fragment.global.exclude.value.is_empty()
        || !fragment.rules.is_empty();
    if has_any {
        Ok(Some(fragment))
    } else {
        Ok(None)
    }
}

/// Parses rumdl.toml / .rumdl.toml content.
fn parse_rumdl_toml(content: &str, path: &str) -> Result<SourcedConfigFragment, ConfigError> {
    let doc = content
        .parse::<DocumentMut>()
        .map_err(|e| ConfigError::ParseError(format!("{}: Failed to parse TOML: {}", path, e)))?;
    let mut fragment = SourcedConfigFragment::default();
    let source = ConfigSource::RumdlToml;
    let file = Some(path.to_string());

    // Define known rules before the loop
    let all_rules = rules::all_rules(&Config::default());
    let registry = RuleRegistry::from_rules(&all_rules);
    let known_rule_names: BTreeSet<String> = registry
        .rule_names()
        .into_iter()
        .map(|s| s.to_ascii_uppercase())
        .collect();

    // Handle [global] section
    if let Some(global_item) = doc.get("global") {
        if let Some(global_table) = global_item.as_table() {
            for (key, value_item) in global_table.iter() {
                let norm_key = normalize_key(key);
                match norm_key.as_str() {
                    "enable" | "disable" | "include" | "exclude" => {
                        if let Some(toml_edit::Value::Array(formatted_array)) =
                            value_item.as_value()
                        {
                            // Corrected: Iterate directly over the Formatted<Array>
                            let values: Vec<String> = formatted_array
                                .iter()
                                .filter_map(|item| item.as_str()) // Extract strings
                                .map(|s| s.to_string())
                                .collect();

                            // Normalize rule names for enable/disable
                            let final_values = if norm_key == "enable" || norm_key == "disable" {
                                // Corrected: Pass &str to normalize_key
                                values.into_iter().map(|s| normalize_key(&s)).collect()
                            } else {
                                values
                            };

                            match norm_key.as_str() {
                                "enable" => fragment.global.enable.push_override(
                                    final_values,
                                    source,
                                    file.clone(),
                                    None,
                                ),
                                "disable" => fragment.global.disable.push_override(
                                    final_values,
                                    source,
                                    file.clone(),
                                    None,
                                ),
                                "include" => fragment.global.include.push_override(
                                    final_values,
                                    source,
                                    file.clone(),
                                    None,
                                ),
                                "exclude" => fragment.global.exclude.push_override(
                                    final_values,
                                    source,
                                    file.clone(),
                                    None,
                                ),
                                _ => unreachable!(), // Should not happen due to outer match
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
                        if let Some(toml_edit::Value::Boolean(formatted_bool)) =
                            value_item.as_value()
                        {
                            let val = *formatted_bool.value();
                            fragment.global.respect_gitignore.push_override(
                                val,
                                source,
                                file.clone(),
                                None,
                            );
                        } else {
                            log::warn!(
                                "[WARN] Expected boolean for global key '{}' in {}, found {}",
                                key,
                                path,
                                value_item.type_name()
                            );
                        }
                    }
                    _ => {
                        // Add to unknown_keys for potential validation later
                        // fragment.unknown_keys.push(("[global]".to_string(), key.to_string()));
                        log::warn!(
                            "[WARN] Unknown key in [global] section of {}: {}",
                            path,
                            key
                        );
                    }
                }
            }
        }
    }

    // Rule-specific: all other top-level tables
    for (key, item) in doc.iter() {
        let norm_rule_name = key.to_ascii_uppercase();
        if !known_rule_names.contains(&norm_rule_name) {
            continue;
        }
        if let Some(tbl) = item.as_table() {
            let rule_entry = fragment.rules.entry(norm_rule_name.clone()).or_default();
            for (rk, rv_item) in tbl.iter() {
                let norm_rk = normalize_key(rk);
                let maybe_toml_val: Option<toml::Value> = match rv_item.as_value() {
                    Some(toml_edit::Value::String(formatted)) => {
                        Some(toml::Value::String(formatted.value().clone()))
                    }
                    Some(toml_edit::Value::Integer(formatted)) => {
                        Some(toml::Value::Integer(*formatted.value()))
                    }
                    Some(toml_edit::Value::Float(formatted)) => {
                        Some(toml::Value::Float(*formatted.value()))
                    }
                    Some(toml_edit::Value::Boolean(formatted)) => {
                        Some(toml::Value::Boolean(*formatted.value()))
                    }
                    Some(toml_edit::Value::Datetime(formatted)) => {
                        Some(toml::Value::Datetime(*formatted.value()))
                    }
                    Some(toml_edit::Value::Array(_)) => {
                        log::warn!(
                                "[WARN] Skipping array value for key '{}.{}' in {}. Array conversion not yet fully implemented in parser.",
                                norm_rule_name, norm_rk, path
                            );
                        None
                    }
                    Some(toml_edit::Value::InlineTable(_)) => {
                        log::warn!(
                                "[WARN] Skipping inline table value for key '{}.{}' in {}. Table conversion not yet fully implemented in parser.",
                                norm_rule_name, norm_rk, path
                            );
                        None
                    }
                    None => {
                        log::warn!(
                                "[WARN] Skipping non-value item for key '{}.{}' in {}. Expected simple value.",
                                norm_rule_name, norm_rk, path
                            );
                        None
                    }
                };
                if let Some(toml_val) = maybe_toml_val {
                    let sv = rule_entry.values.entry(norm_rk.clone()).or_insert_with(|| {
                        SourcedValue::new(toml_val.clone(), ConfigSource::Default)
                    });
                    sv.push_override(toml_val, source, file.clone(), None);
                }
            }
        } else if item.is_value() {
            log::warn!(
                "[WARN] Ignoring top-level value key in {}: '{}'. Expected a table like [{}].",
                path,
                key,
                key
            );
        }
    }

    Ok(fragment)
}

/// Loads and converts a markdownlint config file (.json or .yaml) into a SourcedConfigFragment.
fn load_from_markdownlint(path: &str) -> Result<SourcedConfigFragment, ConfigError> {
    // Use the unified loader from markdownlint_config.rs
    let ml_config = crate::markdownlint_config::load_markdownlint_config(path)
        .map_err(|e| ConfigError::ParseError(format!("{}: {}", path, e)))?;
    Ok(ml_config.map_to_sourced_rumdl_config_fragment(Some(path)))
}
