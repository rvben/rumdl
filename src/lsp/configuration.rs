//! LSP configuration management
//!
//! Handles LSP settings merging, config loading, file-level config resolution,
//! and rule enable/disable overrides from editor settings.

use std::path::PathBuf;

use anyhow::Result;
use tower_lsp::lsp_types::*;

use crate::config::Config;
use crate::rule::Rule;

use super::server::{ConfigCacheEntry, RumdlLanguageServer};
use super::types::{ConfigurationPreference, LspRuleSettings, RumdlLspConfig};

impl RumdlLanguageServer {
    /// Apply enable_rules/disable_rules overrides from LSP config
    pub(super) fn apply_lsp_config_overrides(
        &self,
        mut filtered_rules: Vec<Box<dyn Rule>>,
        lsp_config: &RumdlLspConfig,
    ) -> Vec<Box<dyn Rule>> {
        // Collect enable rules from both top-level and settings
        let mut enable_rules: Vec<String> = Vec::new();
        if let Some(enable) = &lsp_config.enable_rules {
            enable_rules.extend(enable.iter().cloned());
        }
        if let Some(settings) = &lsp_config.settings
            && let Some(enable) = &settings.enable
        {
            enable_rules.extend(enable.iter().cloned());
        }

        // Apply enable_rules override (if specified, only these rules are active)
        if !enable_rules.is_empty() {
            let enable_set: std::collections::HashSet<String> = enable_rules.into_iter().collect();
            filtered_rules.retain(|rule| enable_set.contains(rule.name()));
        }

        // Collect disable rules from both top-level and settings
        let mut disable_rules: Vec<String> = Vec::new();
        if let Some(disable) = &lsp_config.disable_rules {
            disable_rules.extend(disable.iter().cloned());
        }
        if let Some(settings) = &lsp_config.settings
            && let Some(disable) = &settings.disable
        {
            disable_rules.extend(disable.iter().cloned());
        }

        // Apply disable_rules override
        if !disable_rules.is_empty() {
            let disable_set: std::collections::HashSet<String> = disable_rules.into_iter().collect();
            filtered_rules.retain(|rule| !disable_set.contains(rule.name()));
        }

        filtered_rules
    }

    /// Merge LSP settings into a Config based on configuration preference
    ///
    /// This follows Ruff's pattern where editors can pass per-rule configuration
    /// via LSP initialization options. The `configuration_preference` controls
    /// whether editor settings override filesystem configs or vice versa.
    pub(super) fn merge_lsp_settings(&self, mut file_config: Config, lsp_config: &RumdlLspConfig) -> Config {
        let Some(settings) = &lsp_config.settings else {
            return file_config;
        };

        match lsp_config.configuration_preference {
            ConfigurationPreference::EditorFirst => {
                // Editor settings take priority - apply them on top of file config
                self.apply_lsp_settings_to_config(&mut file_config, settings);
            }
            ConfigurationPreference::FilesystemFirst => {
                // File config takes priority - only apply settings for values not in file config
                self.apply_lsp_settings_if_absent(&mut file_config, settings);
            }
            ConfigurationPreference::EditorOnly => {
                // Ignore file config completely - start from default and apply editor settings
                let mut default_config = Config::default();
                self.apply_lsp_settings_to_config(&mut default_config, settings);
                return default_config;
            }
        }

        file_config
    }

    /// Apply all LSP settings to config, overriding existing values
    fn apply_lsp_settings_to_config(&self, config: &mut Config, settings: &LspRuleSettings) {
        // Apply global line length
        if let Some(line_length) = settings.line_length {
            config.global.line_length = crate::types::LineLength::new(line_length);
        }

        // Apply disable list
        if let Some(disable) = &settings.disable {
            config.global.disable.extend(disable.iter().cloned());
        }

        // Apply enable list
        if let Some(enable) = &settings.enable {
            config.global.enable.extend(enable.iter().cloned());
        }

        // Apply per-rule settings (e.g., "MD013": { "lineLength": 120 })
        for (rule_name, rule_config) in &settings.rules {
            self.apply_rule_config(config, rule_name, rule_config);
        }
    }

    /// Apply LSP settings to config only where file config doesn't specify values
    fn apply_lsp_settings_if_absent(&self, config: &mut Config, settings: &LspRuleSettings) {
        // Apply global line length only if using default value
        // LineLength default is 80, so we can check if it's still the default
        if config.global.line_length.get() == 80
            && let Some(line_length) = settings.line_length
        {
            config.global.line_length = crate::types::LineLength::new(line_length);
        }

        // For disable/enable lists, we merge them (filesystem values are already there)
        if let Some(disable) = &settings.disable {
            config.global.disable.extend(disable.iter().cloned());
        }

        if let Some(enable) = &settings.enable {
            config.global.enable.extend(enable.iter().cloned());
        }

        // Apply per-rule settings only if not already configured in file
        for (rule_name, rule_config) in &settings.rules {
            self.apply_rule_config_if_absent(config, rule_name, rule_config);
        }
    }

    /// Apply per-rule configuration from LSP settings
    ///
    /// Converts JSON values from LSP settings to TOML values and merges them
    /// into the config's rule-specific BTreeMap.
    pub(super) fn apply_rule_config(&self, config: &mut Config, rule_name: &str, rule_config: &serde_json::Value) {
        let rule_key = rule_name.to_uppercase();

        // Get or create the rule config entry
        let rule_entry = config.rules.entry(rule_key.clone()).or_default();

        // Convert JSON object to TOML values and merge
        if let Some(obj) = rule_config.as_object() {
            for (key, value) in obj {
                // Convert camelCase to snake_case for config compatibility
                let config_key = Self::camel_to_snake(key);

                // Handle severity specially - it's a first-class field on RuleConfig
                if config_key == "severity" {
                    if let Some(severity_str) = value.as_str() {
                        match serde_json::from_value::<crate::rule::Severity>(serde_json::Value::String(
                            severity_str.to_string(),
                        )) {
                            Ok(severity) => {
                                rule_entry.severity = Some(severity);
                            }
                            Err(_) => {
                                log::warn!(
                                    "Invalid severity '{severity_str}' for rule {rule_key}. \
                                     Valid values: error, warning, info"
                                );
                            }
                        }
                    }
                    continue;
                }

                // Convert JSON value to TOML value
                if let Some(toml_value) = Self::json_to_toml(value) {
                    rule_entry.values.insert(config_key, toml_value);
                }
            }
        }
    }

    /// Apply per-rule configuration only if not already set in file config
    ///
    /// For FilesystemFirst mode: file config takes precedence for each setting.
    /// This means:
    /// - If file has severity set, don't override it with LSP severity
    /// - If file has values set, don't override them with LSP values
    /// - Handle severity and values independently
    pub(super) fn apply_rule_config_if_absent(
        &self,
        config: &mut Config,
        rule_name: &str,
        rule_config: &serde_json::Value,
    ) {
        let rule_key = rule_name.to_uppercase();

        // Check existing config state
        let existing_rule = config.rules.get(&rule_key);
        let has_existing_values = existing_rule.map(|r| !r.values.is_empty()).unwrap_or(false);
        let has_existing_severity = existing_rule.and_then(|r| r.severity).is_some();

        // Apply LSP settings, respecting file config
        if let Some(obj) = rule_config.as_object() {
            let rule_entry = config.rules.entry(rule_key.clone()).or_default();

            for (key, value) in obj {
                let config_key = Self::camel_to_snake(key);

                // Handle severity independently
                if config_key == "severity" {
                    if !has_existing_severity && let Some(severity_str) = value.as_str() {
                        match serde_json::from_value::<crate::rule::Severity>(serde_json::Value::String(
                            severity_str.to_string(),
                        )) {
                            Ok(severity) => {
                                rule_entry.severity = Some(severity);
                            }
                            Err(_) => {
                                log::warn!(
                                    "Invalid severity '{severity_str}' for rule {rule_key}. \
                                     Valid values: error, warning, info"
                                );
                            }
                        }
                    }
                    continue;
                }

                // Handle other values only if file config doesn't have any values for this rule
                if !has_existing_values && let Some(toml_value) = Self::json_to_toml(value) {
                    rule_entry.values.insert(config_key, toml_value);
                }
            }
        }
    }

    /// Convert camelCase to snake_case
    fn camel_to_snake(s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap_or(c));
        }
        result
    }

    /// Convert a JSON value to a TOML value
    fn json_to_toml(json: &serde_json::Value) -> Option<toml::Value> {
        match json {
            serde_json::Value::Bool(b) => Some(toml::Value::Boolean(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Some(toml::Value::Integer(i))
                } else {
                    n.as_f64().map(toml::Value::Float)
                }
            }
            serde_json::Value::String(s) => Some(toml::Value::String(s.clone())),
            serde_json::Value::Array(arr) => {
                let toml_arr: Vec<toml::Value> = arr.iter().filter_map(Self::json_to_toml).collect();
                Some(toml::Value::Array(toml_arr))
            }
            serde_json::Value::Object(obj) => {
                let mut table = toml::map::Map::new();
                for (k, v) in obj {
                    if let Some(toml_v) = Self::json_to_toml(v) {
                        table.insert(Self::camel_to_snake(k), toml_v);
                    }
                }
                Some(toml::Value::Table(table))
            }
            serde_json::Value::Null => None,
        }
    }

    /// Load or reload rumdl configuration from files
    pub(super) async fn load_configuration(&self, notify_client: bool) {
        let config_guard = self.config.read().await;
        let explicit_config_path = config_guard.config_path.clone();
        drop(config_guard);

        // Use the same discovery logic as CLI but with LSP-specific error handling
        match Self::load_config_for_lsp(explicit_config_path.as_deref()) {
            Ok(sourced_config) => {
                let loaded_files = sourced_config.loaded_files.clone();
                // Use into_validated_unchecked since LSP doesn't need validation warnings
                *self.rumdl_config.write().await = sourced_config.into_validated_unchecked().into();

                if !loaded_files.is_empty() {
                    let message = format!("Loaded rumdl config from: {}", loaded_files.join(", "));
                    log::info!("{message}");
                    if notify_client {
                        self.client.log_message(MessageType::INFO, &message).await;
                    }
                } else {
                    log::info!("Using default rumdl configuration (no config files found)");
                }
            }
            Err(e) => {
                let message = format!("Failed to load rumdl config: {e}");
                log::warn!("{message}");
                if notify_client {
                    self.client.log_message(MessageType::WARNING, &message).await;
                }
                // Use default configuration
                *self.rumdl_config.write().await = crate::config::Config::default();
            }
        }
    }

    /// Reload rumdl configuration from files (with client notification)
    pub(super) async fn reload_configuration(&self) {
        self.load_configuration(true).await;
    }

    /// Load configuration for LSP - similar to CLI loading but returns Result
    pub(crate) fn load_config_for_lsp(
        config_path: Option<&str>,
    ) -> Result<crate::config::SourcedConfig, crate::config::ConfigError> {
        // Use the same configuration loading as the CLI
        crate::config::SourcedConfig::load_with_discovery(config_path, None, false)
    }

    /// Resolve configuration for a specific file
    ///
    /// This method searches for a configuration file starting from the file's directory
    /// and walking up the directory tree until a workspace root is hit or a config is found.
    ///
    /// Results are cached to avoid repeated filesystem access.
    pub(crate) async fn resolve_config_for_file(&self, file_path: &std::path::Path) -> Config {
        // Get the directory to start searching from
        let search_dir = file_path.parent().unwrap_or(file_path).to_path_buf();

        // Check cache first
        {
            let cache = self.config_cache.read().await;
            if let Some(entry) = cache.get(&search_dir) {
                let source_owned: String; // ensure owned storage for logging
                let source: &str = if entry.from_global_fallback {
                    "global/user fallback"
                } else if let Some(path) = &entry.config_file {
                    source_owned = path.to_string_lossy().to_string();
                    &source_owned
                } else {
                    "<unknown>"
                };
                log::debug!(
                    "Config cache hit for directory: {} (loaded from: {})",
                    search_dir.display(),
                    source
                );
                return entry.config.clone();
            }
        }

        // Cache miss - need to search for config
        log::debug!(
            "Config cache miss for directory: {}, searching for config...",
            search_dir.display()
        );

        // Try to find workspace root for this file
        let workspace_root = {
            let workspace_roots = self.workspace_roots.read().await;
            workspace_roots
                .iter()
                .find(|root| search_dir.starts_with(root))
                .map(|p| p.to_path_buf())
        };

        // Search upward from the file's directory
        let mut current_dir = search_dir.clone();
        let mut found_config: Option<(Config, Option<PathBuf>)> = None;

        loop {
            // Try to find a config file in the current directory
            const CONFIG_FILES: &[&str] = &[".rumdl.toml", "rumdl.toml", "pyproject.toml", ".markdownlint.json"];

            for config_file_name in CONFIG_FILES {
                let config_path = current_dir.join(config_file_name);
                if config_path.exists() {
                    // For pyproject.toml, verify it contains [tool.rumdl] section (same as CLI)
                    if *config_file_name == "pyproject.toml" {
                        if let Ok(content) = std::fs::read_to_string(&config_path) {
                            if content.contains("[tool.rumdl]") || content.contains("tool.rumdl") {
                                log::debug!("Found config file: {} (with [tool.rumdl])", config_path.display());
                            } else {
                                log::debug!("Found pyproject.toml but no [tool.rumdl] section, skipping");
                                continue;
                            }
                        } else {
                            log::warn!("Failed to read pyproject.toml: {}", config_path.display());
                            continue;
                        }
                    } else {
                        log::debug!("Found config file: {}", config_path.display());
                    }

                    // Load the config
                    if let Some(config_path_str) = config_path.to_str() {
                        if let Ok(sourced) = Self::load_config_for_lsp(Some(config_path_str)) {
                            found_config = Some((sourced.into_validated_unchecked().into(), Some(config_path)));
                            break;
                        }
                    } else {
                        log::warn!("Skipping config file with non-UTF-8 path: {}", config_path.display());
                    }
                }
            }

            if found_config.is_some() {
                break;
            }

            // Check if we've hit a workspace root
            if let Some(ref root) = workspace_root
                && &current_dir == root
            {
                log::debug!("Hit workspace root without finding config: {}", root.display());
                break;
            }

            // Move up to parent directory
            if let Some(parent) = current_dir.parent() {
                current_dir = parent.to_path_buf();
            } else {
                // Hit filesystem root
                break;
            }
        }

        // Use found config or fall back to global/user config loaded at initialization
        let (config, config_file) = if let Some((cfg, path)) = found_config {
            (cfg, path)
        } else {
            log::debug!("No project config found; using global/user fallback config");
            let fallback = self.rumdl_config.read().await.clone();
            (fallback, None)
        };

        // Cache the result
        let from_global = config_file.is_none();
        let entry = ConfigCacheEntry {
            config: config.clone(),
            config_file,
            from_global_fallback: from_global,
        };

        self.config_cache.write().await.insert(search_dir, entry);

        config
    }
}
