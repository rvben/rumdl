//! Main Language Server Protocol server implementation for rumdl
//!
//! This module implements the core LSP server following Ruff's architecture.
//! It provides real-time markdown linting, diagnostics, and code actions.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use futures::future::join_all;
use tokio::sync::{RwLock, mpsc};
use tower_lsp::jsonrpc::Result as JsonRpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::config::{Config, is_valid_rule_name};
use crate::lint;
use crate::lsp::index_worker::IndexWorker;
use crate::lsp::types::{
    ConfigurationPreference, IndexState, IndexUpdate, LspRuleSettings, RumdlLspConfig, warning_to_code_actions,
    warning_to_diagnostic,
};
use crate::rule::{FixCapability, Rule};
use crate::rules;
use crate::workspace_index::WorkspaceIndex;

/// Supported markdown file extensions (without leading dot)
const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdx", "mkd", "mkdn", "mdown", "mdwn", "qmd", "rmd"];

/// Maximum number of rules in enable/disable lists (DoS protection)
const MAX_RULE_LIST_SIZE: usize = 100;

/// Maximum allowed line length value (DoS protection)
const MAX_LINE_LENGTH: usize = 10_000;

/// Check if a file extension is a markdown extension
#[inline]
fn is_markdown_extension(ext: &str) -> bool {
    MARKDOWN_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

/// Represents a document in the LSP server's cache
#[derive(Clone, Debug, PartialEq)]
struct DocumentEntry {
    /// The document content
    content: String,
    /// Version number from the editor (None for disk-loaded documents)
    version: Option<i32>,
    /// Whether the document was loaded from disk (true) or opened in editor (false)
    from_disk: bool,
}

/// Cache entry for resolved configuration
#[derive(Clone, Debug)]
pub(crate) struct ConfigCacheEntry {
    /// The resolved configuration
    pub(crate) config: Config,
    /// Config file path that was loaded (for invalidation)
    pub(crate) config_file: Option<PathBuf>,
    /// True if this entry came from the global/user fallback (no project config)
    pub(crate) from_global_fallback: bool,
}

/// Main LSP server for rumdl
///
/// Following Ruff's pattern, this server provides:
/// - Real-time diagnostics as users type
/// - Code actions for automatic fixes
/// - Configuration management
/// - Multi-file support
/// - Multi-root workspace support with per-file config resolution
/// - Cross-file analysis with workspace indexing
#[derive(Clone)]
pub struct RumdlLanguageServer {
    client: Client,
    /// Configuration for the LSP server
    config: Arc<RwLock<RumdlLspConfig>>,
    /// Rumdl core configuration (fallback/default)
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) rumdl_config: Arc<RwLock<Config>>,
    /// Document store for open files and cached disk files
    documents: Arc<RwLock<HashMap<Url, DocumentEntry>>>,
    /// Workspace root folders from the client
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) workspace_roots: Arc<RwLock<Vec<PathBuf>>>,
    /// Configuration cache: maps directory path to resolved config
    /// Key is the directory where config search started (file's parent dir)
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) config_cache: Arc<RwLock<HashMap<PathBuf, ConfigCacheEntry>>>,
    /// Workspace index for cross-file analysis (MD051)
    workspace_index: Arc<RwLock<WorkspaceIndex>>,
    /// Current state of the workspace index (building/ready/error)
    index_state: Arc<RwLock<IndexState>>,
    /// Channel to send updates to the background index worker
    update_tx: mpsc::Sender<IndexUpdate>,
    /// Whether the client supports pull diagnostics (textDocument/diagnostic)
    /// When true, we skip pushing diagnostics to avoid duplicates
    client_supports_pull_diagnostics: Arc<RwLock<bool>>,
}

impl RumdlLanguageServer {
    pub fn new(client: Client, cli_config_path: Option<&str>) -> Self {
        // Initialize with CLI config path if provided (for `rumdl server --config` convenience)
        let mut initial_config = RumdlLspConfig::default();
        if let Some(path) = cli_config_path {
            initial_config.config_path = Some(path.to_string());
        }

        // Create shared state for workspace indexing
        let workspace_index = Arc::new(RwLock::new(WorkspaceIndex::new()));
        let index_state = Arc::new(RwLock::new(IndexState::default()));
        let workspace_roots = Arc::new(RwLock::new(Vec::new()));

        // Create channels for index worker communication
        let (update_tx, update_rx) = mpsc::channel::<IndexUpdate>(100);
        let (relint_tx, _relint_rx) = mpsc::channel::<PathBuf>(100);

        // Spawn the background index worker
        let worker = IndexWorker::new(
            update_rx,
            workspace_index.clone(),
            index_state.clone(),
            client.clone(),
            workspace_roots.clone(),
            relint_tx,
        );
        tokio::spawn(worker.run());

        Self {
            client,
            config: Arc::new(RwLock::new(initial_config)),
            rumdl_config: Arc::new(RwLock::new(Config::default())),
            documents: Arc::new(RwLock::new(HashMap::new())),
            workspace_roots,
            config_cache: Arc::new(RwLock::new(HashMap::new())),
            workspace_index,
            index_state,
            update_tx,
            client_supports_pull_diagnostics: Arc::new(RwLock::new(false)),
        }
    }

    /// Get document content, either from cache or by reading from disk
    ///
    /// This method first checks if the document is in the cache (opened in editor).
    /// If not found, it attempts to read the file from disk and caches it for
    /// future requests.
    async fn get_document_content(&self, uri: &Url) -> Option<String> {
        // First check the cache
        {
            let docs = self.documents.read().await;
            if let Some(entry) = docs.get(uri) {
                return Some(entry.content.clone());
            }
        }

        // If not in cache and it's a file URI, try to read from disk
        if let Ok(path) = uri.to_file_path() {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                // Cache the document for future requests
                let entry = DocumentEntry {
                    content: content.clone(),
                    version: None,
                    from_disk: true,
                };

                let mut docs = self.documents.write().await;
                docs.insert(uri.clone(), entry);

                log::debug!("Loaded document from disk and cached: {uri}");
                return Some(content);
            } else {
                log::debug!("Failed to read file from disk: {uri}");
            }
        }

        None
    }

    /// Get document content only if the document is currently open in the editor.
    ///
    /// We intentionally do not read from disk here because diagnostics should be
    /// scoped to open documents. This avoids lingering diagnostics after a file
    /// is closed when clients use pull diagnostics.
    async fn get_open_document_content(&self, uri: &Url) -> Option<String> {
        let docs = self.documents.read().await;
        docs.get(uri)
            .and_then(|entry| (!entry.from_disk).then(|| entry.content.clone()))
    }

    /// Apply LSP config overrides to the filtered rules
    fn apply_lsp_config_overrides(
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
    fn merge_lsp_settings(&self, mut file_config: Config, lsp_config: &RumdlLspConfig) -> Config {
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
    fn apply_lsp_settings_to_config(&self, config: &mut Config, settings: &crate::lsp::types::LspRuleSettings) {
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
    fn apply_lsp_settings_if_absent(&self, config: &mut Config, settings: &crate::lsp::types::LspRuleSettings) {
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
    fn apply_rule_config(&self, config: &mut Config, rule_name: &str, rule_config: &serde_json::Value) {
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
    fn apply_rule_config_if_absent(&self, config: &mut Config, rule_name: &str, rule_config: &serde_json::Value) {
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

    /// Check if a file URI should be excluded based on exclude patterns
    async fn should_exclude_uri(&self, uri: &Url) -> bool {
        // Try to convert URI to file path
        let file_path = match uri.to_file_path() {
            Ok(path) => path,
            Err(_) => return false, // If we can't get a path, don't exclude
        };

        // Resolve configuration for this specific file to get its exclude patterns
        let rumdl_config = self.resolve_config_for_file(&file_path).await;
        let exclude_patterns = &rumdl_config.global.exclude;

        // If no exclude patterns, don't exclude
        if exclude_patterns.is_empty() {
            return false;
        }

        // Convert path to relative path for pattern matching
        // This matches the CLI behavior in find_markdown_files
        let path_to_check = if file_path.is_absolute() {
            // Try to make it relative to the current directory
            if let Ok(cwd) = std::env::current_dir() {
                // Canonicalize both paths to handle symlinks
                if let (Ok(canonical_cwd), Ok(canonical_path)) = (cwd.canonicalize(), file_path.canonicalize()) {
                    if let Ok(relative) = canonical_path.strip_prefix(&canonical_cwd) {
                        relative.to_string_lossy().to_string()
                    } else {
                        // Path is absolute but not under cwd
                        file_path.to_string_lossy().to_string()
                    }
                } else {
                    // Canonicalization failed
                    file_path.to_string_lossy().to_string()
                }
            } else {
                file_path.to_string_lossy().to_string()
            }
        } else {
            // Already relative
            file_path.to_string_lossy().to_string()
        };

        // Check if path matches any exclude pattern
        for pattern in exclude_patterns {
            if let Ok(glob) = globset::Glob::new(pattern) {
                let matcher = glob.compile_matcher();
                if matcher.is_match(&path_to_check) {
                    log::debug!("Excluding file from LSP linting: {path_to_check}");
                    return true;
                }
            }
        }

        false
    }

    /// Lint a document and return diagnostics
    pub(crate) async fn lint_document(&self, uri: &Url, text: &str) -> Result<Vec<Diagnostic>> {
        let config_guard = self.config.read().await;

        // Skip linting if disabled
        if !config_guard.enable_linting {
            return Ok(Vec::new());
        }

        let lsp_config = config_guard.clone();
        drop(config_guard); // Release config lock early

        // Check if file should be excluded based on exclude patterns
        if self.should_exclude_uri(uri).await {
            return Ok(Vec::new());
        }

        // Resolve configuration for this specific file
        let file_path = uri.to_file_path().ok();
        let file_config = if let Some(ref path) = file_path {
            self.resolve_config_for_file(path).await
        } else {
            // Fallback to global config for non-file URIs
            (*self.rumdl_config.read().await).clone()
        };

        // Merge LSP settings with file config based on configuration_preference
        let rumdl_config = self.merge_lsp_settings(file_config, &lsp_config);

        let all_rules = rules::all_rules(&rumdl_config);
        let flavor = if let Some(ref path) = file_path {
            rumdl_config.get_flavor_for_file(path)
        } else {
            rumdl_config.markdown_flavor()
        };

        // Use the standard filter_rules function which respects config's disabled rules
        let mut filtered_rules = rules::filter_rules(&all_rules, &rumdl_config.global);

        // Apply LSP config overrides (select_rules, ignore_rules from VSCode settings)
        filtered_rules = self.apply_lsp_config_overrides(filtered_rules, &lsp_config);

        // Run rumdl linting with the configured flavor
        let mut all_warnings = match crate::lint(text, &filtered_rules, false, flavor, Some(&rumdl_config)) {
            Ok(warnings) => warnings,
            Err(e) => {
                log::error!("Failed to lint document {uri}: {e}");
                return Ok(Vec::new());
            }
        };

        // Run cross-file checks if workspace index is ready
        if let Some(ref path) = file_path {
            let index_state = self.index_state.read().await.clone();
            if matches!(index_state, IndexState::Ready) {
                let workspace_index = self.workspace_index.read().await;
                if let Some(file_index) = workspace_index.get_file(path) {
                    match crate::run_cross_file_checks(
                        path,
                        file_index,
                        &filtered_rules,
                        &workspace_index,
                        Some(&rumdl_config),
                    ) {
                        Ok(cross_file_warnings) => {
                            all_warnings.extend(cross_file_warnings);
                        }
                        Err(e) => {
                            log::warn!("Failed to run cross-file checks for {uri}: {e}");
                        }
                    }
                }
            }
        }

        let diagnostics = all_warnings.iter().map(warning_to_diagnostic).collect();
        Ok(diagnostics)
    }

    /// Update diagnostics for a document
    ///
    /// This method pushes diagnostics to the client via publishDiagnostics.
    /// When the client supports pull diagnostics (textDocument/diagnostic),
    /// we skip pushing to avoid duplicate diagnostics.
    async fn update_diagnostics(&self, uri: Url, text: String) {
        // Skip pushing if client supports pull diagnostics to avoid duplicates
        if *self.client_supports_pull_diagnostics.read().await {
            log::debug!("Skipping push diagnostics for {uri} - client supports pull model");
            return;
        }

        // Get the document version if available
        let version = {
            let docs = self.documents.read().await;
            docs.get(&uri).and_then(|entry| entry.version)
        };

        match self.lint_document(&uri, &text).await {
            Ok(diagnostics) => {
                self.client.publish_diagnostics(uri, diagnostics, version).await;
            }
            Err(e) => {
                log::error!("Failed to update diagnostics: {e}");
            }
        }
    }

    /// Apply all available fixes to a document
    async fn apply_all_fixes(&self, uri: &Url, text: &str) -> Result<Option<String>> {
        // Check if file should be excluded based on exclude patterns
        if self.should_exclude_uri(uri).await {
            return Ok(None);
        }

        let config_guard = self.config.read().await;
        let lsp_config = config_guard.clone();
        drop(config_guard);

        // Resolve configuration for this specific file
        let file_path = uri.to_file_path().ok();
        let file_config = if let Some(ref path) = file_path {
            self.resolve_config_for_file(path).await
        } else {
            // Fallback to global config for non-file URIs
            (*self.rumdl_config.read().await).clone()
        };

        // Merge LSP settings with file config based on configuration_preference
        let rumdl_config = self.merge_lsp_settings(file_config, &lsp_config);

        let all_rules = rules::all_rules(&rumdl_config);
        let flavor = if let Some(ref path) = file_path {
            rumdl_config.get_flavor_for_file(path)
        } else {
            rumdl_config.markdown_flavor()
        };

        // Use the standard filter_rules function which respects config's disabled rules
        let mut filtered_rules = rules::filter_rules(&all_rules, &rumdl_config.global);

        // Apply LSP config overrides (select_rules, ignore_rules from VSCode settings)
        filtered_rules = self.apply_lsp_config_overrides(filtered_rules, &lsp_config);

        // First, run lint to get active warnings (respecting ignore comments)
        // This tells us which rules actually have unfixed issues
        let mut rules_with_warnings = std::collections::HashSet::new();
        let mut fixed_text = text.to_string();

        match lint(&fixed_text, &filtered_rules, false, flavor, Some(&rumdl_config)) {
            Ok(warnings) => {
                for warning in warnings {
                    if let Some(rule_name) = &warning.rule_name {
                        rules_with_warnings.insert(rule_name.clone());
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to lint document for auto-fix: {e}");
                return Ok(None);
            }
        }

        // Early return if no warnings to fix
        if rules_with_warnings.is_empty() {
            return Ok(None);
        }

        // Only apply fixes for rules that have active warnings
        let mut any_changes = false;

        for rule in &filtered_rules {
            // Skip rules that don't have any active warnings
            if !rules_with_warnings.contains(rule.name()) {
                continue;
            }

            let ctx = crate::lint_context::LintContext::new(&fixed_text, flavor, None);
            match rule.fix(&ctx) {
                Ok(new_text) => {
                    if new_text != fixed_text {
                        fixed_text = new_text;
                        any_changes = true;
                    }
                }
                Err(e) => {
                    // Only log if it's an actual error, not just "rule doesn't support auto-fix"
                    let msg = e.to_string();
                    if !msg.contains("does not support automatic fixing") {
                        log::warn!("Failed to apply fix for rule {}: {}", rule.name(), e);
                    }
                }
            }
        }

        if any_changes { Ok(Some(fixed_text)) } else { Ok(None) }
    }

    /// Get the end position of a document
    fn get_end_position(&self, text: &str) -> Position {
        let mut line = 0u32;
        let mut character = 0u32;

        for ch in text.chars() {
            if ch == '\n' {
                line += 1;
                character = 0;
            } else {
                character += 1;
            }
        }

        Position { line, character }
    }

    /// Apply LSP FormattingOptions to content
    ///
    /// This implements the standard LSP formatting options that editors send:
    /// - `trim_trailing_whitespace`: Remove trailing whitespace from each line
    /// - `insert_final_newline`: Ensure file ends with a newline
    /// - `trim_final_newlines`: Remove extra blank lines at end of file
    ///
    /// This is applied AFTER lint fixes to ensure we respect editor preferences
    /// even when the editor's buffer content differs from the file on disk
    /// (e.g., nvim may strip trailing newlines from its buffer representation).
    fn apply_formatting_options(content: String, options: &FormattingOptions) -> String {
        // If the original content is empty, keep it empty regardless of options
        // This prevents marking empty documents as needing formatting
        if content.is_empty() {
            return content;
        }

        let mut result = content.clone();
        let original_ended_with_newline = content.ends_with('\n');

        // 1. Trim trailing whitespace from each line (if requested)
        if options.trim_trailing_whitespace.unwrap_or(false) {
            result = result
                .lines()
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n");
            // Preserve final newline status for next steps
            if original_ended_with_newline && !result.ends_with('\n') {
                result.push('\n');
            }
        }

        // 2. Trim final newlines (remove extra blank lines at EOF)
        // This runs BEFORE insert_final_newline to handle the case where
        // we have multiple trailing newlines and want exactly one
        if options.trim_final_newlines.unwrap_or(false) {
            // Remove all trailing newlines
            while result.ends_with('\n') {
                result.pop();
            }
            // We'll add back exactly one in the next step if insert_final_newline is true
        }

        // 3. Insert final newline (ensure file ends with exactly one newline)
        if options.insert_final_newline.unwrap_or(false) && !result.ends_with('\n') {
            result.push('\n');
        }

        result
    }

    /// Get code actions for diagnostics at a position
    async fn get_code_actions(&self, uri: &Url, text: &str, range: Range) -> Result<Vec<CodeAction>> {
        let config_guard = self.config.read().await;
        let lsp_config = config_guard.clone();
        drop(config_guard);

        // Resolve configuration for this specific file
        let file_path = uri.to_file_path().ok();
        let file_config = if let Some(ref path) = file_path {
            self.resolve_config_for_file(path).await
        } else {
            // Fallback to global config for non-file URIs
            (*self.rumdl_config.read().await).clone()
        };

        // Merge LSP settings with file config based on configuration_preference
        let rumdl_config = self.merge_lsp_settings(file_config, &lsp_config);

        let all_rules = rules::all_rules(&rumdl_config);
        let flavor = if let Some(ref path) = file_path {
            rumdl_config.get_flavor_for_file(path)
        } else {
            rumdl_config.markdown_flavor()
        };

        // Use the standard filter_rules function which respects config's disabled rules
        let mut filtered_rules = rules::filter_rules(&all_rules, &rumdl_config.global);

        // Apply LSP config overrides (select_rules, ignore_rules from VSCode settings)
        filtered_rules = self.apply_lsp_config_overrides(filtered_rules, &lsp_config);

        match crate::lint(text, &filtered_rules, false, flavor, Some(&rumdl_config)) {
            Ok(warnings) => {
                let mut actions = Vec::new();
                let mut fixable_count = 0;

                for warning in &warnings {
                    // Check if warning is within the requested range
                    let warning_line = (warning.line.saturating_sub(1)) as u32;
                    if warning_line >= range.start.line && warning_line <= range.end.line {
                        // Get all code actions for this warning (fix + ignore actions)
                        let mut warning_actions = warning_to_code_actions(warning, uri, text);
                        actions.append(&mut warning_actions);

                        if warning.fix.is_some() {
                            fixable_count += 1;
                        }
                    }
                }

                // Add "Fix all" action if there are multiple fixable issues in range
                if fixable_count > 1 {
                    // Only apply fixes from fixable rules during "Fix all"
                    // Unfixable rules provide warning-level fixes for individual Quick Fix actions
                    let fixable_warnings: Vec<_> = warnings
                        .iter()
                        .filter(|w| {
                            if let Some(rule_name) = &w.rule_name {
                                filtered_rules
                                    .iter()
                                    .find(|r| r.name() == rule_name)
                                    .map(|r| r.fix_capability() != FixCapability::Unfixable)
                                    .unwrap_or(false)
                            } else {
                                false
                            }
                        })
                        .cloned()
                        .collect();

                    // Count total fixable issues (excluding Unfixable rules)
                    let total_fixable = fixable_warnings.len();

                    if let Ok(fixed_content) = crate::utils::fix_utils::apply_warning_fixes(text, &fixable_warnings)
                        && fixed_content != text
                    {
                        // Calculate proper end position
                        let mut line = 0u32;
                        let mut character = 0u32;
                        for ch in text.chars() {
                            if ch == '\n' {
                                line += 1;
                                character = 0;
                            } else {
                                character += 1;
                            }
                        }

                        let fix_all_action = CodeAction {
                            title: format!("Fix all rumdl issues ({total_fixable} fixable)"),
                            kind: Some(CodeActionKind::new("source.fixAll.rumdl")),
                            diagnostics: Some(Vec::new()),
                            edit: Some(WorkspaceEdit {
                                changes: Some(
                                    [(
                                        uri.clone(),
                                        vec![TextEdit {
                                            range: Range {
                                                start: Position { line: 0, character: 0 },
                                                end: Position { line, character },
                                            },
                                            new_text: fixed_content,
                                        }],
                                    )]
                                    .into_iter()
                                    .collect(),
                                ),
                                ..Default::default()
                            }),
                            command: None,
                            is_preferred: Some(true),
                            disabled: None,
                            data: None,
                        };

                        // Insert at the beginning to make it prominent
                        actions.insert(0, fix_all_action);
                    }
                }

                Ok(actions)
            }
            Err(e) => {
                log::error!("Failed to get code actions: {e}");
                Ok(Vec::new())
            }
        }
    }

    /// Load or reload rumdl configuration from files
    async fn load_configuration(&self, notify_client: bool) {
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
    async fn reload_configuration(&self) {
        self.load_configuration(true).await;
    }

    /// Load configuration for LSP - similar to CLI loading but returns Result
    fn load_config_for_lsp(
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

#[tower_lsp::async_trait]
impl LanguageServer for RumdlLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> JsonRpcResult<InitializeResult> {
        log::info!("Initializing rumdl Language Server");

        // Parse client capabilities and configuration
        if let Some(options) = params.initialization_options
            && let Ok(config) = serde_json::from_value::<RumdlLspConfig>(options)
        {
            *self.config.write().await = config;
        }

        // Detect if client supports pull diagnostics (textDocument/diagnostic)
        // When the client supports pull, we avoid pushing to prevent duplicate diagnostics
        let supports_pull = params
            .capabilities
            .text_document
            .as_ref()
            .and_then(|td| td.diagnostic.as_ref())
            .is_some();

        if supports_pull {
            log::info!("Client supports pull diagnostics - disabling push to avoid duplicates");
            *self.client_supports_pull_diagnostics.write().await = true;
        } else {
            log::info!("Client does not support pull diagnostics - using push model");
        }

        // Extract and store workspace roots
        let mut roots = Vec::new();
        if let Some(workspace_folders) = params.workspace_folders {
            for folder in workspace_folders {
                if let Ok(path) = folder.uri.to_file_path() {
                    log::info!("Workspace root: {}", path.display());
                    roots.push(path);
                }
            }
        } else if let Some(root_uri) = params.root_uri
            && let Ok(path) = root_uri.to_file_path()
        {
            log::info!("Workspace root: {}", path.display());
            roots.push(path);
        }
        *self.workspace_roots.write().await = roots;

        // Load rumdl configuration with auto-discovery (fallback/default)
        self.load_configuration(false).await;

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::FULL),
                    will_save: Some(false),
                    will_save_wait_until: Some(true),
                    save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                        include_text: Some(false),
                    })),
                })),
                code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
                    code_action_kinds: Some(vec![
                        CodeActionKind::QUICKFIX,
                        CodeActionKind::SOURCE_FIX_ALL,
                        CodeActionKind::new("source.fixAll.rumdl"),
                    ]),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                    resolve_provider: None,
                })),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                    identifier: Some("rumdl".to_string()),
                    inter_file_dependencies: true,
                    workspace_diagnostics: false,
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "rumdl".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let version = env!("CARGO_PKG_VERSION");

        // Get binary path and build time
        let (binary_path, build_time) = std::env::current_exe()
            .ok()
            .map(|path| {
                let path_str = path.to_str().unwrap_or("unknown").to_string();
                let build_time = std::fs::metadata(&path)
                    .ok()
                    .and_then(|metadata| metadata.modified().ok())
                    .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|duration| {
                        let secs = duration.as_secs();
                        chrono::DateTime::from_timestamp(secs as i64, 0)
                            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    })
                    .unwrap_or_else(|| "unknown".to_string());
                (path_str, build_time)
            })
            .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));

        let working_dir = std::env::current_dir()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown".to_string());

        log::info!("rumdl Language Server v{version} initialized (built: {build_time}, binary: {binary_path})");
        log::info!("Working directory: {working_dir}");

        self.client
            .log_message(MessageType::INFO, format!("rumdl v{version} Language Server started"))
            .await;

        // Trigger initial workspace indexing for cross-file analysis
        if self.update_tx.send(IndexUpdate::FullRescan).await.is_err() {
            log::warn!("Failed to trigger initial workspace indexing");
        } else {
            log::info!("Triggered initial workspace indexing for cross-file analysis");
        }

        // Register file watcher for markdown files to detect external changes
        // Watch all supported markdown extensions
        let markdown_patterns = [
            "**/*.md",
            "**/*.markdown",
            "**/*.mdx",
            "**/*.mkd",
            "**/*.mkdn",
            "**/*.mdown",
            "**/*.mdwn",
            "**/*.qmd",
            "**/*.rmd",
        ];
        let watchers: Vec<_> = markdown_patterns
            .iter()
            .map(|pattern| FileSystemWatcher {
                glob_pattern: GlobPattern::String((*pattern).to_string()),
                kind: Some(WatchKind::all()),
            })
            .collect();

        let registration = Registration {
            id: "markdown-watcher".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options: Some(
                serde_json::to_value(DidChangeWatchedFilesRegistrationOptions { watchers }).unwrap(),
            ),
        };

        if self.client.register_capability(vec![registration]).await.is_err() {
            log::debug!("Client does not support file watching capability");
        }
    }

    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        // Update workspace roots
        let mut roots = self.workspace_roots.write().await;

        // Remove deleted workspace folders
        for removed in &params.event.removed {
            if let Ok(path) = removed.uri.to_file_path() {
                roots.retain(|r| r != &path);
                log::info!("Removed workspace root: {}", path.display());
            }
        }

        // Add new workspace folders
        for added in &params.event.added {
            if let Ok(path) = added.uri.to_file_path()
                && !roots.contains(&path)
            {
                log::info!("Added workspace root: {}", path.display());
                roots.push(path);
            }
        }
        drop(roots);

        // Clear config cache as workspace structure changed
        self.config_cache.write().await.clear();

        // Reload fallback configuration
        self.reload_configuration().await;

        // Trigger full workspace rescan for cross-file index
        if self.update_tx.send(IndexUpdate::FullRescan).await.is_err() {
            log::warn!("Failed to trigger workspace rescan after folder change");
        }
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        log::debug!("Configuration changed: {:?}", params.settings);

        // Parse settings from the notification
        // Neovim sends: { "rumdl": { "MD013": {...}, ... } }
        // VSCode might send the full RumdlLspConfig or similar structure
        let settings_value = params.settings;

        // Try to extract "rumdl" key from settings (Neovim style)
        let rumdl_settings = if let serde_json::Value::Object(ref obj) = settings_value {
            obj.get("rumdl").cloned().unwrap_or(settings_value.clone())
        } else {
            settings_value
        };

        // Track if we successfully applied any configuration
        let mut config_applied = false;
        let mut warnings: Vec<String> = Vec::new();

        // Try to parse as LspRuleSettings first (Neovim style with "disable", "enable", rule keys)
        // We check this first because RumdlLspConfig with #[serde(default)] will accept any JSON
        // and just ignore unknown fields, which would lose the Neovim-style settings
        if let Ok(rule_settings) = serde_json::from_value::<LspRuleSettings>(rumdl_settings.clone())
            && (rule_settings.disable.is_some()
                || rule_settings.enable.is_some()
                || rule_settings.line_length.is_some()
                || !rule_settings.rules.is_empty())
        {
            // Validate rule names in disable/enable lists
            if let Some(ref disable) = rule_settings.disable {
                for rule in disable {
                    if !is_valid_rule_name(rule) {
                        warnings.push(format!("Unknown rule in disable list: {rule}"));
                    }
                }
            }
            if let Some(ref enable) = rule_settings.enable {
                for rule in enable {
                    if !is_valid_rule_name(rule) {
                        warnings.push(format!("Unknown rule in enable list: {rule}"));
                    }
                }
            }
            // Validate rule-specific settings
            for rule_name in rule_settings.rules.keys() {
                if !is_valid_rule_name(rule_name) {
                    warnings.push(format!("Unknown rule in settings: {rule_name}"));
                }
            }

            log::info!("Applied rule settings from configuration (Neovim style)");
            let mut config = self.config.write().await;
            config.settings = Some(rule_settings);
            drop(config);
            config_applied = true;
        } else if let Ok(full_config) = serde_json::from_value::<RumdlLspConfig>(rumdl_settings.clone())
            && (full_config.config_path.is_some()
                || full_config.enable_rules.is_some()
                || full_config.disable_rules.is_some()
                || full_config.settings.is_some()
                || !full_config.enable_linting
                || full_config.enable_auto_fix)
        {
            // Validate rule names
            if let Some(ref rules) = full_config.enable_rules {
                for rule in rules {
                    if !is_valid_rule_name(rule) {
                        warnings.push(format!("Unknown rule in enableRules: {rule}"));
                    }
                }
            }
            if let Some(ref rules) = full_config.disable_rules {
                for rule in rules {
                    if !is_valid_rule_name(rule) {
                        warnings.push(format!("Unknown rule in disableRules: {rule}"));
                    }
                }
            }

            log::info!("Applied full LSP configuration from settings");
            *self.config.write().await = full_config;
            config_applied = true;
        } else if let serde_json::Value::Object(obj) = rumdl_settings {
            // Otherwise, treat as per-rule settings with manual parsing
            // Format: { "MD013": { "lineLength": 80 }, "disable": ["MD009"] }
            let mut config = self.config.write().await;

            // Manual parsing for Neovim format
            let mut rules = std::collections::HashMap::new();
            let mut disable = Vec::new();
            let mut enable = Vec::new();
            let mut line_length = None;

            for (key, value) in obj {
                match key.as_str() {
                    "disable" => match serde_json::from_value::<Vec<String>>(value.clone()) {
                        Ok(d) => {
                            if d.len() > MAX_RULE_LIST_SIZE {
                                warnings.push(format!(
                                    "Too many rules in 'disable' ({} > {}), truncating",
                                    d.len(),
                                    MAX_RULE_LIST_SIZE
                                ));
                            }
                            for rule in d.iter().take(MAX_RULE_LIST_SIZE) {
                                if !is_valid_rule_name(rule) {
                                    warnings.push(format!("Unknown rule in disable: {rule}"));
                                }
                            }
                            disable = d.into_iter().take(MAX_RULE_LIST_SIZE).collect();
                        }
                        Err(_) => {
                            warnings.push(format!(
                                "Invalid 'disable' value: expected array of strings, got {value}"
                            ));
                        }
                    },
                    "enable" => match serde_json::from_value::<Vec<String>>(value.clone()) {
                        Ok(e) => {
                            if e.len() > MAX_RULE_LIST_SIZE {
                                warnings.push(format!(
                                    "Too many rules in 'enable' ({} > {}), truncating",
                                    e.len(),
                                    MAX_RULE_LIST_SIZE
                                ));
                            }
                            for rule in e.iter().take(MAX_RULE_LIST_SIZE) {
                                if !is_valid_rule_name(rule) {
                                    warnings.push(format!("Unknown rule in enable: {rule}"));
                                }
                            }
                            enable = e.into_iter().take(MAX_RULE_LIST_SIZE).collect();
                        }
                        Err(_) => {
                            warnings.push(format!(
                                "Invalid 'enable' value: expected array of strings, got {value}"
                            ));
                        }
                    },
                    "lineLength" | "line_length" | "line-length" => {
                        if let Some(l) = value.as_u64() {
                            match usize::try_from(l) {
                                Ok(len) if len <= MAX_LINE_LENGTH => line_length = Some(len),
                                Ok(len) => warnings.push(format!(
                                    "Invalid 'lineLength' value: {len} exceeds maximum ({MAX_LINE_LENGTH})"
                                )),
                                Err(_) => warnings.push(format!("Invalid 'lineLength' value: {l} is too large")),
                            }
                        } else {
                            warnings.push(format!("Invalid 'lineLength' value: expected number, got {value}"));
                        }
                    }
                    // Rule-specific settings (e.g., "MD013": { "lineLength": 80 })
                    _ if key.starts_with("MD") || key.starts_with("md") => {
                        let normalized = key.to_uppercase();
                        if !is_valid_rule_name(&normalized) {
                            warnings.push(format!("Unknown rule: {key}"));
                        }
                        rules.insert(normalized, value);
                    }
                    _ => {
                        // Unknown key - warn and ignore
                        warnings.push(format!("Unknown configuration key: {key}"));
                    }
                }
            }

            let settings = LspRuleSettings {
                line_length,
                disable: if disable.is_empty() { None } else { Some(disable) },
                enable: if enable.is_empty() { None } else { Some(enable) },
                rules,
            };

            log::info!("Applied Neovim-style rule settings (manual parse)");
            config.settings = Some(settings);
            drop(config);
            config_applied = true;
        } else {
            log::warn!("Could not parse configuration settings: {rumdl_settings:?}");
        }

        // Log warnings for invalid configuration
        for warning in &warnings {
            log::warn!("{warning}");
        }

        // Notify client of configuration warnings via window/logMessage
        if !warnings.is_empty() {
            let message = if warnings.len() == 1 {
                format!("rumdl: {}", warnings[0])
            } else {
                format!("rumdl configuration warnings:\n{}", warnings.join("\n"))
            };
            self.client.log_message(MessageType::WARNING, message).await;
        }

        if !config_applied {
            log::debug!("No configuration changes applied");
        }

        // Clear config cache to pick up new settings
        self.config_cache.write().await.clear();

        // Collect all open documents first (to avoid holding lock during async operations)
        let doc_list: Vec<_> = {
            let documents = self.documents.read().await;
            documents
                .iter()
                .map(|(uri, entry)| (uri.clone(), entry.content.clone()))
                .collect()
        };

        // Refresh diagnostics for all open documents concurrently
        let tasks = doc_list.into_iter().map(|(uri, text)| {
            let server = self.clone();
            tokio::spawn(async move {
                server.update_diagnostics(uri, text).await;
            })
        });

        // Wait for all diagnostics to complete
        let _ = join_all(tasks).await;
    }

    async fn shutdown(&self) -> JsonRpcResult<()> {
        log::info!("Shutting down rumdl Language Server");

        // Signal the index worker to shut down
        let _ = self.update_tx.send(IndexUpdate::Shutdown).await;

        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        let version = params.text_document.version;

        let entry = DocumentEntry {
            content: text.clone(),
            version: Some(version),
            from_disk: false,
        };
        self.documents.write().await.insert(uri.clone(), entry);

        // Send update to index worker for cross-file analysis
        if let Ok(path) = uri.to_file_path() {
            let _ = self
                .update_tx
                .send(IndexUpdate::FileChanged {
                    path,
                    content: text.clone(),
                })
                .await;
        }

        self.update_diagnostics(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        if let Some(change) = params.content_changes.into_iter().next() {
            let text = change.text;

            let entry = DocumentEntry {
                content: text.clone(),
                version: Some(version),
                from_disk: false,
            };
            self.documents.write().await.insert(uri.clone(), entry);

            // Send update to index worker for cross-file analysis
            if let Ok(path) = uri.to_file_path() {
                let _ = self
                    .update_tx
                    .send(IndexUpdate::FileChanged {
                        path,
                        content: text.clone(),
                    })
                    .await;
            }

            self.update_diagnostics(uri, text).await;
        }
    }

    async fn will_save_wait_until(&self, params: WillSaveTextDocumentParams) -> JsonRpcResult<Option<Vec<TextEdit>>> {
        // Only apply fixes on manual saves (Cmd+S / Ctrl+S), not on autosave
        // This respects VSCode's editor.formatOnSave: "explicit" setting
        if params.reason != TextDocumentSaveReason::MANUAL {
            return Ok(None);
        }

        let config_guard = self.config.read().await;
        let enable_auto_fix = config_guard.enable_auto_fix;
        drop(config_guard);

        if !enable_auto_fix {
            return Ok(None);
        }

        // Get the current document content
        let Some(text) = self.get_document_content(&params.text_document.uri).await else {
            return Ok(None);
        };

        // Apply all fixes
        match self.apply_all_fixes(&params.text_document.uri, &text).await {
            Ok(Some(fixed_text)) => {
                // Return a single edit that replaces the entire document
                Ok(Some(vec![TextEdit {
                    range: Range {
                        start: Position { line: 0, character: 0 },
                        end: self.get_end_position(&text),
                    },
                    new_text: fixed_text,
                }]))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                log::error!("Failed to generate fixes in will_save_wait_until: {e}");
                Ok(None)
            }
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        // Re-lint the document after save
        // Note: Auto-fixing is now handled by will_save_wait_until which runs before the save
        if let Some(entry) = self.documents.read().await.get(&params.text_document.uri) {
            self.update_diagnostics(params.text_document.uri, entry.content.clone())
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // Remove document from storage
        self.documents.write().await.remove(&params.text_document.uri);

        // Always clear diagnostics on close to ensure cleanup
        // (Ruff does this unconditionally as a defensive measure)
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        // Check if any of the changed files are config files
        const CONFIG_FILES: &[&str] = &[".rumdl.toml", "rumdl.toml", "pyproject.toml", ".markdownlint.json"];

        let mut config_changed = false;

        for change in &params.changes {
            if let Ok(path) = change.uri.to_file_path() {
                let file_name = path.file_name().and_then(|f| f.to_str());
                let extension = path.extension().and_then(|e| e.to_str());

                // Handle config file changes
                if let Some(name) = file_name
                    && CONFIG_FILES.contains(&name)
                    && !config_changed
                {
                    log::info!("Config file changed: {}, invalidating config cache", path.display());

                    // Invalidate all cache entries that were loaded from this config file
                    let mut cache = self.config_cache.write().await;
                    cache.retain(|_, entry| {
                        if let Some(config_file) = &entry.config_file {
                            config_file != &path
                        } else {
                            true
                        }
                    });

                    // Also reload the global fallback configuration
                    drop(cache);
                    self.reload_configuration().await;
                    config_changed = true;
                }

                // Handle markdown file changes for workspace index
                if let Some(ext) = extension
                    && is_markdown_extension(ext)
                {
                    match change.typ {
                        FileChangeType::CREATED | FileChangeType::CHANGED => {
                            // Read file content and update index
                            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                                let _ = self
                                    .update_tx
                                    .send(IndexUpdate::FileChanged {
                                        path: path.clone(),
                                        content,
                                    })
                                    .await;
                            }
                        }
                        FileChangeType::DELETED => {
                            let _ = self
                                .update_tx
                                .send(IndexUpdate::FileDeleted { path: path.clone() })
                                .await;
                        }
                        _ => {}
                    }
                }
            }
        }

        // Re-lint all open documents if config changed
        if config_changed {
            let docs_to_update: Vec<(Url, String)> = {
                let docs = self.documents.read().await;
                docs.iter()
                    .filter(|(_, entry)| !entry.from_disk)
                    .map(|(uri, entry)| (uri.clone(), entry.content.clone()))
                    .collect()
            };

            for (uri, text) in docs_to_update {
                self.update_diagnostics(uri, text).await;
            }
        }
    }

    async fn code_action(&self, params: CodeActionParams) -> JsonRpcResult<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;
        let requested_kinds = params.context.only;

        if let Some(text) = self.get_document_content(&uri).await {
            match self.get_code_actions(&uri, &text, range).await {
                Ok(actions) => {
                    // Filter actions by requested kinds (if specified and non-empty)
                    // LSP spec: "If provided with no kinds, all supported kinds are returned"
                    // LSP code action kinds are hierarchical: source.fixAll.rumdl matches source.fixAll
                    let filtered_actions = if let Some(ref kinds) = requested_kinds
                        && !kinds.is_empty()
                    {
                        actions
                            .into_iter()
                            .filter(|action| {
                                action.kind.as_ref().is_some_and(|action_kind| {
                                    let action_kind_str = action_kind.as_str();
                                    kinds.iter().any(|requested| {
                                        let requested_str = requested.as_str();
                                        // Match if action kind starts with requested kind
                                        // e.g., "source.fixAll.rumdl" matches "source.fixAll"
                                        action_kind_str.starts_with(requested_str)
                                    })
                                })
                            })
                            .collect()
                    } else {
                        actions
                    };

                    let response: Vec<CodeActionOrCommand> = filtered_actions
                        .into_iter()
                        .map(CodeActionOrCommand::CodeAction)
                        .collect();
                    Ok(Some(response))
                }
                Err(e) => {
                    log::error!("Failed to get code actions: {e}");
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }

    async fn range_formatting(&self, params: DocumentRangeFormattingParams) -> JsonRpcResult<Option<Vec<TextEdit>>> {
        // For markdown linting, we format the entire document because:
        // 1. Many markdown rules have document-wide implications (e.g., heading hierarchy, list consistency)
        // 2. Fixes often need surrounding context to be applied correctly
        // 3. This approach is common among linters (ESLint, rustfmt, etc. do similar)
        log::debug!(
            "Range formatting requested for {:?}, formatting entire document due to rule interdependencies",
            params.range
        );

        let formatting_params = DocumentFormattingParams {
            text_document: params.text_document,
            options: params.options,
            work_done_progress_params: params.work_done_progress_params,
        };

        self.formatting(formatting_params).await
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> JsonRpcResult<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let options = params.options;

        log::debug!("Formatting request for: {uri}");
        log::debug!(
            "FormattingOptions: insert_final_newline={:?}, trim_final_newlines={:?}, trim_trailing_whitespace={:?}",
            options.insert_final_newline,
            options.trim_final_newlines,
            options.trim_trailing_whitespace
        );

        if let Some(text) = self.get_document_content(&uri).await {
            // Get config with LSP overrides
            let config_guard = self.config.read().await;
            let lsp_config = config_guard.clone();
            drop(config_guard);

            // Resolve configuration for this specific file
            let file_path = uri.to_file_path().ok();
            let file_config = if let Some(ref path) = file_path {
                self.resolve_config_for_file(path).await
            } else {
                // Fallback to global config for non-file URIs
                self.rumdl_config.read().await.clone()
            };

            // Merge LSP settings with file config based on configuration_preference
            let rumdl_config = self.merge_lsp_settings(file_config, &lsp_config);

            let all_rules = rules::all_rules(&rumdl_config);
            let flavor = if let Some(ref path) = file_path {
                rumdl_config.get_flavor_for_file(path)
            } else {
                rumdl_config.markdown_flavor()
            };

            // Use the standard filter_rules function which respects config's disabled rules
            let mut filtered_rules = rules::filter_rules(&all_rules, &rumdl_config.global);

            // Apply LSP config overrides
            filtered_rules = self.apply_lsp_config_overrides(filtered_rules, &lsp_config);

            // Phase 1: Apply lint rule fixes
            let mut result = text.clone();
            match crate::lint(&text, &filtered_rules, false, flavor, Some(&rumdl_config)) {
                Ok(warnings) => {
                    log::debug!(
                        "Found {} warnings, {} with fixes",
                        warnings.len(),
                        warnings.iter().filter(|w| w.fix.is_some()).count()
                    );

                    let has_fixes = warnings.iter().any(|w| w.fix.is_some());
                    if has_fixes {
                        // Only apply fixes from fixable rules during formatting
                        let fixable_warnings: Vec<_> = warnings
                            .iter()
                            .filter(|w| {
                                if let Some(rule_name) = &w.rule_name {
                                    filtered_rules
                                        .iter()
                                        .find(|r| r.name() == rule_name)
                                        .map(|r| r.fix_capability() != FixCapability::Unfixable)
                                        .unwrap_or(false)
                                } else {
                                    false
                                }
                            })
                            .cloned()
                            .collect();

                        match crate::utils::fix_utils::apply_warning_fixes(&text, &fixable_warnings) {
                            Ok(fixed_content) => {
                                result = fixed_content;
                            }
                            Err(e) => {
                                log::error!("Failed to apply fixes: {e}");
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to lint document: {e}");
                }
            }

            // Phase 2: Apply FormattingOptions (standard LSP behavior)
            // This ensures we respect editor preferences even if lint rules don't catch everything
            result = Self::apply_formatting_options(result, &options);

            // Return edit if content changed
            if result != text {
                log::debug!("Returning formatting edits");
                let end_position = self.get_end_position(&text);
                let edit = TextEdit {
                    range: Range {
                        start: Position { line: 0, character: 0 },
                        end: end_position,
                    },
                    new_text: result,
                };
                return Ok(Some(vec![edit]));
            }

            Ok(Some(Vec::new()))
        } else {
            log::warn!("Document not found: {uri}");
            Ok(None)
        }
    }

    async fn diagnostic(&self, params: DocumentDiagnosticParams) -> JsonRpcResult<DocumentDiagnosticReportResult> {
        let uri = params.text_document.uri;

        if let Some(text) = self.get_open_document_content(&uri).await {
            match self.lint_document(&uri, &text).await {
                Ok(diagnostics) => Ok(DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
                    RelatedFullDocumentDiagnosticReport {
                        related_documents: None,
                        full_document_diagnostic_report: FullDocumentDiagnosticReport {
                            result_id: None,
                            items: diagnostics,
                        },
                    },
                ))),
                Err(e) => {
                    log::error!("Failed to get diagnostics: {e}");
                    Ok(DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
                        RelatedFullDocumentDiagnosticReport {
                            related_documents: None,
                            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                                result_id: None,
                                items: Vec::new(),
                            },
                        },
                    )))
                }
            }
        } else {
            Ok(DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
                RelatedFullDocumentDiagnosticReport {
                    related_documents: None,
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: None,
                        items: Vec::new(),
                    },
                },
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::LintWarning;
    use tower_lsp::LspService;

    fn create_test_server() -> RumdlLanguageServer {
        let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));
        service.inner().clone()
    }

    #[test]
    fn test_is_valid_rule_name() {
        // Valid rule names - canonical MDxxx format
        assert!(is_valid_rule_name("MD001"));
        assert!(is_valid_rule_name("md001")); // lowercase
        assert!(is_valid_rule_name("Md001")); // mixed case
        assert!(is_valid_rule_name("mD001")); // mixed case
        assert!(is_valid_rule_name("MD003"));
        assert!(is_valid_rule_name("MD005"));
        assert!(is_valid_rule_name("MD007"));
        assert!(is_valid_rule_name("MD009"));
        assert!(is_valid_rule_name("MD041"));
        assert!(is_valid_rule_name("MD060"));
        assert!(is_valid_rule_name("MD061"));

        // Valid rule names - special "all" value
        assert!(is_valid_rule_name("all"));
        assert!(is_valid_rule_name("ALL"));
        assert!(is_valid_rule_name("All"));

        // Valid rule names - aliases (new in shared implementation)
        assert!(is_valid_rule_name("line-length")); // alias for MD013
        assert!(is_valid_rule_name("LINE-LENGTH")); // case insensitive
        assert!(is_valid_rule_name("heading-increment")); // alias for MD001
        assert!(is_valid_rule_name("no-bare-urls")); // alias for MD034
        assert!(is_valid_rule_name("ul-style")); // alias for MD004
        assert!(is_valid_rule_name("ul_style")); // underscore variant

        // Invalid rule names - not in alias map
        assert!(!is_valid_rule_name("MD000")); // doesn't exist
        assert!(!is_valid_rule_name("MD999")); // doesn't exist
        assert!(!is_valid_rule_name("MD100")); // doesn't exist
        assert!(!is_valid_rule_name("INVALID"));
        assert!(!is_valid_rule_name("not-a-rule"));
        assert!(!is_valid_rule_name(""));
        assert!(!is_valid_rule_name("random-text"));
    }

    #[tokio::test]
    async fn test_server_creation() {
        let server = create_test_server();

        // Verify default configuration
        let config = server.config.read().await;
        assert!(config.enable_linting);
        assert!(!config.enable_auto_fix);
    }

    #[tokio::test]
    async fn test_lint_document() {
        let server = create_test_server();

        // Test linting with a simple markdown document
        let uri = Url::parse("file:///test.md").unwrap();
        let text = "# Test\n\nThis is a test  \nWith trailing spaces  ";

        let diagnostics = server.lint_document(&uri, text).await.unwrap();

        // Should find trailing spaces violations
        assert!(!diagnostics.is_empty());
        assert!(diagnostics.iter().any(|d| d.message.contains("trailing")));
    }

    #[tokio::test]
    async fn test_lint_document_disabled() {
        let server = create_test_server();

        // Disable linting
        server.config.write().await.enable_linting = false;

        let uri = Url::parse("file:///test.md").unwrap();
        let text = "# Test\n\nThis is a test  \nWith trailing spaces  ";

        let diagnostics = server.lint_document(&uri, text).await.unwrap();

        // Should return empty diagnostics when disabled
        assert!(diagnostics.is_empty());
    }

    #[tokio::test]
    async fn test_get_code_actions() {
        let server = create_test_server();

        let uri = Url::parse("file:///test.md").unwrap();
        let text = "# Test\n\nThis is a test  \nWith trailing spaces  ";

        // Create a range covering the whole document
        let range = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 3, character: 21 },
        };

        let actions = server.get_code_actions(&uri, text, range).await.unwrap();

        // Should have code actions for fixing trailing spaces
        assert!(!actions.is_empty());
        assert!(actions.iter().any(|a| a.title.contains("trailing")));
    }

    #[tokio::test]
    async fn test_get_code_actions_outside_range() {
        let server = create_test_server();

        let uri = Url::parse("file:///test.md").unwrap();
        let text = "# Test\n\nThis is a test  \nWith trailing spaces  ";

        // Create a range that doesn't cover the violations
        let range = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 6 },
        };

        let actions = server.get_code_actions(&uri, text, range).await.unwrap();

        // Should have no code actions for this range
        assert!(actions.is_empty());
    }

    #[tokio::test]
    async fn test_document_storage() {
        let server = create_test_server();

        let uri = Url::parse("file:///test.md").unwrap();
        let text = "# Test Document";

        // Store document
        let entry = DocumentEntry {
            content: text.to_string(),
            version: Some(1),
            from_disk: false,
        };
        server.documents.write().await.insert(uri.clone(), entry);

        // Verify storage
        let stored = server.documents.read().await.get(&uri).map(|e| e.content.clone());
        assert_eq!(stored, Some(text.to_string()));

        // Remove document
        server.documents.write().await.remove(&uri);

        // Verify removal
        let stored = server.documents.read().await.get(&uri).cloned();
        assert_eq!(stored, None);
    }

    #[tokio::test]
    async fn test_configuration_loading() {
        let server = create_test_server();

        // Load configuration with auto-discovery
        server.load_configuration(false).await;

        // Verify configuration was loaded successfully
        // The config could be from: .rumdl.toml, pyproject.toml, .markdownlint.json, or default
        let rumdl_config = server.rumdl_config.read().await;
        // The loaded config is valid regardless of source
        drop(rumdl_config); // Just verify we can access it without panic
    }

    #[tokio::test]
    async fn test_load_config_for_lsp() {
        // Test with no config file
        let result = RumdlLanguageServer::load_config_for_lsp(None);
        assert!(result.is_ok());

        // Test with non-existent config file
        let result = RumdlLanguageServer::load_config_for_lsp(Some("/nonexistent/config.toml"));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_warning_conversion() {
        let warning = LintWarning {
            message: "Test warning".to_string(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 10,
            severity: crate::rule::Severity::Warning,
            fix: None,
            rule_name: Some("MD001".to_string()),
        };

        // Test diagnostic conversion
        let diagnostic = warning_to_diagnostic(&warning);
        assert_eq!(diagnostic.message, "Test warning");
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diagnostic.code, Some(NumberOrString::String("MD001".to_string())));

        // Test code action conversion (no fix, but should have ignore action)
        let uri = Url::parse("file:///test.md").unwrap();
        let actions = warning_to_code_actions(&warning, &uri, "Test content");
        // Should have 1 action: ignore-line (no fix available)
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Ignore MD001 for this line");
    }

    #[tokio::test]
    async fn test_multiple_documents() {
        let server = create_test_server();

        let uri1 = Url::parse("file:///test1.md").unwrap();
        let uri2 = Url::parse("file:///test2.md").unwrap();
        let text1 = "# Document 1";
        let text2 = "# Document 2";

        // Store multiple documents
        {
            let mut docs = server.documents.write().await;
            let entry1 = DocumentEntry {
                content: text1.to_string(),
                version: Some(1),
                from_disk: false,
            };
            let entry2 = DocumentEntry {
                content: text2.to_string(),
                version: Some(1),
                from_disk: false,
            };
            docs.insert(uri1.clone(), entry1);
            docs.insert(uri2.clone(), entry2);
        }

        // Verify both are stored
        let docs = server.documents.read().await;
        assert_eq!(docs.len(), 2);
        assert_eq!(docs.get(&uri1).map(|s| s.content.as_str()), Some(text1));
        assert_eq!(docs.get(&uri2).map(|s| s.content.as_str()), Some(text2));
    }

    #[tokio::test]
    async fn test_auto_fix_on_save() {
        let server = create_test_server();

        // Enable auto-fix
        {
            let mut config = server.config.write().await;
            config.enable_auto_fix = true;
        }

        let uri = Url::parse("file:///test.md").unwrap();
        let text = "#Heading without space"; // MD018 violation

        // Store document
        let entry = DocumentEntry {
            content: text.to_string(),
            version: Some(1),
            from_disk: false,
        };
        server.documents.write().await.insert(uri.clone(), entry);

        // Test apply_all_fixes
        let fixed = server.apply_all_fixes(&uri, text).await.unwrap();
        assert!(fixed.is_some());
        // MD018 adds space, MD047 adds trailing newline
        assert_eq!(fixed.unwrap(), "# Heading without space\n");
    }

    #[tokio::test]
    async fn test_get_end_position() {
        let server = create_test_server();

        // Single line
        let pos = server.get_end_position("Hello");
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 5);

        // Multiple lines
        let pos = server.get_end_position("Hello\nWorld\nTest");
        assert_eq!(pos.line, 2);
        assert_eq!(pos.character, 4);

        // Empty string
        let pos = server.get_end_position("");
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);

        // Ends with newline - position should be at start of next line
        let pos = server.get_end_position("Hello\n");
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
    }

    #[tokio::test]
    async fn test_empty_document_handling() {
        let server = create_test_server();

        let uri = Url::parse("file:///empty.md").unwrap();
        let text = "";

        // Test linting empty document
        let diagnostics = server.lint_document(&uri, text).await.unwrap();
        assert!(diagnostics.is_empty());

        // Test code actions on empty document
        let range = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 0 },
        };
        let actions = server.get_code_actions(&uri, text, range).await.unwrap();
        assert!(actions.is_empty());
    }

    #[tokio::test]
    async fn test_config_update() {
        let server = create_test_server();

        // Update config
        {
            let mut config = server.config.write().await;
            config.enable_auto_fix = true;
            config.config_path = Some("/custom/path.toml".to_string());
        }

        // Verify update
        let config = server.config.read().await;
        assert!(config.enable_auto_fix);
        assert_eq!(config.config_path, Some("/custom/path.toml".to_string()));
    }

    #[tokio::test]
    async fn test_document_formatting() {
        let server = create_test_server();
        let uri = Url::parse("file:///test.md").unwrap();
        let text = "# Test\n\nThis is a test  \nWith trailing spaces  ";

        // Store document
        let entry = DocumentEntry {
            content: text.to_string(),
            version: Some(1),
            from_disk: false,
        };
        server.documents.write().await.insert(uri.clone(), entry);

        // Create formatting params
        let params = DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            options: FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
                properties: HashMap::new(),
                trim_trailing_whitespace: Some(true),
                insert_final_newline: Some(true),
                trim_final_newlines: Some(true),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        // Call formatting
        let result = server.formatting(params).await.unwrap();

        // Should return text edits that fix the trailing spaces
        assert!(result.is_some());
        let edits = result.unwrap();
        assert!(!edits.is_empty());

        // The new text should have trailing spaces removed from ALL lines
        // because trim_trailing_whitespace: Some(true) is set
        let edit = &edits[0];
        // The formatted text should have:
        // - Trailing spaces removed from ALL lines (trim_trailing_whitespace)
        // - Exactly one final newline (trim_final_newlines + insert_final_newline)
        let expected = "# Test\n\nThis is a test\nWith trailing spaces\n";
        assert_eq!(edit.new_text, expected);
    }

    /// Test that Unfixable rules are excluded from formatting/Fix All but available for Quick Fix
    /// Regression test for issue #158: formatting deleted HTML img tags
    #[tokio::test]
    async fn test_unfixable_rules_excluded_from_formatting() {
        let server = create_test_server();
        let uri = Url::parse("file:///test.md").unwrap();

        // Content with both fixable (trailing spaces) and unfixable (HTML) issues
        let text = "# Test Document\n\n<img src=\"test.png\" alt=\"Test\" />\n\nTrailing spaces  ";

        // Store document
        let entry = DocumentEntry {
            content: text.to_string(),
            version: Some(1),
            from_disk: false,
        };
        server.documents.write().await.insert(uri.clone(), entry);

        // Test 1: Formatting should preserve HTML (Unfixable) but fix trailing spaces (fixable)
        let format_params = DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            options: FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
                properties: HashMap::new(),
                trim_trailing_whitespace: Some(true),
                insert_final_newline: Some(true),
                trim_final_newlines: Some(true),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        let format_result = server.formatting(format_params).await.unwrap();
        assert!(format_result.is_some(), "Should return formatting edits");

        let edits = format_result.unwrap();
        assert!(!edits.is_empty(), "Should have formatting edits");

        let formatted = &edits[0].new_text;
        assert!(
            formatted.contains("<img src=\"test.png\" alt=\"Test\" />"),
            "HTML should be preserved during formatting (Unfixable rule)"
        );
        assert!(
            !formatted.contains("spaces  "),
            "Trailing spaces should be removed (fixable rule)"
        );

        // Test 2: Quick Fix actions should still be available for Unfixable rules
        let range = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 10, character: 0 },
        };

        let code_actions = server.get_code_actions(&uri, text, range).await.unwrap();

        // Should have individual Quick Fix actions for each warning
        let html_fix_actions: Vec<_> = code_actions
            .iter()
            .filter(|action| action.title.contains("MD033") || action.title.contains("HTML"))
            .collect();

        assert!(
            !html_fix_actions.is_empty(),
            "Quick Fix actions should be available for HTML (Unfixable rules)"
        );

        // Test 3: "Fix All" action should exclude Unfixable rules
        let fix_all_actions: Vec<_> = code_actions
            .iter()
            .filter(|action| action.title.contains("Fix all"))
            .collect();

        if let Some(fix_all_action) = fix_all_actions.first()
            && let Some(ref edit) = fix_all_action.edit
            && let Some(ref changes) = edit.changes
            && let Some(text_edits) = changes.get(&uri)
            && let Some(text_edit) = text_edits.first()
        {
            let fixed_all = &text_edit.new_text;
            assert!(
                fixed_all.contains("<img src=\"test.png\" alt=\"Test\" />"),
                "Fix All should preserve HTML (Unfixable rules)"
            );
            assert!(
                !fixed_all.contains("spaces  "),
                "Fix All should remove trailing spaces (fixable rules)"
            );
        }
    }

    /// Test that resolve_config_for_file() finds the correct config in multi-root workspace
    #[tokio::test]
    async fn test_resolve_config_for_file_multi_root() {
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // Setup project A with line_length=60
        let project_a = temp_path.join("project_a");
        let project_a_docs = project_a.join("docs");
        fs::create_dir_all(&project_a_docs).unwrap();

        let config_a = project_a.join(".rumdl.toml");
        fs::write(
            &config_a,
            r#"
[global]

[MD013]
line_length = 60
"#,
        )
        .unwrap();

        // Setup project B with line_length=120
        let project_b = temp_path.join("project_b");
        fs::create_dir(&project_b).unwrap();

        let config_b = project_b.join(".rumdl.toml");
        fs::write(
            &config_b,
            r#"
[global]

[MD013]
line_length = 120
"#,
        )
        .unwrap();

        // Create LSP server and initialize with workspace roots
        let server = create_test_server();

        // Set workspace roots
        {
            let mut roots = server.workspace_roots.write().await;
            roots.push(project_a.clone());
            roots.push(project_b.clone());
        }

        // Test file in project A
        let file_a = project_a_docs.join("test.md");
        fs::write(&file_a, "# Test A\n").unwrap();

        let config_for_a = server.resolve_config_for_file(&file_a).await;
        let line_length_a = crate::config::get_rule_config_value::<usize>(&config_for_a, "MD013", "line_length");
        assert_eq!(line_length_a, Some(60), "File in project_a should get line_length=60");

        // Test file in project B
        let file_b = project_b.join("test.md");
        fs::write(&file_b, "# Test B\n").unwrap();

        let config_for_b = server.resolve_config_for_file(&file_b).await;
        let line_length_b = crate::config::get_rule_config_value::<usize>(&config_for_b, "MD013", "line_length");
        assert_eq!(line_length_b, Some(120), "File in project_b should get line_length=120");
    }

    /// Test that config resolution respects workspace root boundaries
    #[tokio::test]
    async fn test_config_resolution_respects_workspace_boundaries() {
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // Create parent config that should NOT be used
        let parent_config = temp_path.join(".rumdl.toml");
        fs::write(
            &parent_config,
            r#"
[global]

[MD013]
line_length = 80
"#,
        )
        .unwrap();

        // Create workspace root with its own config
        let workspace_root = temp_path.join("workspace");
        let workspace_subdir = workspace_root.join("subdir");
        fs::create_dir_all(&workspace_subdir).unwrap();

        let workspace_config = workspace_root.join(".rumdl.toml");
        fs::write(
            &workspace_config,
            r#"
[global]

[MD013]
line_length = 100
"#,
        )
        .unwrap();

        let server = create_test_server();

        // Register workspace_root as a workspace root
        {
            let mut roots = server.workspace_roots.write().await;
            roots.push(workspace_root.clone());
        }

        // Test file deep in subdirectory
        let test_file = workspace_subdir.join("deep").join("test.md");
        fs::create_dir_all(test_file.parent().unwrap()).unwrap();
        fs::write(&test_file, "# Test\n").unwrap();

        let config = server.resolve_config_for_file(&test_file).await;
        let line_length = crate::config::get_rule_config_value::<usize>(&config, "MD013", "line_length");

        // Should find workspace_root/.rumdl.toml (100), NOT parent config (80)
        assert_eq!(
            line_length,
            Some(100),
            "Should find workspace config, not parent config outside workspace"
        );
    }

    /// Test that config cache works (cache hit scenario)
    #[tokio::test]
    async fn test_config_cache_hit() {
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        let project = temp_path.join("project");
        fs::create_dir(&project).unwrap();

        let config_file = project.join(".rumdl.toml");
        fs::write(
            &config_file,
            r#"
[global]

[MD013]
line_length = 75
"#,
        )
        .unwrap();

        let server = create_test_server();
        {
            let mut roots = server.workspace_roots.write().await;
            roots.push(project.clone());
        }

        let test_file = project.join("test.md");
        fs::write(&test_file, "# Test\n").unwrap();

        // First call - cache miss
        let config1 = server.resolve_config_for_file(&test_file).await;
        let line_length1 = crate::config::get_rule_config_value::<usize>(&config1, "MD013", "line_length");
        assert_eq!(line_length1, Some(75));

        // Verify cache was populated
        {
            let cache = server.config_cache.read().await;
            let search_dir = test_file.parent().unwrap();
            assert!(
                cache.contains_key(search_dir),
                "Cache should be populated after first call"
            );
        }

        // Second call - cache hit (should return same config without filesystem access)
        let config2 = server.resolve_config_for_file(&test_file).await;
        let line_length2 = crate::config::get_rule_config_value::<usize>(&config2, "MD013", "line_length");
        assert_eq!(line_length2, Some(75));
    }

    /// Test nested directory config search (file searches upward)
    #[tokio::test]
    async fn test_nested_directory_config_search() {
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        let project = temp_path.join("project");
        fs::create_dir(&project).unwrap();

        // Config at project root
        let config = project.join(".rumdl.toml");
        fs::write(
            &config,
            r#"
[global]

[MD013]
line_length = 110
"#,
        )
        .unwrap();

        // File deep in nested structure
        let deep_dir = project.join("src").join("docs").join("guides");
        fs::create_dir_all(&deep_dir).unwrap();
        let deep_file = deep_dir.join("test.md");
        fs::write(&deep_file, "# Test\n").unwrap();

        let server = create_test_server();
        {
            let mut roots = server.workspace_roots.write().await;
            roots.push(project.clone());
        }

        let resolved_config = server.resolve_config_for_file(&deep_file).await;
        let line_length = crate::config::get_rule_config_value::<usize>(&resolved_config, "MD013", "line_length");

        assert_eq!(
            line_length,
            Some(110),
            "Should find config by searching upward from deep directory"
        );
    }

    /// Test fallback to default config when no config file found
    #[tokio::test]
    async fn test_fallback_to_default_config() {
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        let project = temp_path.join("project");
        fs::create_dir(&project).unwrap();

        // No config file created!

        let test_file = project.join("test.md");
        fs::write(&test_file, "# Test\n").unwrap();

        let server = create_test_server();
        {
            let mut roots = server.workspace_roots.write().await;
            roots.push(project.clone());
        }

        let config = server.resolve_config_for_file(&test_file).await;

        // Default global line_length is 80
        assert_eq!(
            config.global.line_length.get(),
            80,
            "Should fall back to default config when no config file found"
        );
    }

    /// Test config priority: closer config wins over parent config
    #[tokio::test]
    async fn test_config_priority_closer_wins() {
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        let project = temp_path.join("project");
        fs::create_dir(&project).unwrap();

        // Parent config
        let parent_config = project.join(".rumdl.toml");
        fs::write(
            &parent_config,
            r#"
[global]

[MD013]
line_length = 100
"#,
        )
        .unwrap();

        // Subdirectory with its own config (should override parent)
        let subdir = project.join("subdir");
        fs::create_dir(&subdir).unwrap();

        let subdir_config = subdir.join(".rumdl.toml");
        fs::write(
            &subdir_config,
            r#"
[global]

[MD013]
line_length = 50
"#,
        )
        .unwrap();

        let server = create_test_server();
        {
            let mut roots = server.workspace_roots.write().await;
            roots.push(project.clone());
        }

        // File in subdirectory
        let test_file = subdir.join("test.md");
        fs::write(&test_file, "# Test\n").unwrap();

        let config = server.resolve_config_for_file(&test_file).await;
        let line_length = crate::config::get_rule_config_value::<usize>(&config, "MD013", "line_length");

        assert_eq!(
            line_length,
            Some(50),
            "Closer config (subdir) should override parent config"
        );
    }

    /// Test for issue #131: LSP should skip pyproject.toml without [tool.rumdl] section
    ///
    /// This test verifies the fix in resolve_config_for_file() at lines 574-585 that checks
    /// for [tool.rumdl] presence before loading pyproject.toml. The fix ensures LSP behavior
    /// matches CLI behavior.
    #[tokio::test]
    async fn test_issue_131_pyproject_without_rumdl_section() {
        use std::fs;
        use tempfile::tempdir;

        // Create a parent temp dir that we control
        let parent_dir = tempdir().unwrap();

        // Create a child subdirectory for the project
        let project_dir = parent_dir.path().join("project");
        fs::create_dir(&project_dir).unwrap();

        // Create pyproject.toml WITHOUT [tool.rumdl] section in project dir
        fs::write(
            project_dir.join("pyproject.toml"),
            r#"
[project]
name = "test-project"
version = "0.1.0"
"#,
        )
        .unwrap();

        // Create .rumdl.toml in PARENT that SHOULD be found
        // because pyproject.toml without [tool.rumdl] should be skipped
        fs::write(
            parent_dir.path().join(".rumdl.toml"),
            r#"
[global]
disable = ["MD013"]
"#,
        )
        .unwrap();

        let test_file = project_dir.join("test.md");
        fs::write(&test_file, "# Test\n").unwrap();

        let server = create_test_server();

        // Set workspace root to parent so upward search doesn't stop at project_dir
        {
            let mut roots = server.workspace_roots.write().await;
            roots.push(parent_dir.path().to_path_buf());
        }

        // Resolve config for file in project_dir
        let config = server.resolve_config_for_file(&test_file).await;

        // CRITICAL TEST: The pyproject.toml in project_dir should be SKIPPED because it lacks
        // [tool.rumdl], and the search should continue upward to find parent .rumdl.toml
        assert!(
            config.global.disable.contains(&"MD013".to_string()),
            "Issue #131 regression: LSP must skip pyproject.toml without [tool.rumdl] \
             and continue upward search. Expected MD013 from parent .rumdl.toml to be disabled."
        );

        // Verify the config came from the parent directory, not project_dir
        // (we can check this by looking at the cache)
        let cache = server.config_cache.read().await;
        let cache_entry = cache.get(&project_dir).expect("Config should be cached");

        assert!(
            cache_entry.config_file.is_some(),
            "Should have found a config file (parent .rumdl.toml)"
        );

        let found_config_path = cache_entry.config_file.as_ref().unwrap();
        assert!(
            found_config_path.ends_with(".rumdl.toml"),
            "Should have loaded .rumdl.toml, not pyproject.toml. Found: {found_config_path:?}"
        );
        assert!(
            found_config_path.parent().unwrap() == parent_dir.path(),
            "Should have loaded config from parent directory, not project_dir"
        );
    }

    /// Test for issue #131: LSP should detect and load pyproject.toml WITH [tool.rumdl] section
    ///
    /// This test verifies that when pyproject.toml contains [tool.rumdl], the fix at lines 574-585
    /// correctly allows it through and loads the configuration.
    #[tokio::test]
    async fn test_issue_131_pyproject_with_rumdl_section() {
        use std::fs;
        use tempfile::tempdir;

        // Create a parent temp dir that we control
        let parent_dir = tempdir().unwrap();

        // Create a child subdirectory for the project
        let project_dir = parent_dir.path().join("project");
        fs::create_dir(&project_dir).unwrap();

        // Create pyproject.toml WITH [tool.rumdl] section in project dir
        fs::write(
            project_dir.join("pyproject.toml"),
            r#"
[project]
name = "test-project"

[tool.rumdl.global]
disable = ["MD033"]
"#,
        )
        .unwrap();

        // Create a parent directory with different config that should NOT be used
        fs::write(
            parent_dir.path().join(".rumdl.toml"),
            r#"
[global]
disable = ["MD041"]
"#,
        )
        .unwrap();

        let test_file = project_dir.join("test.md");
        fs::write(&test_file, "# Test\n").unwrap();

        let server = create_test_server();

        // Set workspace root to parent
        {
            let mut roots = server.workspace_roots.write().await;
            roots.push(parent_dir.path().to_path_buf());
        }

        // Resolve config for file
        let config = server.resolve_config_for_file(&test_file).await;

        // CRITICAL TEST: The pyproject.toml should be LOADED (not skipped) because it has [tool.rumdl]
        assert!(
            config.global.disable.contains(&"MD033".to_string()),
            "Issue #131 regression: LSP must load pyproject.toml when it has [tool.rumdl]. \
             Expected MD033 from project_dir pyproject.toml to be disabled."
        );

        // Verify we did NOT get the parent config
        assert!(
            !config.global.disable.contains(&"MD041".to_string()),
            "Should use project_dir pyproject.toml, not parent .rumdl.toml"
        );

        // Verify the config came from pyproject.toml specifically
        let cache = server.config_cache.read().await;
        let cache_entry = cache.get(&project_dir).expect("Config should be cached");

        assert!(cache_entry.config_file.is_some(), "Should have found a config file");

        let found_config_path = cache_entry.config_file.as_ref().unwrap();
        assert!(
            found_config_path.ends_with("pyproject.toml"),
            "Should have loaded pyproject.toml. Found: {found_config_path:?}"
        );
        assert!(
            found_config_path.parent().unwrap() == project_dir,
            "Should have loaded pyproject.toml from project_dir, not parent"
        );
    }

    /// Test for issue #131: Verify pyproject.toml with only "tool.rumdl" (no brackets) is detected
    ///
    /// The fix checks for both "[tool.rumdl]" and "tool.rumdl" (line 576), ensuring it catches
    /// any valid TOML structure like [tool.rumdl.global] or [[tool.rumdl.something]].
    #[tokio::test]
    async fn test_issue_131_pyproject_with_tool_rumdl_subsection() {
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();

        // Create pyproject.toml with [tool.rumdl.global] but not [tool.rumdl] directly
        fs::write(
            temp_dir.path().join("pyproject.toml"),
            r#"
[project]
name = "test-project"

[tool.rumdl.global]
disable = ["MD022"]
"#,
        )
        .unwrap();

        let test_file = temp_dir.path().join("test.md");
        fs::write(&test_file, "# Test\n").unwrap();

        let server = create_test_server();

        // Set workspace root
        {
            let mut roots = server.workspace_roots.write().await;
            roots.push(temp_dir.path().to_path_buf());
        }

        // Resolve config for file
        let config = server.resolve_config_for_file(&test_file).await;

        // Should detect "tool.rumdl" substring and load the config
        assert!(
            config.global.disable.contains(&"MD022".to_string()),
            "Should detect tool.rumdl substring in [tool.rumdl.global] and load config"
        );

        // Verify it loaded pyproject.toml
        let cache = server.config_cache.read().await;
        let cache_entry = cache.get(temp_dir.path()).expect("Config should be cached");
        assert!(
            cache_entry.config_file.as_ref().unwrap().ends_with("pyproject.toml"),
            "Should have loaded pyproject.toml"
        );
    }

    /// Test for issue #182: Client pull diagnostics capability detection
    ///
    /// When a client supports pull diagnostics (textDocument/diagnostic), the server
    /// should skip pushing diagnostics via publishDiagnostics to avoid duplicates.
    #[tokio::test]
    async fn test_issue_182_pull_diagnostics_capability_default() {
        let server = create_test_server();

        // By default, client_supports_pull_diagnostics should be false
        assert!(
            !*server.client_supports_pull_diagnostics.read().await,
            "Default should be false - push diagnostics by default"
        );
    }

    /// Test that we can set the pull diagnostics flag
    #[tokio::test]
    async fn test_issue_182_pull_diagnostics_flag_update() {
        let server = create_test_server();

        // Simulate detecting pull capability
        *server.client_supports_pull_diagnostics.write().await = true;

        assert!(
            *server.client_supports_pull_diagnostics.read().await,
            "Flag should be settable to true"
        );
    }

    /// Test issue #182: Verify capability detection logic matches Ruff's pattern
    ///
    /// The detection should check: params.capabilities.text_document.diagnostic.is_some()
    #[tokio::test]
    async fn test_issue_182_capability_detection_with_diagnostic_support() {
        use tower_lsp::lsp_types::{ClientCapabilities, DiagnosticClientCapabilities, TextDocumentClientCapabilities};

        // Create client capabilities WITH diagnostic support
        let caps_with_diagnostic = ClientCapabilities {
            text_document: Some(TextDocumentClientCapabilities {
                diagnostic: Some(DiagnosticClientCapabilities {
                    dynamic_registration: Some(true),
                    related_document_support: Some(false),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Verify the detection logic (same as in initialize)
        let supports_pull = caps_with_diagnostic
            .text_document
            .as_ref()
            .and_then(|td| td.diagnostic.as_ref())
            .is_some();

        assert!(supports_pull, "Should detect pull diagnostic support");
    }

    /// Test issue #182: Verify capability detection when diagnostic is NOT supported
    #[tokio::test]
    async fn test_issue_182_capability_detection_without_diagnostic_support() {
        use tower_lsp::lsp_types::{ClientCapabilities, TextDocumentClientCapabilities};

        // Create client capabilities WITHOUT diagnostic support
        let caps_without_diagnostic = ClientCapabilities {
            text_document: Some(TextDocumentClientCapabilities {
                diagnostic: None, // No diagnostic support
                ..Default::default()
            }),
            ..Default::default()
        };

        // Verify the detection logic
        let supports_pull = caps_without_diagnostic
            .text_document
            .as_ref()
            .and_then(|td| td.diagnostic.as_ref())
            .is_some();

        assert!(!supports_pull, "Should NOT detect pull diagnostic support");
    }

    /// Test issue #182: Verify capability detection with empty text_document
    #[tokio::test]
    async fn test_issue_182_capability_detection_no_text_document() {
        use tower_lsp::lsp_types::ClientCapabilities;

        // Create client capabilities with no text_document at all
        let caps_no_text_doc = ClientCapabilities {
            text_document: None,
            ..Default::default()
        };

        // Verify the detection logic
        let supports_pull = caps_no_text_doc
            .text_document
            .as_ref()
            .and_then(|td| td.diagnostic.as_ref())
            .is_some();

        assert!(
            !supports_pull,
            "Should NOT detect pull diagnostic support when text_document is None"
        );
    }

    #[test]
    fn test_resource_limit_constants() {
        // Verify resource limit constants have expected values
        assert_eq!(MAX_RULE_LIST_SIZE, 100);
        assert_eq!(MAX_LINE_LENGTH, 10_000);
    }

    #[test]
    fn test_is_valid_rule_name_edge_cases() {
        // Test malformed MDxxx patterns - not in alias map
        assert!(!is_valid_rule_name("MD/01")); // invalid character
        assert!(!is_valid_rule_name("MD:01")); // invalid character
        assert!(!is_valid_rule_name("ND001")); // 'N' instead of 'M'
        assert!(!is_valid_rule_name("ME001")); // 'E' instead of 'D'

        // Test non-ASCII characters - not in alias map
        assert!(!is_valid_rule_name("MD0①1")); // Unicode digit
        assert!(!is_valid_rule_name("ＭD001")); // Fullwidth M

        // Test special characters - not in alias map
        assert!(!is_valid_rule_name("MD\x00\x00\x00")); // null bytes
    }

    /// Generic parity test: LSP config must produce identical results to TOML config.
    ///
    /// This test ensures that ANY config field works identically whether applied via:
    /// 1. LSP settings (JSON → apply_rule_config)
    /// 2. TOML file parsing (direct RuleConfig construction)
    ///
    /// When adding new config fields to RuleConfig, add them to TEST_CONFIGS below.
    /// The test will fail if LSP handling diverges from TOML handling.
    #[tokio::test]
    async fn test_lsp_toml_config_parity_generic() {
        use crate::config::RuleConfig;
        use crate::rule::Severity;

        let server = create_test_server();

        // Define test configurations covering all field types and combinations.
        // Each entry: (description, LSP JSON, expected TOML RuleConfig)
        // When adding new RuleConfig fields, add test cases here.
        let test_configs: Vec<(&str, serde_json::Value, RuleConfig)> = vec![
            // Severity alone (the bug from issue #229)
            (
                "severity only - error",
                serde_json::json!({"severity": "error"}),
                RuleConfig {
                    severity: Some(Severity::Error),
                    values: std::collections::BTreeMap::new(),
                },
            ),
            (
                "severity only - warning",
                serde_json::json!({"severity": "warning"}),
                RuleConfig {
                    severity: Some(Severity::Warning),
                    values: std::collections::BTreeMap::new(),
                },
            ),
            (
                "severity only - info",
                serde_json::json!({"severity": "info"}),
                RuleConfig {
                    severity: Some(Severity::Info),
                    values: std::collections::BTreeMap::new(),
                },
            ),
            // Value types: integer
            (
                "integer value",
                serde_json::json!({"lineLength": 120}),
                RuleConfig {
                    severity: None,
                    values: [("line_length".to_string(), toml::Value::Integer(120))]
                        .into_iter()
                        .collect(),
                },
            ),
            // Value types: boolean
            (
                "boolean value",
                serde_json::json!({"enabled": true}),
                RuleConfig {
                    severity: None,
                    values: [("enabled".to_string(), toml::Value::Boolean(true))]
                        .into_iter()
                        .collect(),
                },
            ),
            // Value types: string
            (
                "string value",
                serde_json::json!({"style": "consistent"}),
                RuleConfig {
                    severity: None,
                    values: [("style".to_string(), toml::Value::String("consistent".to_string()))]
                        .into_iter()
                        .collect(),
                },
            ),
            // Value types: array
            (
                "array value",
                serde_json::json!({"allowedElements": ["div", "span"]}),
                RuleConfig {
                    severity: None,
                    values: [(
                        "allowed_elements".to_string(),
                        toml::Value::Array(vec![
                            toml::Value::String("div".to_string()),
                            toml::Value::String("span".to_string()),
                        ]),
                    )]
                    .into_iter()
                    .collect(),
                },
            ),
            // Mixed: severity + values (critical combination)
            (
                "severity + integer",
                serde_json::json!({"severity": "info", "lineLength": 80}),
                RuleConfig {
                    severity: Some(Severity::Info),
                    values: [("line_length".to_string(), toml::Value::Integer(80))]
                        .into_iter()
                        .collect(),
                },
            ),
            (
                "severity + multiple values",
                serde_json::json!({
                    "severity": "warning",
                    "lineLength": 100,
                    "strict": false,
                    "style": "atx"
                }),
                RuleConfig {
                    severity: Some(Severity::Warning),
                    values: [
                        ("line_length".to_string(), toml::Value::Integer(100)),
                        ("strict".to_string(), toml::Value::Boolean(false)),
                        ("style".to_string(), toml::Value::String("atx".to_string())),
                    ]
                    .into_iter()
                    .collect(),
                },
            ),
            // camelCase to snake_case conversion
            (
                "camelCase conversion",
                serde_json::json!({"codeBlocks": true, "headingStyle": "setext"}),
                RuleConfig {
                    severity: None,
                    values: [
                        ("code_blocks".to_string(), toml::Value::Boolean(true)),
                        ("heading_style".to_string(), toml::Value::String("setext".to_string())),
                    ]
                    .into_iter()
                    .collect(),
                },
            ),
        ];

        for (description, lsp_json, expected_toml_config) in test_configs {
            let mut lsp_config = crate::config::Config::default();
            server.apply_rule_config(&mut lsp_config, "TEST", &lsp_json);

            let lsp_rule = lsp_config.rules.get("TEST").expect("Rule should exist");

            // Compare severity
            assert_eq!(
                lsp_rule.severity, expected_toml_config.severity,
                "Parity failure [{description}]: severity mismatch. \
                 LSP={:?}, TOML={:?}",
                lsp_rule.severity, expected_toml_config.severity
            );

            // Compare values
            assert_eq!(
                lsp_rule.values, expected_toml_config.values,
                "Parity failure [{description}]: values mismatch. \
                 LSP={:?}, TOML={:?}",
                lsp_rule.values, expected_toml_config.values
            );
        }
    }

    /// Test apply_rule_config_if_absent preserves all existing config
    #[tokio::test]
    async fn test_lsp_config_if_absent_preserves_existing() {
        use crate::config::RuleConfig;
        use crate::rule::Severity;

        let server = create_test_server();

        // Pre-existing file config with severity AND values
        let mut config = crate::config::Config::default();
        config.rules.insert(
            "MD013".to_string(),
            RuleConfig {
                severity: Some(Severity::Error),
                values: [("line_length".to_string(), toml::Value::Integer(80))]
                    .into_iter()
                    .collect(),
            },
        );

        // LSP tries to override with different values
        let lsp_json = serde_json::json!({
            "severity": "info",
            "lineLength": 120
        });
        server.apply_rule_config_if_absent(&mut config, "MD013", &lsp_json);

        let rule = config.rules.get("MD013").expect("Rule should exist");

        // Original severity preserved
        assert_eq!(
            rule.severity,
            Some(Severity::Error),
            "Existing severity should not be overwritten"
        );

        // Original values preserved
        assert_eq!(
            rule.values.get("line_length"),
            Some(&toml::Value::Integer(80)),
            "Existing values should not be overwritten"
        );
    }

    // Tests for apply_formatting_options (issue #265)

    #[test]
    fn test_apply_formatting_options_insert_final_newline() {
        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            properties: HashMap::new(),
            trim_trailing_whitespace: None,
            insert_final_newline: Some(true),
            trim_final_newlines: None,
        };

        // Content without final newline should get one added
        let result = RumdlLanguageServer::apply_formatting_options("hello".to_string(), &options);
        assert_eq!(result, "hello\n");

        // Content with final newline should stay the same
        let result = RumdlLanguageServer::apply_formatting_options("hello\n".to_string(), &options);
        assert_eq!(result, "hello\n");
    }

    #[test]
    fn test_apply_formatting_options_trim_final_newlines() {
        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            properties: HashMap::new(),
            trim_trailing_whitespace: None,
            insert_final_newline: None,
            trim_final_newlines: Some(true),
        };

        // Multiple trailing newlines should be removed
        let result = RumdlLanguageServer::apply_formatting_options("hello\n\n\n".to_string(), &options);
        assert_eq!(result, "hello");

        // Single trailing newline should also be removed (trim_final_newlines removes ALL)
        let result = RumdlLanguageServer::apply_formatting_options("hello\n".to_string(), &options);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_apply_formatting_options_trim_and_insert_combined() {
        // This is the common case: trim extra newlines, then ensure exactly one
        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            properties: HashMap::new(),
            trim_trailing_whitespace: None,
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
        };

        // Multiple trailing newlines → exactly one
        let result = RumdlLanguageServer::apply_formatting_options("hello\n\n\n".to_string(), &options);
        assert_eq!(result, "hello\n");

        // No trailing newline → add one
        let result = RumdlLanguageServer::apply_formatting_options("hello".to_string(), &options);
        assert_eq!(result, "hello\n");

        // Already has exactly one → unchanged
        let result = RumdlLanguageServer::apply_formatting_options("hello\n".to_string(), &options);
        assert_eq!(result, "hello\n");
    }

    #[test]
    fn test_apply_formatting_options_trim_trailing_whitespace() {
        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            properties: HashMap::new(),
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: None,
        };

        // Trailing whitespace on lines should be removed
        let result = RumdlLanguageServer::apply_formatting_options("hello  \nworld\t\n".to_string(), &options);
        assert_eq!(result, "hello\nworld\n");
    }

    #[test]
    fn test_apply_formatting_options_issue_265_scenario() {
        // Issue #265: MD012 at end of file doesn't work with LSP formatting
        // The editor (nvim) may strip trailing newlines from buffer before sending to LSP
        // With proper FormattingOptions handling, we should still get the right result

        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            properties: HashMap::new(),
            trim_trailing_whitespace: None,
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
        };

        // Scenario 1: Editor sends content with multiple trailing newlines
        let result = RumdlLanguageServer::apply_formatting_options("hello foobar hello.\n\n\n".to_string(), &options);
        assert_eq!(
            result, "hello foobar hello.\n",
            "Should have exactly one trailing newline"
        );

        // Scenario 2: Editor sends content with trailing newlines stripped
        let result = RumdlLanguageServer::apply_formatting_options("hello foobar hello.".to_string(), &options);
        assert_eq!(result, "hello foobar hello.\n", "Should add final newline");

        // Scenario 3: Content is already correct
        let result = RumdlLanguageServer::apply_formatting_options("hello foobar hello.\n".to_string(), &options);
        assert_eq!(result, "hello foobar hello.\n", "Should remain unchanged");
    }

    #[test]
    fn test_apply_formatting_options_no_options() {
        // When all options are None/false, content should be unchanged
        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            properties: HashMap::new(),
            trim_trailing_whitespace: None,
            insert_final_newline: None,
            trim_final_newlines: None,
        };

        let content = "hello  \nworld\n\n\n";
        let result = RumdlLanguageServer::apply_formatting_options(content.to_string(), &options);
        assert_eq!(result, content, "Content should be unchanged when no options set");
    }

    #[test]
    fn test_apply_formatting_options_empty_content() {
        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            properties: HashMap::new(),
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
        };

        // Empty content should stay empty (no newline added to truly empty documents)
        let result = RumdlLanguageServer::apply_formatting_options("".to_string(), &options);
        assert_eq!(result, "");

        // Just newlines should become single newline (content existed, so gets final newline)
        let result = RumdlLanguageServer::apply_formatting_options("\n\n\n".to_string(), &options);
        assert_eq!(result, "\n");
    }

    #[test]
    fn test_apply_formatting_options_multiline_content() {
        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            properties: HashMap::new(),
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
        };

        let content = "# Heading  \n\nParagraph  \n- List item  \n\n\n";
        let result = RumdlLanguageServer::apply_formatting_options(content.to_string(), &options);
        assert_eq!(result, "# Heading\n\nParagraph\n- List item\n");
    }

    #[test]
    fn test_code_action_kind_filtering() {
        // Test the hierarchical code action kind matching used in code_action handler
        // LSP spec: source.fixAll.rumdl should match requests for source.fixAll

        let matches = |action_kind: &str, requested: &str| -> bool { action_kind.starts_with(requested) };

        // source.fixAll.rumdl matches source.fixAll (parent kind)
        assert!(matches("source.fixAll.rumdl", "source.fixAll"));

        // source.fixAll.rumdl matches source.fixAll.rumdl (exact match)
        assert!(matches("source.fixAll.rumdl", "source.fixAll.rumdl"));

        // source.fixAll.rumdl matches source (grandparent kind)
        assert!(matches("source.fixAll.rumdl", "source"));

        // quickfix matches quickfix (exact match)
        assert!(matches("quickfix", "quickfix"));

        // source.fixAll.rumdl does NOT match quickfix
        assert!(!matches("source.fixAll.rumdl", "quickfix"));

        // quickfix does NOT match source.fixAll
        assert!(!matches("quickfix", "source.fixAll"));

        // source.fixAll does NOT match source.fixAll.rumdl (child is more specific)
        assert!(!matches("source.fixAll", "source.fixAll.rumdl"));
    }

    #[test]
    fn test_code_action_kind_filter_with_empty_array() {
        // LSP spec: "If provided with no kinds, all supported kinds are returned"
        // An empty array should be treated the same as None (return all actions)

        let filter_actions = |kinds: Option<Vec<&str>>| -> bool {
            // Simulates our filtering logic
            if let Some(ref k) = kinds
                && !k.is_empty()
            {
                // Would filter
                false
            } else {
                // Return all
                true
            }
        };

        // None returns all actions
        assert!(filter_actions(None));

        // Empty array returns all actions (per LSP spec)
        assert!(filter_actions(Some(vec![])));

        // Non-empty array triggers filtering
        assert!(!filter_actions(Some(vec!["source.fixAll"])));
    }

    #[test]
    fn test_code_action_kind_constants() {
        // Verify our custom code action kind string matches LSP conventions
        let fix_all_rumdl = CodeActionKind::new("source.fixAll.rumdl");
        assert_eq!(fix_all_rumdl.as_str(), "source.fixAll.rumdl");

        // Verify it's a sub-kind of SOURCE_FIX_ALL
        assert!(
            fix_all_rumdl
                .as_str()
                .starts_with(CodeActionKind::SOURCE_FIX_ALL.as_str())
        );
    }
}
