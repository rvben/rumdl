use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

/// Represents a rule-specific configuration
#[derive(Debug, Deserialize, Default)]
pub struct RuleConfig {
    /// Configuration values for the rule
    #[serde(flatten)]
    pub values: HashMap<String, toml::Value>,
}

/// Represents the complete configuration loaded from rumdl.toml
#[derive(Debug, Deserialize, Default)]
pub struct Config {
    /// Global configuration options
    #[serde(default)]
    pub global: GlobalConfig,

    /// Rule-specific configurations
    #[serde(flatten)]
    pub rules: HashMap<String, RuleConfig>,
}

/// Global configuration options
#[derive(Debug, Deserialize, Default)]
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

    /// Ignore .gitignore file
    #[serde(default)]
    pub ignore_gitignore: bool,
}

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

# Ignore .gitignore files when scanning directories (default: false)
ignore_gitignore = false

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
