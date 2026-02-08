use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use super::flavor::ConfigLoaded;
use super::flavor::ConfigValidated;
use super::parsers;
use super::registry::RuleRegistry;
use super::source_tracking::{
    ConfigSource, ConfigValidationWarning, SourcedConfig, SourcedConfigFragment, SourcedGlobalConfig, SourcedValue,
};
use super::types::{Config, ConfigError, GlobalConfig, MARKDOWNLINT_CONFIG_FILES, RuleConfig};
use super::validation::validate_config_sourced_internal;

impl SourcedConfig<ConfigLoaded> {
    /// Merges another SourcedConfigFragment into this SourcedConfig.
    /// Uses source precedence to determine which values take effect.
    pub(super) fn merge(&mut self, fragment: SourcedConfigFragment) {
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

        // Merge per_file_flavor
        self.per_file_flavor.merge_override(
            fragment.per_file_flavor.value,
            fragment.per_file_flavor.source,
            fragment.per_file_flavor.overrides.first().and_then(|o| o.file.clone()),
            fragment.per_file_flavor.overrides.first().and_then(|o| o.line),
        );

        // Merge code_block_tools
        self.code_block_tools.merge_override(
            fragment.code_block_tools.value,
            fragment.code_block_tools.source,
            fragment.code_block_tools.overrides.first().and_then(|o| o.file.clone()),
            fragment.code_block_tools.overrides.first().and_then(|o| o.line),
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
        // Convert relative paths to absolute to ensure correct traversal
        let mut current = if start_dir.is_relative() {
            std::env::current_dir()
                .map(|cwd| cwd.join(start_dir))
                .unwrap_or_else(|_| start_dir.to_path_buf())
        } else {
            start_dir.to_path_buf()
        };
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
                if let Some(fragment) = parsers::parse_pyproject_toml(&content, &path_str)? {
                    sourced_config.merge(fragment);
                    sourced_config.loaded_files.push(path_str);
                }
            } else {
                let fragment = parsers::parse_rumdl_toml(&content, &path_str, ConfigSource::ProjectConfig)?;
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
            let fragment = parsers::load_from_markdownlint(&path_str)?;
            sourced_config.merge(fragment);
            sourced_config.loaded_files.push(path_str);
        } else {
            // Try TOML only
            let content = std::fs::read_to_string(path).map_err(|e| ConfigError::IoError {
                source: e,
                path: path_str.clone(),
            })?;
            let fragment = parsers::parse_rumdl_toml(&content, &path_str, ConfigSource::ProjectConfig)?;
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
                if let Some(fragment) = parsers::parse_pyproject_toml(&content, &path_str)? {
                    sourced_config.merge(fragment);
                    sourced_config.loaded_files.push(path_str);
                }
            } else {
                let content = std::fs::read_to_string(&user_config_path).map_err(|e| ConfigError::IoError {
                    source: e,
                    path: path_str.clone(),
                })?;
                let fragment = parsers::parse_rumdl_toml(&content, &path_str, ConfigSource::UserConfig)?;
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
                    if let Some(fragment) = parsers::parse_pyproject_toml(&content, &path_str)? {
                        sourced_config.merge(fragment);
                        sourced_config.loaded_files.push(path_str);
                    }
                } else if filename == ".rumdl.toml" || filename == "rumdl.toml" {
                    let content = std::fs::read_to_string(&config_file).map_err(|e| ConfigError::IoError {
                        source: e,
                        path: path_str.clone(),
                    })?;
                    let fragment = parsers::parse_rumdl_toml(&content, &path_str, ConfigSource::ProjectConfig)?;
                    sourced_config.merge(fragment);
                    sourced_config.loaded_files.push(path_str);
                }
            } else {
                // No rumdl project config - try markdownlint config
                log::debug!("[rumdl-config] No rumdl config found, checking markdownlint config");

                if let Some(markdownlint_path) = Self::discover_markdownlint_config_upward() {
                    let path_str = markdownlint_path.display().to_string();
                    log::debug!("[rumdl-config] Found markdownlint config: {path_str}");
                    match parsers::load_from_markdownlint(&path_str) {
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
            per_file_flavor: self.per_file_flavor,
            code_block_tools: self.code_block_tools,
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
            per_file_flavor: self.per_file_flavor,
            code_block_tools: self.code_block_tools,
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
        // Enable is "explicit" if it was set by something other than the Default source
        let enable_is_explicit = sourced.global.enable.source != ConfigSource::Default;

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
            enable_is_explicit,
        };
        Config {
            global,
            per_file_ignores: sourced.per_file_ignores.value,
            per_file_flavor: sourced.per_file_flavor.value,
            code_block_tools: sourced.code_block_tools.value,
            rules,
            project_root: sourced.project_root,
            per_file_ignores_cache: Arc::new(OnceLock::new()),
            per_file_flavor_cache: Arc::new(OnceLock::new()),
        }
    }
}
