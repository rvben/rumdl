//! Main Language Server Protocol server implementation for rumdl
//!
//! This module implements the core LSP server following Ruff's architecture.
//! It provides real-time markdown linting, diagnostics, and code actions.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result as JsonRpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::config::Config;
use crate::lsp::types::{RumdlLspConfig, warning_to_code_action, warning_to_diagnostic};
use crate::rule::Rule;
use crate::rules;

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

/// Main LSP server for rumdl
///
/// Following Ruff's pattern, this server provides:
/// - Real-time diagnostics as users type
/// - Code actions for automatic fixes
/// - Configuration management
/// - Multi-file support
#[derive(Clone)]
pub struct RumdlLanguageServer {
    client: Client,
    /// Configuration for the LSP server
    config: Arc<RwLock<RumdlLspConfig>>,
    /// Rumdl core configuration
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) rumdl_config: Arc<RwLock<Config>>,
    /// Document store for open files and cached disk files
    documents: Arc<RwLock<HashMap<Url, DocumentEntry>>>,
}

impl RumdlLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            config: Arc::new(RwLock::new(RumdlLspConfig::default())),
            rumdl_config: Arc::new(RwLock::new(Config::default())),
            documents: Arc::new(RwLock::new(HashMap::new())),
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

    /// Apply LSP config overrides to the filtered rules
    fn apply_lsp_config_overrides(
        &self,
        mut filtered_rules: Vec<Box<dyn Rule>>,
        lsp_config: &RumdlLspConfig,
    ) -> Vec<Box<dyn Rule>> {
        // Apply enable_rules override from LSP config (if specified, only these rules are active)
        if let Some(enable) = &lsp_config.enable_rules
            && !enable.is_empty()
        {
            let enable_set: std::collections::HashSet<String> = enable.iter().cloned().collect();
            filtered_rules.retain(|rule| enable_set.contains(rule.name()));
        }

        // Apply disable_rules override from LSP config
        if let Some(disable) = &lsp_config.disable_rules
            && !disable.is_empty()
        {
            let disable_set: std::collections::HashSet<String> = disable.iter().cloned().collect();
            filtered_rules.retain(|rule| !disable_set.contains(rule.name()));
        }

        filtered_rules
    }

    /// Check if a file URI should be excluded based on exclude patterns
    async fn should_exclude_uri(&self, uri: &Url) -> bool {
        // Try to convert URI to file path
        let file_path = match uri.to_file_path() {
            Ok(path) => path,
            Err(_) => return false, // If we can't get a path, don't exclude
        };

        let rumdl_config = self.rumdl_config.read().await;
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

        // Get rumdl configuration
        let rumdl_config = self.rumdl_config.read().await;
        let all_rules = rules::all_rules(&rumdl_config);
        let flavor = rumdl_config.markdown_flavor();

        // Use the standard filter_rules function which respects config's disabled rules
        let mut filtered_rules = rules::filter_rules(&all_rules, &rumdl_config.global);
        drop(rumdl_config); // Release config lock early

        // Apply LSP config overrides (select_rules, ignore_rules from VSCode settings)
        filtered_rules = self.apply_lsp_config_overrides(filtered_rules, &lsp_config);

        // Run rumdl linting with the configured flavor
        match crate::lint(text, &filtered_rules, false, flavor) {
            Ok(warnings) => {
                let diagnostics = warnings.iter().map(warning_to_diagnostic).collect();
                Ok(diagnostics)
            }
            Err(e) => {
                log::error!("Failed to lint document {uri}: {e}");
                Ok(Vec::new())
            }
        }
    }

    /// Update diagnostics for a document
    async fn update_diagnostics(&self, uri: Url, text: String) {
        match self.lint_document(&uri, &text).await {
            Ok(diagnostics) => {
                self.client.publish_diagnostics(uri, diagnostics, None).await;
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

        let rumdl_config = self.rumdl_config.read().await;
        let all_rules = rules::all_rules(&rumdl_config);
        let flavor = rumdl_config.markdown_flavor();

        // Use the standard filter_rules function which respects config's disabled rules
        let mut filtered_rules = rules::filter_rules(&all_rules, &rumdl_config.global);
        drop(rumdl_config);

        // Apply LSP config overrides (select_rules, ignore_rules from VSCode settings)
        filtered_rules = self.apply_lsp_config_overrides(filtered_rules, &lsp_config);

        // Apply fixes sequentially for each rule
        let mut fixed_text = text.to_string();
        let mut any_changes = false;

        for rule in &filtered_rules {
            let ctx = crate::lint_context::LintContext::new(&fixed_text, flavor);
            match rule.fix(&ctx) {
                Ok(new_text) => {
                    if new_text != fixed_text {
                        fixed_text = new_text;
                        any_changes = true;
                    }
                }
                Err(e) => {
                    log::warn!("Failed to apply fix for rule {}: {}", rule.name(), e);
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

    /// Get code actions for diagnostics at a position
    async fn get_code_actions(&self, uri: &Url, text: &str, range: Range) -> Result<Vec<CodeAction>> {
        let config_guard = self.config.read().await;
        let lsp_config = config_guard.clone();
        drop(config_guard);

        let rumdl_config = self.rumdl_config.read().await;
        let all_rules = rules::all_rules(&rumdl_config);
        let flavor = rumdl_config.markdown_flavor();

        // Use the standard filter_rules function which respects config's disabled rules
        let mut filtered_rules = rules::filter_rules(&all_rules, &rumdl_config.global);
        drop(rumdl_config);

        // Apply LSP config overrides (select_rules, ignore_rules from VSCode settings)
        filtered_rules = self.apply_lsp_config_overrides(filtered_rules, &lsp_config);

        match crate::lint(text, &filtered_rules, false, flavor) {
            Ok(warnings) => {
                let mut actions = Vec::new();
                let mut fixable_count = 0;

                for warning in &warnings {
                    // Check if warning is within the requested range
                    let warning_line = (warning.line.saturating_sub(1)) as u32;
                    if warning_line >= range.start.line
                        && warning_line <= range.end.line
                        && let Some(action) = warning_to_code_action(warning, uri, text)
                    {
                        actions.push(action);
                        if warning.fix.is_some() {
                            fixable_count += 1;
                        }
                    }
                }

                // Add "Fix all" action if there are multiple fixable issues in range
                if fixable_count > 1 {
                    // Count total fixable issues in the document
                    let total_fixable = warnings.iter().filter(|w| w.fix.is_some()).count();

                    if let Ok(fixed_content) = crate::utils::fix_utils::apply_warning_fixes(text, &warnings)
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
                            kind: Some(CodeActionKind::QUICKFIX),
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
                *self.rumdl_config.write().await = sourced_config.into();

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

        // Load rumdl configuration with auto-discovery
        self.load_configuration(false).await;

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                    identifier: Some("rumdl".to_string()),
                    inter_file_dependencies: false,
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
        log::info!("rumdl Language Server initialized");

        self.client
            .log_message(MessageType::INFO, "rumdl Language Server started")
            .await;
    }

    async fn did_change_workspace_folders(&self, _params: DidChangeWorkspaceFoldersParams) {
        // Reload configuration when workspace folders change
        self.reload_configuration().await;
    }

    async fn shutdown(&self) -> JsonRpcResult<()> {
        log::info!("Shutting down rumdl Language Server");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        let version = params.text_document.version;

        // Store document with version information
        let entry = DocumentEntry {
            content: text.clone(),
            version: Some(version),
            from_disk: false,
        };
        self.documents.write().await.insert(uri.clone(), entry);

        // Update diagnostics
        self.update_diagnostics(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        // Apply changes (we're using FULL sync, so just take the full text)
        if let Some(change) = params.content_changes.into_iter().next() {
            let text = change.text;

            // Update stored document with new version
            let entry = DocumentEntry {
                content: text.clone(),
                version: Some(version),
                from_disk: false,
            };
            self.documents.write().await.insert(uri.clone(), entry);

            // Update diagnostics
            self.update_diagnostics(uri, text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let config_guard = self.config.read().await;
        let enable_auto_fix = config_guard.enable_auto_fix;
        drop(config_guard);

        // Auto-fix on save if enabled
        if enable_auto_fix && let Some(entry) = self.documents.read().await.get(&params.text_document.uri) {
            let text = &entry.content;
            match self.apply_all_fixes(&params.text_document.uri, text).await {
                Ok(Some(fixed_text)) => {
                    // Create a workspace edit to apply the fixes
                    let edit = TextEdit {
                        range: Range {
                            start: Position { line: 0, character: 0 },
                            end: self.get_end_position(text),
                        },
                        new_text: fixed_text.clone(),
                    };

                    let mut changes = std::collections::HashMap::new();
                    changes.insert(params.text_document.uri.clone(), vec![edit]);

                    let workspace_edit = WorkspaceEdit {
                        changes: Some(changes),
                        document_changes: None,
                        change_annotations: None,
                    };

                    // Apply the edit
                    match self.client.apply_edit(workspace_edit).await {
                        Ok(response) => {
                            if response.applied {
                                log::info!("Auto-fix applied successfully");
                                // Update our stored version
                                let entry = DocumentEntry {
                                    content: fixed_text,
                                    version: None, // Will be updated by the next didChange from client
                                    from_disk: false,
                                };
                                self.documents
                                    .write()
                                    .await
                                    .insert(params.text_document.uri.clone(), entry);
                            } else {
                                log::warn!("Auto-fix was not applied: {:?}", response.failure_reason);
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to apply auto-fix: {e}");
                        }
                    }
                }
                Ok(None) => {
                    log::debug!("No fixes to apply");
                }
                Err(e) => {
                    log::error!("Failed to generate fixes: {e}");
                }
            }
        }

        // Re-lint the document
        if let Some(entry) = self.documents.read().await.get(&params.text_document.uri) {
            self.update_diagnostics(params.text_document.uri, entry.content.clone())
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // Remove document from storage
        self.documents.write().await.remove(&params.text_document.uri);

        // Clear diagnostics
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn code_action(&self, params: CodeActionParams) -> JsonRpcResult<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;

        if let Some(text) = self.get_document_content(&uri).await {
            match self.get_code_actions(&uri, &text, range).await {
                Ok(actions) => {
                    let response: Vec<CodeActionOrCommand> =
                        actions.into_iter().map(CodeActionOrCommand::CodeAction).collect();
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

        log::debug!("Formatting request for: {uri}");

        if let Some(text) = self.get_document_content(&uri).await {
            // Get config with LSP overrides
            let config_guard = self.config.read().await;
            let lsp_config = config_guard.clone();
            drop(config_guard);

            // Get all rules from config
            let rumdl_config = self.rumdl_config.read().await;
            let all_rules = rules::all_rules(&rumdl_config);
            let flavor = rumdl_config.markdown_flavor();

            // Use the standard filter_rules function which respects config's disabled rules
            let mut filtered_rules = rules::filter_rules(&all_rules, &rumdl_config.global);

            drop(rumdl_config);

            // Apply LSP config overrides
            filtered_rules = self.apply_lsp_config_overrides(filtered_rules, &lsp_config);

            // Use warning fixes for all rules
            match crate::lint(&text, &filtered_rules, false, flavor) {
                Ok(warnings) => {
                    log::debug!(
                        "Found {} warnings, {} with fixes",
                        warnings.len(),
                        warnings.iter().filter(|w| w.fix.is_some()).count()
                    );

                    let has_fixes = warnings.iter().any(|w| w.fix.is_some());
                    if has_fixes {
                        match crate::utils::fix_utils::apply_warning_fixes(&text, &warnings) {
                            Ok(fixed_content) => {
                                if fixed_content != text {
                                    log::debug!("Returning formatting edits");
                                    let end_position = self.get_end_position(&text);
                                    let edit = TextEdit {
                                        range: Range {
                                            start: Position { line: 0, character: 0 },
                                            end: end_position,
                                        },
                                        new_text: fixed_content,
                                    };
                                    return Ok(Some(vec![edit]));
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to apply fixes: {e}");
                            }
                        }
                    }
                    Ok(Some(Vec::new()))
                }
                Err(e) => {
                    log::error!("Failed to format document: {e}");
                    Ok(Some(Vec::new()))
                }
            }
        } else {
            log::warn!("Document not found: {uri}");
            Ok(None)
        }
    }

    async fn diagnostic(&self, params: DocumentDiagnosticParams) -> JsonRpcResult<DocumentDiagnosticReportResult> {
        let uri = params.text_document.uri;

        if let Some(text) = self.get_document_content(&uri).await {
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
        let (service, _socket) = LspService::new(RumdlLanguageServer::new);
        service.inner().clone()
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
            rule_name: Some("MD001"),
        };

        // Test diagnostic conversion
        let diagnostic = warning_to_diagnostic(&warning);
        assert_eq!(diagnostic.message, "Test warning");
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diagnostic.code, Some(NumberOrString::String("MD001".to_string())));

        // Test code action conversion (no fix)
        let uri = Url::parse("file:///test.md").unwrap();
        let action = warning_to_code_action(&warning, &uri, "Test content");
        assert!(action.is_none());
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
        assert_eq!(fixed.unwrap(), "# Heading without space");
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

        // The new text should have trailing spaces removed
        let edit = &edits[0];
        // The formatted text should have the trailing spaces removed from the middle line
        // and a final newline added
        let expected = "# Test\n\nThis is a test  \nWith trailing spaces\n";
        assert_eq!(edit.new_text, expected);
    }
}
