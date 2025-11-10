#![allow(deprecated)] // root_path is deprecated but required for InitializeParams

/// Test for LSP formatting on unopened documents
/// This test verifies that formatting works even when textDocument/didOpen
/// hasn't been called first, which is the behavior some LSP clients expect
use rumdl_lib::lsp::server::RumdlLanguageServer;
use tower_lsp::lsp_types::*;
use tower_lsp::{LanguageServer, LspService};

#[tokio::test]
async fn test_formatting_without_did_open() {
    // Create LSP service
    let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));

    // Initialize the server
    let init_params = InitializeParams {
        capabilities: ClientCapabilities::default(),
        initialization_options: None,
        process_id: Some(1),
        root_path: None,
        root_uri: None,
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    let _init_result = service.inner().initialize(init_params).await.unwrap();

    // Create a test markdown file on disk
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_unopened.md");
    let test_content = "#Missing space\n\nTrailing spaces   \n";
    std::fs::write(&test_file, test_content).unwrap();

    // Create file URI
    let uri = Url::from_file_path(&test_file).unwrap();

    // IMPORTANT: We do NOT call did_open here - this is the test scenario

    // Request formatting directly without opening the document
    let formatting_params = DocumentFormattingParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        options: FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            properties: std::collections::HashMap::new(),
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };

    let result = service.inner().formatting(formatting_params).await;

    // Clean up
    std::fs::remove_file(&test_file).ok();

    // Verify the result
    assert!(result.is_ok(), "Formatting should not error");

    let edits = result.unwrap();
    assert!(edits.is_some(), "Should return Some(edits), not None");

    let edits = edits.unwrap();

    // THE BUG: Currently this returns an empty array because the document
    // isn't in the cache. It SHOULD read from disk and return actual edits.

    // This assertion currently FAILS, demonstrating the bug
    assert!(
        !edits.is_empty(),
        "Should return formatting edits for the file on disk, but got empty array"
    );

    // If it worked correctly, we should get one edit that fixes the issues
    if !edits.is_empty() {
        assert_eq!(edits.len(), 1, "Should have one edit replacing the document");
        let edit = &edits[0];
        assert!(edit.new_text.contains("# Missing space"), "Should fix the heading");
        assert!(!edit.new_text.contains("   \n"), "Should remove trailing spaces");
        assert!(edit.new_text.ends_with('\n'), "Should add final newline");
    }
}

#[tokio::test]
async fn test_formatting_with_did_open_still_works() {
    // This test ensures our fix doesn't break the existing functionality

    let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));

    let init_params = InitializeParams {
        capabilities: ClientCapabilities::default(),
        initialization_options: None,
        process_id: Some(1),
        root_path: None,
        root_uri: None,
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    let _init_result = service.inner().initialize(init_params).await.unwrap();

    let uri = Url::parse("file:///test/opened.md").unwrap();
    let text = "#Missing space\n\nTrailing spaces   \n";

    // This time we DO call did_open
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

    // Request formatting
    let formatting_params = DocumentFormattingParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        options: FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            properties: std::collections::HashMap::new(),
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };

    let result = service.inner().formatting(formatting_params).await;

    assert!(result.is_ok(), "Formatting should succeed");
    let edits = result.unwrap();
    assert!(edits.is_some(), "Should return Some(edits)");
    let edits = edits.unwrap();

    // This should work (and currently does)
    assert!(!edits.is_empty(), "Should return formatting edits");
    assert_eq!(edits.len(), 1, "Should have one edit");

    let edit = &edits[0];
    assert!(edit.new_text.contains("# Missing space"), "Should fix the heading");
    assert!(!edit.new_text.contains("   \n"), "Should remove trailing spaces");
    assert!(edit.new_text.ends_with('\n'), "Should add final newline");
}

#[tokio::test]
async fn test_formatting_nonexistent_file() {
    // Test that formatting a non-existent file returns empty array, not error

    let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));

    let init_params = InitializeParams {
        capabilities: ClientCapabilities::default(),
        initialization_options: None,
        process_id: Some(1),
        root_path: None,
        root_uri: None,
        trace: None,
        workspace_folders: None,
        client_info: None,
        locale: None,
    };

    let _init_result = service.inner().initialize(init_params).await.unwrap();

    let uri = Url::parse("file:///nonexistent/file.md").unwrap();

    let formatting_params = DocumentFormattingParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        options: FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            properties: std::collections::HashMap::new(),
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };

    let result = service.inner().formatting(formatting_params).await;

    assert!(result.is_ok(), "Should not error on non-existent file");
    let edits = result.unwrap();
    assert!(edits.is_none(), "Should return None for non-existent file per LSP spec");
}
