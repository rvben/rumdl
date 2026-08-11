//! LSP configuration management
//!
//! Handles LSP settings merging, config loading, file-level config resolution,
//! and rule enable/disable overrides from editor settings.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tower_lsp::lsp_types::*;

use crate::config::{
    Config, ConfigValidated, DiscoveredConfigError, MARKDOWNLINT_CONFIG_FILES, RUMDL_CONFIG_FILES, SourcedConfig,
    editorconfig,
};
use crate::rule::Rule;

use super::server::{ConfigCacheEntry, ConfigResolver, RumdlLanguageServer};
use super::types::{ConfigurationPreference, LspRuleSettings, RumdlLspConfig};

/// A project config that loaded, and where it came from.
struct LoadedProjectConfig {
    config: Config,
    /// Present only when the config opts into `.editorconfig` reading.
    sourced: Option<Arc<SourcedConfig<ConfigValidated>>>,
    path: PathBuf,
}

/// Outcome of walking a file's project-config candidates.
enum ResolvedProjectConfig {
    /// A candidate loaded. Boxed so the variant does not dominate the enum's size.
    Loaded(Box<LoadedProjectConfig>),
    /// No candidate applies to this file, so the config loaded at startup governs it.
    NotFound,
    /// A candidate applies but cannot be loaded, because the user config it merges
    /// onto is broken. `rumdl check` exits with a config error here. The server
    /// cannot exit, and any substitute - a config from further up the tree, or the
    /// one discovery found at startup - would lint against a ruleset that exists
    /// nowhere on disk, so the answer is defaults.
    Unresolvable,
}

/// Merge editor settings with a file's resolved filesystem configuration.
///
/// This is intentionally independent of the language-server transport so both
/// request handling and workspace indexing apply the same precedence policy.
fn merge_lsp_settings(mut file_config: Config, lsp_config: &RumdlLspConfig) -> Config {
    let Some(settings) = &lsp_config.settings else {
        return file_config;
    };

    match lsp_config.configuration_preference {
        ConfigurationPreference::EditorFirst => apply_lsp_settings_to_config(&mut file_config, settings),
        ConfigurationPreference::FilesystemFirst => apply_lsp_settings_if_absent(&mut file_config, settings),
        ConfigurationPreference::EditorOnly => {
            let mut default_config = Config::default();
            apply_lsp_settings_to_config(&mut default_config, settings);
            return default_config;
        }
    }

    file_config
}

fn apply_lsp_settings_to_config(config: &mut Config, settings: &LspRuleSettings) {
    if let Some(line_length) = settings.line_length {
        config.global.line_length = crate::types::LineLength::new(line_length);
    }
    if let Some(disable) = &settings.disable {
        config.global.disable.extend(disable.iter().cloned());
    }
    if let Some(enable) = &settings.enable {
        config.global.enable.extend(enable.iter().cloned());
    }
    for (rule_name, rule_config) in &settings.rules {
        apply_rule_config(config, rule_name, rule_config);
    }
    config.canonicalize_rule_lists();
}

fn apply_lsp_settings_if_absent(config: &mut Config, settings: &LspRuleSettings) {
    if config.global.line_length.get() == 80
        && let Some(line_length) = settings.line_length
    {
        config.global.line_length = crate::types::LineLength::new(line_length);
    }
    if let Some(disable) = &settings.disable {
        config.global.disable.extend(disable.iter().cloned());
    }
    if let Some(enable) = &settings.enable {
        config.global.enable.extend(enable.iter().cloned());
    }
    for (rule_name, rule_config) in &settings.rules {
        apply_rule_config_if_absent(config, rule_name, rule_config);
    }
    config.canonicalize_rule_lists();
}

pub(super) fn apply_rule_config(config: &mut Config, rule_name: &str, rule_config: &serde_json::Value) {
    let rule_key = rule_name.to_uppercase();
    let rule_entry = config.rules.entry(rule_key.clone()).or_default();

    if let Some(obj) = rule_config.as_object() {
        for (key, value) in obj {
            let config_key = camel_to_snake(key);
            if config_key == "severity" {
                if let Some(severity_str) = value.as_str() {
                    match serde_json::from_value::<crate::rule::Severity>(serde_json::Value::String(
                        severity_str.to_string(),
                    )) {
                        Ok(severity) => rule_entry.severity = Some(severity),
                        Err(_) => log::warn!(
                            "Invalid severity '{severity_str}' for rule {rule_key}. Valid values: error, warning, info"
                        ),
                    }
                }
                continue;
            }

            if let Some(toml_value) = json_to_toml(value) {
                rule_entry.values.insert(config_key, toml_value);
            }
        }
    }
}

pub(super) fn apply_rule_config_if_absent(config: &mut Config, rule_name: &str, rule_config: &serde_json::Value) {
    let rule_key = rule_name.to_uppercase();
    let existing_rule = config.rules.get(&rule_key);
    let has_existing_values = existing_rule.is_some_and(|rule| !rule.values.is_empty());
    let has_existing_severity = existing_rule.and_then(|rule| rule.severity).is_some();

    if let Some(obj) = rule_config.as_object() {
        let rule_entry = config.rules.entry(rule_key.clone()).or_default();
        for (key, value) in obj {
            let config_key = camel_to_snake(key);
            if config_key == "severity" {
                if !has_existing_severity && let Some(severity_str) = value.as_str() {
                    match serde_json::from_value::<crate::rule::Severity>(serde_json::Value::String(
                        severity_str.to_string(),
                    )) {
                        Ok(severity) => rule_entry.severity = Some(severity),
                        Err(_) => log::warn!(
                            "Invalid severity '{severity_str}' for rule {rule_key}. Valid values: error, warning, info"
                        ),
                    }
                }
                continue;
            }

            if !has_existing_values && let Some(toml_value) = json_to_toml(value) {
                rule_entry.values.insert(config_key, toml_value);
            }
        }
    }
}

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

fn json_to_toml(json: &serde_json::Value) -> Option<toml::Value> {
    match json {
        serde_json::Value::Bool(value) => Some(toml::Value::Boolean(*value)),
        serde_json::Value::Number(value) => value
            .as_i64()
            .map(toml::Value::Integer)
            .or_else(|| value.as_f64().map(toml::Value::Float)),
        serde_json::Value::String(value) => Some(toml::Value::String(value.clone())),
        serde_json::Value::Array(values) => Some(toml::Value::Array(values.iter().filter_map(json_to_toml).collect())),
        serde_json::Value::Object(values) => Some(toml::Value::Table(
            values
                .iter()
                .filter_map(|(key, value)| Some((camel_to_snake(key), json_to_toml(value)?)))
                .collect(),
        )),
        serde_json::Value::Null => None,
    }
}

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
            // Canonicalise so editor input (`"no-inline-html"`) matches `Rule::name()` (`"MD033"`).
            let enable_set: std::collections::HashSet<String> = enable_rules
                .into_iter()
                .map(|s| crate::config::resolve_rule_name(&s))
                .collect();
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
            let disable_set: std::collections::HashSet<String> = disable_rules
                .into_iter()
                .map(|s| crate::config::resolve_rule_name(&s))
                .collect();
            filtered_rules.retain(|rule| !disable_set.contains(rule.name()));
        }

        filtered_rules
    }

    /// Merge LSP settings into a Config based on configuration preference
    ///
    /// This follows Ruff's pattern where editors can pass per-rule configuration
    /// via LSP initialization options. The `configuration_preference` controls
    /// whether editor settings override filesystem configs or vice versa.
    pub(super) fn merge_lsp_settings(&self, file_config: Config, lsp_config: &RumdlLspConfig) -> Config {
        merge_lsp_settings(file_config, lsp_config)
    }

    /// Load or reload rumdl configuration from files.
    ///
    /// Resolves the explicit config path with this precedence:
    /// 1. `cli_config_path` -- supplied via `rumdl server --config`. Highest priority
    ///    so users distributing a canonical ruleset (e.g. a Claude Code plugin)
    ///    cannot have it overridden by editor settings.
    /// 2. `self.config.config_path` -- supplied by the client via initialization
    ///    options or `workspace/didChangeConfiguration`.
    /// 3. None -- fall through to auto-discovery in `load_config_for_lsp`, which
    ///    walks up from the workspace root rather than the process working
    ///    directory: the editor chooses where to launch the server, so that
    ///    directory can sit in a project the user is not editing.
    pub(super) async fn load_configuration(&self, notify_client: bool) {
        self.load_configuration_impl(notify_client, None, None).await
    }

    /// Internal implementation that accepts the user-config directory and the home
    /// directory for testing, mirroring `resolve_config_for_file_impl`.
    pub(crate) async fn load_configuration_impl(
        &self,
        notify_client: bool,
        user_config_dir: Option<&Path>,
        home_dir: Option<&Path>,
    ) {
        let explicit_config_path = match self.cli_config_path.as_deref() {
            Some(path) => Some(path.to_string()),
            None => self.config.read().await.config_path.clone(),
        };

        // A multi-root workspace has no single answer here, and per-file resolution
        // already covers files in every root; the first root is the one the rest of
        // the server treats as primary.
        let workspace_root = self.workspace_roots.read().await.first().cloned();

        // Use the same discovery logic as CLI but with LSP-specific error handling
        match Self::load_config_for_lsp(
            explicit_config_path.as_deref(),
            workspace_root.as_deref(),
            user_config_dir,
            home_dir,
        ) {
            Ok(sourced_config) => {
                let loaded_files = sourced_config.loaded_files.clone();
                let discovery_warnings = sourced_config.discovery_warnings.clone();
                // Use into_validated_unchecked since LSP doesn't need validation warnings
                let validated = sourced_config.into_validated_unchecked();
                let config: crate::config::Config = validated.clone().into();
                *self.rumdl_sourced.write().await = sourced_for_editorconfig(&config, validated);
                *self.rumdl_config.write().await = config;

                // Surface shadowed-config collisions (e.g. `rumdl.toml` ignored next to
                // `.rumdl.toml`) so editor users learn which file is winning.
                for warning in &discovery_warnings {
                    log::warn!("{warning}");
                    if notify_client {
                        self.client.log_message(MessageType::WARNING, warning).await;
                    }
                }

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
                *self.rumdl_sourced.write().await = None;
                *self.rumdl_config.write().await = crate::config::Config::default();
            }
        }
    }

    /// Reload rumdl configuration from files (with client notification)
    pub(super) async fn reload_configuration(&self) {
        self.load_configuration(true).await;
    }

    /// Whether any config this server resolved reads `.editorconfig` files.
    ///
    /// The opt-in can come from a directory config, which only per-file
    /// resolution knows about, so the cache answers alongside the fallback: an
    /// entry keeps its sourced form exactly when its config opted in.
    pub(crate) async fn reads_editorconfig(&self) -> bool {
        if self.rumdl_config.read().await.global.editorconfig {
            return true;
        }
        self.config_cache
            .read()
            .await
            .values()
            .any(|entry| entry.sourced.is_some())
    }

    /// Load the workspace-level configuration, the way the CLI would inside `start_dir`.
    ///
    /// `start_dir` is the workspace root. Without one the walk falls back to the
    /// process working directory, which is right only when nothing better exists:
    /// a client that sends neither `workspaceFolders` nor `rootUri`, or a server
    /// started by hand in a terminal, where that directory is the user's own
    /// choice exactly as it is for `rumdl check`.
    ///
    /// `user_config_dir` and `home_dir` override the platform user-config directory
    /// and the home-directory walk boundary, which tests set to keep discovery
    /// inside a temporary tree.
    pub(crate) fn load_config_for_lsp(
        config_path: Option<&str>,
        start_dir: Option<&Path>,
        user_config_dir: Option<&Path>,
        home_dir: Option<&Path>,
    ) -> Result<crate::config::SourcedConfig, crate::config::ConfigError> {
        match start_dir {
            Some(dir) => crate::config::SourcedConfig::load_for_workspace(dir, config_path, user_config_dir, home_dir),
            None => crate::config::SourcedConfig::load_with_discovery_impl(
                config_path,
                None,
                false,
                user_config_dir,
                home_dir,
            ),
        }
    }

    pub(crate) async fn resolve_config_for_file(&self, file_path: &Path) -> Config {
        self.resolve_config_for_file_impl(file_path, None, None).await
    }

    pub(crate) async fn resolve_config_for_file_impl(
        &self,
        file_path: &Path,
        user_config_dir: Option<&Path>,
        home_dir_override: Option<&Path>,
    ) -> Config {
        self.config_resolver
            .resolve_config_for_file_impl(file_path, user_config_dir, home_dir_override)
            .await
    }
}

impl ConfigResolver {
    /// The root configuration used to select files during a workspace scan.
    pub(crate) async fn workspace_config(&self) -> Config {
        self.rumdl_config.read().await.clone()
    }

    /// Resolve the complete configuration used to interpret one document.
    ///
    /// Project and `.editorconfig` settings are resolved for the path first;
    /// editor settings are then layered according to `configurationPreference`.
    pub(crate) async fn resolve_effective_config_for_file(&self, file_path: &Path) -> Config {
        let file_config = self.resolve_config_for_file(file_path).await;
        let lsp_config = self.config.read().await.clone();
        merge_lsp_settings(file_config, &lsp_config)
    }

    /// Resolve configuration for a specific file
    ///
    /// This method searches for a configuration file starting from the file's directory
    /// and walking up the directory tree until a workspace root is hit or a config is found.
    ///
    /// Results are cached to avoid repeated filesystem access.
    ///
    /// When an explicit config path is set -- either via `rumdl server --config`
    /// (`cli_config_path`) or by the client (`self.config.config_path`) -- per-file
    /// discovery is skipped entirely and the already-loaded `rumdl_config` is returned.
    /// This mirrors the CLI's "explicit config is standalone" rule (see
    /// `src/config/loading.rs::load_with_discovery_impl`) and ensures that a distributed
    /// ruleset is not silently overridden by `.rumdl.toml` files in the user's project.
    pub(crate) async fn resolve_config_for_file(&self, file_path: &std::path::Path) -> Config {
        self.resolve_config_for_file_impl(file_path, None, None).await
    }

    /// Internal implementation that accepts the user-config directory and the home
    /// directory for testing, mirroring `SourcedConfig::load_with_discovery_impl`.
    /// Both are resolved from the platform when not given.
    pub(crate) async fn resolve_config_for_file_impl(
        &self,
        file_path: &std::path::Path,
        user_config_dir: Option<&std::path::Path>,
        home_dir_override: Option<&std::path::Path>,
    ) -> Config {
        if self.cli_config_path.is_some() || self.config.read().await.config_path.is_some() {
            log::debug!(
                "Explicit config path set; bypassing per-file discovery for {}",
                file_path.display()
            );
            let config = self.rumdl_config.read().await.clone();
            let sourced = self.rumdl_sourced.read().await.clone();
            return with_editorconfig(config, sourced.as_deref(), file_path);
        }

        // Get the directory to start searching from
        let search_dir = file_path.parent().unwrap_or(file_path).to_path_buf();

        // Check cache first
        {
            let cache = self.config_cache.read().await;
            if let Some(entry) = cache.get(&search_dir) {
                // If the cached entry is a global fallback, check whether a config file
                // has since been created in the directory. If so, treat as a cache miss
                // so we pick up the new config file.
                if entry.from_global_fallback {
                    let config_now_exists = RUMDL_CONFIG_FILES
                        .iter()
                        .chain(MARKDOWNLINT_CONFIG_FILES.iter())
                        .any(|name| search_dir.join(name).exists());
                    if config_now_exists {
                        log::debug!(
                            "Config cache fallback entry for {} is stale: config file now exists, re-resolving",
                            search_dir.display()
                        );
                        // Drop the read lock and fall through to cache miss path
                    } else {
                        log::debug!(
                            "Config cache hit for directory: {} (loaded from: global/user fallback)",
                            search_dir.display(),
                        );
                        return with_editorconfig(entry.config.clone(), entry.sourced.as_deref(), file_path);
                    }
                } else {
                    let source_owned: String;
                    let source: &str = if let Some(path) = &entry.config_file {
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
                    return with_editorconfig(entry.config.clone(), entry.sourced.as_deref(), file_path);
                }
            }
        }

        // Cache miss - need to search for config
        log::debug!(
            "Config cache miss for directory: {}, searching for config...",
            search_dir.display()
        );

        // Try to find workspace root for this file, and note whether it is the root
        // the server loaded its own configuration from.
        let (workspace_root, in_primary_root) = {
            let workspace_roots = self.workspace_roots.read().await;
            let root = workspace_roots
                .iter()
                .find(|root| search_dir.starts_with(root))
                .cloned();
            let is_primary = match (&root, workspace_roots.first()) {
                (Some(root), Some(primary)) => root == primary,
                _ => false,
            };
            (root, is_primary)
        };

        // Resolve the home-directory boundary so a user-level `~/.rumdl.toml` is not
        // mistaken for a project config; it stays a user-config fallback instead,
        // preserving the platform user-config directory's precedence over the dotfile.
        let home_dir = home_dir_override.map(std::path::Path::to_path_buf).or_else(|| {
            use etcetera::{BaseStrategy, choose_base_strategy};
            choose_base_strategy().ok().map(|s| s.home_dir().to_path_buf())
        });

        // Walk upward from the file's directory, bounded by the workspace root and the
        // home directory. Candidates are nearest-first; load the first that parses so a
        // malformed nearer config falls through to the next one up, mirroring the CLI.
        let candidates = crate::config::collect_project_config_candidates(
            &search_dir,
            workspace_root.as_deref(),
            home_dir.as_deref(),
        );

        // These candidates were discovered, not named by the user, so they load through
        // the discovery loader: it keeps the user config as a base under a markdownlint
        // project config, which is what `rumdl check` resolves for the same file. The
        // explicit-config loader is standalone and belongs to the `--config` path above.
        let mut resolution = ResolvedProjectConfig::NotFound;
        for config_path in candidates {
            match crate::config::SourcedConfig::load_discovered(&config_path, user_config_dir, home_dir.as_deref()) {
                Ok(sourced) => {
                    log::debug!("Found config file: {}", config_path.display());
                    let validated = sourced.into_validated_unchecked();
                    let config: Config = validated.clone().into();
                    resolution = ResolvedProjectConfig::Loaded(Box::new(LoadedProjectConfig {
                        sourced: sourced_for_editorconfig(&config, validated),
                        config,
                        path: config_path,
                    }));
                    break;
                }
                Err(DiscoveredConfigError::ProjectConfig(e)) => {
                    log::debug!("Skipping unloadable config {}: {e}", config_path.display());
                }
                Err(DiscoveredConfigError::UserConfig(e)) => {
                    log::warn!(
                        "Cannot resolve {} for {}: {e}. Using default rules until the user config is fixed.",
                        config_path.display(),
                        file_path.display()
                    );
                    resolution = ResolvedProjectConfig::Unresolvable;
                    break;
                }
            }
        }

        // The user config lives outside the workspace, so nothing this server watches
        // changes when it is repaired. Answering with defaults and skipping the cache
        // keeps the file linting on the next resolution instead of for the session.
        if matches!(resolution, ResolvedProjectConfig::Unresolvable) {
            return Config::default();
        }

        // Use found config or fall back to global/user config loaded at initialization
        let (config, sourced, config_file) = match resolution {
            ResolvedProjectConfig::Loaded(loaded) => {
                let LoadedProjectConfig { config, sourced, path } = *loaded;
                (config, sourced, Some(path))
            }
            _ => {
                let fallback = self
                    .fallback_config_for(
                        workspace_root.as_deref(),
                        in_primary_root,
                        user_config_dir,
                        home_dir.as_deref(),
                    )
                    .await;
                // The config this root cannot resolve lies outside it, so nothing this
                // server watches changes when it is repaired. Answering with defaults
                // and skipping the cache keeps the file resolving on the next request.
                let Some((fallback, sourced)) = fallback else {
                    return Config::default();
                };
                (fallback, sourced, None)
            }
        };

        // Cache the result
        let from_global = config_file.is_none();
        let entry = ConfigCacheEntry {
            config: config.clone(),
            sourced: sourced.clone(),
            config_file,
            from_global_fallback: from_global,
        };

        self.config_cache.write().await.insert(search_dir, entry);

        with_editorconfig(config, sourced.as_deref(), file_path)
    }

    /// The configuration for a file whose own scope holds no config file.
    ///
    /// The per-file walk stops at the workspace root, so a config living above the
    /// root is still the one `rumdl check` would resolve there. That is what the
    /// server loaded at startup, but only for the root it treats as primary: every
    /// other root is a separate project, and answering with the primary root's
    /// config would lint one project under another's settings. Those roots resolve
    /// their own scope instead. A file in no root at all keeps the startup config,
    /// which is the only scope the server has for it.
    ///
    /// `None` means that root's scope could not be resolved. Answering with the
    /// startup config would put the project straight back under another project's
    /// settings, so the caller answers with defaults, the same way it does for a
    /// user config it cannot read.
    async fn fallback_config_for(
        &self,
        workspace_root: Option<&Path>,
        in_primary_root: bool,
        user_config_dir: Option<&Path>,
        home_dir: Option<&Path>,
    ) -> Option<(Config, Option<Arc<SourcedConfig<ConfigValidated>>>)> {
        if let Some(root) = workspace_root.filter(|_| !in_primary_root) {
            match crate::config::SourcedConfig::load_for_workspace(root, None, user_config_dir, home_dir) {
                Ok(sourced) => {
                    log::debug!(
                        "No project config found; using the scope of workspace root {}",
                        root.display()
                    );
                    let validated = sourced.into_validated_unchecked();
                    let config: Config = validated.clone().into();
                    let sourced = sourced_for_editorconfig(&config, validated);
                    return Some((config, sourced));
                }
                Err(e) => {
                    log::warn!(
                        "Cannot resolve the configuration of workspace root {}: {e}. Using default rules until it is fixed.",
                        root.display()
                    );
                    return None;
                }
            }
        }

        log::debug!("No project config found; using global/user fallback config");
        Some((
            self.rumdl_config.read().await.clone(),
            self.rumdl_sourced.read().await.clone(),
        ))
    }
}

/// Keep a config's sourced form only when it opts into `.editorconfig` reading.
///
/// Layering `.editorconfig` in needs to know which settings a rumdl config set,
/// which only the sourced form records. Every other server drops it rather than
/// hold a second copy of the configuration for its whole session.
fn sourced_for_editorconfig(
    config: &Config,
    sourced: SourcedConfig<ConfigValidated>,
) -> Option<Arc<SourcedConfig<ConfigValidated>>> {
    config.global.editorconfig.then(|| Arc::new(sourced))
}

/// Layer the `.editorconfig` properties that apply to `file_path` onto a config.
///
/// A `sourced` of `None` means the configuration never opted in, so this is the
/// identity. Properties rumdl read but does not act on are logged rather than
/// sent to the client: a divergence is a property of the project's settings, not
/// of the file being edited, so it would otherwise repeat on every keystroke.
fn with_editorconfig(config: Config, sourced: Option<&SourcedConfig<ConfigValidated>>, file_path: &Path) -> Config {
    let Some(sourced) = sourced else {
        return config;
    };

    let resolution = editorconfig::resolve(file_path);
    for warning in &resolution.warnings {
        log::warn!("{}", warning.message);
    }
    if resolution.settings.is_empty() {
        return config;
    }

    let mut sourced = sourced.clone();
    editorconfig::apply(&mut sourced, &resolution.settings, resolution.origin.as_deref());
    sourced.into()
}
