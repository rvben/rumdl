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
use crate::lsp::types::{warning_to_code_action, warning_to_diagnostic, RumdlLspConfig};
use crate::rules;

/// Main LSP server for rumdl
///
/// Following Ruff's pattern, this server provides:
/// - Real-time diagnostics as users type
/// - Code actions for automatic fixes
/// - Configuration management
/// - Multi-file support
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
                log::error!("Failed to lint document {}: {}", uri, e);
                Ok(Vec::new())
            }
        }
    }

    /// Update diagnostics for a document
    async fn update_diagnostics(&self, uri: Url, text: String) {
        match self.lint_document(&uri, &text).await {
            Ok(diagnostics) => {
                self.client
                    .publish_diagnostics(uri, diagnostics, None)
                    .await;
            }
            Err(e) => {
                log::error!("Failed to update diagnostics: {}", e);
            }
        }
    }

    /// Get code actions for diagnostics at a position
    async fn get_code_actions(
        &self,
        uri: &Url,
        text: &str,
        range: Range,
    ) -> Result<Vec<CodeAction>> {
        let rumdl_config = self.rumdl_config.read().await;
        let all_rules = rules::all_rules(&rumdl_config);
        drop(rumdl_config);

        match crate::lint(text, &all_rules, false) {
            Ok(warnings) => {
                let mut actions = Vec::new();

                for warning in warnings {
                    // Check if warning is within the requested range
                    let warning_line = (warning.line.saturating_sub(1)) as u32;
                    if warning_line >= range.start.line && warning_line <= range.end.line {
                        if let Some(action) = warning_to_code_action(&warning, uri) {
                            actions.push(action);
                        }
                    }
                }

                Ok(actions)
            }
            Err(e) => {
                log::error!("Failed to get code actions: {}", e);
                Ok(Vec::new())
            }
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for RumdlLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> JsonRpcResult<InitializeResult> {
        log::info!("Initializing rumdl Language Server");

        // Parse client capabilities and configuration
        if let Some(options) = params.initialization_options {
            if let Ok(config) = serde_json::from_value::<RumdlLspConfig>(options) {
                *self.config.write().await = config;
            }
        }

        // Load rumdl configuration if specified
        let config_guard = self.config.read().await;
        if let Some(config_path) = &config_guard.config_path {
            match crate::config::SourcedConfig::load(Some(config_path), None) {
                Ok(sourced_config) => {
                    *self.rumdl_config.write().await = sourced_config.into();
                    log::info!("Loaded rumdl config from: {}", config_path);
                }
                Err(e) => {
                    log::warn!("Failed to load config from {}: {}", config_path, e);
                }
            }
        }
        drop(config_guard);

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("rumdl".to_string()),
                        inter_file_dependencies: false,
                        workspace_diagnostics: false,
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                    },
                )),
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

    async fn shutdown(&self) -> JsonRpcResult<()> {
        log::info!("Shutting down rumdl Language Server");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        // Store document
        self.documents
            .write()
            .await
            .insert(uri.clone(), text.clone());

        // Update diagnostics
        self.update_diagnostics(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // Apply changes (we're using FULL sync, so just take the full text)
        if let Some(change) = params.content_changes.into_iter().next() {
            let text = change.text;

            // Update stored document
            self.documents
                .write()
                .await
                .insert(uri.clone(), text.clone());

            // Update diagnostics
            self.update_diagnostics(uri, text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let config_guard = self.config.read().await;

        // Auto-fix on save if enabled
        if config_guard.enable_auto_fix {
            // TODO: Implement auto-fix on save
            log::debug!("Auto-fix on save not yet implemented");
        }

        drop(config_guard);

        // Re-lint the document
        if let Some(text) = self.documents.read().await.get(&params.text_document.uri) {
            self.update_diagnostics(params.text_document.uri, text.clone())
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // Remove document from storage
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);

        // Clear diagnostics
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> JsonRpcResult<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;

        if let Some(text) = self.documents.read().await.get(&uri) {
            match self.get_code_actions(&uri, text, range).await {
                Ok(actions) => {
                    let response: Vec<CodeActionOrCommand> = actions
                        .into_iter()
                        .map(CodeActionOrCommand::CodeAction)
                        .collect();
                    Ok(Some(response))
                }
                Err(e) => {
                    log::error!("Failed to get code actions: {}", e);
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }

    async fn diagnostic(
        &self,
        params: DocumentDiagnosticParams,
    ) -> JsonRpcResult<DocumentDiagnosticReportResult> {
        let uri = params.text_document.uri;

        if let Some(text) = self.documents.read().await.get(&uri) {
            match self.lint_document(&uri, text).await {
                Ok(diagnostics) => Ok(DocumentDiagnosticReportResult::Report(
                    DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                        related_documents: None,
                        full_document_diagnostic_report: FullDocumentDiagnosticReport {
                            result_id: None,
                            items: diagnostics,
                        },
                    }),
                )),
                Err(e) => {
                    log::error!("Failed to get diagnostics: {}", e);
                    Ok(DocumentDiagnosticReportResult::Report(
                        DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                            related_documents: None,
                            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                                result_id: None,
                                items: Vec::new(),
                            },
                        }),
                    ))
                }
            }
        } else {
            Ok(DocumentDiagnosticReportResult::Report(
                DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                    related_documents: None,
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: None,
                        items: Vec::new(),
                    },
                }),
            ))
        }
    }
}
