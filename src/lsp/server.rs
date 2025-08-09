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
use crate::rules;

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
    rumdl_config: Arc<RwLock<Config>>,
    /// Document store for open files
    documents: Arc<RwLock<HashMap<Url, String>>>,
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

    /// Lint a document and return diagnostics
    async fn lint_document(&self, uri: &Url, text: &str) -> Result<Vec<Diagnostic>> {
        let config_guard = self.config.read().await;

        // Skip linting if disabled
        if !config_guard.enable_linting {
            return Ok(Vec::new());
        }

        drop(config_guard); // Release config lock early

        // Get rumdl configuration
        let rumdl_config = self.rumdl_config.read().await;
        let all_rules = rules::all_rules(&rumdl_config);
        drop(rumdl_config); // Release config lock early

        // Run rumdl linting
        match crate::lint(text, &all_rules, false) {
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
    async fn apply_all_fixes(&self, _uri: &Url, text: &str) -> Result<Option<String>> {
        let rumdl_config = self.rumdl_config.read().await;
        let all_rules = rules::all_rules(&rumdl_config);
        drop(rumdl_config);

        // Apply fixes sequentially for each rule
        let mut fixed_text = text.to_string();
        let mut any_changes = false;

        for rule in &all_rules {
            let ctx = crate::lint_context::LintContext::new(&fixed_text);
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
        let lines: Vec<&str> = text.lines().collect();
        let line = lines.len().saturating_sub(1) as u32;
        let character = lines.last().map_or(0, |l| l.len() as u32);
        Position { line, character }
    }

    /// Get code actions for diagnostics at a position
    async fn get_code_actions(&self, uri: &Url, text: &str, range: Range) -> Result<Vec<CodeAction>> {
        let rumdl_config = self.rumdl_config.read().await;
        let all_rules = rules::all_rules(&rumdl_config);
        drop(rumdl_config);

        match crate::lint(text, &all_rules, false) {
            Ok(warnings) => {
                let mut actions = Vec::new();

                for warning in warnings {
                    // Check if warning is within the requested range
                    let warning_line = (warning.line.saturating_sub(1)) as u32;
                    if warning_line >= range.start.line
                        && warning_line <= range.end.line
                        && let Some(action) = warning_to_code_action(&warning, uri, text)
                    {
                        actions.push(action);
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

        // Store document
        self.documents.write().await.insert(uri.clone(), text.clone());

        // Update diagnostics
        self.update_diagnostics(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // Apply changes (we're using FULL sync, so just take the full text)
        if let Some(change) = params.content_changes.into_iter().next() {
            let text = change.text;

            // Update stored document
            self.documents.write().await.insert(uri.clone(), text.clone());

            // Update diagnostics
            self.update_diagnostics(uri, text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let config_guard = self.config.read().await;
        let enable_auto_fix = config_guard.enable_auto_fix;
        drop(config_guard);

        // Auto-fix on save if enabled
        if enable_auto_fix && let Some(text) = self.documents.read().await.get(&params.text_document.uri) {
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
                                self.documents
                                    .write()
                                    .await
                                    .insert(params.text_document.uri.clone(), fixed_text);
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
        if let Some(text) = self.documents.read().await.get(&params.text_document.uri) {
            self.update_diagnostics(params.text_document.uri, text.clone()).await;
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

        if let Some(text) = self.documents.read().await.get(&uri) {
            match self.get_code_actions(&uri, text, range).await {
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

    async fn diagnostic(&self, params: DocumentDiagnosticParams) -> JsonRpcResult<DocumentDiagnosticReportResult> {
        let uri = params.text_document.uri;

        if let Some(text) = self.documents.read().await.get(&uri) {
            match self.lint_document(&uri, text).await {
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
        server.documents.write().await.insert(uri.clone(), text.to_string());

        // Verify storage
        let stored = server.documents.read().await.get(&uri).cloned();
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
            docs.insert(uri1.clone(), text1.to_string());
            docs.insert(uri2.clone(), text2.to_string());
        }

        // Verify both are stored
        let docs = server.documents.read().await;
        assert_eq!(docs.len(), 2);
        assert_eq!(docs.get(&uri1).map(|s| s.as_str()), Some(text1));
        assert_eq!(docs.get(&uri2).map(|s| s.as_str()), Some(text2));
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
        server.documents.write().await.insert(uri.clone(), text.to_string());

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

        // Ends with newline - lines() doesn't include the empty line after \n
        let pos = server.get_end_position("Hello\n");
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 5);
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
}
