use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::io;

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
    /// List of rules to disable
    #[serde(default)]
    pub disable: Vec<String>,
    
    /// List of rules to enable exclusively (if provided, only these rules will run)
    #[serde(default)]
    pub enable: Vec<String>,
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
        Err(err) => Err(ConfigError::IoError { source: err, path: path.to_string() }),
    }
}

/// Errors that can occur when loading configuration
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read the configuration file
    #[error("Failed to read config file at {path}: {source}")]
    IoError {
        source: io::Error,
        path: String,
    },
    
    /// Failed to parse the TOML content
    #[error("Failed to parse TOML: {0}")]
    ParseError(#[from] toml::de::Error),
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