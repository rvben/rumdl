use super::*;
use crate::lsp::types::{warning_to_code_actions, warning_to_diagnostic};
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

    let diagnostics = server.lint_document(&uri, text, true).await.unwrap();

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

    let diagnostics = server.lint_document(&uri, text, true).await.unwrap();

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
async fn test_source_fix_all_with_single_fixable_issue() {
    let server = create_test_server();

    let uri = Url::parse("file:///test.md").unwrap();
    // Content with exactly 1 fixable issue: missing final newline (MD047)
    let text = "# Test";

    let range = Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: 0, character: 6 },
    };

    let actions = server.get_code_actions(&uri, text, range).await.unwrap();

    let fix_all_actions: Vec<_> = actions
        .iter()
        .filter(|a| a.kind.as_ref().is_some_and(|k| k.as_str() == "source.fixAll.rumdl"))
        .collect();

    assert!(
        !fix_all_actions.is_empty(),
        "source.fixAll.rumdl should be available even with a single fixable issue"
    );
}

#[tokio::test]
async fn test_get_code_actions_outside_range() {
    let server = create_test_server();

    let uri = Url::parse("file:///test.md").unwrap();
    // Line 2 and 3 have hard tabs (MD010, fixable), range only covers line 0
    let text = "# Test\n\n\tThis is a test\n\tWith tabs\n";

    // Range that doesn't cover the violations (line 0 only)
    let range = Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: 0, character: 6 },
    };

    let actions = server.get_code_actions(&uri, text, range).await.unwrap();

    // Per-warning actions should not appear for this range
    let per_warning_actions: Vec<_> = actions
        .iter()
        .filter(|a| a.kind.as_ref().is_some_and(|k| k.as_str() != "source.fixAll.rumdl"))
        .collect();
    assert!(
        per_warning_actions.is_empty(),
        "No per-warning actions for out-of-range lines"
    );

    // source.fixAll.rumdl is document-wide, so it should still appear
    let fix_all_actions: Vec<_> = actions
        .iter()
        .filter(|a| a.kind.as_ref().is_some_and(|k| k.as_str() == "source.fixAll.rumdl"))
        .collect();
    assert!(
        !fix_all_actions.is_empty(),
        "fixAll is document-wide and should appear regardless of requested range"
    );
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
    let diagnostics = server.lint_document(&uri, text, true).await.unwrap();
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
/// 1. LSP settings (JSON -> apply_rule_config)
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

    // Multiple trailing newlines -> exactly one
    let result = RumdlLanguageServer::apply_formatting_options("hello\n\n\n".to_string(), &options);
    assert_eq!(result, "hello\n");

    // No trailing newline -> add one
    let result = RumdlLanguageServer::apply_formatting_options("hello".to_string(), &options);
    assert_eq!(result, "hello\n");

    // Already has exactly one -> unchanged
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

// ==================== Completion Tests ====================

#[test]
fn test_detect_code_fence_language_position_basic() {
    // Basic case: cursor right after ```
    let text = "```\ncode\n```";
    let pos = Position { line: 0, character: 3 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(result.is_some());
    let (start_col, current_text) = result.unwrap();
    assert_eq!(start_col, 3);
    assert_eq!(current_text, "");
}

#[test]
fn test_detect_code_fence_language_position_partial_lang() {
    // Cursor in the middle of typing a language
    let text = "```py\ncode\n```";
    let pos = Position { line: 0, character: 5 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(result.is_some());
    let (start_col, current_text) = result.unwrap();
    assert_eq!(start_col, 3);
    assert_eq!(current_text, "py");
}

#[test]
fn test_detect_code_fence_language_position_full_lang() {
    // Cursor at end of language tag
    let text = "```python\ncode\n```";
    let pos = Position { line: 0, character: 9 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(result.is_some());
    let (start_col, current_text) = result.unwrap();
    assert_eq!(start_col, 3);
    assert_eq!(current_text, "python");
}

#[test]
fn test_detect_code_fence_language_position_tilde_fence() {
    // Using ~~~ instead of ```
    let text = "~~~rust\ncode\n~~~";
    let pos = Position { line: 0, character: 7 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(result.is_some());
    let (start_col, current_text) = result.unwrap();
    assert_eq!(start_col, 3);
    assert_eq!(current_text, "rust");
}

#[test]
fn test_detect_code_fence_language_position_indented() {
    // Indented code fence
    let text = "  ```js\ncode\n  ```";
    let pos = Position { line: 0, character: 7 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(result.is_some());
    let (start_col, current_text) = result.unwrap();
    assert_eq!(start_col, 5); // 2 spaces + 3 backticks
    assert_eq!(current_text, "js");
}

#[test]
fn test_detect_code_fence_language_position_not_fence_line() {
    // Not on a fence line (inside code block content)
    let text = "```python\ncode\n```";
    let pos = Position { line: 1, character: 2 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(result.is_none());
}

#[test]
fn test_detect_code_fence_language_position_closing_fence() {
    // On closing fence - should NOT trigger completion
    let text = "```python\ncode\n```";
    let pos = Position { line: 2, character: 3 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    // Closing fence should return None (no completion on closing fences)
    assert!(result.is_none(), "Should not offer completion on closing fence");
}

#[test]
fn test_detect_code_fence_language_position_extended_fence() {
    // Extended fence with 4 backticks
    let text = "````python\ncode\n````";
    let pos = Position { line: 0, character: 10 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(result.is_some());
    let (start_col, current_text) = result.unwrap();
    assert_eq!(start_col, 4); // 4 backticks
    assert_eq!(current_text, "python");
}

#[test]
fn test_detect_code_fence_language_position_extended_fence_5_backticks() {
    // Extended fence with 5 backticks
    let text = "`````js\ncode\n`````";
    let pos = Position { line: 0, character: 7 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(result.is_some());
    let (start_col, current_text) = result.unwrap();
    assert_eq!(start_col, 5);
    assert_eq!(current_text, "js");
}

#[test]
fn test_detect_code_fence_language_position_nested_code_blocks() {
    // Nested code block (documenting markdown in markdown)
    // Outer: 4 backticks, Inner: 3 backticks
    let text = "````markdown\n```python\ncode\n```\n````";

    // Opening fence of outer block
    let pos = Position { line: 0, character: 12 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(result.is_some());
    let (_, current_text) = result.unwrap();
    assert_eq!(current_text, "markdown");

    // Inner opening fence - should be treated as content (we're inside outer block)
    // Note: This is actually content of the outer block, not a real code fence
    // The detection is line-based and doesn't have full context, so it will detect it
    // This is acceptable behavior - editors typically don't complete inside code blocks anyway
}

#[test]
fn test_detect_code_fence_language_position_extended_closing_fence() {
    // Extended closing fence should not trigger completion
    let text = "````python\ncode here\n````";
    let pos = Position { line: 2, character: 4 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(
        result.is_none(),
        "Should not offer completion on extended closing fence"
    );
}

#[test]
fn test_detect_code_fence_language_position_cursor_before_fence() {
    // Cursor before the fence characters
    let text = "```python\ncode\n```";
    let pos = Position { line: 0, character: 2 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(result.is_none());
}

#[test]
fn test_detect_code_fence_language_position_with_info_string() {
    // Info string with space (should not complete after space)
    let text = "```python filename.py\ncode\n```";
    let pos = Position { line: 0, character: 15 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    // Should return None because cursor is after a space
    assert!(result.is_none());
}

#[test]
fn test_detect_code_fence_language_position_regular_text() {
    // Regular markdown text (not a code fence)
    let text = "# Heading\n\nSome text.";
    let pos = Position { line: 0, character: 5 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(result.is_none());
}

#[test]
fn test_detect_code_fence_language_position_non_ascii_language() {
    // "é" is U+00E9: 2 UTF-8 bytes, 1 UTF-16 code unit.
    // "```résumé" — fence ends at byte/UTF-16 col 3.
    // "résumé" = r(1)+é(1)+s(1)+u(1)+m(1)+é(1) = 6 UTF-16 units, 8 UTF-8 bytes.
    // UTF-16 cursor at 9 (3 + 6); byte offset is 11 (3 + 8).
    // Old code: &line[3..9] slices into the middle of the second é → panic.
    // Fixed code: converts UTF-16 9 → byte 11 first.
    let text = "```résumé";
    let pos = Position { line: 0, character: 9 }; // UTF-16 cursor at end of "résumé"
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(result.is_some(), "should detect fence language with non-ASCII text");
    let (start_col, current_text) = result.unwrap();
    assert_eq!(start_col, 3); // fence_end is always ASCII, so col == byte offset
    assert_eq!(current_text, "résumé");
}

#[test]
fn test_detect_code_fence_language_position_inline_code() {
    // Inline code (not a fenced block)
    let text = "Use `code` here.";
    let pos = Position { line: 0, character: 5 };
    let result = RumdlLanguageServer::detect_code_fence_language_position(text, pos);
    assert!(result.is_none());
}

#[tokio::test]
async fn test_completion_provides_language_items() {
    use std::fs;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    fs::write(&test_file, "```py\ncode\n```").unwrap();

    let server = create_test_server();
    let uri = Url::from_file_path(&test_file).unwrap();

    // Open the document
    let content = "```py\ncode\n```".to_string();
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.clone(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Get completions at position after ```
    let items = server
        .get_language_completions(&uri, "py", 3, Position { line: 0, character: 5 })
        .await;

    // Should have python-related items
    assert!(!items.is_empty(), "Should return completion items");

    // Check that python is in the results
    let has_python = items.iter().any(|item| item.label.to_lowercase() == "python");
    assert!(has_python, "Should include 'python' as a completion item");
}

#[tokio::test]
async fn test_completion_filters_by_prefix() {
    let temp_dir = tempfile::tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    std::fs::write(&test_file, "```ru\ncode\n```").unwrap();

    let server = create_test_server();
    let uri = Url::from_file_path(&test_file).unwrap();

    // Get completions filtered by "ru"
    let items = server
        .get_language_completions(&uri, "ru", 3, Position { line: 0, character: 5 })
        .await;

    // All items should start with "ru"
    for item in &items {
        assert!(
            item.label.to_lowercase().starts_with("ru"),
            "Completion '{}' should start with 'ru'",
            item.label
        );
    }

    // Should include rust and ruby
    let has_rust = items.iter().any(|item| item.label.to_lowercase() == "rust");
    let has_ruby = items.iter().any(|item| item.label.to_lowercase() == "ruby");
    assert!(has_rust, "Should include 'rust'");
    assert!(has_ruby, "Should include 'ruby'");
}

#[tokio::test]
async fn test_completion_empty_prefix_returns_all() {
    let temp_dir = tempfile::tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    std::fs::write(&test_file, "```\ncode\n```").unwrap();

    let server = create_test_server();
    let uri = Url::from_file_path(&test_file).unwrap();

    // Get completions with empty prefix
    let items = server
        .get_language_completions(&uri, "", 3, Position { line: 0, character: 3 })
        .await;

    // Should have many items (up to the limit of 100)
    assert!(items.len() >= 10, "Should return multiple language options");
    assert!(items.len() <= 100, "Should be limited to 100 items");
}

#[tokio::test]
async fn test_completion_respects_md040_allowed_languages() {
    use std::fs;

    let temp_dir = tempfile::tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    fs::write(&test_file, "```\ncode\n```").unwrap();

    // Create config with allowed_languages
    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(
        &config_file,
        r#"
[MD040]
allowed-languages = ["Python", "Rust", "Go"]
"#,
    )
    .unwrap();

    let server = create_test_server();

    // Set workspace root so config is discovered
    {
        let mut roots = server.workspace_roots.write().await;
        roots.push(temp_dir.path().to_path_buf());
    }

    let uri = Url::from_file_path(&test_file).unwrap();

    // Get completions
    let items = server
        .get_language_completions(&uri, "", 3, Position { line: 0, character: 3 })
        .await;

    // Should only have items for Python, Rust, Go and their aliases
    for item in &items {
        let label_lower = item.label.to_lowercase();
        let detail = item.detail.as_ref().map(|d| d.to_lowercase()).unwrap_or_default();

        // Check that the canonical language (in detail) is one of the allowed ones
        let is_allowed = detail.contains("python") || detail.contains("rust") || detail.contains("go");
        assert!(
            is_allowed,
            "Completion '{label_lower}' (detail: '{detail}') should be for Python, Rust, or Go"
        );
    }
}

#[tokio::test]
async fn test_completion_respects_md040_disallowed_languages() {
    use std::fs;

    let temp_dir = tempfile::tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    fs::write(&test_file, "```py\ncode\n```").unwrap();

    // Create config with disallowed_languages
    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(
        &config_file,
        r#"
[MD040]
disallowed-languages = ["Python"]
"#,
    )
    .unwrap();

    let server = create_test_server();

    // Set workspace root so config is discovered
    {
        let mut roots = server.workspace_roots.write().await;
        roots.push(temp_dir.path().to_path_buf());
    }

    let uri = Url::from_file_path(&test_file).unwrap();

    // Get completions filtered by "py"
    let items = server
        .get_language_completions(&uri, "py", 3, Position { line: 0, character: 5 })
        .await;

    // Should NOT include Python or py
    for item in &items {
        let detail = item.detail.as_ref().map(|d| d.to_lowercase()).unwrap_or_default();
        assert!(
            !detail.contains("python"),
            "Completion '{}' should not include Python (disallowed)",
            item.label
        );
    }
}

#[test]
fn test_is_closing_fence_basic() {
    // Opening fence only - the next fence IS a closing fence
    // (markdown spec: opening fence creates a code block that needs closing)
    let lines = vec!["```python"];
    assert!(
        RumdlLanguageServer::is_closing_fence(&lines, '`', 3),
        "After opening fence, next fence is closing"
    );
}

#[test]
fn test_is_closing_fence_with_content() {
    // Opening fence with content - next fence would be closing
    let lines = vec!["```python", "some code"];
    assert!(
        RumdlLanguageServer::is_closing_fence(&lines, '`', 3),
        "After opening fence with content, next fence is closing"
    );
}

#[test]
fn test_is_closing_fence_no_prior_fence() {
    // No prior fence - next fence is opening
    let lines: Vec<&str> = vec!["# Hello", "Some text"];
    assert!(
        !RumdlLanguageServer::is_closing_fence(&lines, '`', 3),
        "With no prior fence, next fence is opening"
    );
}

#[test]
fn test_is_closing_fence_already_closed() {
    // Closed code block - next fence would be opening
    let lines = vec!["```python", "some code", "```"];
    assert!(
        !RumdlLanguageServer::is_closing_fence(&lines, '`', 3),
        "After closed code block, next fence is opening"
    );
}

#[test]
fn test_is_closing_fence_extended() {
    // Extended fence - needs matching or longer fence to close
    let lines = vec!["````python", "some code"];
    // 3 backticks won't close 4-backtick fence
    assert!(
        !RumdlLanguageServer::is_closing_fence(&lines, '`', 3),
        "3 backticks cannot close 4-backtick fence"
    );
    // 4 backticks will close
    assert!(
        RumdlLanguageServer::is_closing_fence(&lines, '`', 4),
        "4 backticks can close 4-backtick fence"
    );
    // 5 backticks will also close (>= rule)
    assert!(
        RumdlLanguageServer::is_closing_fence(&lines, '`', 5),
        "5 backticks can close 4-backtick fence"
    );
}

#[test]
fn test_is_closing_fence_mixed_chars() {
    // Tilde fence cannot be closed by backtick fence
    let lines = vec!["~~~python", "some code"];
    assert!(
        !RumdlLanguageServer::is_closing_fence(&lines, '`', 3),
        "Backtick fence cannot close tilde fence"
    );
    assert!(
        RumdlLanguageServer::is_closing_fence(&lines, '~', 3),
        "Tilde fence can close tilde fence"
    );
}

#[tokio::test]
async fn test_completion_method_integration() {
    use std::fs;

    let temp_dir = tempfile::tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    let content = "# Hello\n\n```py\nprint('hi')\n```";
    fs::write(&test_file, content).unwrap();

    let server = create_test_server();
    let uri = Url::from_file_path(&test_file).unwrap();

    // Open the document
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Call completion method directly
    let params = CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position { line: 2, character: 5 }, // After ```py
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    };

    let result = server.completion(params).await.unwrap();
    assert!(result.is_some(), "Completion should return items");

    if let Some(CompletionResponse::Array(items)) = result {
        assert!(!items.is_empty(), "Should have completion items");
        // Check python is in the results
        let has_python = items.iter().any(|i| i.label.to_lowercase() == "python");
        assert!(has_python, "Should include python as completion");
    } else {
        panic!("Expected CompletionResponse::Array");
    }
}

#[tokio::test]
async fn test_completion_not_triggered_on_closing_fence() {
    use std::fs;

    let temp_dir = tempfile::tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    let content = "```python\nprint('hi')\n```";
    fs::write(&test_file, content).unwrap();

    let server = create_test_server();
    let uri = Url::from_file_path(&test_file).unwrap();

    // Open the document
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Call completion method on closing fence
    let params = CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position { line: 2, character: 3 }, // On closing ```
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    };

    let result = server.completion(params).await.unwrap();
    assert!(result.is_none(), "Should NOT offer completion on closing fence");
}

#[tokio::test]
async fn test_completion_graceful_when_document_not_found() {
    let server = create_test_server();

    // Use a URI for a document that doesn't exist and isn't opened
    let uri = Url::parse("file:///nonexistent/path/test.md").unwrap();

    let params = CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position: Position { line: 0, character: 3 },
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    };

    // Should return Ok(None), not an error
    let result = server.completion(params).await;
    assert!(result.is_ok(), "Completion should not error for missing document");
    assert!(result.unwrap().is_none(), "Should return None for missing document");
}

// ==================== Link Target Completion Tests ====================

#[test]
fn test_detect_link_target_file_path_empty() {
    // Cursor right after `](` — `](` is at columns 9-10, content starts at column 11
    let text = "See [text](";
    let pos = Position { line: 0, character: 11 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.file_path, "");
    assert_eq!(info.path_start_col, 11); // UTF-16 column right after `(`
    assert!(info.anchor.is_none());
}

#[test]
fn test_detect_link_target_file_path_partial() {
    // Cursor mid-way through a file path
    // `](` is at columns 9-10; content_start = 11, so path_start_col = 11
    let text = "See [text](docs/guide";
    let pos = Position { line: 0, character: 21 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.file_path, "docs/guide");
    assert_eq!(info.path_start_col, 11);
    assert!(info.anchor.is_none());
}

#[test]
fn test_detect_link_target_anchor_empty() {
    // Cursor right after `#`
    let text = "See [text](guide.md#";
    let pos = Position { line: 0, character: 20 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.file_path, "guide.md");
    assert!(info.anchor.is_some());
    let (partial, start_col) = info.anchor.unwrap();
    assert_eq!(partial, "");
    assert_eq!(start_col, 20); // after `#`
}

#[test]
fn test_detect_link_target_anchor_partial() {
    // Cursor mid-way through an anchor
    let text = "See [text](guide.md#install";
    let pos = Position { line: 0, character: 27 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.file_path, "guide.md");
    let (partial, start_col) = info.anchor.unwrap();
    assert_eq!(partial, "install");
    assert_eq!(start_col, 20);
}

#[test]
fn test_detect_link_target_anchor_same_file() {
    // Fragment-only link `[text](#anchor` — empty file path
    let text = "[text](#sec";
    let pos = Position { line: 0, character: 11 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.file_path, "");
    let (partial, _start_col) = info.anchor.unwrap();
    assert_eq!(partial, "sec");
}

#[test]
fn test_detect_link_target_closed_paren_no_completion() {
    // Cursor AFTER the closing `)` — before_cursor includes `)`, so content
    // contains `)` and the function returns None.
    // "See [text](guide.md)" is 21 chars; `)` is at byte 20.
    // Cursor at 21 → before_cursor = whole string → content = "guide.md)" → None.
    let text = "See [text](guide.md) more";
    let pos = Position { line: 0, character: 21 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_none(), "Should not complete after a closed link");
}

#[test]
fn test_detect_link_target_no_link_syntax() {
    // Regular text with no link
    let text = "Just plain text here";
    let pos = Position { line: 0, character: 10 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_none());
}

#[test]
fn test_detect_link_target_code_span_skipped() {
    // `](` inside a code span — should not trigger completion
    let text = "Use `[text](path` for links";
    // cursor is after `path` (position 16), which is inside the code span
    let pos = Position { line: 0, character: 16 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_none(), "Should not complete inside a code span");
}

#[test]
fn test_detect_link_target_image_link() {
    // Image links `![alt](` should also trigger completion
    let text = "![image](imgs/";
    let pos = Position { line: 0, character: 14 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    // `![image](` contains `](` so the backward scan will find it
    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.file_path, "imgs/");
}

#[test]
fn test_detect_link_target_multiple_links_on_line() {
    // Two links on the same line — should detect the second one being typed
    let text = "[first](a.md) and [second](b.md";
    let pos = Position { line: 0, character: 31 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.file_path, "b.md");
}

#[test]
fn test_detect_link_target_non_ascii_link_text() {
    // "é" is U+00E9: 2 UTF-8 bytes, 1 UTF-16 code unit.
    // "[résumé](" is:  `[` + r(1) + é(1) + s(1) + u(1) + m(1) + é(1) + `]` + `(` = 9 UTF-16 code units
    // So `](` spans columns 7-8, and content_start is column 9.
    let text = "[résumé](";
    let pos = Position { line: 0, character: 9 }; // right after `(`
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_some(), "should detect link in non-ASCII context");
    let info = result.unwrap();
    assert_eq!(info.file_path, "");
    // path_start_col must be the UTF-16 column after `(`, which is 9
    assert_eq!(info.path_start_col, 9);
    assert!(info.anchor.is_none());
}

#[test]
fn test_detect_link_target_non_ascii_with_path() {
    // "[café](docs/" — "café" = c(1)+a(1)+f(1)+é(1) = 4 UTF-16 code units
    // "[café](" spans columns 0-6; content_start is column 7
    // cursor at 12 (7 + len("docs/") = 7+5 = 12)
    let text = "[café](docs/";
    let pos = Position { line: 0, character: 12 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.file_path, "docs/");
    assert_eq!(info.path_start_col, 7); // UTF-16 column after `(`
}

#[test]
fn test_detect_link_target_out_of_bounds_position() {
    let text = "short";
    let pos = Position {
        line: 0,
        character: 100,
    };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_none());
}

#[test]
fn test_detect_link_target_out_of_bounds_line() {
    let text = "single line";
    let pos = Position { line: 5, character: 0 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_file_completions_returns_workspace_files() {
    use std::fs;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    // Create a few markdown files
    let current = temp_dir.path().join("current.md");
    let other = temp_dir.path().join("other.md");
    let sub_dir = temp_dir.path().join("docs");
    fs::create_dir(&sub_dir).unwrap();
    let sub_file = sub_dir.join("guide.md");

    fs::write(&current, "# Current").unwrap();
    fs::write(&other, "# Other").unwrap();
    fs::write(&sub_file, "# Guide").unwrap();

    let server = create_test_server();
    let uri = Url::from_file_path(&current).unwrap();

    // Populate the workspace index manually
    {
        use crate::workspace_index::{FileIndex, HeadingIndex, WorkspaceIndex};
        let mut index = server.workspace_index.write().await;
        *index = WorkspaceIndex::new();
        let mut fi = FileIndex::default();
        fi.headings.push(HeadingIndex {
            text: "Current".to_string(),
            auto_anchor: "current".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(current.clone(), fi);

        let mut fi2 = FileIndex::default();
        fi2.headings.push(HeadingIndex {
            text: "Other".to_string(),
            auto_anchor: "other".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(other.clone(), fi2);

        let mut fi3 = FileIndex::default();
        fi3.headings.push(HeadingIndex {
            text: "Guide".to_string(),
            auto_anchor: "guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(sub_file.clone(), fi3);
    }

    // Get all file completions (empty prefix)
    let items = server
        .get_file_completions(&uri, "", 10, Position { line: 0, character: 10 })
        .await;

    // Should have 2 completions (other.md and docs/guide.md), NOT current.md
    assert_eq!(items.len(), 2, "Should return 2 files (excluding current)");

    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"other.md"), "Should include other.md");
    assert!(labels.contains(&"docs/guide.md"), "Should include docs/guide.md");
    assert!(!labels.contains(&"current.md"), "Should exclude current.md");
}

#[tokio::test]
async fn test_get_file_completions_filters_by_prefix() {
    use std::fs;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let current = temp_dir.path().join("current.md");
    let docs_dir = temp_dir.path().join("docs");
    fs::create_dir(&docs_dir).unwrap();
    let guide = docs_dir.join("guide.md");
    let ref_doc = docs_dir.join("reference.md");

    fs::write(&current, "").unwrap();
    fs::write(&guide, "").unwrap();
    fs::write(&ref_doc, "").unwrap();

    let server = create_test_server();
    let uri = Url::from_file_path(&current).unwrap();

    {
        use crate::workspace_index::{FileIndex, WorkspaceIndex};
        let mut index = server.workspace_index.write().await;
        *index = WorkspaceIndex::new();
        index.insert_file(current.clone(), FileIndex::default());
        index.insert_file(guide.clone(), FileIndex::default());
        index.insert_file(ref_doc.clone(), FileIndex::default());
    }

    // Filter by "docs/g" prefix
    let items = server
        .get_file_completions(&uri, "docs/g", 10, Position { line: 0, character: 16 })
        .await;

    assert_eq!(items.len(), 1, "Should return only docs/guide.md");
    assert_eq!(items[0].label, "docs/guide.md");
}

#[tokio::test]
async fn test_get_anchor_completions_returns_headings() {
    use std::fs;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let current = temp_dir.path().join("index.md");
    let target = temp_dir.path().join("guide.md");

    fs::write(&current, "").unwrap();
    fs::write(&target, "# Installation\n\n## Configuration\n\n## Troubleshooting").unwrap();

    let server = create_test_server();
    let uri = Url::from_file_path(&current).unwrap();

    {
        use crate::workspace_index::{FileIndex, HeadingIndex, WorkspaceIndex};
        let mut index = server.workspace_index.write().await;
        *index = WorkspaceIndex::new();
        index.insert_file(current.clone(), FileIndex::default());

        let mut fi = FileIndex::default();
        fi.headings = vec![
            HeadingIndex {
                text: "Installation".to_string(),
                auto_anchor: "installation".to_string(),
                custom_anchor: None,
                line: 1,
                is_setext: false,
            },
            HeadingIndex {
                text: "Configuration".to_string(),
                auto_anchor: "configuration".to_string(),
                custom_anchor: None,
                line: 3,
                is_setext: false,
            },
            HeadingIndex {
                text: "Troubleshooting".to_string(),
                auto_anchor: "troubleshooting".to_string(),
                custom_anchor: None,
                line: 5,
                is_setext: false,
            },
        ];
        index.insert_file(target.clone(), fi);
    }

    // Completions for all anchors in guide.md (empty prefix)
    let items = server
        .get_anchor_completions(&uri, "guide.md", "", 27, Position { line: 0, character: 27 })
        .await;

    assert_eq!(items.len(), 3, "Should return all 3 headings");

    // Items should be in document order (sorted by line number)
    assert_eq!(items[0].insert_text.as_deref(), Some("installation"));
    assert_eq!(items[1].insert_text.as_deref(), Some("configuration"));
    assert_eq!(items[2].insert_text.as_deref(), Some("troubleshooting"));
}

#[tokio::test]
async fn test_get_anchor_completions_filters_by_prefix() {
    use std::fs;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let current = temp_dir.path().join("index.md");
    let target = temp_dir.path().join("guide.md");

    fs::write(&current, "").unwrap();
    fs::write(&target, "").unwrap();

    let server = create_test_server();
    let uri = Url::from_file_path(&current).unwrap();

    {
        use crate::workspace_index::{FileIndex, HeadingIndex, WorkspaceIndex};
        let mut index = server.workspace_index.write().await;
        *index = WorkspaceIndex::new();
        index.insert_file(current.clone(), FileIndex::default());

        let mut fi = FileIndex::default();
        fi.headings = vec![
            HeadingIndex {
                text: "Installation".to_string(),
                auto_anchor: "installation".to_string(),
                custom_anchor: None,
                line: 1,
                is_setext: false,
            },
            HeadingIndex {
                text: "Introduction".to_string(),
                auto_anchor: "introduction".to_string(),
                custom_anchor: None,
                line: 2,
                is_setext: false,
            },
            HeadingIndex {
                text: "Configuration".to_string(),
                auto_anchor: "configuration".to_string(),
                custom_anchor: None,
                line: 3,
                is_setext: false,
            },
        ];
        index.insert_file(target.clone(), fi);
    }

    let items = server
        .get_anchor_completions(&uri, "guide.md", "in", 27, Position { line: 0, character: 27 })
        .await;

    assert_eq!(items.len(), 2, "Should return installation and introduction");
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"Installation"));
    assert!(labels.contains(&"Introduction"));
}

#[tokio::test]
async fn test_get_anchor_completions_uses_custom_anchor() {
    use std::fs;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let current = temp_dir.path().join("index.md");
    let target = temp_dir.path().join("guide.md");

    fs::write(&current, "").unwrap();
    fs::write(&target, "").unwrap();

    let server = create_test_server();
    let uri = Url::from_file_path(&current).unwrap();

    {
        use crate::workspace_index::{FileIndex, HeadingIndex, WorkspaceIndex};
        let mut index = server.workspace_index.write().await;
        *index = WorkspaceIndex::new();
        index.insert_file(current.clone(), FileIndex::default());

        let mut fi = FileIndex::default();
        fi.headings = vec![HeadingIndex {
            text: "Getting Started".to_string(),
            auto_anchor: "getting-started".to_string(),
            custom_anchor: Some("start".to_string()),
            line: 1,
            is_setext: false,
        }];
        index.insert_file(target.clone(), fi);
    }

    let items = server
        .get_anchor_completions(&uri, "guide.md", "", 10, Position { line: 0, character: 10 })
        .await;

    assert_eq!(items.len(), 1);
    // Should use custom_anchor "start", not auto_anchor "getting-started"
    assert_eq!(items[0].insert_text.as_deref(), Some("start"));
    assert_eq!(items[0].label, "Getting Started");
    assert_eq!(items[0].detail.as_deref(), Some("#start"));
}

#[tokio::test]
async fn test_get_anchor_completions_empty_file_path_uses_current() {
    use std::fs;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let current = temp_dir.path().join("page.md");
    fs::write(&current, "# Section One\n\n# Section Two").unwrap();

    let server = create_test_server();
    let uri = Url::from_file_path(&current).unwrap();

    {
        use crate::workspace_index::{FileIndex, HeadingIndex, WorkspaceIndex};
        let mut index = server.workspace_index.write().await;
        *index = WorkspaceIndex::new();

        let mut fi = FileIndex::default();
        fi.headings = vec![
            HeadingIndex {
                text: "Section One".to_string(),
                auto_anchor: "section-one".to_string(),
                custom_anchor: None,
                line: 1,
                is_setext: false,
            },
            HeadingIndex {
                text: "Section Two".to_string(),
                auto_anchor: "section-two".to_string(),
                custom_anchor: None,
                line: 3,
                is_setext: false,
            },
        ];
        index.insert_file(current.clone(), fi);
    }

    // Empty file_path means "anchor in the current file"
    let items = server
        .get_anchor_completions(&uri, "", "", 8, Position { line: 0, character: 8 })
        .await;

    assert_eq!(items.len(), 2);
    let insert_texts: Vec<&str> = items.iter().map(|i| i.insert_text.as_deref().unwrap_or("")).collect();
    assert!(insert_texts.contains(&"section-one"));
    assert!(insert_texts.contains(&"section-two"));
}

#[tokio::test]
async fn test_get_anchor_completions_unknown_file_returns_empty() {
    let temp_dir = tempfile::tempdir().unwrap();
    let current = temp_dir.path().join("index.md");
    std::fs::write(&current, "").unwrap();

    let server = create_test_server();
    let uri = Url::from_file_path(&current).unwrap();

    // Don't populate the workspace index — file not found should return empty
    let items = server
        .get_anchor_completions(&uri, "nonexistent.md", "", 10, Position { line: 0, character: 10 })
        .await;

    assert!(items.is_empty(), "Unknown file should return no completions");
}

#[test]
fn test_detect_link_target_relative_parent_path() {
    // Cursor inside a `../` relative path
    let text = "See [link](../other/file";
    let pos = Position { line: 0, character: 24 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.file_path, "../other/file");
    assert!(info.anchor.is_none());
}

#[test]
fn test_detect_link_target_path_and_anchor() {
    // Full path with anchor: `](../dir/file.md#section`
    let text = "See [link](../dir/file.md#section";
    let pos = Position { line: 0, character: 33 };
    let result = RumdlLanguageServer::detect_link_target_position(text, pos);
    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.file_path, "../dir/file.md");
    let (partial, _) = info.anchor.unwrap();
    assert_eq!(partial, "section");
}

#[tokio::test]
async fn test_link_completions_disabled_returns_none() {
    use std::fs;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    let content = "See [text](";
    fs::write(&test_file, content).unwrap();

    let server = create_test_server();

    // Disable link completions
    server.config.write().await.enable_link_completions = false;

    let uri = Url::from_file_path(&test_file).unwrap();
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    let params = CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position: Position { line: 0, character: 11 },
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    };

    let result = server.completion(params).await.unwrap();
    assert!(result.is_none(), "Link completions should be suppressed when disabled");
}

#[tokio::test]
async fn test_link_navigation_disabled_hover_returns_none() {
    let server = create_test_server();
    server.config.write().await.enable_link_navigation = false;

    let params = HoverParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: Url::parse("file:///tmp/test.md").unwrap(),
            },
            position: Position { line: 0, character: 5 },
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };

    let result = server.hover(params).await.unwrap();
    assert!(
        result.is_none(),
        "Hover should be suppressed when link navigation is disabled"
    );
}

#[tokio::test]
async fn test_link_navigation_disabled_goto_definition_returns_none() {
    let server = create_test_server();
    server.config.write().await.enable_link_navigation = false;

    let params = GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: Url::parse("file:///tmp/test.md").unwrap(),
            },
            position: Position { line: 0, character: 5 },
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = server.goto_definition(params).await.unwrap();
    assert!(
        result.is_none(),
        "Go-to-definition should be suppressed when link navigation is disabled"
    );
}

#[tokio::test]
async fn test_link_navigation_disabled_references_returns_none() {
    let server = create_test_server();
    server.config.write().await.enable_link_navigation = false;

    let params = ReferenceParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: Url::parse("file:///tmp/test.md").unwrap(),
            },
            position: Position { line: 0, character: 5 },
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: ReferenceContext {
            include_declaration: false,
        },
    };

    let result = server.references(params).await.unwrap();
    assert!(
        result.is_none(),
        "References should be suppressed when link navigation is disabled"
    );
}

#[tokio::test]
async fn test_link_navigation_disabled_prepare_rename_returns_none() {
    let server = create_test_server();
    server.config.write().await.enable_link_navigation = false;

    let params = TextDocumentPositionParams {
        text_document: TextDocumentIdentifier {
            uri: Url::parse("file:///tmp/test.md").unwrap(),
        },
        position: Position { line: 0, character: 5 },
    };

    let result = server.prepare_rename(params).await.unwrap();
    assert!(
        result.is_none(),
        "Prepare-rename should be suppressed when link navigation is disabled"
    );
}

#[tokio::test]
async fn test_link_navigation_disabled_rename_returns_none() {
    let server = create_test_server();
    server.config.write().await.enable_link_navigation = false;

    let params = RenameParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: Url::parse("file:///tmp/test.md").unwrap(),
            },
            position: Position { line: 0, character: 5 },
        },
        new_name: "new-heading".to_string(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };

    let result = server.rename(params).await.unwrap();
    assert!(
        result.is_none(),
        "Rename should be suppressed when link navigation is disabled"
    );
}

#[tokio::test]
async fn test_link_navigation_disabled_via_did_change_configuration() {
    let server = create_test_server();

    // Default state: navigation is enabled
    assert!(server.config.read().await.enable_link_navigation);

    // Simulate a live config update with only enableLinkNavigation: false
    server
        .did_change_configuration(DidChangeConfigurationParams {
            settings: serde_json::json!({ "enableLinkNavigation": false }),
        })
        .await;

    // Flag must be applied — this is what the did_change_configuration heuristic guards
    assert!(
        !server.config.read().await.enable_link_navigation,
        "did_change_configuration must apply enableLinkNavigation: false"
    );

    // Confirm the handler respects the updated flag
    let result = server
        .hover(HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::parse("file:///tmp/test.md").unwrap(),
                },
                position: Position { line: 0, character: 5 },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .unwrap();
    assert!(
        result.is_none(),
        "Hover should be suppressed after live config disables link navigation"
    );
}

#[test]
fn test_link_navigation_config_serde_roundtrip() {
    // Verify `enableLinkNavigation: false` round-trips correctly through serde
    let json = r#"{"enableLinkNavigation": false}"#;
    let config: RumdlLspConfig = serde_json::from_str(json).unwrap();
    assert!(
        !config.enable_link_navigation,
        "enableLinkNavigation should deserialize to false"
    );
    // All other fields should use their defaults
    assert!(config.enable_linting, "enableLinting should default to true");
    assert!(
        config.enable_link_completions,
        "enableLinkCompletions should default to true"
    );

    // Verify serialization produces camelCase key
    let serialized = serde_json::to_string(&config).unwrap();
    assert!(serialized.contains("\"enableLinkNavigation\":false"));
}

/// Test that MD013 semantic-line-breaks config produces no false positives with CRLF line endings.
/// The LSP receives content from the editor which may use CRLF line endings on Windows.
/// The reflow comparison must account for line ending differences.
/// Regression test for issue #459.
#[tokio::test]
async fn test_lsp_md013_semantic_line_breaks_crlf() {
    use tempfile::tempdir;

    let server = create_test_server();

    // Create a temp directory with pyproject.toml
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let pyproject_path = temp_dir.path().join("pyproject.toml");
    std::fs::write(
        &pyproject_path,
        r#"
[tool.rumdl.MD013]
line-length = 80
reflow = true
reflow-mode = "semantic-line-breaks"
"#,
    )
    .expect("Failed to write pyproject.toml");

    // Create a test markdown file with CRLF line endings
    let test_md_path = temp_dir.path().join("test.md");
    // This content is properly formatted for semantic line breaks
    // but uses CRLF line endings (as sent by editors on Windows)
    let content_crlf = "# Title\r\n\r\nLorem ipsum dolor sit amet, consectetur adipiscing elit.\r\nNullam vehicula commodo lobortis.\r\nDonec a venenatis lorem.\r\n";
    std::fs::write(&test_md_path, content_crlf).expect("Failed to write test.md");

    let canonical_test_path = test_md_path.canonicalize().unwrap_or_else(|_| test_md_path.clone());

    // Add workspace root
    let canonical_temp = temp_dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| temp_dir.path().to_path_buf());
    server.workspace_roots.write().await.push(canonical_temp);

    // Lint via LSP path with CRLF content
    let uri = Url::from_file_path(&canonical_test_path).unwrap();
    let diagnostics = server.lint_document(&uri, content_crlf, true).await.unwrap();

    // Filter for MD013 diagnostics
    let md013_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code
                .as_ref()
                .is_some_and(|c| matches!(c, NumberOrString::String(s) if s == "MD013"))
        })
        .collect();

    assert!(
        md013_diagnostics.is_empty(),
        "LSP should produce no MD013 warnings for properly formatted semantic-line-break content \
         with CRLF line endings, but found {} warnings: {:?}",
        md013_diagnostics.len(),
        md013_diagnostics
            .iter()
            .map(|d| format!("line {}: {}", d.range.start.line, d.message))
            .collect::<Vec<_>>()
    );
}

/// Test that MD013 still emits warnings for improperly formatted CRLF content.
/// The fix for issue #459 must not suppress legitimate warnings.
#[tokio::test]
async fn test_lsp_md013_semantic_line_breaks_crlf_still_warns_when_needed() {
    use tempfile::tempdir;

    let server = create_test_server();

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let pyproject_path = temp_dir.path().join("pyproject.toml");
    std::fs::write(
        &pyproject_path,
        r#"
[tool.rumdl.MD013]
line-length = 80
reflow = true
reflow-mode = "semantic-line-breaks"
"#,
    )
    .expect("Failed to write pyproject.toml");

    let test_md_path = temp_dir.path().join("test.md");
    // Content with multiple sentences on one line (needs reflow) using CRLF
    let content_crlf =
        "# Title\r\n\r\nLorem ipsum dolor sit amet. Consectetur adipiscing elit. Nullam vehicula commodo lobortis.\r\n";
    std::fs::write(&test_md_path, content_crlf).expect("Failed to write test.md");

    let canonical_test_path = test_md_path.canonicalize().unwrap_or_else(|_| test_md_path.clone());
    let canonical_temp = temp_dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| temp_dir.path().to_path_buf());
    server.workspace_roots.write().await.push(canonical_temp);

    let uri = Url::from_file_path(&canonical_test_path).unwrap();
    let diagnostics = server.lint_document(&uri, content_crlf, true).await.unwrap();

    let md013_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code
                .as_ref()
                .is_some_and(|c| matches!(c, NumberOrString::String(s) if s == "MD013"))
        })
        .collect();

    assert!(
        !md013_diagnostics.is_empty(),
        "LSP should produce MD013 warnings for improperly formatted semantic-line-break CRLF content"
    );
}

/// Test that MD013 semantic-line-breaks config from pyproject.toml is respected in LSP
/// This verifies that the LSP and CLI produce the same results for the same config.
/// Regression test for issue #459.
#[tokio::test]
async fn test_lsp_md013_semantic_line_breaks_config_parity() {
    use tempfile::tempdir;

    let server = create_test_server();

    // Create a temp directory with pyproject.toml
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let pyproject_path = temp_dir.path().join("pyproject.toml");
    std::fs::write(
        &pyproject_path,
        r#"
[tool.rumdl.MD013]
line-length = 80
reflow = true
reflow-mode = "semantic-line-breaks"
"#,
    )
    .expect("Failed to write pyproject.toml");

    // Create a test markdown file in the same directory
    let test_md_path = temp_dir.path().join("test.md");
    let content = "# Title\n\nLorem ipsum dolor sit amet, consectetur adipiscing elit.\nNullam vehicula commodo lobortis.\nDonec a venenatis lorem.\n";
    std::fs::write(&test_md_path, content).expect("Failed to write test.md");

    let uri = Url::from_file_path(&test_md_path).unwrap();

    // Simulate what resolve_config_for_file does: load config from the pyproject.toml
    let config_path_str = pyproject_path.to_str().unwrap();
    let sourced = RumdlLanguageServer::load_config_for_lsp(Some(config_path_str)).expect("Should load config");
    let file_config: crate::config::Config = sourced.into_validated_unchecked().into();

    // Verify the config loaded correctly
    let md013_rule_config = file_config.rules.get("MD013");
    assert!(
        md013_rule_config.is_some(),
        "MD013 config should be present in loaded config"
    );
    let md013_values = &md013_rule_config.unwrap().values;
    assert!(
        md013_values.get("reflow-mode").is_some() || md013_values.get("reflow_mode").is_some(),
        "reflow-mode should be in MD013 config values, got: {:?}",
        md013_values.keys().collect::<Vec<_>>()
    );

    // Set the config on the server
    *server.rumdl_config.write().await = file_config;

    // Lint via LSP path
    let diagnostics = server.lint_document(&uri, content, true).await.unwrap();

    // Filter for MD013 diagnostics
    let md013_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code
                .as_ref()
                .is_some_and(|c| matches!(c, NumberOrString::String(s) if s == "MD013"))
        })
        .collect();

    assert!(
        md013_diagnostics.is_empty(),
        "LSP should produce no MD013 warnings for properly formatted semantic-line-break content, \
         but found {} warnings: {:?}",
        md013_diagnostics.len(),
        md013_diagnostics
            .iter()
            .map(|d| format!("line {}: {}", d.range.start.line, d.message))
            .collect::<Vec<_>>()
    );

    // Also verify CLI path produces same result
    let config_path_str2 = pyproject_path.to_str().unwrap();
    let sourced2 = crate::config::SourcedConfig::load_with_discovery(Some(config_path_str2), None, false)
        .expect("Should load config");
    let cli_config: crate::config::Config = sourced2.into_validated_unchecked().into();
    let all_rules = crate::rules::all_rules(&cli_config);
    let filtered_rules = crate::rules::filter_rules(&all_rules, &cli_config.global);
    let cli_warnings = crate::lint(
        content,
        &filtered_rules,
        false,
        crate::config::MarkdownFlavor::Standard,
        None,
        Some(&cli_config),
    )
    .expect("CLI lint should succeed");

    let cli_md013: Vec<_> = cli_warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD013"))
        .collect();
    assert!(cli_md013.is_empty(), "CLI should produce no MD013 warnings either");
}

/// Test that MD013 semantic-line-breaks config works through the full resolve_config_for_file path.
/// This tests the actual file discovery path the LSP uses, which is different from directly setting config.
/// Regression test for issue #459.
#[tokio::test]
async fn test_lsp_md013_resolve_config_for_file_path() {
    use tempfile::tempdir;

    let server = create_test_server();

    // Create a temp directory with pyproject.toml
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let pyproject_path = temp_dir.path().join("pyproject.toml");
    std::fs::write(
        &pyproject_path,
        r#"
[tool.rumdl]

[tool.rumdl.MD013]
line-length = 80
reflow = true
reflow-mode = "semantic-line-breaks"
"#,
    )
    .expect("Failed to write pyproject.toml");

    // Create a test markdown file in the same directory
    let test_md_path = temp_dir.path().join("test.md");
    let content = "# Title\n\nLorem ipsum dolor sit amet, consectetur adipiscing elit.\nNullam vehicula commodo lobortis.\nDonec a venenatis lorem.\n";
    std::fs::write(&test_md_path, content).expect("Failed to write test.md");

    // Add the temp dir as a workspace root (otherwise resolve_config_for_file walks up forever)
    let canonical_temp = temp_dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| temp_dir.path().to_path_buf());
    server.workspace_roots.write().await.push(canonical_temp.clone());

    // Use the real resolve_config_for_file path
    let canonical_test_path = test_md_path.canonicalize().unwrap_or_else(|_| test_md_path.clone());
    let resolved_config = server.resolve_config_for_file(&canonical_test_path).await;

    // Verify the config loaded correctly
    let md013_rule_config = resolved_config.rules.get("MD013");
    assert!(
        md013_rule_config.is_some(),
        "MD013 config should be present after resolve_config_for_file. Rules: {:?}",
        resolved_config.rules.keys().collect::<Vec<_>>()
    );

    let md013_values = &md013_rule_config.unwrap().values;
    let has_reflow_mode = md013_values.get("reflow-mode").is_some() || md013_values.get("reflow_mode").is_some();
    assert!(
        has_reflow_mode,
        "reflow-mode should be in MD013 config values after resolve_config_for_file, got: {:?}",
        md013_values.keys().collect::<Vec<_>>()
    );

    let has_reflow = md013_values.get("reflow").is_some();
    assert!(
        has_reflow,
        "reflow should be in MD013 config values after resolve_config_for_file, got: {:?}",
        md013_values.keys().collect::<Vec<_>>()
    );

    // Now create rules from this config and check linting result
    let all_rules = crate::rules::all_rules(&resolved_config);
    let filtered_rules = crate::rules::filter_rules(&all_rules, &resolved_config.global);
    let warnings = crate::lint(
        content,
        &filtered_rules,
        false,
        crate::config::MarkdownFlavor::Standard,
        None,
        Some(&resolved_config),
    )
    .expect("Lint should succeed");

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD013"))
        .collect();

    assert!(
        md013_warnings.is_empty(),
        "Should produce no MD013 warnings for semantic-line-break content via resolve_config_for_file path, \
         but found {} warnings: {:?}",
        md013_warnings.len(),
        md013_warnings
            .iter()
            .map(|w| format!("line {}: {} - {}", w.line, w.message, w.message))
            .collect::<Vec<_>>()
    );

    // Also test the full lint_document path
    let uri = Url::from_file_path(&canonical_test_path).unwrap();
    let diagnostics = server.lint_document(&uri, content, true).await.unwrap();
    let md013_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code
                .as_ref()
                .is_some_and(|c| matches!(c, NumberOrString::String(s) if s == "MD013"))
        })
        .collect();

    assert!(
        md013_diags.is_empty(),
        "lint_document should produce no MD013 diagnostics for semantic-line-break content, \
         but found {} diagnostics: {:?}",
        md013_diags.len(),
        md013_diags
            .iter()
            .map(|d| format!("line {}: {}", d.range.start.line, d.message))
            .collect::<Vec<_>>()
    );
}

/// Test that MD007 indent=4 config is respected through the full LSP formatting path.
/// Verifies that [ul-indent] alias, indent=4, and style="fixed" all propagate correctly
/// from config file through resolve_config_for_file to the formatted output.
#[tokio::test]
async fn test_lsp_md007_formatting_respects_indent_config() {
    use tempfile::tempdir;

    let server = create_test_server();

    // Create a temp directory with .rumdl.toml using [ul-indent] alias
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join(".rumdl.toml");
    std::fs::write(
        &config_path,
        r#"
[ul-indent]
indent = 4
style = "fixed"
"#,
    )
    .expect("Failed to write .rumdl.toml");

    // Create test markdown with 2-space indentation (should be fixed to 4-space)
    let test_md_path = temp_dir.path().join("test.md");
    let content = "# Test\n\n- Bullet item\n  - Nested bullet\n";
    std::fs::write(&test_md_path, content).expect("Failed to write test.md");

    // Set up workspace root
    let canonical_temp = temp_dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| temp_dir.path().to_path_buf());
    server.workspace_roots.write().await.push(canonical_temp.clone());

    // Step 1: Verify config is loaded correctly
    let canonical_test_path = test_md_path.canonicalize().unwrap_or_else(|_| test_md_path.clone());
    let resolved_config = server.resolve_config_for_file(&canonical_test_path).await;

    let md007_config = resolved_config.rules.get("MD007");
    assert!(
        md007_config.is_some(),
        "MD007 config should be present. Rules: {:?}",
        resolved_config.rules.keys().collect::<Vec<_>>()
    );

    let md007_values = &md007_config.unwrap().values;
    let indent_value = md007_values.get("indent").map(|v| v.as_integer().unwrap_or(0));
    assert_eq!(indent_value, Some(4), "MD007 indent should be 4, got: {md007_values:?}",);

    // Step 2: Verify detection works (should find 2-space indent as violation)
    let all_rules = crate::rules::all_rules(&resolved_config);
    let filtered_rules = crate::rules::filter_rules(&all_rules, &resolved_config.global);
    let warnings = crate::lint(
        content,
        &filtered_rules,
        false,
        crate::config::MarkdownFlavor::Standard,
        None,
        Some(&resolved_config),
    )
    .expect("Lint should succeed");

    let md007_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD007"))
        .collect();
    assert_eq!(
        md007_warnings.len(),
        1,
        "Should find exactly 1 MD007 warning for 2-space indent, found: {:?}",
        md007_warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(
        md007_warnings[0].message.contains("Expected 4 spaces"),
        "Warning should mention 4 spaces, got: {}",
        md007_warnings[0].message
    );

    // Step 3: Verify the fix produces 4-space indent (not 2-space!)
    assert!(md007_warnings[0].fix.is_some(), "MD007 warning should have a fix");
    let fix = md007_warnings[0].fix.as_ref().unwrap();
    assert_eq!(
        fix.replacement, "    ",
        "Fix replacement should be 4 spaces, got: {:?}",
        fix.replacement
    );

    // Step 4: Exercise the full LSP formatting path
    let uri = Url::from_file_path(&canonical_test_path).unwrap();
    let entry = DocumentEntry {
        content: content.to_string(),
        version: Some(1),
        from_disk: false,
    };
    server.documents.write().await.insert(uri.clone(), entry);

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

    let result = server.formatting(params).await.unwrap();
    assert!(result.is_some(), "Formatting should return edits");

    let edits = result.unwrap();
    assert!(!edits.is_empty(), "Should have at least one edit");

    let formatted_text = &edits[0].new_text;
    assert!(
        formatted_text.contains("    - Nested bullet"),
        "Formatted text should have 4-space indent, got:\n{formatted_text}",
    );
    assert!(
        !formatted_text.contains("\n  - Nested"),
        "Formatted text should NOT have 2-space indent, got:\n{formatted_text}",
    );
}

/// Test that the source.fixAll.rumdl code action path (used by Zed's code_actions_on_format)
/// correctly applies MD007 indent=4 config. This is the other path editors can use
/// besides textDocument/formatting.
#[tokio::test]
async fn test_lsp_md007_code_action_fix_all_respects_indent_config() {
    use tempfile::tempdir;

    let server = create_test_server();

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join(".rumdl.toml");
    std::fs::write(
        &config_path,
        r#"
[ul-indent]
indent = 4
style = "fixed"
"#,
    )
    .expect("Failed to write .rumdl.toml");

    // User's exact test data from issue #210
    let content = "- Bullet item\n  - Nested bullet\n  1. Ordered child\n     - Bullet under ordered\n";

    let test_md_path = temp_dir.path().join("test.md");
    std::fs::write(&test_md_path, content).expect("Failed to write test.md");

    let canonical_temp = temp_dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| temp_dir.path().to_path_buf());
    server.workspace_roots.write().await.push(canonical_temp.clone());

    let canonical_test_path = test_md_path.canonicalize().unwrap_or_else(|_| test_md_path.clone());
    let uri = Url::from_file_path(&canonical_test_path).unwrap();
    let entry = DocumentEntry {
        content: content.to_string(),
        version: Some(1),
        from_disk: false,
    };
    server.documents.write().await.insert(uri.clone(), entry);

    // Request code actions for the full document (simulates code_actions_on_format)
    let range = Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: 3, character: 26 },
    };

    let actions = server.get_code_actions(&uri, content, range).await.unwrap();

    // Find the source.fixAll.rumdl action
    let fix_all_actions: Vec<_> = actions
        .iter()
        .filter(|a| a.kind.as_ref().is_some_and(|k| k.as_str() == "source.fixAll.rumdl"))
        .collect();

    assert!(
        !fix_all_actions.is_empty(),
        "source.fixAll.rumdl action should be created"
    );

    // Extract the fixed content from the action's workspace edit
    let fix_all = &fix_all_actions[0];
    let edit = fix_all.edit.as_ref().expect("fixAll action should have an edit");
    let changes = edit.changes.as_ref().expect("edit should have changes");
    let text_edits = changes.get(&uri).expect("changes should include our file");
    let fixed_text = &text_edits[0].new_text;

    // Verify 4-space indent for nested bullet (depth 1)
    assert!(
        fixed_text.contains("\n    - Nested bullet"),
        "source.fixAll should produce 4-space indent, got:\n{fixed_text}",
    );
    assert!(
        !fixed_text.contains("\n  - Nested"),
        "source.fixAll should NOT have 2-space indent, got:\n{fixed_text}",
    );
}

// =============================================================================
// Navigation tests: go-to-definition and find-references
// =============================================================================

#[tokio::test]
async fn test_goto_definition_file_path_only() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    // Set up file paths
    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-test/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();

    // Content with a link to guide.md (cursor will be on the link target)
    let content = "# Index\n\nSee [the guide](guide.md) for details.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Populate workspace index with target file
    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Guide".to_string(),
            auto_anchor: "guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    // Position cursor on "guide.md" in `](guide.md)`
    // Line 2 (0-indexed): "See [the guide](guide.md) for details."
    // The `](` is at column 15, so "guide.md" starts at column 17
    let position = Position { line: 2, character: 20 };

    let result = server.handle_goto_definition(&current_uri, position).await;
    assert!(result.is_some(), "Should return a definition location");

    if let Some(GotoDefinitionResponse::Scalar(location)) = result {
        assert_eq!(
            location.uri,
            Url::from_file_path(&target_file).unwrap(),
            "Should point to guide.md"
        );
        // No anchor, so should target line 0
        assert_eq!(location.range.start.line, 0, "Should target line 0 (top of file)");
    } else {
        panic!("Expected Scalar response");
    }
}

#[tokio::test]
async fn test_goto_definition_file_with_anchor() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-test2/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();

    // Content with a link that has both file and anchor
    let content = "# Index\n\nSee [install](guide.md#installation) here.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Populate workspace index with target file and heading
    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Getting Started".to_string(),
            auto_anchor: "getting-started".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        fi.add_heading(HeadingIndex {
            text: "Installation".to_string(),
            auto_anchor: "installation".to_string(),
            custom_anchor: None,
            line: 10,
            is_setext: false,
        });
        fi.add_heading(HeadingIndex {
            text: "Configuration".to_string(),
            auto_anchor: "configuration".to_string(),
            custom_anchor: None,
            line: 25,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    // Position cursor on "guide.md#installation"
    // Line 2: "See [install](guide.md#installation) here."
    let position = Position { line: 2, character: 18 };

    let result = server.handle_goto_definition(&current_uri, position).await;
    assert!(result.is_some(), "Should return a definition location for file+anchor");

    if let Some(GotoDefinitionResponse::Scalar(location)) = result {
        assert_eq!(
            location.uri,
            Url::from_file_path(&target_file).unwrap(),
            "Should point to guide.md"
        );
        // "Installation" heading is at line 10 (1-indexed) = line 9 (0-indexed)
        assert_eq!(location.range.start.line, 9, "Should target the Installation heading");
    } else {
        panic!("Expected Scalar response");
    }
}

#[tokio::test]
async fn test_goto_definition_same_file_anchor() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let file = std::path::PathBuf::from("/tmp/rumdl-nav-test3/readme.md");
    let uri = Url::from_file_path(&file).unwrap();

    // Content with a same-file anchor link
    let content = "# Title\n\nSee [below](#configuration) for config.\n\n## Configuration\n\nSettings here.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Populate workspace index with the file's headings
    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Title".to_string(),
            auto_anchor: "title".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        fi.add_heading(HeadingIndex {
            text: "Configuration".to_string(),
            auto_anchor: "configuration".to_string(),
            custom_anchor: None,
            line: 5,
            is_setext: false,
        });
        index.insert_file(file.clone(), fi);
    }

    // Position cursor on "#configuration" in `](#configuration)`
    // Line 2: "See [below](#configuration) for config."
    let position = Position { line: 2, character: 16 };

    let result = server.handle_goto_definition(&uri, position).await;
    assert!(result.is_some(), "Should return a definition for same-file anchor");

    if let Some(GotoDefinitionResponse::Scalar(location)) = result {
        assert_eq!(location.uri, uri, "Should point to the same file");
        // "Configuration" heading is at line 5 (1-indexed) = line 4 (0-indexed)
        assert_eq!(location.range.start.line, 4, "Should target the Configuration heading");
    } else {
        panic!("Expected Scalar response");
    }
}

#[tokio::test]
async fn test_goto_definition_cursor_not_on_link() {
    let server = create_test_server();

    let file = std::path::PathBuf::from("/tmp/rumdl-nav-test4/readme.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "# Title\n\nJust some plain text here.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Position cursor on plain text (no link)
    let position = Position { line: 2, character: 5 };

    let result = server.handle_goto_definition(&uri, position).await;
    assert!(result.is_none(), "Should return None when cursor is not on a link");
}

#[tokio::test]
async fn test_find_references_heading_with_incoming_links() {
    use crate::workspace_index::{CrossFileLinkIndex, FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-test5/docs");
    let target_file = docs_dir.join("guide.md");
    let source_file_a = docs_dir.join("index.md");
    let source_file_b = docs_dir.join("faq.md");

    let target_uri = Url::from_file_path(&target_file).unwrap();

    // Target file content with the heading
    let content = "# Installation\n\nHow to install.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Populate workspace index: target file has heading, two source files link to it
    {
        let mut index = server.workspace_index.write().await;

        // Target file with heading
        let mut target_fi = FileIndex::default();
        target_fi.add_heading(HeadingIndex {
            text: "Installation".to_string(),
            auto_anchor: "installation".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), target_fi);

        // Source file A links to guide.md#installation
        let mut source_a_fi = FileIndex::default();
        source_a_fi.cross_file_links.push(CrossFileLinkIndex {
            target_path: "guide.md".to_string(),
            fragment: "installation".to_string(),
            line: 5,
            column: 10,
        });
        index.insert_file(source_file_a.clone(), source_a_fi);

        // Source file B also links to guide.md#installation
        let mut source_b_fi = FileIndex::default();
        source_b_fi.cross_file_links.push(CrossFileLinkIndex {
            target_path: "guide.md".to_string(),
            fragment: "installation".to_string(),
            line: 3,
            column: 15,
        });
        index.insert_file(source_file_b.clone(), source_b_fi);
    }

    // Position cursor on the heading "# Installation" (line 0, any column)
    let position = Position { line: 0, character: 5 };

    let result = server.handle_references(&target_uri, position).await;
    assert!(result.is_some(), "Should find references to the heading");

    let locations = result.unwrap();
    assert_eq!(locations.len(), 2, "Should find 2 references from two files");

    let uris: Vec<_> = locations.iter().map(|l| l.uri.clone()).collect();
    assert!(
        uris.contains(&Url::from_file_path(&source_file_a).unwrap()),
        "Should include reference from index.md"
    );
    assert!(
        uris.contains(&Url::from_file_path(&source_file_b).unwrap()),
        "Should include reference from faq.md"
    );

    // Verify line/column conversion (1-indexed to 0-indexed)
    let a_loc = locations
        .iter()
        .find(|l| l.uri == Url::from_file_path(&source_file_a).unwrap())
        .unwrap();
    assert_eq!(
        a_loc.range.start.line, 4,
        "Line 5 (1-indexed) should become 4 (0-indexed)"
    );
    assert_eq!(
        a_loc.range.start.character, 9,
        "Column 10 (1-indexed) should become 9 (0-indexed)"
    );
}

#[tokio::test]
async fn test_find_references_heading_no_incoming_links() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let file = std::path::PathBuf::from("/tmp/rumdl-nav-test6/docs/lonely.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "# Lonely Heading\n\nNo one links here.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Lonely Heading".to_string(),
            auto_anchor: "lonely-heading".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(file.clone(), fi);
    }

    // Position cursor on the heading
    let position = Position { line: 0, character: 5 };

    let result = server.handle_references(&uri, position).await;
    assert!(result.is_none(), "Should return None when no references exist");
}

#[tokio::test]
async fn test_goto_definition_with_custom_anchor() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-test7/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();

    // Link uses a custom anchor
    let content = "# Index\n\nSee [install](guide.md#install) here.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Installation Guide".to_string(),
            auto_anchor: "installation-guide".to_string(),
            custom_anchor: Some("install".to_string()),
            line: 15,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    // Position cursor on the link target
    let position = Position { line: 2, character: 18 };

    let result = server.handle_goto_definition(&current_uri, position).await;
    assert!(result.is_some(), "Should resolve custom anchor");

    if let Some(GotoDefinitionResponse::Scalar(location)) = result {
        // Line 15 (1-indexed) = line 14 (0-indexed)
        assert_eq!(
            location.range.start.line, 14,
            "Should target the heading with the custom anchor"
        );
    } else {
        panic!("Expected Scalar response");
    }
}

#[tokio::test]
async fn test_goto_definition_anchor_not_found_falls_back_to_line_zero() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-test8/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();

    // Link to a non-existent anchor
    let content = "# Index\n\nSee [x](guide.md#nonexistent) here.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Introduction".to_string(),
            auto_anchor: "introduction".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    let position = Position { line: 2, character: 15 };

    let result = server.handle_goto_definition(&current_uri, position).await;
    assert!(result.is_some(), "Should still return a location for unresolved anchor");

    if let Some(GotoDefinitionResponse::Scalar(location)) = result {
        assert_eq!(
            location.range.start.line, 0,
            "Should fall back to line 0 when anchor not found"
        );
    } else {
        panic!("Expected Scalar response");
    }
}

#[tokio::test]
async fn test_find_references_from_link_position() {
    use crate::workspace_index::{CrossFileLinkIndex, FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-test9/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");
    let other_file = docs_dir.join("faq.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();

    // Current file contains a link -- find other links to the same target
    let content = "# Index\n\nSee [guide](guide.md) for info.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;

        let mut target_fi = FileIndex::default();
        target_fi.add_heading(HeadingIndex {
            text: "Guide".to_string(),
            auto_anchor: "guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), target_fi);

        // Current file links to guide.md (no fragment)
        let mut current_fi = FileIndex::default();
        current_fi.cross_file_links.push(CrossFileLinkIndex {
            target_path: "guide.md".to_string(),
            fragment: "".to_string(),
            line: 3,
            column: 12,
        });
        index.insert_file(current_file.clone(), current_fi);

        // Other file also links to guide.md (no fragment)
        let mut other_fi = FileIndex::default();
        other_fi.cross_file_links.push(CrossFileLinkIndex {
            target_path: "guide.md".to_string(),
            fragment: "".to_string(),
            line: 7,
            column: 5,
        });
        index.insert_file(other_file.clone(), other_fi);
    }

    // Position cursor on the link target "guide.md" in ](guide.md)
    let position = Position { line: 2, character: 16 };

    let result = server.handle_references(&current_uri, position).await;
    assert!(result.is_some(), "Should find references when cursor is on a link");

    let locations = result.unwrap();
    assert_eq!(locations.len(), 2, "Should find both links to guide.md");

    let uris: Vec<_> = locations.iter().map(|l| l.uri.clone()).collect();
    assert!(uris.contains(&Url::from_file_path(&current_file).unwrap()));
    assert!(uris.contains(&Url::from_file_path(&other_file).unwrap()));
}

#[tokio::test]
async fn test_find_references_from_target_file_without_selecting_link() {
    use crate::workspace_index::{CrossFileLinkIndex, FileIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-test9b/docs");
    let source_file = docs_dir.join("file-to-link-from.md");
    let target_file = docs_dir.join("file-to-link-to.md");

    let target_uri = Url::from_file_path(&target_file).unwrap();

    // Simulate a target file opened via go-to-definition. Cursor is not on a heading
    // or link target, but users still expect find-references to discover incoming links.
    let target_content = "---\ntitle: Heading\n---\n\nTarget file content.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: target_content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;

        let mut source_fi = FileIndex::default();
        source_fi.cross_file_links.push(CrossFileLinkIndex {
            target_path: "file-to-link-to.md".to_string(),
            fragment: "".to_string(),
            line: 5,
            column: 24,
        });
        index.insert_file(source_file.clone(), source_fi);
    }

    // Cursor at top of target file (typical after go-to-definition opens the file)
    let position = Position { line: 0, character: 0 };

    let result = server.handle_references(&target_uri, position).await;
    assert!(
        result.is_some(),
        "Should find incoming file-level references even when cursor is not on a link"
    );

    let locations = result.unwrap();
    assert_eq!(locations.len(), 1, "Should find one incoming link");
    assert_eq!(
        locations[0].uri,
        Url::from_file_path(&source_file).unwrap(),
        "Reference should point to the linking source file"
    );
    assert_eq!(locations[0].range.start.line, 4);
}

#[tokio::test]
async fn test_goto_definition_link_with_title() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-test10/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();

    // Link with a title attribute
    let content = "# Index\n\nSee [guide](guide.md \"The Guide\") for details.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Guide".to_string(),
            auto_anchor: "guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    // Position cursor on "guide.md" inside `](guide.md "The Guide")`
    // Line 2: `See [guide](guide.md "The Guide") for details.`
    let position = Position { line: 2, character: 16 };

    let result = server.handle_goto_definition(&current_uri, position).await;
    assert!(result.is_some(), "Should resolve link target even with title attribute");

    if let Some(GotoDefinitionResponse::Scalar(location)) = result {
        assert_eq!(
            location.uri,
            Url::from_file_path(&target_file).unwrap(),
            "Should point to guide.md despite title in link"
        );
        assert_eq!(location.range.start.line, 0, "Should target line 0");
    } else {
        panic!("Expected Scalar response");
    }
}

#[tokio::test]
async fn test_goto_definition_angle_bracket_link() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-test11/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();

    // Angle-bracket link target
    let content = "# Index\n\nSee [guide](<guide.md>) for details.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Guide".to_string(),
            auto_anchor: "guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    // Position cursor inside `](<guide.md>)`
    let position = Position { line: 2, character: 16 };

    let result = server.handle_goto_definition(&current_uri, position).await;
    assert!(result.is_some(), "Should resolve angle-bracket link target");

    if let Some(GotoDefinitionResponse::Scalar(location)) = result {
        assert_eq!(
            location.uri,
            Url::from_file_path(&target_file).unwrap(),
            "Should point to guide.md despite angle brackets"
        );
    } else {
        panic!("Expected Scalar response");
    }
}

#[tokio::test]
async fn test_find_references_includes_same_file_fragment_links() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let file = std::path::PathBuf::from("/tmp/rumdl-nav-test12/docs/readme.md");
    let uri = Url::from_file_path(&file).unwrap();

    // File with a heading and a same-file fragment link to it
    let content = "# Installation\n\nSee [above](#installation) for details.\n\nMore text here.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Installation".to_string(),
            auto_anchor: "installation".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(file.clone(), fi);
    }

    // Position cursor on the heading (line 0)
    let position = Position { line: 0, character: 5 };

    let result = server.handle_references(&uri, position).await;
    assert!(
        result.is_some(),
        "Should find same-file fragment references to the heading"
    );

    let locations = result.unwrap();
    assert_eq!(locations.len(), 1, "Should find the same-file #installation link");
    assert_eq!(locations[0].range.start.line, 2, "Reference should be on line 2");
}

// =============================================================================
// Navigation: external URL returns None
// =============================================================================

#[tokio::test]
async fn test_goto_definition_external_url_returns_none() {
    let server = create_test_server();

    let file = std::path::PathBuf::from("/tmp/rumdl-nav-url-test/readme.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "See [example](https://example.com) for details.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Cursor on "https://example.com"
    let position = Position { line: 0, character: 18 };
    let result = server.handle_goto_definition(&uri, position).await;
    assert!(result.is_none(), "Should return None for external URLs");
}

#[tokio::test]
async fn test_goto_definition_mailto_returns_none() {
    let server = create_test_server();

    let file = std::path::PathBuf::from("/tmp/rumdl-nav-mailto-test/readme.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "Email [us](mailto:info@example.com) for help.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    let position = Position { line: 0, character: 15 };
    let result = server.handle_goto_definition(&uri, position).await;
    assert!(result.is_none(), "Should return None for mailto: links");
}

// =============================================================================
// Navigation: reference-style links
// =============================================================================

#[tokio::test]
async fn test_goto_definition_reference_link() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-ref-test/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();

    // Full reference link: [click here][guide]
    let content = "# Index\n\nSee [click here][guide] for info.\n\n[guide]: guide.md#install\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Installation".to_string(),
            auto_anchor: "installation".to_string(),
            custom_anchor: Some("install".to_string()),
            line: 10,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    // Cursor on "click here" — inside the first bracket pair
    let position = Position { line: 2, character: 8 };
    let result = server.handle_goto_definition(&current_uri, position).await;
    assert!(result.is_some(), "Should resolve full reference link");

    if let Some(GotoDefinitionResponse::Scalar(location)) = result {
        assert_eq!(
            location.uri,
            Url::from_file_path(&target_file).unwrap(),
            "Should point to guide.md"
        );
        assert_eq!(location.range.start.line, 9, "Should target the install heading");
    } else {
        panic!("Expected Scalar response");
    }
}

#[tokio::test]
async fn test_goto_definition_collapsed_reference() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-collapsed-test/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();

    // Collapsed reference: [guide][]
    let content = "# Index\n\nSee [guide][] for info.\n\n[guide]: guide.md\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Guide".to_string(),
            auto_anchor: "guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    // Cursor on "guide" in [guide][]
    let position = Position { line: 2, character: 7 };
    let result = server.handle_goto_definition(&current_uri, position).await;
    assert!(result.is_some(), "Should resolve collapsed reference link");

    if let Some(GotoDefinitionResponse::Scalar(location)) = result {
        assert_eq!(
            location.uri,
            Url::from_file_path(&target_file).unwrap(),
            "Should point to guide.md"
        );
    } else {
        panic!("Expected Scalar response");
    }
}

#[tokio::test]
async fn test_goto_definition_reference_definition_line() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-refdef-test/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();

    // Cursor on the reference definition line itself
    let content = "# Index\n\n[guide]: guide.md#install\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Installation".to_string(),
            auto_anchor: "installation".to_string(),
            custom_anchor: Some("install".to_string()),
            line: 10,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    // Cursor on the definition line
    let position = Position { line: 2, character: 5 };
    let result = server.handle_goto_definition(&current_uri, position).await;
    assert!(result.is_some(), "Should navigate from reference definition line");

    if let Some(GotoDefinitionResponse::Scalar(location)) = result {
        assert_eq!(location.uri, Url::from_file_path(&target_file).unwrap(),);
        assert_eq!(location.range.start.line, 9);
    } else {
        panic!("Expected Scalar response");
    }
}

#[tokio::test]
async fn test_goto_definition_reference_link_external_url() {
    let server = create_test_server();

    let file = std::path::PathBuf::from("/tmp/rumdl-nav-ref-ext-test/readme.md");
    let uri = Url::from_file_path(&file).unwrap();

    // Reference link that resolves to an external URL
    let content = "See [example] for info.\n\n[example]: https://example.com\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Cursor on "example" in [example]
    let position = Position { line: 0, character: 7 };
    let result = server.handle_goto_definition(&uri, position).await;
    assert!(
        result.is_none(),
        "Should return None for reference link resolving to external URL"
    );
}

// =============================================================================
// Narrow range / Zed code_actions_on_format tests
// =============================================================================

/// Demonstrates that a narrow range (cursor position) prevents the fixAll action
/// from being created, even when fixable warnings exist elsewhere in the document.
/// This is the likely root cause for Zed's code_actions_on_format failing: Zed
/// sends the cursor position as the range, but fixable_count only counts
/// in-range warnings.
#[tokio::test]
async fn test_fix_all_action_available_regardless_of_range() {
    let server = create_test_server();

    let uri = Url::parse("file:///test.md").unwrap();
    // Line 0: "# Title"       -- no fixable issues here
    // Line 1: ""              -- blank line
    // Line 2: "\tTabbed text" -- hard tab (MD010, fixable)
    let text = "# Title\n\n\tTabbed text\n";

    // Narrow range: only line 0 (where Zed cursor might be)
    let narrow_range = Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: 0, character: 0 },
    };

    let actions = server.get_code_actions(&uri, text, narrow_range).await.unwrap();

    let fix_all_actions: Vec<_> = actions
        .iter()
        .filter(|a| a.kind.as_ref().is_some_and(|k| k.as_str() == "source.fixAll.rumdl"))
        .collect();

    // source.fixAll.rumdl counts fixable warnings across the entire document,
    // so it should appear even when the cursor is on a line without warnings
    assert!(
        !fix_all_actions.is_empty(),
        "fixAll should be created regardless of cursor position when document has fixable issues"
    );

    // Full document range should also have fixAll
    let full_range = Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: 3, character: 0 },
    };

    let actions = server.get_code_actions(&uri, text, full_range).await.unwrap();

    let fix_all_actions: Vec<_> = actions
        .iter()
        .filter(|a| a.kind.as_ref().is_some_and(|k| k.as_str() == "source.fixAll.rumdl"))
        .collect();

    assert!(
        !fix_all_actions.is_empty(),
        "fixAll should be created with full document range"
    );
}

/// Verifies that fixAll fixes ALL document issues, not just those in the requested range.
#[tokio::test]
async fn test_fix_all_applies_all_document_fixes_regardless_of_range() {
    let server = create_test_server();

    let uri = Url::parse("file:///test.md").unwrap();
    // Two fixable issues on different lines (hard tabs → MD010):
    // Line 2: "\tFirst issue"
    // Line 4: "\tSecond issue"
    let text = "# Title\n\n\tFirst issue\n\n\tSecond issue\n";

    // Range covering only line 2 (first issue)
    let partial_range = Range {
        start: Position { line: 2, character: 0 },
        end: Position { line: 2, character: 13 },
    };

    let actions = server.get_code_actions(&uri, text, partial_range).await.unwrap();

    let fix_all_actions: Vec<_> = actions
        .iter()
        .filter(|a| a.kind.as_ref().is_some_and(|k| k.as_str() == "source.fixAll.rumdl"))
        .collect();

    // fixAll should be created because there are fixable issues in the document
    assert!(
        !fix_all_actions.is_empty(),
        "fixAll should be created when the document has fixable issues"
    );

    // Verify the fixed content addresses BOTH issues, not just the in-range one
    let fix_all = &fix_all_actions[0];
    let edit = fix_all.edit.as_ref().expect("fixAll should have an edit");
    let changes = edit.changes.as_ref().expect("edit should have changes");
    let text_edits = changes.get(&uri).expect("changes should include our file");
    let fixed_text = &text_edits[0].new_text;

    assert!(
        !fixed_text.contains('\t'),
        "fixAll should fix all tab issues in the document, not just those in range"
    );
}

/// Test issue #210: Config cache serves stale config when config file is created or modified
///
/// Scenario:
/// 1. User opens a project with no .rumdl.toml (default indent=2 for MD007)
/// 2. Config cache populates with default config (config_file: None, from_global_fallback: true)
/// 3. User creates/updates .rumdl.toml with [MD007] indent=4
/// 4. resolve_config_for_file returns cached default (stale!) instead of new config
///
/// Root cause: The cache invalidation in did_change_watched_files only removes entries
/// where config_file matches the changed path. Entries with config_file: None (global
/// fallback) are never invalidated when a new config file appears.
///
/// Additionally, the server only registers file watchers for markdown files, not config
/// files, so did_change_watched_files may never fire for .rumdl.toml changes.
#[tokio::test]
async fn test_config_cache_stale_after_config_file_created() {
    use std::fs;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("project");
    fs::create_dir(&project).unwrap();

    let test_file = project.join("test.md");
    fs::write(&test_file, "# Test\n").unwrap();

    let server = create_test_server();
    {
        let mut roots = server.workspace_roots.write().await;
        roots.push(project.clone());
    }

    // Step 1: Resolve config with NO .rumdl.toml present -> should get default (indent=2)
    let config_before = server.resolve_config_for_file(&test_file).await;
    let indent_before = crate::config::get_rule_config_value::<usize>(&config_before, "MD007", "indent");
    // Default MD007 indent is 2 (or None if not in config, which means default applies)
    assert!(
        indent_before.is_none() || indent_before == Some(2),
        "Before config file exists, MD007 indent should be default (2 or absent). Got: {indent_before:?}"
    );

    // Verify cache was populated with fallback entry
    {
        let cache = server.config_cache.read().await;
        let entry = cache
            .get(&project)
            .expect("Cache should be populated after first resolve");
        assert!(
            entry.from_global_fallback,
            "Cache entry should be from global fallback since no config file exists"
        );
        assert!(
            entry.config_file.is_none(),
            "Cache entry should have no config_file since it's a global fallback"
        );
    }

    // Step 2: Create .rumdl.toml with indent=4
    let config_path = project.join(".rumdl.toml");
    fs::write(
        &config_path,
        r#"
[MD007]
indent = 4
"#,
    )
    .unwrap();

    // Step 3: Resolve config again WITHOUT clearing cache
    // The cache detects that a config file now exists and re-resolves instead of
    // serving the stale global fallback entry.
    let config_after = server.resolve_config_for_file(&test_file).await;
    let indent_after = crate::config::get_rule_config_value::<usize>(&config_after, "MD007", "indent");

    assert_eq!(
        indent_after,
        Some(4),
        "After .rumdl.toml is created, resolve_config_for_file should pick up the new config. \
         Expected MD007 indent=4, got {indent_after:?}"
    );

    // Verify the cache was updated with the new config file entry
    {
        let cache = server.config_cache.read().await;
        let entry = cache.get(&project).expect("Cache entry should exist after re-resolve");
        assert!(
            !entry.from_global_fallback,
            "Cache entry should no longer be a global fallback after config file was discovered"
        );
        assert!(
            entry.config_file.is_some(),
            "Cache entry should reference the newly discovered config file"
        );
    }
}

/// Test that manually clearing the config cache picks up new config
///
/// This verifies the workaround: if the cache is cleared (e.g., via did_change_configuration),
/// the new config file is correctly discovered.
#[tokio::test]
async fn test_config_cache_picks_up_new_config_after_manual_clear() {
    use std::fs;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("project");
    fs::create_dir(&project).unwrap();

    let test_file = project.join("test.md");
    fs::write(&test_file, "# Test\n").unwrap();

    let server = create_test_server();
    {
        let mut roots = server.workspace_roots.write().await;
        roots.push(project.clone());
    }

    // Resolve config with no config file -> populates cache with default
    let config_before = server.resolve_config_for_file(&test_file).await;
    let indent_before = crate::config::get_rule_config_value::<usize>(&config_before, "MD007", "indent");
    assert!(
        indent_before.is_none() || indent_before == Some(2),
        "Should start with default indent"
    );

    // Create config file
    let config_path = project.join(".rumdl.toml");
    fs::write(
        &config_path,
        r#"
[MD007]
indent = 4
"#,
    )
    .unwrap();

    // Manually clear cache (simulates what did_change_configuration does)
    server.config_cache.write().await.clear();

    // Now resolve again - should pick up the new config
    let config_after = server.resolve_config_for_file(&test_file).await;
    let indent_after = crate::config::get_rule_config_value::<usize>(&config_after, "MD007", "indent");
    assert_eq!(
        indent_after,
        Some(4),
        "After cache clear, should pick up new config with indent=4"
    );
}

/// Test that did_change_watched_files retains stale fallback cache entries
///
/// When a config file is modified, did_change_watched_files invalidates cache entries
/// whose config_file path matches. But entries with config_file=None (global fallback)
/// survive, even though a new config file now exists in that directory.
#[tokio::test]
async fn test_config_cache_retain_logic_misses_fallback_entries() {
    use std::fs;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("project");
    fs::create_dir(&project).unwrap();

    let test_file = project.join("test.md");
    fs::write(&test_file, "# Test\n").unwrap();

    let server = create_test_server();
    {
        let mut roots = server.workspace_roots.write().await;
        roots.push(project.clone());
    }

    // Populate cache with fallback entry (no config file)
    let _ = server.resolve_config_for_file(&test_file).await;

    // Create config file
    let config_path = project.join(".rumdl.toml");
    fs::write(
        &config_path,
        r#"
[MD007]
indent = 4
"#,
    )
    .unwrap();

    // Simulate did_change_watched_files config invalidation logic
    // This is the retain logic from server.rs lines 844-852
    {
        let mut cache = server.config_cache.write().await;
        cache.retain(|_, entry| {
            if let Some(config_file) = &entry.config_file {
                config_file != &config_path
            } else {
                true // BUG: fallback entries (config_file=None) are always retained
            }
        });
    }

    // The fallback entry should have been removed, but the retain logic keeps it
    let cache = server.config_cache.read().await;
    let entry = cache.get(&project);
    assert!(
        entry.is_some(),
        "BUG: Fallback cache entry survives retain() because config_file is None"
    );
    assert!(
        entry.unwrap().from_global_fallback,
        "The surviving entry is the stale global fallback"
    );
}

// ─── Embedded markdown and code-block-tools integration tests ───

/// Helper to create a code-block-tools config with rumdl enabled for markdown blocks
fn make_embedded_markdown_config() -> crate::code_block_tools::CodeBlockToolsConfig {
    let lang = crate::code_block_tools::LanguageToolConfig {
        enabled: true,
        lint: vec!["rumdl".to_string()],
        format: Vec::new(),
        on_error: None,
    };
    let mut languages = std::collections::HashMap::new();
    languages.insert("markdown".to_string(), lang);
    crate::code_block_tools::CodeBlockToolsConfig {
        enabled: true,
        languages,
        ..Default::default()
    }
}

#[tokio::test]
async fn test_lint_document_embedded_markdown_when_enabled() {
    let server = create_test_server();

    // Enable code-block-tools with rumdl for markdown
    {
        let mut cfg = server.rumdl_config.write().await;
        cfg.code_block_tools = make_embedded_markdown_config();
    }

    let uri = Url::parse("file:///test.md").unwrap();
    // The embedded markdown block has trailing spaces (MD009 violation)
    let text = "# Test\n\n```markdown\n# Hello  \n```\n";

    let diagnostics = server.lint_document(&uri, text, true).await.unwrap();

    // Should contain a diagnostic from the embedded block (trailing spaces on line 4)
    let embedded_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            // Line 4 (0-indexed: 3) is the "# Hello  " line inside the code block
            d.range.start.line == 3
        })
        .collect();

    assert!(
        !embedded_diags.is_empty(),
        "Expected embedded markdown diagnostics for trailing spaces inside code block, got none. All diagnostics: {diagnostics:?}"
    );
}

#[tokio::test]
async fn test_lint_document_no_embedded_markdown_when_disabled() {
    let server = create_test_server();

    // code_block_tools defaults to disabled
    let uri = Url::parse("file:///test.md").unwrap();
    let text = "# Test\n\n```markdown\n# Hello  \n```\n";

    let diagnostics = server.lint_document(&uri, text, true).await.unwrap();

    // No diagnostics should come from the embedded block (line 4, 0-indexed: 3)
    // since code-block-tools is not enabled
    let embedded_diags: Vec<_> = diagnostics.iter().filter(|d| d.range.start.line == 3).collect();

    assert!(
        embedded_diags.is_empty(),
        "Expected no embedded markdown diagnostics when code-block-tools is disabled, but got: {embedded_diags:?}"
    );
}

#[tokio::test]
async fn test_lint_document_embedded_markdown_empty_block() {
    let server = create_test_server();

    {
        let mut cfg = server.rumdl_config.write().await;
        cfg.code_block_tools = make_embedded_markdown_config();
    }

    let uri = Url::parse("file:///test.md").unwrap();
    // Empty embedded markdown block should produce no extra diagnostics
    let text = "# Test\n\n```markdown\n```\n";

    let diagnostics = server.lint_document(&uri, text, true).await.unwrap();

    // No diagnostics from the embedded block (it's empty)
    let embedded_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.range.start.line >= 2 && d.range.start.line <= 3)
        .collect();

    assert!(
        embedded_diags.is_empty(),
        "Expected no diagnostics from empty embedded markdown block, got: {embedded_diags:?}"
    );
}

#[tokio::test]
async fn test_lint_document_multiple_embedded_blocks() {
    let server = create_test_server();

    {
        let mut cfg = server.rumdl_config.write().await;
        cfg.code_block_tools = make_embedded_markdown_config();
    }

    let uri = Url::parse("file:///test.md").unwrap();
    // Two markdown blocks, each with trailing spaces
    let text = "# Test\n\n```markdown\n# One  \n```\n\n```markdown\n# Two  \n```\n";

    let diagnostics = server.lint_document(&uri, text, true).await.unwrap();

    // Should have diagnostics from both embedded blocks
    let block1_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.range.start.line == 3) // "# One  " is line 4 (0-indexed: 3)
        .collect();

    let block2_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.range.start.line == 7) // "# Two  " is line 8 (0-indexed: 7)
        .collect();

    assert!(
        !block1_diags.is_empty(),
        "Expected diagnostics from first embedded block. All diagnostics: {diagnostics:?}"
    );
    assert!(
        !block2_diags.is_empty(),
        "Expected diagnostics from second embedded block. All diagnostics: {diagnostics:?}"
    );
}

#[tokio::test]
async fn test_lint_document_embedded_markdown_md_alias() {
    let server = create_test_server();

    // Enable for "md" alias instead of "markdown"
    {
        let mut cfg = server.rumdl_config.write().await;
        let mut languages = std::collections::HashMap::new();
        languages.insert(
            "md".to_string(),
            crate::code_block_tools::LanguageToolConfig {
                enabled: true,
                lint: vec!["rumdl".to_string()],
                format: Vec::new(),
                on_error: None,
            },
        );
        cfg.code_block_tools = crate::code_block_tools::CodeBlockToolsConfig {
            enabled: true,
            languages,
            ..Default::default()
        };
    }

    let uri = Url::parse("file:///test.md").unwrap();
    let text = "# Test\n\n```md\n# Hello  \n```\n";

    let diagnostics = server.lint_document(&uri, text, true).await.unwrap();

    let embedded_diags: Vec<_> = diagnostics.iter().filter(|d| d.range.start.line == 3).collect();

    assert!(
        !embedded_diags.is_empty(),
        "Expected embedded markdown diagnostics for `md` alias. All diagnostics: {diagnostics:?}"
    );
}

// =============================================================================
// find-references fallback: cursor not on heading or link
// =============================================================================

/// When cursor is in a target file but not on a heading or link,
/// find-references should return all cross-file links pointing to that file.
#[tokio::test]
async fn test_find_references_fallback_to_file_links() {
    use crate::workspace_index::{CrossFileLinkIndex, FileIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-test-fallback/docs");
    let target_file = docs_dir.join("file-to-link-to.md");
    let source_file = docs_dir.join("file-to-link-from.md");

    let target_uri = Url::from_file_path(&target_file).unwrap();

    // Target file content: cursor on a plain text line (not heading, not link)
    let content = "---\ntitle: Heading\n---\n\nWe are linking to this file from `file-to-link-from.md`.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Populate workspace index
    {
        let mut index = server.workspace_index.write().await;

        // Target file has no headings relevant to the query
        let target_fi = FileIndex::default();
        index.insert_file(target_file.clone(), target_fi);

        // Source file links to target file (file-only link, no fragment)
        let mut source_fi = FileIndex::default();
        source_fi.cross_file_links.push(CrossFileLinkIndex {
            target_path: "file-to-link-to.md".to_string(),
            fragment: "".to_string(),
            line: 5,
            column: 22,
        });
        index.insert_file(source_file.clone(), source_fi);
    }

    // Cursor on line 4 (0-indexed), plain text - not on a heading or link
    let position = Position { line: 4, character: 10 };

    let result = server.handle_references(&target_uri, position).await;
    assert!(result.is_some(), "Should find references to the file via fallback");

    let locations = result.unwrap();
    assert_eq!(locations.len(), 1, "Should find 1 reference from source file");
    assert_eq!(
        locations[0].uri,
        Url::from_file_path(&source_file).unwrap(),
        "Reference should come from the source file"
    );
}

/// When cursor is in a file with no incoming references and cursor is not on
/// a heading or link, find-references should return None.
#[tokio::test]
async fn test_find_references_fallback_no_references() {
    use crate::workspace_index::FileIndex;

    let server = create_test_server();

    let file = std::path::PathBuf::from("/tmp/rumdl-nav-test-fallback2/docs/lonely.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "Some text without anything special.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let fi = FileIndex::default();
        index.insert_file(file.clone(), fi);
    }

    let position = Position { line: 0, character: 5 };

    let result = server.handle_references(&uri, position).await;
    assert!(result.is_none(), "Should return None when no references exist");
}

/// When multiple files link to the same target, the fallback should return all of them.
#[tokio::test]
async fn test_find_references_fallback_multiple_sources() {
    use crate::workspace_index::{CrossFileLinkIndex, FileIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-test-fallback3/docs");
    let target_file = docs_dir.join("target.md");
    let source_a = docs_dir.join("a.md");
    let source_b = docs_dir.join("b.md");
    let source_c = docs_dir.join("c.md");

    let target_uri = Url::from_file_path(&target_file).unwrap();

    let content = "This file is referenced by multiple others.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;

        let target_fi = FileIndex::default();
        index.insert_file(target_file.clone(), target_fi);

        // Three files link to target.md with various fragments
        for (source, fragment, line) in [(&source_a, "", 2), (&source_b, "section", 5), (&source_c, "", 10)] {
            let mut fi = FileIndex::default();
            fi.cross_file_links.push(CrossFileLinkIndex {
                target_path: "target.md".to_string(),
                fragment: fragment.to_string(),
                line,
                column: 1,
            });
            index.insert_file(source.clone(), fi);
        }
    }

    // Cursor not on heading or link
    let position = Position { line: 0, character: 5 };

    let result = server.handle_references(&target_uri, position).await;
    assert!(result.is_some(), "Should find references from multiple files");

    let locations = result.unwrap();
    assert_eq!(
        locations.len(),
        3,
        "Should find all 3 references regardless of fragment"
    );

    let uris: std::collections::HashSet<_> = locations.iter().map(|l| l.uri.clone()).collect();
    assert!(uris.contains(&Url::from_file_path(&source_a).unwrap()));
    assert!(uris.contains(&Url::from_file_path(&source_b).unwrap()));
    assert!(uris.contains(&Url::from_file_path(&source_c).unwrap()));
}

/// When cursor is on a heading, heading-specific references take priority over
/// the file-level fallback (ensuring the heading path still works correctly).
#[tokio::test]
async fn test_find_references_heading_takes_priority_over_fallback() {
    use crate::workspace_index::{CrossFileLinkIndex, FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-nav-test-fallback4/docs");
    let target_file = docs_dir.join("guide.md");
    let source_with_anchor = docs_dir.join("a.md");
    let source_without_anchor = docs_dir.join("b.md");

    let target_uri = Url::from_file_path(&target_file).unwrap();

    let content = "# Installation\n\nHow to install.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;

        let mut target_fi = FileIndex::default();
        target_fi.add_heading(HeadingIndex {
            text: "Installation".to_string(),
            auto_anchor: "installation".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), target_fi);

        // Source A links to guide.md#installation (matches heading anchor)
        let mut fi_a = FileIndex::default();
        fi_a.cross_file_links.push(CrossFileLinkIndex {
            target_path: "guide.md".to_string(),
            fragment: "installation".to_string(),
            line: 3,
            column: 5,
        });
        index.insert_file(source_with_anchor.clone(), fi_a);

        // Source B links to guide.md (no anchor - only visible in file-level fallback)
        let mut fi_b = FileIndex::default();
        fi_b.cross_file_links.push(CrossFileLinkIndex {
            target_path: "guide.md".to_string(),
            fragment: "".to_string(),
            line: 7,
            column: 10,
        });
        index.insert_file(source_without_anchor.clone(), fi_b);
    }

    // Cursor on heading - should only find anchor-specific references
    let position = Position { line: 0, character: 5 };

    let result = server.handle_references(&target_uri, position).await;
    assert!(result.is_some(), "Should find heading-specific references");

    let locations = result.unwrap();
    assert_eq!(
        locations.len(),
        1,
        "Should only find anchor-matching reference, not file-level"
    );
    assert_eq!(
        locations[0].uri,
        Url::from_file_path(&source_with_anchor).unwrap(),
        "Should find the anchor-specific reference"
    );
}

// =============================================================================
// Hover preview tests
// =============================================================================

#[tokio::test]
async fn test_hover_inline_link_to_file() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-hover-test1/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();
    let target_uri = Url::from_file_path(&target_file).unwrap();

    let content = "# Index\n\nSee [the guide](guide.md) for details.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Add target file content to document cache
    let target_content = "# Guide\n\nThis is the guide.\n\nIt has multiple lines.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: target_content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Populate workspace index
    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Guide".to_string(),
            auto_anchor: "guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    // Cursor on "guide.md" in `](guide.md)`
    let position = Position { line: 2, character: 20 };

    let result = server.handle_hover(&current_uri, position).await;
    assert!(result.is_some(), "Should return hover for file link");

    let hover = result.unwrap();
    if let HoverContents::Markup(markup) = hover.contents {
        assert!(markup.value.contains("guide.md"), "Should contain filename");
        assert!(
            markup.value.contains("This is the guide"),
            "Should contain file content"
        );
    } else {
        panic!("Expected Markup hover contents");
    }
}

#[tokio::test]
async fn test_hover_inline_link_with_anchor() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-hover-test2/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();
    let target_uri = Url::from_file_path(&target_file).unwrap();

    let content = "# Index\n\nSee [install](guide.md#installation) here.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    let target_content = "# Guide\n\nIntro text.\n\n## Installation\n\nRun `cargo install rumdl`.\n\nThen configure.\n\n## Usage\n\nRun it.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: target_content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Guide".to_string(),
            auto_anchor: "guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        fi.add_heading(HeadingIndex {
            text: "Installation".to_string(),
            auto_anchor: "installation".to_string(),
            custom_anchor: None,
            line: 5,
            is_setext: false,
        });
        fi.add_heading(HeadingIndex {
            text: "Usage".to_string(),
            auto_anchor: "usage".to_string(),
            custom_anchor: None,
            line: 11,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    // Cursor on the link target
    let position = Position { line: 2, character: 18 };

    let result = server.handle_hover(&current_uri, position).await;
    assert!(result.is_some(), "Should return hover for anchor link");

    let hover = result.unwrap();
    if let HoverContents::Markup(markup) = hover.contents {
        assert!(
            markup.value.contains("## Installation"),
            "Should contain the heading: got '{}'",
            markup.value
        );
        assert!(
            markup.value.contains("cargo install rumdl"),
            "Should contain section content"
        );
        // Should NOT contain the next heading's content
        assert!(
            !markup.value.contains("## Usage"),
            "Should stop at next heading of equal level"
        );
    } else {
        panic!("Expected Markup hover contents");
    }
}

#[tokio::test]
async fn test_hover_reference_style_link() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-hover-test3/docs");
    let current_file = docs_dir.join("readme.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();
    let target_uri = Url::from_file_path(&target_file).unwrap();

    let content = "See [the guide][guide] for details.\n\n[guide]: guide.md\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    let target_content = "# Guide\n\nWelcome to the guide.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: target_content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Guide".to_string(),
            auto_anchor: "guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    // Cursor on "the guide" in [the guide][guide]
    let position = Position { line: 0, character: 8 };

    let result = server.handle_hover(&current_uri, position).await;
    assert!(result.is_some(), "Should return hover for reference-style link");

    let hover = result.unwrap();
    if let HoverContents::Markup(markup) = hover.contents {
        assert!(markup.value.contains("guide.md"), "Should contain filename");
        assert!(
            markup.value.contains("Welcome to the guide"),
            "Should contain file content"
        );
    } else {
        panic!("Expected Markup hover contents");
    }
}

#[tokio::test]
async fn test_hover_external_url() {
    let server = create_test_server();

    let file = std::path::PathBuf::from("/tmp/rumdl-hover-test4/readme.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "Visit [example](https://example.com) for more.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Cursor on "https://example.com"
    let position = Position { line: 0, character: 18 };

    let result = server.handle_hover(&uri, position).await;
    assert!(result.is_some(), "Should return hover for external URL");

    let hover = result.unwrap();
    if let HoverContents::Markup(markup) = hover.contents {
        assert!(markup.value.contains("https://example.com"), "Should show the URL");
        assert!(markup.value.contains("External link"), "Should indicate it's external");
    } else {
        panic!("Expected Markup hover contents");
    }
}

#[tokio::test]
async fn test_hover_plain_text_returns_none() {
    let server = create_test_server();

    let file = std::path::PathBuf::from("/tmp/rumdl-hover-test5/readme.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "Just some plain text here.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Cursor on plain text
    let position = Position { line: 0, character: 5 };

    let result = server.handle_hover(&uri, position).await;
    assert!(result.is_none(), "Should return None when cursor is not on a link");
}

#[tokio::test]
async fn test_hover_nonexistent_file_returns_none() {
    let server = create_test_server();

    let file = std::path::PathBuf::from("/tmp/rumdl-hover-test6/readme.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "See [missing](nonexistent.md) file.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Cursor on the link target
    let position = Position { line: 0, character: 18 };

    let result = server.handle_hover(&uri, position).await;
    assert!(result.is_none(), "Should return None for link to nonexistent file");
}

#[tokio::test]
async fn test_hover_same_file_anchor() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let file = std::path::PathBuf::from("/tmp/rumdl-hover-test7/readme.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "# Title\n\nSee [below](#configuration) for config.\n\nSome text.\n\n## Configuration\n\nSet `key = value` in config.\n\nMore config details.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Title".to_string(),
            auto_anchor: "title".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        fi.add_heading(HeadingIndex {
            text: "Configuration".to_string(),
            auto_anchor: "configuration".to_string(),
            custom_anchor: None,
            line: 7,
            is_setext: false,
        });
        index.insert_file(file.clone(), fi);
    }

    // Cursor on "#configuration" in `](#configuration)`
    let position = Position { line: 2, character: 16 };

    let result = server.handle_hover(&uri, position).await;
    assert!(result.is_some(), "Should return hover for same-file anchor");

    let hover = result.unwrap();
    if let HoverContents::Markup(markup) = hover.contents {
        assert!(markup.value.contains("## Configuration"), "Should contain the heading");
        assert!(markup.value.contains("key = value"), "Should contain section content");
    } else {
        panic!("Expected Markup hover contents");
    }
}

#[tokio::test]
async fn test_hover_file_preview_truncates() {
    use crate::workspace_index::FileIndex;

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-hover-test8/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("long.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();
    let target_uri = Url::from_file_path(&target_file).unwrap();

    let content = "See [long file](long.md) here.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Create a target file with more than 15 lines
    let long_content: String = (1..=30).map(|i| format!("Line {i}\n")).collect();
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: long_content.clone(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        index.insert_file(target_file.clone(), FileIndex::default());
    }

    let position = Position { line: 0, character: 18 };

    let result = server.handle_hover(&current_uri, position).await;
    assert!(result.is_some(), "Should return hover for long file");

    let hover = result.unwrap();
    if let HoverContents::Markup(markup) = hover.contents {
        assert!(markup.value.contains("Line 1"), "Should contain first line");
        assert!(markup.value.contains("Line 15"), "Should contain line 15");
        assert!(!markup.value.contains("Line 16"), "Should not contain line 16");
        assert!(markup.value.contains("..."), "Should indicate truncation");
    } else {
        panic!("Expected Markup hover contents");
    }
}

#[tokio::test]
async fn test_hover_anchor_section_end_no_ellipsis() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-hover-test9/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();
    let target_uri = Url::from_file_path(&target_file).unwrap();

    let content = "See [install](guide.md#install) here.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Target has two sections — "Install" is short, "Usage" follows
    let target_content = "# Guide\n\n## Install\n\nRun `cargo install`.\n\n## Usage\n\nRun the binary.\n\nMore usage info.\n\nEven more.\n\nAnd more.\n\nAnd more.\n\nAnd more.\n\nAnd more.\n\nAnd more.\n\nAnd more.\n\nAnd more.\n\nAnd more.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: target_content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Guide".to_string(),
            auto_anchor: "guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        fi.add_heading(HeadingIndex {
            text: "Install".to_string(),
            auto_anchor: "install".to_string(),
            custom_anchor: None,
            line: 3,
            is_setext: false,
        });
        fi.add_heading(HeadingIndex {
            text: "Usage".to_string(),
            auto_anchor: "usage".to_string(),
            custom_anchor: None,
            line: 7,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    let position = Position { line: 0, character: 18 };

    let result = server.handle_hover(&current_uri, position).await;
    assert!(result.is_some(), "Should return hover for anchor link");

    let hover = result.unwrap();
    if let HoverContents::Markup(markup) = hover.contents {
        assert!(markup.value.contains("## Install"), "Should contain the heading");
        assert!(markup.value.contains("cargo install"), "Should contain section content");
        assert!(!markup.value.contains("## Usage"), "Should stop at next heading");
        assert!(
            !markup.value.contains("..."),
            "Should NOT show ellipsis when section ended at heading boundary"
        );
    } else {
        panic!("Expected Markup hover contents");
    }
}

#[tokio::test]
async fn test_hover_anchor_preview_skips_code_block_hashes() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-hover-test10/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();
    let target_uri = Url::from_file_path(&target_file).unwrap();

    let content = "See [config](guide.md#configuration) here.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Target has a code block with `# comment` that looks like a heading
    let target_content = "## Configuration\n\nExample:\n\n```python\n# This is a Python comment\nconfig = True\n```\n\nMore config info.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: target_content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Configuration".to_string(),
            auto_anchor: "configuration".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    let position = Position { line: 0, character: 18 };

    let result = server.handle_hover(&current_uri, position).await;
    assert!(result.is_some(), "Should return hover");

    let hover = result.unwrap();
    if let HoverContents::Markup(markup) = hover.contents {
        assert!(
            markup.value.contains("# This is a Python comment"),
            "Should include code block content (not treat # as heading): got '{}'",
            markup.value
        );
        assert!(
            markup.value.contains("More config info"),
            "Should include content after code block"
        );
    } else {
        panic!("Expected Markup hover contents");
    }
}

#[tokio::test]
async fn test_hover_empty_file() {
    use crate::workspace_index::FileIndex;

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-hover-test11/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("empty.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();
    let target_uri = Url::from_file_path(&target_file).unwrap();

    let content = "See [empty](empty.md) here.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Empty target file
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: String::new(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        index.insert_file(target_file.clone(), FileIndex::default());
    }

    let position = Position { line: 0, character: 15 };

    let result = server.handle_hover(&current_uri, position).await;
    assert!(result.is_some(), "Should return hover even for empty file");

    let hover = result.unwrap();
    if let HoverContents::Markup(markup) = hover.contents {
        assert!(markup.value.contains("empty.md"), "Should show filename");
    } else {
        panic!("Expected Markup hover contents");
    }
}

#[tokio::test]
async fn test_hover_anchor_does_not_stop_at_hashtag_word() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-hover-test12/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();
    let target_uri = Url::from_file_path(&target_file).unwrap();

    let content = "See [tags](guide.md#tags) here.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Target has "#hashtag" words that are NOT headings (no space after #)
    let target_content = "## Tags\n\nUse #markdown and #linting tags.\n\nThey help organize.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: target_content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Tags".to_string(),
            auto_anchor: "tags".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    let position = Position { line: 0, character: 15 };

    let result = server.handle_hover(&current_uri, position).await;
    assert!(result.is_some(), "Should return hover");

    let hover = result.unwrap();
    if let HoverContents::Markup(markup) = hover.contents {
        assert!(
            markup.value.contains("#markdown"),
            "Should NOT treat #hashtag as a heading and stop: got '{}'",
            markup.value
        );
        assert!(
            markup.value.contains("They help organize"),
            "Should include content after hashtag line"
        );
    } else {
        panic!("Expected Markup hover contents");
    }
}

#[tokio::test]
async fn test_hover_anchor_stops_at_indented_heading() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();

    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-hover-test13/docs");
    let current_file = docs_dir.join("index.md");
    let target_file = docs_dir.join("guide.md");

    let current_uri = Url::from_file_path(&current_file).unwrap();
    let target_uri = Url::from_file_path(&target_file).unwrap();

    let content = "See [intro](guide.md#intro) here.\n";
    server.documents.write().await.insert(
        current_uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // CommonMark allows up to 3 spaces of indentation for ATX headings
    let target_content = "## Intro\n\nSome intro text.\n\n  ## Next Section\n\nNext content.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: target_content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Intro".to_string(),
            auto_anchor: "intro".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        fi.add_heading(HeadingIndex {
            text: "Next Section".to_string(),
            auto_anchor: "next-section".to_string(),
            custom_anchor: None,
            line: 5,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), fi);
    }

    let position = Position { line: 0, character: 15 };

    let result = server.handle_hover(&current_uri, position).await;
    assert!(result.is_some(), "Should return hover");

    let hover = result.unwrap();
    if let HoverContents::Markup(markup) = hover.contents {
        assert!(markup.value.contains("Some intro text"), "Should contain intro content");
        assert!(
            !markup.value.contains("Next Section"),
            "Should stop at indented heading of same level"
        );
    } else {
        panic!("Expected Markup hover contents");
    }
}

// =========================================================================
// Rename support tests
// =========================================================================

#[tokio::test]
async fn test_prepare_rename_atx_heading() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();
    let file = std::path::PathBuf::from("/tmp/rumdl-rename-test1/doc.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "## Installation Guide\n\nSome content.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Installation Guide".to_string(),
            auto_anchor: "installation-guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(file.clone(), fi);
    }

    let position = Position { line: 0, character: 5 };
    let result = server.handle_prepare_rename(&uri, position).await;
    assert!(result.is_some(), "Should allow renaming ATX heading");

    if let Some(PrepareRenameResponse::Range(range)) = result {
        // Should cover "Installation Guide" (after "## ")
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 3); // after "## "
        assert_eq!(range.end.character, 21); // end of "Installation Guide"
    } else {
        panic!("Expected Range response");
    }
}

#[tokio::test]
async fn test_prepare_rename_setext_heading() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();
    let file = std::path::PathBuf::from("/tmp/rumdl-rename-test2/doc.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "Installation Guide\n==================\n\nContent.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Installation Guide".to_string(),
            auto_anchor: "installation-guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: true,
        });
        index.insert_file(file.clone(), fi);
    }

    let position = Position { line: 0, character: 5 };
    let result = server.handle_prepare_rename(&uri, position).await;
    assert!(result.is_some(), "Should allow renaming Setext heading");

    if let Some(PrepareRenameResponse::Range(range)) = result {
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.character, 18); // "Installation Guide"
    } else {
        panic!("Expected Range response");
    }
}

#[tokio::test]
async fn test_prepare_rename_not_heading() {
    let server = create_test_server();
    let file = std::path::PathBuf::from("/tmp/rumdl-rename-test3/doc.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "Just some text here.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    let position = Position { line: 0, character: 5 };
    let result = server.handle_prepare_rename(&uri, position).await;
    assert!(result.is_none(), "Should not allow renaming non-heading text");
}

#[tokio::test]
async fn test_rename_heading_updates_same_file_links() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();
    let file = std::path::PathBuf::from("/tmp/rumdl-rename-test4/doc.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content =
        "## Getting Started\n\nSee [below](#getting-started) for details.\n\nMore [info](#getting-started).\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Getting Started".to_string(),
            auto_anchor: "getting-started".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(file.clone(), fi);
    }

    let position = Position { line: 0, character: 5 };
    let result = server.handle_rename(&uri, position, "Quick Start").await;
    assert!(result.is_some(), "Should produce a workspace edit");

    let edit = result.unwrap();
    let changes = edit.changes.unwrap();
    let edits = changes.get(&uri).expect("Should have edits for the document");

    // 1 heading rename + 2 link anchor updates
    assert_eq!(edits.len(), 3, "Should have 3 edits: heading + 2 link anchors");

    // Verify heading edit
    let heading_edit = edits.iter().find(|e| e.range.start.line == 0).unwrap();
    assert_eq!(heading_edit.new_text, "Quick Start");

    // Verify link anchor edits
    let link_edits: Vec<_> = edits.iter().filter(|e| e.new_text == "quick-start").collect();
    assert_eq!(link_edits.len(), 2, "Should update both link anchors");
}

#[tokio::test]
async fn test_rename_heading_with_custom_anchor_only_changes_text() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();
    let file = std::path::PathBuf::from("/tmp/rumdl-rename-test5/doc.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "## Guide {#install}\n\nSee [link](#install) for details.\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Guide".to_string(),
            auto_anchor: "guide".to_string(),
            custom_anchor: Some("install".to_string()),
            line: 1,
            is_setext: false,
        });
        index.insert_file(file.clone(), fi);
    }

    let position = Position { line: 0, character: 5 };
    let result = server.handle_rename(&uri, position, "Tutorial").await;
    assert!(result.is_some(), "Should produce a workspace edit");

    let edit = result.unwrap();
    let changes = edit.changes.unwrap();
    let edits = changes.get(&uri).expect("Should have edits");

    // Only the heading text should change, not the link anchors
    assert_eq!(edits.len(), 1, "Should only have 1 edit: heading text change");
    assert_eq!(edits[0].new_text, "Tutorial");
}

#[tokio::test]
async fn test_rename_heading_updates_cross_file_links() {
    use crate::workspace_index::{CrossFileLinkIndex, FileIndex, HeadingIndex};

    let server = create_test_server();
    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-rename-test6/docs");
    let target_file = docs_dir.join("guide.md");
    let source_file = docs_dir.join("index.md");

    let target_uri = Url::from_file_path(&target_file).unwrap();
    let source_uri = Url::from_file_path(&source_file).unwrap();

    let target_content = "## API Reference\n\nAPI docs here.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: target_content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    let source_content = "See [api](guide.md#api-reference) for details.\n";
    server.documents.write().await.insert(
        source_uri.clone(),
        DocumentEntry {
            content: source_content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;

        let mut target_fi = FileIndex::default();
        target_fi.add_heading(HeadingIndex {
            text: "API Reference".to_string(),
            auto_anchor: "api-reference".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), target_fi);

        let mut source_fi = FileIndex::default();
        source_fi.add_cross_file_link(CrossFileLinkIndex {
            target_path: "guide.md".to_string(),
            fragment: "api-reference".to_string(),
            line: 1,
            column: 11, // byte column of "guide.md" in the link
        });
        index.insert_file(source_file.clone(), source_fi);
    }

    let position = Position { line: 0, character: 5 };
    let result = server.handle_rename(&target_uri, position, "REST API").await;
    assert!(result.is_some(), "Should produce workspace edit");

    let edit = result.unwrap();
    let changes = edit.changes.unwrap();

    // Target file: heading text change
    let target_edits = changes.get(&target_uri).expect("Should have target edits");
    assert!(
        target_edits.iter().any(|e| e.new_text == "REST API"),
        "Should rename the heading text"
    );

    // Source file: anchor update
    let source_edits = changes.get(&source_uri).expect("Should have source edits");
    assert!(
        source_edits.iter().any(|e| e.new_text == "rest-api"),
        "Should update the cross-file link anchor"
    );
}

#[tokio::test]
async fn test_rename_refuses_empty_name() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();
    let file = std::path::PathBuf::from("/tmp/rumdl-rename-test7/doc.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "## Heading\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Heading".to_string(),
            auto_anchor: "heading".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(file.clone(), fi);
    }

    let position = Position { line: 0, character: 5 };
    let result = server.handle_rename(&uri, position, "").await;
    assert!(result.is_none(), "Should refuse empty rename");

    let result = server.handle_rename(&uri, position, "   ").await;
    assert!(result.is_none(), "Should refuse whitespace-only rename");
}

#[tokio::test]
async fn test_rename_refuses_anchor_collision() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();
    let file = std::path::PathBuf::from("/tmp/rumdl-rename-test8/doc.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "## Foo\n\n## Bar\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Foo".to_string(),
            auto_anchor: "foo".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        fi.add_heading(HeadingIndex {
            text: "Bar".to_string(),
            auto_anchor: "bar".to_string(),
            custom_anchor: None,
            line: 3,
            is_setext: false,
        });
        index.insert_file(file.clone(), fi);
    }

    // Try to rename "Foo" to "Bar" — should be refused due to collision
    let position = Position { line: 0, character: 5 };
    let result = server.handle_rename(&uri, position, "Bar").await;
    assert!(result.is_none(), "Should refuse rename that causes anchor collision");
}

#[tokio::test]
async fn test_rename_heading_with_closing_atx() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();
    let file = std::path::PathBuf::from("/tmp/rumdl-rename-test9/doc.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "## Hello ##\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Hello".to_string(),
            auto_anchor: "hello".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(file.clone(), fi);
    }

    let position = Position { line: 0, character: 5 };
    let result = server.handle_prepare_rename(&uri, position).await;
    assert!(result.is_some());

    if let Some(PrepareRenameResponse::Range(range)) = result {
        // Should cover just "Hello", not the trailing "##"
        assert_eq!(range.start.character, 3);
        assert_eq!(range.end.character, 8); // "Hello" is 5 chars, starts at 3
    } else {
        panic!("Expected Range response");
    }
}

#[tokio::test]
async fn test_rename_updates_same_file_ref_definitions() {
    use crate::workspace_index::{FileIndex, HeadingIndex};

    let server = create_test_server();
    let file = std::path::PathBuf::from("/tmp/rumdl-rename-test10/doc.md");
    let uri = Url::from_file_path(&file).unwrap();

    let content = "## Getting Started\n\nSee [ref] for info.\n\n[ref]: #getting-started\n";
    server.documents.write().await.insert(
        uri.clone(),
        DocumentEntry {
            content: content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;
        let mut fi = FileIndex::default();
        fi.add_heading(HeadingIndex {
            text: "Getting Started".to_string(),
            auto_anchor: "getting-started".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(file.clone(), fi);
    }

    let position = Position { line: 0, character: 5 };
    let result = server.handle_rename(&uri, position, "Quick Start").await;
    assert!(result.is_some());

    let edit = result.unwrap();
    let changes = edit.changes.unwrap();
    let edits = changes.get(&uri).expect("Should have edits");

    // Should have: heading text change + ref definition anchor update
    // The [ref] usage text doesn't change — it references by label, not by anchor.
    // Only the definition line `[ref]: #getting-started` needs its anchor updated.
    let heading_edits: Vec<_> = edits.iter().filter(|e| e.new_text == "Quick Start").collect();
    assert_eq!(heading_edits.len(), 1, "Should have 1 heading text edit");

    let anchor_edits: Vec<_> = edits.iter().filter(|e| e.new_text == "quick-start").collect();
    assert_eq!(
        anchor_edits.len(),
        1,
        "Should update the reference definition anchor. Got {} edits",
        anchor_edits.len()
    );
}

#[tokio::test]
async fn test_rename_cross_file_multiple_links_same_line() {
    use crate::workspace_index::{CrossFileLinkIndex, FileIndex, HeadingIndex};

    let server = create_test_server();
    let docs_dir = std::path::PathBuf::from("/tmp/rumdl-rename-test11/docs");
    let target_file = docs_dir.join("guide.md");
    let source_file = docs_dir.join("index.md");

    let target_uri = Url::from_file_path(&target_file).unwrap();
    let source_uri = Url::from_file_path(&source_file).unwrap();

    let target_content = "## Getting Started\n\nContent here.\n";
    server.documents.write().await.insert(
        target_uri.clone(),
        DocumentEntry {
            content: target_content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    // Two links to the same heading on the same line
    let source_content = "See [a](guide.md#getting-started) and [b](guide.md#getting-started) here.\n";
    server.documents.write().await.insert(
        source_uri.clone(),
        DocumentEntry {
            content: source_content.to_string(),
            version: Some(1),
            from_disk: false,
        },
    );

    {
        let mut index = server.workspace_index.write().await;

        let mut target_fi = FileIndex::default();
        target_fi.add_heading(HeadingIndex {
            text: "Getting Started".to_string(),
            auto_anchor: "getting-started".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        index.insert_file(target_file.clone(), target_fi);

        let mut source_fi = FileIndex::default();
        // Two cross-file links on the same line
        source_fi.add_cross_file_link(CrossFileLinkIndex {
            target_path: "guide.md".to_string(),
            fragment: "getting-started".to_string(),
            line: 1,
            column: 9,
        });
        source_fi.add_cross_file_link(CrossFileLinkIndex {
            target_path: "guide.md".to_string(),
            fragment: "getting-started".to_string(),
            line: 1,
            column: 43,
        });
        index.insert_file(source_file.clone(), source_fi);
    }

    let position = Position { line: 0, character: 5 };
    let result = server.handle_rename(&target_uri, position, "Quick Start").await;
    assert!(result.is_some());

    let edit = result.unwrap();
    let changes = edit.changes.unwrap();

    // Source file should have 2 anchor edits (one per link), not 1 or duplicates
    let source_edits = changes.get(&source_uri).expect("Should have source edits");
    let anchor_edits: Vec<_> = source_edits.iter().filter(|e| e.new_text == "quick-start").collect();
    assert_eq!(
        anchor_edits.len(),
        2,
        "Should update both cross-file link anchors on the same line"
    );

    // The two edits should have different character positions
    assert_ne!(
        anchor_edits[0].range.start.character, anchor_edits[1].range.start.character,
        "Edits should target different positions on the line"
    );
}

// ── enable_link_navigation tests ────────────────────────────────────────────

#[tokio::test]
async fn test_link_navigation_enabled_by_default() {
    let server = create_test_server();
    let config = server.config.read().await;
    assert!(
        config.enable_link_navigation,
        "enable_link_navigation must default to true"
    );
}

#[tokio::test]
async fn test_link_navigation_config_deserialization() {
    // Omitting the field must leave it true (backwards-compatible default)
    let json = r#"{"enableLinting": true}"#;
    let config: crate::lsp::types::RumdlLspConfig = serde_json::from_str(json).unwrap();
    assert!(config.enable_link_navigation);

    // Explicitly disabling must be honoured
    let json = r#"{"enableLinkNavigation": false}"#;
    let config: crate::lsp::types::RumdlLspConfig = serde_json::from_str(json).unwrap();
    assert!(!config.enable_link_navigation);
}

#[tokio::test]
async fn test_hover_disabled_when_link_navigation_off() {
    let server = create_test_server();
    server.config.write().await.enable_link_navigation = false;

    let params = HoverParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: Url::parse("file:///test.md").unwrap(),
            },
            position: Position { line: 0, character: 0 },
        },
        work_done_progress_params: WorkDoneProgressParams { work_done_token: None },
    };

    let result = LanguageServer::hover(&server, params).await.unwrap();
    assert!(
        result.is_none(),
        "hover must return None when enable_link_navigation is false"
    );
}

#[tokio::test]
async fn test_goto_definition_disabled_when_link_navigation_off() {
    let server = create_test_server();
    server.config.write().await.enable_link_navigation = false;

    let params = GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: Url::parse("file:///test.md").unwrap(),
            },
            position: Position { line: 0, character: 0 },
        },
        work_done_progress_params: WorkDoneProgressParams { work_done_token: None },
        partial_result_params: PartialResultParams {
            partial_result_token: None,
        },
    };

    let result = LanguageServer::goto_definition(&server, params).await.unwrap();
    assert!(
        result.is_none(),
        "goto_definition must return None when enable_link_navigation is false"
    );
}

#[tokio::test]
async fn test_references_disabled_when_link_navigation_off() {
    let server = create_test_server();
    server.config.write().await.enable_link_navigation = false;

    let params = ReferenceParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: Url::parse("file:///test.md").unwrap(),
            },
            position: Position { line: 0, character: 0 },
        },
        work_done_progress_params: WorkDoneProgressParams { work_done_token: None },
        partial_result_params: PartialResultParams {
            partial_result_token: None,
        },
        context: ReferenceContext {
            include_declaration: false,
        },
    };

    let result = LanguageServer::references(&server, params).await.unwrap();
    assert!(
        result.is_none(),
        "references must return None when enable_link_navigation is false"
    );
}

#[tokio::test]
async fn test_prepare_rename_disabled_when_link_navigation_off() {
    let server = create_test_server();
    server.config.write().await.enable_link_navigation = false;

    let params = TextDocumentPositionParams {
        text_document: TextDocumentIdentifier {
            uri: Url::parse("file:///test.md").unwrap(),
        },
        position: Position { line: 0, character: 0 },
    };

    let result = LanguageServer::prepare_rename(&server, params).await.unwrap();
    assert!(
        result.is_none(),
        "prepare_rename must return None when enable_link_navigation is false"
    );
}

#[tokio::test]
async fn test_rename_disabled_when_link_navigation_off() {
    let server = create_test_server();
    server.config.write().await.enable_link_navigation = false;

    let params = RenameParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: Url::parse("file:///test.md").unwrap(),
            },
            position: Position { line: 0, character: 0 },
        },
        new_name: "new-name".to_string(),
        work_done_progress_params: WorkDoneProgressParams { work_done_token: None },
    };

    let result = LanguageServer::rename(&server, params).await.unwrap();
    assert!(
        result.is_none(),
        "rename must return None when enable_link_navigation is false"
    );
}

/// Verify that when `enableLinkNavigation` is false, the server does not advertise
/// hover, definition, references, or rename capabilities during initialization.
/// Clients rely on ServerCapabilities to decide whether to send these requests at all;
/// returning None from the handlers alone is not sufficient.
#[tokio::test]
async fn test_initialize_omits_nav_capabilities_when_disabled() {
    let server = create_test_server();

    let options = serde_json::json!({ "enableLinkNavigation": false });
    let params = InitializeParams {
        initialization_options: Some(options),
        capabilities: ClientCapabilities::default(),
        ..Default::default()
    };

    let result = LanguageServer::initialize(&server, params).await.unwrap();
    let caps = result.capabilities;

    assert!(
        caps.hover_provider.is_none(),
        "hover_provider must be None when enableLinkNavigation is false"
    );
    assert!(
        caps.definition_provider.is_none(),
        "definition_provider must be None when enableLinkNavigation is false"
    );
    assert!(
        caps.references_provider.is_none(),
        "references_provider must be None when enableLinkNavigation is false"
    );
    assert!(
        caps.rename_provider.is_none(),
        "rename_provider must be None when enableLinkNavigation is false"
    );
}

/// Verify that with `enableLinkNavigation` enabled (the default), all four navigation
/// capabilities are advertised to the client.
#[tokio::test]
async fn test_initialize_advertises_nav_capabilities_when_enabled() {
    let server = create_test_server();

    let params = InitializeParams {
        capabilities: ClientCapabilities::default(),
        ..Default::default()
    };

    let result = LanguageServer::initialize(&server, params).await.unwrap();
    let caps = result.capabilities;

    assert!(
        caps.hover_provider.is_some(),
        "hover_provider must be advertised by default"
    );
    assert!(
        caps.definition_provider.is_some(),
        "definition_provider must be advertised by default"
    );
    assert!(
        caps.references_provider.is_some(),
        "references_provider must be advertised by default"
    );
    assert!(
        caps.rename_provider.is_some(),
        "rename_provider must be advertised by default"
    );
}
