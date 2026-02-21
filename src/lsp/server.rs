//! Main Language Server Protocol server implementation for rumdl
//!
//! This module implements the core LSP server following Ruff's architecture.
//! It provides real-time markdown linting, diagnostics, and code actions.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use futures::future::join_all;
use tokio::sync::{RwLock, mpsc};
use tower_lsp::jsonrpc::Result as JsonRpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::config::{Config, is_valid_rule_name};
use crate::lsp::index_worker::IndexWorker;
use crate::lsp::types::{IndexState, IndexUpdate, LspRuleSettings, RumdlLspConfig};
use crate::rule::FixCapability;
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
pub(crate) struct DocumentEntry {
    /// The document content
    pub(crate) content: String,
    /// Version number from the editor (None for disk-loaded documents)
    pub(crate) version: Option<i32>,
    /// Whether the document was loaded from disk (true) or opened in editor (false)
    pub(crate) from_disk: bool,
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
    pub(crate) client: Client,
    /// Configuration for the LSP server
    pub(crate) config: Arc<RwLock<RumdlLspConfig>>,
    /// Rumdl core configuration (fallback/default)
    pub(crate) rumdl_config: Arc<RwLock<Config>>,
    /// Document store for open files and cached disk files
    pub(crate) documents: Arc<RwLock<HashMap<Url, DocumentEntry>>>,
    /// Workspace root folders from the client
    pub(crate) workspace_roots: Arc<RwLock<Vec<PathBuf>>>,
    /// Configuration cache: maps directory path to resolved config
    /// Key is the directory where config search started (file's parent dir)
    pub(crate) config_cache: Arc<RwLock<HashMap<PathBuf, ConfigCacheEntry>>>,
    /// Workspace index for cross-file analysis (MD051)
    pub(crate) workspace_index: Arc<RwLock<WorkspaceIndex>>,
    /// Current state of the workspace index (building/ready/error)
    pub(crate) index_state: Arc<RwLock<IndexState>>,
    /// Channel to send updates to the background index worker
    pub(crate) update_tx: mpsc::Sender<IndexUpdate>,
    /// Whether the client supports pull diagnostics (textDocument/diagnostic)
    /// When true, we skip pushing diagnostics to avoid duplicates
    pub(crate) client_supports_pull_diagnostics: Arc<RwLock<bool>>,
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
                    let path = path.canonicalize().unwrap_or(path);
                    log::info!("Workspace root: {}", path.display());
                    roots.push(path);
                }
            }
        } else if let Some(root_uri) = params.root_uri
            && let Ok(path) = root_uri.to_file_path()
        {
            let path = path.canonicalize().unwrap_or(path);
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
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "`".to_string(),
                        "(".to_string(),
                        "#".to_string(),
                        "/".to_string(),
                        ".".to_string(),
                        "-".to_string(),
                    ]),
                    resolve_provider: Some(false),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                    all_commit_characters: None,
                    completion_item: None,
                }),
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

    async fn completion(&self, params: CompletionParams) -> JsonRpcResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        // Get document content
        let Some(text) = self.get_document_content(&uri).await else {
            return Ok(None);
        };

        // Code fence language completion (backtick trigger)
        if let Some((start_col, current_text)) = Self::detect_code_fence_language_position(&text, position) {
            log::debug!(
                "Code fence completion triggered at {}:{}, current text: '{}'",
                position.line,
                position.character,
                current_text
            );
            let items = self
                .get_language_completions(&uri, &current_text, start_col, position)
                .await;
            if !items.is_empty() {
                return Ok(Some(CompletionResponse::Array(items)));
            }
        }

        // Link target completion: file paths and heading anchors
        if self.config.read().await.enable_link_completions {
            // For trigger characters that fire on many non-link contexts (`.`, `-`),
            // skip the full parse when there is no `](` on the current line before
            // the cursor.  This avoids needless work on list items and contractions.
            let trigger = params.context.as_ref().and_then(|c| c.trigger_character.as_deref());
            let skip_link_check = matches!(trigger, Some("." | "-")) && {
                let line_num = position.line as usize;
                // Scan the whole line — no byte-slicing at a UTF-16 offset needed.
                // A line without `](` anywhere cannot contain a link target.
                !text
                    .lines()
                    .nth(line_num)
                    .map(|line| line.contains("]("))
                    .unwrap_or(false)
            };

            if !skip_link_check && let Some(link_info) = Self::detect_link_target_position(&text, position) {
                let items = if let Some((partial_anchor, anchor_start_col)) = link_info.anchor {
                    log::debug!(
                        "Anchor completion triggered at {}:{}, file: '{}', partial: '{}'",
                        position.line,
                        position.character,
                        link_info.file_path,
                        partial_anchor
                    );
                    self.get_anchor_completions(&uri, &link_info.file_path, &partial_anchor, anchor_start_col, position)
                        .await
                } else {
                    log::debug!(
                        "File path completion triggered at {}:{}, partial: '{}'",
                        position.line,
                        position.character,
                        link_info.file_path
                    );
                    self.get_file_completions(&uri, &link_info.file_path, link_info.path_start_col, position)
                        .await
                };
                if !items.is_empty() {
                    return Ok(Some(CompletionResponse::Array(items)));
                }
            }
        }

        Ok(None)
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
#[path = "tests.rs"]
mod tests;
