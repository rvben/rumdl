//! Comprehensive tests for rumdl Language Server Protocol implementation
//!
//! This module tests the LSP server functionality including:
//! - Type conversions (warnings to diagnostics/code actions)
//! - Server initialization and capabilities
//! - Document synchronization
//! - Real-time linting and diagnostics
//! - Code actions and fixes

#![allow(deprecated)]

use rumdl_lib::lsp::RumdlLanguageServer;
use rumdl_lib::lsp::types::{RumdlLspConfig, warning_to_code_action, warning_to_diagnostic};
use rumdl_lib::rule::{Fix, LintWarning, Severity};
use tower_lsp::lsp_types::*;
use tower_lsp::{LanguageServer, LspService};
use url::Url;

/// Test the RumdlLspConfig struct and its default values
#[test]
fn test_rumdl_lsp_config_defaults() {
    let config = RumdlLspConfig::default();

    assert_eq!(config.config_path, None);
    assert!(config.enable_linting);
    assert!(!config.enable_auto_fix);
    assert!(config.disable_rules.is_empty());
}

/// Test the RumdlLspConfig serialization/deserialization
#[test]
fn test_rumdl_lsp_config_serde() {
    let config = RumdlLspConfig {
        config_path: Some("/path/to/config.toml".to_string()),
        enable_linting: true,
        enable_auto_fix: false,
        disable_rules: vec!["MD001".to_string(), "MD013".to_string()],
    };

    // Test serialization
    let json = serde_json::to_string(&config).expect("Failed to serialize config");

    // Test deserialization
    let deserialized: RumdlLspConfig = serde_json::from_str(&json).expect("Failed to deserialize config");

    assert_eq!(deserialized.config_path, config.config_path);
    assert_eq!(deserialized.enable_linting, config.enable_linting);
    assert_eq!(deserialized.enable_auto_fix, config.enable_auto_fix);
    assert_eq!(deserialized.disable_rules, config.disable_rules);
}

/// Test warning to diagnostic conversion
#[test]
fn test_warning_to_diagnostic_conversion() {
    let warning = LintWarning {
        message: "Test warning message".to_string(),
        line: 5,
        column: 10,
        end_line: 5,
        end_column: 15,
        severity: Severity::Warning,
        fix: None,
        rule_name: Some("MD001"),
    };

    let diagnostic = warning_to_diagnostic(&warning);

    // Check basic properties
    assert_eq!(diagnostic.message, "Test warning message");
    assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::WARNING));
    assert_eq!(diagnostic.source, Some("rumdl".to_string()));

    // Check position conversion (LSP is 0-indexed, rumdl is 1-indexed)
    assert_eq!(diagnostic.range.start.line, 4); // 5 - 1
    assert_eq!(diagnostic.range.start.character, 9); // 10 - 1
    assert_eq!(diagnostic.range.end.line, 4); // 5 - 1
    assert_eq!(diagnostic.range.end.character, 14); // 15 - 1 (end_column - 1)

    // Check rule code
    if let Some(NumberOrString::String(code)) = diagnostic.code {
        assert_eq!(code, "MD001");
    } else {
        panic!("Expected string code");
    }
}

/// Test warning to diagnostic conversion with Error severity
#[test]
fn test_warning_to_diagnostic_error_severity() {
    let warning = LintWarning {
        message: "Test error message".to_string(),
        line: 1,
        column: 1,
        end_line: 1,
        end_column: 5,
        severity: Severity::Error,
        fix: None,
        rule_name: Some("MD999"),
    };

    let diagnostic = warning_to_diagnostic(&warning);
    assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
}

/// Test warning to code action conversion with fix
#[test]
fn test_warning_to_code_action_with_fix() {
    let warning = LintWarning {
        message: "Line too long".to_string(),
        line: 1,
        column: 1,
        end_line: 1,
        end_column: 47,
        severity: Severity::Warning,
        fix: Some(Fix {
            range: 0..47,
            replacement: "shorter text".to_string(),
        }),
        rule_name: Some("MD013"),
    };

    let uri = Url::parse("file:///test.md").expect("Invalid URI");
    let document_text = "This line is too long and needs to be shortened";
    let code_action = warning_to_code_action(&warning, &uri, document_text);

    assert!(code_action.is_some());
    let action = code_action.unwrap();

    // Check basic properties
    assert_eq!(action.title, "Fix: Line too long");
    assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
    assert_eq!(action.is_preferred, Some(true));

    // Check that edit exists
    assert!(action.edit.is_some());
    let edit = action.edit.unwrap();
    assert!(edit.changes.is_some());

    // Check the text edit
    let changes = edit.changes.unwrap();
    let edits = changes.get(&uri).expect("Missing file edits");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "shorter text");
}

/// Test warning to code action conversion without fix
#[test]
fn test_warning_to_code_action_without_fix() {
    let warning = LintWarning {
        message: "No fix available".to_string(),
        line: 1,
        column: 1,
        end_line: 1,
        end_column: 5,
        severity: Severity::Warning,
        fix: None,
        rule_name: Some("MD001"),
    };

    let uri = Url::parse("file:///test.md").expect("Invalid URI");
    let document_text = "Test document content";
    let code_action = warning_to_code_action(&warning, &uri, document_text);

    assert!(code_action.is_none());
}

/// Test LSP server initialization
#[tokio::test]
async fn test_lsp_server_initialization() {
    let (service, _socket) = LspService::new(RumdlLanguageServer::new);

    let init_params = InitializeParams {
        process_id: None,
        root_path: None, // Deprecated but required
        root_uri: Some(Url::parse("file:///test").unwrap()),
        initialization_options: None,
        capabilities: ClientCapabilities::default(),
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    let result = service.inner().initialize(init_params).await;
    assert!(result.is_ok());

    let init_result = result.unwrap();

    // Check server capabilities
    let caps = init_result.capabilities;
    assert!(caps.text_document_sync.is_some());
    assert!(caps.code_action_provider.is_some());
    assert!(caps.diagnostic_provider.is_some());

    // Check server info
    assert!(init_result.server_info.is_some());
    let server_info = init_result.server_info.unwrap();
    assert_eq!(server_info.name, "rumdl");
    assert!(server_info.version.is_some());
}

/// Test LSP server initialization with custom config
#[tokio::test]
async fn test_lsp_server_initialization_with_config() {
    let (service, _socket) = LspService::new(RumdlLanguageServer::new);

    let custom_config = RumdlLspConfig {
        config_path: Some("/custom/path/.rumdl.toml".to_string()),
        enable_linting: true,
        enable_auto_fix: true,
        disable_rules: vec!["MD001".to_string()],
    };

    let init_params = InitializeParams {
        process_id: None,
        root_path: None, // Deprecated but required
        root_uri: Some(Url::parse("file:///test").unwrap()),
        initialization_options: Some(serde_json::to_value(custom_config).unwrap()),
        capabilities: ClientCapabilities::default(),
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    let result = service.inner().initialize(init_params).await;
    assert!(result.is_ok());
}

/// Test document lifecycle (open, change, save, close)
#[tokio::test]
async fn test_document_lifecycle() {
    let (service, _socket) = LspService::new(RumdlLanguageServer::new);
    let server = service.inner();

    // Initialize server first
    let init_params = InitializeParams {
        process_id: None,
        root_path: None, // Deprecated but required
        root_uri: Some(Url::parse("file:///test").unwrap()),
        initialization_options: None,
        capabilities: ClientCapabilities::default(),
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    server.initialize(init_params).await.unwrap();
    server.initialized(InitializedParams {}).await;

    let uri = Url::parse("file:///test.md").unwrap();

    // Test document open
    let open_params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: uri.clone(),
            language_id: "markdown".to_string(),
            version: 1,
            text: "# Test Document\n\nThis is a test.".to_string(),
        },
    };

    server.did_open(open_params).await;

    // Test document change
    let change_params = DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: uri.clone(),
            version: 2,
        },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "# Test Document\n\nThis is a test with more content.".to_string(),
        }],
    };

    server.did_change(change_params).await;

    // Test document save
    let save_params = DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        text: None,
    };

    server.did_save(save_params).await;

    // Test document close
    let close_params = DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier { uri },
    };

    server.did_close(close_params).await;
}

/// Test code action request
#[tokio::test]
async fn test_code_action_request() {
    let (service, _socket) = LspService::new(RumdlLanguageServer::new);
    let server = service.inner();

    // Initialize server
    let init_params = InitializeParams {
        process_id: None,
        root_path: None, // Deprecated but required
        root_uri: Some(Url::parse("file:///test").unwrap()),
        initialization_options: None,
        capabilities: ClientCapabilities::default(),
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    server.initialize(init_params).await.unwrap();
    server.initialized(InitializedParams {}).await;

    let uri = Url::parse("file:///test.md").unwrap();

    // Open a document with markdown content that has issues
    let open_params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: uri.clone(),
            language_id: "markdown".to_string(),
            version: 1,
            text: "# Test\n\n#  Heading with extra space".to_string(),
        },
    };

    server.did_open(open_params).await;

    // Request code actions
    let code_action_params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri },
        range: Range {
            start: Position { line: 2, character: 0 },
            end: Position { line: 2, character: 25 },
        },
        context: CodeActionContext {
            diagnostics: vec![],
            only: None,
            trigger_kind: None,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = server.code_action(code_action_params).await;
    assert!(result.is_ok());

    // The result might be None if no code actions are available
    // or Some with a list of actions
    let _actions = result.unwrap();
    // We can't assert specific behavior here without knowing exactly
    // which rules would trigger on our test content
}

/// Test diagnostic request
#[tokio::test]
async fn test_diagnostic_request() {
    let (service, _socket) = LspService::new(RumdlLanguageServer::new);
    let server = service.inner();

    // Initialize server
    let init_params = InitializeParams {
        process_id: None,
        root_path: None, // Deprecated but required
        root_uri: Some(Url::parse("file:///test").unwrap()),
        initialization_options: None,
        capabilities: ClientCapabilities::default(),
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    server.initialize(init_params).await.unwrap();
    server.initialized(InitializedParams {}).await;

    let uri = Url::parse("file:///test.md").unwrap();

    // Open a document
    let open_params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: uri.clone(),
            language_id: "markdown".to_string(),
            version: 1,
            text: "# Test Document\n\nContent here".to_string(),
        },
    };

    server.did_open(open_params).await;

    // Request diagnostics
    let diagnostic_params = DocumentDiagnosticParams {
        text_document: TextDocumentIdentifier { uri },
        identifier: None,
        previous_result_id: None,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = server.diagnostic(diagnostic_params).await;
    assert!(result.is_ok());

    // Verify we get a diagnostic report (even if empty)
    let report = result.unwrap();
    match report {
        DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(_)) => {
            // Expected result type
        }
        _ => panic!("Unexpected diagnostic report type"),
    }
}

/// Integration test that simulates real LSP workflow
#[tokio::test]
async fn test_real_workflow_integration() {
    let (service, _socket) = LspService::new(RumdlLanguageServer::new);
    let server = service.inner();

    // 1. Initialize
    let init_params = InitializeParams {
        process_id: None,
        root_path: None, // Deprecated but required
        root_uri: Some(Url::parse("file:///workspace").unwrap()),
        initialization_options: Some(serde_json::json!({
            "enableLinting": true,
            "enableAutoFix": false
        })),
        capabilities: ClientCapabilities::default(),
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    let init_result = server.initialize(init_params).await.unwrap();
    assert!(init_result.capabilities.text_document_sync.is_some());

    // 2. Initialized notification
    server.initialized(InitializedParams {}).await;

    // 3. Open document with known issues
    let uri = Url::parse("file:///workspace/test.md").unwrap();
    let problematic_content = r#"#  Heading with extra space

This is a line that is way too long and should trigger MD013 line length warning because it exceeds the default limit.

Another paragraph.
"#;

    let open_params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: uri.clone(),
            language_id: "markdown".to_string(),
            version: 1,
            text: problematic_content.to_string(),
        },
    };

    server.did_open(open_params).await;

    // 4. Request diagnostics
    let diagnostic_params = DocumentDiagnosticParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        identifier: None,
        previous_result_id: None,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let _diagnostic_result = server.diagnostic(diagnostic_params).await.unwrap();

    // 5. Request code actions for the first line
    let code_action_params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range: Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 25 },
        },
        context: CodeActionContext {
            diagnostics: vec![],
            only: None,
            trigger_kind: None,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let code_action_result = server.code_action(code_action_params).await;
    assert!(code_action_result.is_ok());

    // 6. Modify document
    let change_params = DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: uri.clone(),
            version: 2,
        },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "# Fixed Heading\n\nShort line.\n\nAnother paragraph.".to_string(),
        }],
    };

    server.did_change(change_params).await;

    // 7. Save document
    let save_params = DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        text: None,
    };

    server.did_save(save_params).await;

    // 8. Close document
    let close_params = DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier { uri },
    };

    server.did_close(close_params).await;

    // 9. Shutdown
    let shutdown_result = server.shutdown().await;
    assert!(shutdown_result.is_ok());
}

#[cfg(test)]
mod edge_cases {
    use super::*;

    /// Test warning conversion with missing rule name
    #[test]
    fn test_warning_to_diagnostic_no_rule_name() {
        let warning = LintWarning {
            message: "Test message".to_string(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            severity: Severity::Warning,
            fix: None,
            rule_name: None,
        };

        let diagnostic = warning_to_diagnostic(&warning);
        assert_eq!(diagnostic.code, None);
    }

    /// Test warning conversion with line/column at zero
    #[test]
    fn test_warning_to_diagnostic_zero_position() {
        let warning = LintWarning {
            message: "Test message".to_string(),
            line: 0,
            column: 0,
            end_line: 0,
            end_column: 5,
            severity: Severity::Warning,
            fix: None,
            rule_name: Some("MD001"),
        };

        let diagnostic = warning_to_diagnostic(&warning);
        // Should handle edge case gracefully
        assert_eq!(diagnostic.range.start.line, 0);
        assert_eq!(diagnostic.range.start.character, 0);
    }

    /// Test empty document handling
    #[tokio::test]
    async fn test_empty_document_handling() {
        let (service, _socket) = LspService::new(RumdlLanguageServer::new);
        let server = service.inner();

        // Initialize
        let init_params = InitializeParams {
            process_id: None,
            root_path: None, // Deprecated but required
            root_uri: Some(Url::parse("file:///test").unwrap()),
            initialization_options: None,
            capabilities: ClientCapabilities::default(),
            trace: None,
            workspace_folders: None,
            client_info: None,
            locale: None,
        };

        server.initialize(init_params).await.unwrap();
        server.initialized(InitializedParams {}).await;

        // Open empty document
        let uri = Url::parse("file:///empty.md").unwrap();
        let open_params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "markdown".to_string(),
                version: 1,
                text: "".to_string(),
            },
        };

        server.did_open(open_params).await;

        // Request diagnostics for empty document
        let diagnostic_params = DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier { uri },
            identifier: None,
            previous_result_id: None,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let result = server.diagnostic(diagnostic_params).await;
        assert!(result.is_ok());
    }
}
