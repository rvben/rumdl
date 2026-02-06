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
