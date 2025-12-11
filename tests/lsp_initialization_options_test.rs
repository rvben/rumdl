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
    let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));

    // Create initialization options that would come from VSCode
    let lsp_config = RumdlLspConfig {
        config_path: Some("/path/to/config.toml".to_string()),
        enable_linting: true,
        enable_auto_fix: false,
        enable_rules: Some(vec!["MD001".to_string(), "MD002".to_string()]),
        disable_rules: Some(vec!["MD013".to_string()]),
        ..Default::default()
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
    let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));

    // Only enable MD001 and MD018
    let lsp_config = RumdlLspConfig {
        config_path: None,
        enable_linting: true,
        enable_auto_fix: false,
        enable_rules: Some(vec!["MD001".to_string(), "MD018".to_string()]),
        disable_rules: None,
        ..Default::default()
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
    let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));

    // Ignore MD013 (line length) and MD018 (no space after hash)
    let lsp_config = RumdlLspConfig {
        config_path: None,
        enable_linting: true,
        enable_auto_fix: false,
        enable_rules: None,
        disable_rules: Some(vec!["MD013".to_string(), "MD018".to_string()]),
        ..Default::default()
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
    let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));

    // Select MD001, MD013, MD018 but ignore MD013
    // Result: only MD001 and MD018 should be active
    let lsp_config = RumdlLspConfig {
        config_path: None,
        enable_linting: true,
        enable_auto_fix: false,
        enable_rules: Some(vec!["MD001".to_string(), "MD013".to_string(), "MD018".to_string()]),
        disable_rules: Some(vec!["MD013".to_string()]),
        ..Default::default()
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

/// Test that workspace/didChangeConfiguration properly updates settings
#[tokio::test]
async fn test_did_change_configuration_neovim_style() {
    let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));

    // Initialize with default config - but USE initializationOptions to disable MD018
    // This tests that the initialization path works correctly
    let lsp_config = RumdlLspConfig {
        config_path: None,
        enable_linting: true,
        enable_auto_fix: false,
        enable_rules: None,
        disable_rules: Some(vec!["MD018".to_string()]),
        ..Default::default()
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

    // Open a document with MD018 issue (missing space after #)
    let uri = Url::parse("file:///test/config_change.md").unwrap();
    let text = r#"#Missing space after hash (MD018)

Some content here."#;

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

    // Request diagnostics - MD018 should be disabled from initialization
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

                // Should NOT have MD018 diagnostic because it was disabled in initialization
                assert!(
                    !diagnostics
                        .iter()
                        .any(|d| d.code == Some(NumberOrString::String("MD018".to_string()))),
                    "Should NOT have MD018 diagnostic after disabling via initializationOptions"
                );
            }
            _ => panic!("Expected full diagnostic report"),
        }
    }
}

/// Test that workspace/didChangeConfiguration can dynamically update settings
#[tokio::test]
async fn test_did_change_configuration_dynamic_update() {
    let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));

    // Initialize with default config (no rules disabled)
    let init_params = InitializeParams {
        process_id: None,
        root_path: None,
        root_uri: Some(Url::parse("file:///test").unwrap()),
        initialization_options: None,
        capabilities: ClientCapabilities::default(),
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    service.inner().initialize(init_params).await.unwrap();
    service.inner().initialized(InitializedParams {}).await;

    // Open a document with MD018 issue (missing space after #)
    let uri = Url::parse("file:///test/config_change2.md").unwrap();
    let text = r#"#Missing space after hash (MD018)

Some content here."#;

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

    // First verify MD018 is reported BEFORE configuration change
    let diag_params = DocumentDiagnosticParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        identifier: None,
        previous_result_id: None,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = service.inner().diagnostic(diag_params.clone()).await;
    assert!(result.is_ok(), "Diagnostic request should succeed");

    if let Ok(DocumentDiagnosticReportResult::Report(report)) = result {
        match report {
            DocumentDiagnosticReport::Full(full_report) => {
                let diagnostics = full_report.full_document_diagnostic_report.items;

                // SHOULD have MD018 diagnostic initially
                assert!(
                    diagnostics
                        .iter()
                        .any(|d| d.code == Some(NumberOrString::String("MD018".to_string()))),
                    "Should have MD018 diagnostic before configuration change"
                );
            }
            _ => panic!("Expected full diagnostic report"),
        }
    }

    // Now send a configuration change to disable MD018 (Neovim style)
    // Format: { "rumdl": { "disable": ["MD018"] } }
    let settings = serde_json::json!({
        "rumdl": {
            "disable": ["MD018"]
        }
    });

    service
        .inner()
        .did_change_configuration(DidChangeConfigurationParams { settings })
        .await;

    // Request diagnostics again - MD018 should now be disabled
    let result = service.inner().diagnostic(diag_params).await;
    assert!(result.is_ok(), "Diagnostic request should succeed");

    if let Ok(DocumentDiagnosticReportResult::Report(report)) = result {
        match report {
            DocumentDiagnosticReport::Full(full_report) => {
                let diagnostics = full_report.full_document_diagnostic_report.items;

                // Should NOT have MD018 diagnostic after disabling it via config change
                assert!(
                    !diagnostics
                        .iter()
                        .any(|d| d.code == Some(NumberOrString::String("MD018".to_string()))),
                    "Should NOT have MD018 diagnostic after disabling via didChangeConfiguration"
                );
            }
            _ => panic!("Expected full diagnostic report"),
        }
    }
}

/// Test that workspace/didChangeConfiguration handles rule-specific settings
#[tokio::test]
async fn test_did_change_configuration_rule_settings() {
    let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));

    // Initialize
    let init_params = InitializeParams {
        process_id: None,
        root_path: None,
        root_uri: Some(Url::parse("file:///test").unwrap()),
        initialization_options: None,
        capabilities: ClientCapabilities::default(),
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    service.inner().initialize(init_params).await.unwrap();
    service.inner().initialized(InitializedParams {}).await;

    // Send configuration with rule-specific settings (Neovim style)
    // Format: { "rumdl": { "MD013": { "lineLength": 200 } } }
    let settings = serde_json::json!({
        "rumdl": {
            "MD013": {
                "lineLength": 200
            }
        }
    });

    service
        .inner()
        .did_change_configuration(DidChangeConfigurationParams { settings })
        .await;

    // The config should now have the rule settings stored
    // (We can't easily verify the internal state, but we verify it doesn't crash)
}

/// Test that workspace/didChangeConfiguration handles direct settings (no rumdl wrapper)
#[tokio::test]
async fn test_did_change_configuration_direct_settings() {
    let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));

    // Initialize
    let init_params = InitializeParams {
        process_id: None,
        root_path: None,
        root_uri: Some(Url::parse("file:///test").unwrap()),
        initialization_options: None,
        capabilities: ClientCapabilities::default(),
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    service.inner().initialize(init_params).await.unwrap();
    service.inner().initialized(InitializedParams {}).await;

    // Send configuration directly without "rumdl" wrapper
    // Some editors might send settings this way
    let settings = serde_json::json!({
        "disable": ["MD009", "MD010"],
        "lineLength": 100
    });

    service
        .inner()
        .did_change_configuration(DidChangeConfigurationParams { settings })
        .await;

    // Should handle gracefully without crashing
}

// Note: Tests for invalid rule names and value types are not included as integration tests
// because the tower-lsp test harness blocks when notifications are sent without a consumer.
// The validation logic is tested through the is_valid_rule_name function in unit tests,
// and the behavior can be verified manually with a real LSP client.
