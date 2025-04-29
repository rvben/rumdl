//!
//! This module defines configuration structures, loading logic, and provenance tracking for rumdl.
//! Supports TOML, pyproject.toml, and markdownlint config formats, and provides merging and override logic.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

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

/// Load configuration from the specified file or search for a default config file
pub fn load_config(config_path: Option<&str>) -> Result<Config, ConfigError> {
    // If a specific config file is provided, use it
    if let Some(path) = config_path {
        return load_config_from_file(path);
    }

    // Otherwise, look for default config files in standard locations
    for filename in ["rumdl.toml", ".rumdl.toml"] {
        // Try in current directory
        if Path::new(filename).exists() {
            return load_config_from_file(filename);
        }
    }

    // Check for pyproject.toml
    if Path::new("pyproject.toml").exists() {
        return load_config_from_pyproject("pyproject.toml");
    }

    // No config file found, return default config
    Ok(Config::default())
}

/// Load configuration from a specific file
fn load_config_from_file(path: &str) -> Result<Config, ConfigError> {
    match fs::read_to_string(path) {
        Ok(content) => {
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        }
        Err(err) => Err(ConfigError::IoError {
            source: err,
            path: path.to_string(),
        }),
    }
}

/// Load rumdl configuration from a pyproject.toml file
fn load_config_from_pyproject(path: &str) -> Result<Config, ConfigError> {
    match fs::read_to_string(path) {
        Ok(content) => {
            // Parse the entire pyproject.toml
            let pyproject: toml::Value = toml::from_str(&content)?;

            // Try to extract [tool.rumdl] section
            match pyproject.get("tool").and_then(|t| t.get("rumdl")) {
                Some(rumdl_config) => {
                    // Create a new Config with defaults
                    let mut config = Config::default();

                    // Parse the complete section into our configuration struct
                    if let Some(rumdl_table) = rumdl_config.as_table() {
                        // Extract global options from root level
                        if let Some(enable) = rumdl_table.get("enable") {
                            if let Ok(values) = Vec::<String>::deserialize(enable.clone()) {
                                config.global.enable = values;
                            }
                        }

                        if let Some(disable) = rumdl_table.get("disable") {
                            if let Ok(values) = Vec::<String>::deserialize(disable.clone()) {
                                config.global.disable = values;
                            }
                        }

                        if let Some(include) = rumdl_table.get("include") {
                            if let Ok(values) = Vec::<String>::deserialize(include.clone()) {
                                config.global.include = values;
                            }
                        }

                        if let Some(exclude) = rumdl_table.get("exclude") {
                            if let Ok(values) = Vec::<String>::deserialize(exclude.clone()) {
                                config.global.exclude = values;
                            }
                        }

                        if let Some(respect_gitignore) = rumdl_table
                            .get("respect-gitignore")
                            .or_else(|| rumdl_table.get("respect_gitignore"))
                        {
                            if let Ok(value) = bool::deserialize(respect_gitignore.clone()) {
                                config.global.respect_gitignore = value;
                            }
                        }

                        // Handle line-length special case
                        if let Some(line_length) = rumdl_table
                            .get("line-length")
                            .or_else(|| rumdl_table.get("line_length"))
                        {
                            // Create MD013 rule config if it doesn't exist
                            if !config.rules.contains_key("MD013") {
                                config
                                    .rules
                                    .insert("MD013".to_string(), RuleConfig::default());
                            }

                            // Add line_length to the MD013 section
                            if let Some(rule_config) = config.rules.get_mut("MD013") {
                                rule_config
                                    .values
                                    .insert("line_length".to_string(), line_length.clone());
                            }
                        }

                        // Extract rule-specific configurations
                        for (key, value) in rumdl_table {
                            // Skip keys that we've already processed as global options
                            if [
                                "enable",
                                "disable",
                                "include",
                                "exclude",
                                "respect-gitignore",
                                "respect_gitignore",
                                "line-length",
                                "line_length",
                            ]
                            .contains(&key.as_str())
                            {
                                continue;
                            }

                            // If it's a table, treat it as a rule configuration
                            if let Some(rule_table) = value.as_table() {
                                let mut rule_config = RuleConfig::default();

                                // Add all values from the table to the rule config
                                for (rule_key, rule_value) in rule_table {
                                    rule_config
                                        .values
                                        .insert(rule_key.to_string(), rule_value.clone());
                                }

                                // Add to the config
                                config.rules.insert(key.to_string(), rule_config);
                            }
                        }
                    }

                    Ok(config)
                }
                None => {
                    // No rumdl configuration found in pyproject.toml
                    Ok(Config::default())
                }
            }
        }
        Err(err) => Err(ConfigError::IoError {
            source: err,
            path: path.to_string(),
        }),
    }
}

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

    /// Failed to parse the TOML content
    #[error("Failed to parse TOML: {0}")]
    ParseError(#[from] toml::de::Error),

    /// Configuration file already exists
    #[error("Configuration file already exists at {path}")]
    FileExists { path: String },
}

/// Get a rule-specific configuration value
pub fn get_rule_config_value<T: serde::de::DeserializeOwned>(
    config: &Config,
    rule_name: &str,
    key: &str,
) -> Option<T> {
    config
        .rules
        .get(rule_name)
        .and_then(|rule_config| rule_config.values.get(key))
        .and_then(|value| T::deserialize(value.clone()).ok())
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

        // Load the config
        let config = load_config_from_pyproject(config_path.to_str().unwrap()).unwrap();

        // Check global settings
        assert_eq!(config.global.disable, vec!["MD033".to_string()]);
        assert_eq!(
            config.global.enable,
            vec!["MD001".to_string(), "MD004".to_string()]
        );
        assert_eq!(config.global.include, vec!["docs/*.md".to_string()]);
        assert_eq!(config.global.exclude, vec!["node_modules".to_string()]);
        assert!(config.global.respect_gitignore);

        // Check line_length was correctly added to MD013
        let line_length = get_rule_config_value::<usize>(&config, "MD013", "line_length");
        assert_eq!(line_length, Some(120));
    }

    #[test]
    fn test_pyproject_toml_snake_case_and_kebab_case() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("pyproject.toml");

        // Test with both kebab-case and snake_case variants
        let content = r#"
[tool.rumdl]
line_length = 150
respect_gitignore = true
        "#;

        fs::write(&config_path, content).unwrap();

        // Load the config
        let config = load_config_from_pyproject(config_path.to_str().unwrap()).unwrap();

        // Check settings were correctly loaded
        assert!(config.global.respect_gitignore);
        let line_length = get_rule_config_value::<usize>(&config, "MD013", "line_length");
        assert_eq!(line_length, Some(150));
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
    pub fn push_override(
        &mut self,
        value: T,
        source: ConfigSource,
        file: Option<String>,
        line: Option<usize>,
    ) {
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

#[derive(Debug)]
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

#[derive(Debug, Default)]
pub struct SourcedRuleConfig {
    pub values: BTreeMap<String, SourcedValue<toml::Value>>,
}

#[derive(Debug, Default)]
pub struct SourcedConfig {
    pub global: SourcedGlobalConfig,
    pub rules: BTreeMap<String, SourcedRuleConfig>,
    pub loaded_files: Vec<String>,
    pub unknown_keys: Vec<(String, String)>, // (section, key)
}

impl SourcedConfig {
    pub fn load_sourced_config(
        config_path: Option<&str>,
        cli_overrides: Option<&SourcedGlobalConfig>,
    ) -> Self {
        use ConfigSource::*;
        let mut loaded_files = Vec::new();
        let mut unknown_keys = Vec::new();
        let mut global = SourcedGlobalConfig {
            enable: SourcedValue::new(Vec::new(), Default),
            disable: SourcedValue::new(Vec::new(), Default),
            exclude: SourcedValue::new(Vec::new(), Default),
            include: SourcedValue::new(Vec::new(), Default),
            respect_gitignore: SourcedValue::new(true, Default),
        };
        let mut rules: BTreeMap<String, SourcedRuleConfig> = BTreeMap::new();

        // Helper to update a sourced value if the new source has higher precedence
        let update_vec = |current: &mut SourcedValue<Vec<String>>,
                          value: Vec<String>,
                          source,
                          file: Option<String>| {
            if source_precedence(source) >= source_precedence(current.source) {
                current.push_override(value, source, file, None);
            }
        };
        let update_bool =
            |current: &mut SourcedValue<bool>, value: bool, source, file: Option<String>| {
                if source_precedence(source) >= source_precedence(current.source) {
                    current.push_override(value, source, file, None);
                }
            };
        // Precedence: CLI > .rumdl.toml > pyproject.toml > Default
        fn source_precedence(src: ConfigSource) -> u8 {
            match src {
                ConfigSource::Default => 0,
                ConfigSource::PyprojectToml => 1,
                ConfigSource::Markdownlint => 2,
                ConfigSource::RumdlToml => 3,
                ConfigSource::Cli => 4,
            }
        }

        // Track if any TOML/pyproject config was loaded
        let mut loaded_toml_or_pyproject = false;
        // 1. Load pyproject.toml if present
        if config_path.is_none() && std::path::Path::new("pyproject.toml").exists() {
            if let Ok(content) = std::fs::read_to_string("pyproject.toml") {
                if let Ok(pyproject) = toml::from_str::<toml::Value>(&content) {
                    if let Some(rumdl_config) = pyproject.get("tool").and_then(|t| t.get("rumdl")) {
                        loaded_files.push("pyproject.toml".to_string());
                        loaded_toml_or_pyproject = true;
                        if let Some(rumdl_table) = rumdl_config.as_table() {
                            for (key, value) in rumdl_table {
                                match key.as_str() {
                                    "enable" => {
                                        if let Ok(values) =
                                            Vec::<String>::deserialize(value.clone())
                                        {
                                            update_vec(
                                                &mut global.enable,
                                                values,
                                                PyprojectToml,
                                                Some("pyproject.toml".to_string()),
                                            );
                                        }
                                    }
                                    "disable" => {
                                        if let Ok(values) =
                                            Vec::<String>::deserialize(value.clone())
                                        {
                                            update_vec(
                                                &mut global.disable,
                                                values,
                                                PyprojectToml,
                                                Some("pyproject.toml".to_string()),
                                            );
                                        }
                                    }
                                    "include" => {
                                        if let Ok(values) =
                                            Vec::<String>::deserialize(value.clone())
                                        {
                                            update_vec(
                                                &mut global.include,
                                                values,
                                                PyprojectToml,
                                                Some("pyproject.toml".to_string()),
                                            );
                                        }
                                    }
                                    "exclude" => {
                                        if let Ok(values) =
                                            Vec::<String>::deserialize(value.clone())
                                        {
                                            update_vec(
                                                &mut global.exclude,
                                                values,
                                                PyprojectToml,
                                                Some("pyproject.toml".to_string()),
                                            );
                                        }
                                    }
                                    "respect-gitignore" | "respect_gitignore" => {
                                        if let Ok(val) = bool::deserialize(value.clone()) {
                                            update_bool(
                                                &mut global.respect_gitignore,
                                                val,
                                                PyprojectToml,
                                                Some("pyproject.toml".to_string()),
                                            );
                                        }
                                    }
                                    _ => {
                                        // Rule-specific or unknown
                                        if let Some(rule_table) = value.as_table() {
                                            let rule_entry = rules.entry(key.clone()).or_default();
                                            for (rk, rv) in rule_table {
                                                let mut sv = rule_entry
                                                    .values
                                                    .remove(rk)
                                                    .unwrap_or_else(|| {
                                                        SourcedValue::new(
                                                            rv.clone(),
                                                            ConfigSource::Default,
                                                        )
                                                    });
                                                sv.push_override(
                                                    rv.clone(),
                                                    PyprojectToml,
                                                    Some("pyproject.toml".to_string()),
                                                    None,
                                                );
                                                rule_entry.values.insert(rk.clone(), sv);
                                            }
                                        } else {
                                            // Unknown key
                                            unknown_keys.push((
                                                "[tool.rumdl] (pyproject.toml)".to_string(),
                                                key.clone(),
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // 2. Load .rumdl.toml or rumdl.toml if present (higher precedence)
        if config_path.is_none() {
            for filename in [".rumdl.toml", "rumdl.toml"] {
                if std::path::Path::new(filename).exists() {
                    loaded_files.push(filename.to_string());
                    loaded_toml_or_pyproject = true;
                    if let Ok(content) = std::fs::read_to_string(filename) {
                        if let Ok(toml_val) = toml::from_str::<toml::Value>(&content) {
                            if let Some(global_table) =
                                toml_val.get("global").and_then(|v| v.as_table())
                            {
                                for (key, value) in global_table {
                                    match key.as_str() {
                                        "enable" => {
                                            if let Ok(values) =
                                                Vec::<String>::deserialize(value.clone())
                                            {
                                                update_vec(
                                                    &mut global.enable,
                                                    values,
                                                    RumdlToml,
                                                    Some(filename.to_string()),
                                                );
                                            }
                                        }
                                        "disable" => {
                                            if let Ok(values) =
                                                Vec::<String>::deserialize(value.clone())
                                            {
                                                update_vec(
                                                    &mut global.disable,
                                                    values,
                                                    RumdlToml,
                                                    Some(filename.to_string()),
                                                );
                                            }
                                        }
                                        "include" => {
                                            if let Ok(values) =
                                                Vec::<String>::deserialize(value.clone())
                                            {
                                                update_vec(
                                                    &mut global.include,
                                                    values,
                                                    RumdlToml,
                                                    Some(filename.to_string()),
                                                );
                                            }
                                        }
                                        "exclude" => {
                                            if let Ok(values) =
                                                Vec::<String>::deserialize(value.clone())
                                            {
                                                update_vec(
                                                    &mut global.exclude,
                                                    values,
                                                    RumdlToml,
                                                    Some(filename.to_string()),
                                                );
                                            }
                                        }
                                        "respect_gitignore" => {
                                            if let Ok(val) = bool::deserialize(value.clone()) {
                                                update_bool(
                                                    &mut global.respect_gitignore,
                                                    val,
                                                    RumdlToml,
                                                    Some(filename.to_string()),
                                                );
                                            }
                                        }
                                        _ => {
                                            unknown_keys.push((
                                                "[global] (.rumdl.toml)".to_string(),
                                                key.clone(),
                                            ));
                                        }
                                    }
                                }
                            }
                            // Rule-specific
                            for (key, value) in
                                toml_val.as_table().unwrap_or(&toml::map::Map::new())
                            {
                                if key == "global" {
                                    continue;
                                }
                                if let Some(rule_table) = value.as_table() {
                                    let rule_entry = rules.entry(key.clone()).or_default();
                                    for (rk, rv) in rule_table {
                                        let mut sv =
                                            rule_entry.values.remove(rk).unwrap_or_else(|| {
                                                SourcedValue::new(rv.clone(), ConfigSource::Default)
                                            });
                                        sv.push_override(
                                            rv.clone(),
                                            RumdlToml,
                                            Some(filename.to_string()),
                                            None,
                                        );
                                        rule_entry.values.insert(rk.clone(), sv);
                                    }
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }
        // 3. If explicit config path is given, load it (highest precedence except CLI)
        if let Some(path) = config_path {
            loaded_files.push(path.to_string());
            loaded_toml_or_pyproject = true;
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(toml_val) = toml::from_str::<toml::Value>(&content) {
                    if let Some(global_table) = toml_val.get("global").and_then(|v| v.as_table()) {
                        for (key, value) in global_table {
                            match key.as_str() {
                                "enable" => {
                                    if let Ok(values) = Vec::<String>::deserialize(value.clone()) {
                                        update_vec(
                                            &mut global.enable,
                                            values,
                                            RumdlToml,
                                            Some(path.to_string()),
                                        );
                                    }
                                }
                                "disable" => {
                                    if let Ok(values) = Vec::<String>::deserialize(value.clone()) {
                                        update_vec(
                                            &mut global.disable,
                                            values,
                                            RumdlToml,
                                            Some(path.to_string()),
                                        );
                                    }
                                }
                                "include" => {
                                    if let Ok(values) = Vec::<String>::deserialize(value.clone()) {
                                        update_vec(
                                            &mut global.include,
                                            values,
                                            RumdlToml,
                                            Some(path.to_string()),
                                        );
                                    }
                                }
                                "exclude" => {
                                    if let Ok(values) = Vec::<String>::deserialize(value.clone()) {
                                        update_vec(
                                            &mut global.exclude,
                                            values,
                                            RumdlToml,
                                            Some(path.to_string()),
                                        );
                                    }
                                }
                                "respect_gitignore" => {
                                    if let Ok(val) = bool::deserialize(value.clone()) {
                                        update_bool(
                                            &mut global.respect_gitignore,
                                            val,
                                            RumdlToml,
                                            Some(path.to_string()),
                                        );
                                    }
                                }
                                _ => {
                                    unknown_keys.push((
                                        "[global] (explicit config)".to_string(),
                                        key.clone(),
                                    ));
                                }
                            }
                        }
                    }
                    // Rule-specific
                    for (key, value) in toml_val.as_table().unwrap_or(&toml::map::Map::new()) {
                        if key == "global" {
                            continue;
                        }
                        if let Some(rule_table) = value.as_table() {
                            let rule_entry = rules.entry(key.clone()).or_default();
                            for (rk, rv) in rule_table {
                                let mut sv = rule_entry.values.remove(rk).unwrap_or_else(|| {
                                    SourcedValue::new(rv.clone(), ConfigSource::Default)
                                });
                                sv.push_override(
                                    rv.clone(),
                                    RumdlToml,
                                    Some(path.to_string()),
                                    None,
                                );
                                rule_entry.values.insert(rk.clone(), sv);
                            }
                        }
                    }
                }
            }
        }
        // 4. Markdownlint config fallback if no TOML/pyproject config was loaded
        if !loaded_toml_or_pyproject {
            for filename in MARKDOWNLINT_CONFIG_FILES {
                if std::path::Path::new(filename).exists() {
                    let result = crate::markdownlint_config::load_markdownlint_config(filename);
                    if let Ok(ml_config) = result {
                        let sourced = ml_config.map_to_sourced_rumdl_config(Some(filename));
                        // Merge rule configs
                        for (rule, rule_cfg) in sourced.rules {
                            rules.insert(rule, rule_cfg);
                        }
                        // Set provenance for global config values to Markdownlint
                        global.enable = SourcedValue {
                            value: Vec::new(),
                            source: ConfigSource::Markdownlint,
                            overrides: vec![ConfigOverride {
                                value: Vec::new(),
                                source: ConfigSource::Markdownlint,
                                file: Some(filename.to_string()),
                                line: None,
                            }],
                        };
                        global.disable = SourcedValue {
                            value: Vec::new(),
                            source: ConfigSource::Markdownlint,
                            overrides: vec![ConfigOverride {
                                value: Vec::new(),
                                source: ConfigSource::Markdownlint,
                                file: Some(filename.to_string()),
                                line: None,
                            }],
                        };
                        global.exclude = SourcedValue {
                            value: Vec::new(),
                            source: ConfigSource::Markdownlint,
                            overrides: vec![ConfigOverride {
                                value: Vec::new(),
                                source: ConfigSource::Markdownlint,
                                file: Some(filename.to_string()),
                                line: None,
                            }],
                        };
                        global.include = SourcedValue {
                            value: Vec::new(),
                            source: ConfigSource::Markdownlint,
                            overrides: vec![ConfigOverride {
                                value: Vec::new(),
                                source: ConfigSource::Markdownlint,
                                file: Some(filename.to_string()),
                                line: None,
                            }],
                        };
                        global.respect_gitignore = SourcedValue {
                            value: true,
                            source: ConfigSource::Markdownlint,
                            overrides: vec![ConfigOverride {
                                value: true,
                                source: ConfigSource::Markdownlint,
                                file: Some(filename.to_string()),
                                line: None,
                            }],
                        };
                        loaded_files.push(filename.to_string());
                    }
                    break;
                }
            }
        }
        // 5. CLI overrides (if provided)
        if let Some(cli) = cli_overrides {
            update_vec(&mut global.enable, cli.enable.value.clone(), Cli, None);
            update_vec(&mut global.disable, cli.disable.value.clone(), Cli, None);
            update_vec(&mut global.exclude, cli.exclude.value.clone(), Cli, None);
            update_vec(&mut global.include, cli.include.value.clone(), Cli, None);
            update_bool(
                &mut global.respect_gitignore,
                cli.respect_gitignore.value,
                Cli,
                None,
            );
            // No rule-specific CLI overrides for now
        }
        SourcedConfig {
            global,
            rules,
            loaded_files,
            unknown_keys,
        }
    }
}

impl From<SourcedConfig> for Config {
    fn from(sourced: SourcedConfig) -> Self {
        let global = GlobalConfig {
            enable: sourced.global.enable.value,
            disable: sourced.global.disable.value,
            exclude: sourced.global.exclude.value,
            include: sourced.global.include.value,
            respect_gitignore: sourced.global.respect_gitignore.value,
        };
        let mut rules = BTreeMap::new();
        for (rule_name, sourced_rule) in sourced.rules {
            let mut rule_config = RuleConfig::default();
            for (k, v) in sourced_rule.values {
                rule_config.values.insert(k, v.value);
            }
            rules.insert(rule_name, rule_config);
        }
        Config { global, rules }
    }
}
