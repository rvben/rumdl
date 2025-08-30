//! Test that LSP properly handles initialization options for configuration
//! This verifies that VSCode extension settings are properly passed through

#![allow(deprecated)] // root_path is deprecated but required for InitializeParams

use tower_lsp::lsp_types::*;
use tower_lsp::{LanguageServer, LspService};

use rumdl_lib::lsp::RumdlLspConfig;
use rumdl_lib::lsp::server::RumdlLanguageServer;

/// Test that initialization options are properly handled
#[tokio::test]
async fn test_lsp_initialization_options() {
    let (service, _socket) = LspService::new(RumdlLanguageServer::new);

    // Create initialization options that would come from VSCode
    let lsp_config = RumdlLspConfig {
        config_path: Some("/path/to/config.toml".to_string()),
        enable_linting: true,
        enable_auto_fix: false,
        enable_rules: Some(vec!["MD001".to_string(), "MD002".to_string()]),
        disable_rules: Some(vec!["MD013".to_string()]),
    };

    let init_params = InitializeParams {
        process_id: None,
        root_path: None,
        root_uri: Some(Url::parse("file:///test").unwrap()),
        initialization_options: Some(serde_json::to_value(lsp_config).unwrap()),
        capabilities: ClientCapabilities::default(),
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    let result = service.inner().initialize(init_params).await;
    assert!(result.is_ok(), "Initialization with options should succeed");

    // The server should now be configured with the provided options
    service.inner().initialized(InitializedParams {}).await;
}

/// Test that enable_rules properly filters rules
#[tokio::test]
async fn test_enable_rules_filtering() {
    let (service, _socket) = LspService::new(RumdlLanguageServer::new);

    // Only enable MD001 and MD018
    let lsp_config = RumdlLspConfig {
        config_path: None,
        enable_linting: true,
        enable_auto_fix: false,
        enable_rules: Some(vec!["MD001".to_string(), "MD018".to_string()]),
        disable_rules: None,
    };

    let init_params = InitializeParams {
        process_id: None,
        root_path: None,
        root_uri: Some(Url::parse("file:///test").unwrap()),
        initialization_options: Some(serde_json::to_value(lsp_config).unwrap()),
        capabilities: ClientCapabilities::default(),
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    service.inner().initialize(init_params).await.unwrap();
    service.inner().initialized(InitializedParams {}).await;

    // Open a document with multiple issues
    let uri = Url::parse("file:///test/select.md").unwrap();
    let text = r#"#Missing space after hash (MD018)

## Heading 2

This line is way too long and should trigger MD013 but it's not in enable_rules so it should be ignored completely."#;

    service
        .inner()
        .did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "markdown".to_string(),
                version: 1,
                text: text.to_string(),
            },
        })
        .await;

    // Request diagnostics
    let diag_params = DocumentDiagnosticParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        identifier: None,
        previous_result_id: None,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = service.inner().diagnostic(diag_params).await;
    assert!(result.is_ok(), "Diagnostic request should succeed");

    if let Ok(DocumentDiagnosticReportResult::Report(report)) = result {
        match report {
            DocumentDiagnosticReport::Full(full_report) => {
                let diagnostics = full_report.full_document_diagnostic_report.items;

                // Should only have MD018 diagnostic, not MD013
                assert!(
                    diagnostics
                        .iter()
                        .any(|d| d.code == Some(NumberOrString::String("MD018".to_string()))),
                    "Should have MD018 diagnostic"
                );
                assert!(
                    !diagnostics
                        .iter()
                        .any(|d| d.code == Some(NumberOrString::String("MD013".to_string()))),
                    "Should NOT have MD013 diagnostic as it's not in enable_rules"
                );
            }
            _ => panic!("Expected full diagnostic report"),
        }
    }
}

/// Test that disable_rules properly filters out rules
#[tokio::test]
async fn test_disable_rules_filtering() {
    let (service, _socket) = LspService::new(RumdlLanguageServer::new);

    // Ignore MD013 (line length) and MD018 (no space after hash)
    let lsp_config = RumdlLspConfig {
        config_path: None,
        enable_linting: true,
        enable_auto_fix: false,
        enable_rules: None,
        disable_rules: Some(vec!["MD013".to_string(), "MD018".to_string()]),
    };

    let init_params = InitializeParams {
        process_id: None,
        root_path: None,
        root_uri: Some(Url::parse("file:///test").unwrap()),
        initialization_options: Some(serde_json::to_value(lsp_config).unwrap()),
        capabilities: ClientCapabilities::default(),
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    service.inner().initialize(init_params).await.unwrap();
    service.inner().initialized(InitializedParams {}).await;

    // Open a document with issues that should be ignored
    let uri = Url::parse("file:///test/ignore.md").unwrap();
    let text = r#"#Missing space (MD018 - should be ignored)

This line is way too long and would normally trigger MD013 but it should be ignored due to disable_rules configuration."#;

    service
        .inner()
        .did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "markdown".to_string(),
                version: 1,
                text: text.to_string(),
            },
        })
        .await;

    // Request diagnostics
    let diag_params = DocumentDiagnosticParams {
        text_document: TextDocumentIdentifier { uri },
        identifier: None,
        previous_result_id: None,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = service.inner().diagnostic(diag_params).await;
    assert!(result.is_ok(), "Diagnostic request should succeed");

    if let Ok(DocumentDiagnosticReportResult::Report(report)) = result {
        match report {
            DocumentDiagnosticReport::Full(full_report) => {
                let diagnostics = full_report.full_document_diagnostic_report.items;

                // Should not have MD013 or MD018 diagnostics as they're ignored
                assert!(
                    !diagnostics
                        .iter()
                        .any(|d| d.code == Some(NumberOrString::String("MD013".to_string()))),
                    "Should NOT have MD013 diagnostic as it's in disable_rules"
                );
                assert!(
                    !diagnostics
                        .iter()
                        .any(|d| d.code == Some(NumberOrString::String("MD018".to_string()))),
                    "Should NOT have MD018 diagnostic as it's in disable_rules"
                );
            }
            _ => panic!("Expected full diagnostic report"),
        }
    }
}

/// Test that both enable_rules and disable_rules work together
#[tokio::test]
async fn test_select_and_disable_rules_together() {
    let (service, _socket) = LspService::new(RumdlLanguageServer::new);

    // Select MD001, MD013, MD018 but ignore MD013
    // Result: only MD001 and MD018 should be active
    let lsp_config = RumdlLspConfig {
        config_path: None,
        enable_linting: true,
        enable_auto_fix: false,
        enable_rules: Some(vec!["MD001".to_string(), "MD013".to_string(), "MD018".to_string()]),
        disable_rules: Some(vec!["MD013".to_string()]),
    };

    let init_params = InitializeParams {
        process_id: None,
        root_path: None,
        root_uri: Some(Url::parse("file:///test").unwrap()),
        initialization_options: Some(serde_json::to_value(lsp_config).unwrap()),
        capabilities: ClientCapabilities::default(),
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    service.inner().initialize(init_params).await.unwrap();
    service.inner().initialized(InitializedParams {}).await;

    // Open a document with various issues
    let uri = Url::parse("file:///test/combined.md").unwrap();
    let text = r#"#Missing space (MD018)

This line is way too long and would trigger MD013 but it's in disable_rules so should be filtered out even though it's in enable_rules."#;

    service
        .inner()
        .did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "markdown".to_string(),
                version: 1,
                text: text.to_string(),
            },
        })
        .await;

    // Request diagnostics
    let diag_params = DocumentDiagnosticParams {
        text_document: TextDocumentIdentifier { uri },
        identifier: None,
        previous_result_id: None,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = service.inner().diagnostic(diag_params).await;
    assert!(result.is_ok(), "Diagnostic request should succeed");

    if let Ok(DocumentDiagnosticReportResult::Report(report)) = result {
        match report {
            DocumentDiagnosticReport::Full(full_report) => {
                let diagnostics = full_report.full_document_diagnostic_report.items;

                // Should have MD018 but not MD013
                assert!(
                    diagnostics
                        .iter()
                        .any(|d| d.code == Some(NumberOrString::String("MD018".to_string()))),
                    "Should have MD018 diagnostic as it's in enable_rules and not ignored"
                );
                assert!(
                    !diagnostics
                        .iter()
                        .any(|d| d.code == Some(NumberOrString::String("MD013".to_string()))),
                    "Should NOT have MD013 diagnostic as it's in disable_rules (even though also in enable_rules)"
                );
            }
            _ => panic!("Expected full diagnostic report"),
        }
    }
}
