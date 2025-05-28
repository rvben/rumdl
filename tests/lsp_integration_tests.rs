//! Integration tests for rumdl LSP server simulating real editor workflows
//!
//! These tests verify the LSP server works correctly in scenarios that
//! mirror how editors like VS Code, Neovim, etc. would interact with rumdl.

use rumdl::lsp::types::{warning_to_diagnostic, RumdlLspConfig};
use std::time::Duration;

/// Test the core LSP workflow without full server setup
#[tokio::test]
async fn test_basic_lsp_workflow() {
    // Test that we can create LSP types properly
    let config = RumdlLspConfig::default();

    assert_eq!(config.config_path, None);
    assert!(config.enable_linting);
    assert!(!config.enable_auto_fix);
    assert!(config.disable_rules.is_empty());
}

/// Test realistic document content processing
#[tokio::test]
async fn test_document_content_processing() {
    let content = r#"# My Document

This line is way too long and exceeds the maximum line length limit specified by MD013 which should trigger a warning.

##  Double space in heading

- List item 1
- List item 2
*  Mixed list markers

Here's some `inline code` and a [link](https://example.com).

> Blockquote here
"#;

    // Test that we can process this content with rumdl
    let rules = rumdl::rules::all_rules(&rumdl::config::Config::default());
    let warnings = rumdl::lint(content, &rules, false).unwrap();

    // Should find some issues in this content
    assert!(
        !warnings.is_empty(),
        "Expected to find linting issues in test content"
    );

    // Test converting warnings to LSP diagnostics
    for warning in &warnings {
        let diagnostic = warning_to_diagnostic(warning);
        assert!(!diagnostic.message.is_empty());
        assert!(diagnostic.range.start.line < 100); // Reasonable upper bound
        assert!(diagnostic.range.start.character < 1000); // Reasonable upper bound
    }
}

/// Test multiple file scenarios
#[tokio::test]
async fn test_multiple_file_scenarios() {
    let files = vec![
        ("README.md", "# README\n\nProject description."),
        ("docs/api.md", "# API\n\n## Endpoints"),
        ("CHANGELOG.md", "# Changelog\n\n## v1.0.0"),
    ];

    let rules = rumdl::rules::all_rules(&rumdl::config::Config::default());

    for (filename, content) in files {
        let warnings = rumdl::lint(content, &rules, false).unwrap();

        // Each file should be processable
        for warning in &warnings {
            let diagnostic = warning_to_diagnostic(warning);

            // Basic validation of diagnostic
            assert!(!diagnostic.message.is_empty());
            assert!(diagnostic.severity.is_some());
            assert_eq!(diagnostic.source, Some("rumdl".to_string()));
        }

        println!("Processed {} with {} warnings", filename, warnings.len());
    }
}

/// Test configuration handling
#[tokio::test]
async fn test_configuration_handling() {
    // Test default configuration
    let default_config = RumdlLspConfig::default();
    assert!(default_config.enable_linting);
    assert!(!default_config.enable_auto_fix);

    // Test custom configuration
    let custom_config = RumdlLspConfig {
        config_path: Some("/custom/path/.rumdl.toml".to_string()),
        enable_linting: true,
        enable_auto_fix: true,
        disable_rules: vec!["MD001".to_string(), "MD013".to_string()],
    };

    // Test serialization/deserialization
    let json = serde_json::to_string(&custom_config).unwrap();
    let deserialized: RumdlLspConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.config_path, custom_config.config_path);
    assert_eq!(deserialized.enable_linting, custom_config.enable_linting);
    assert_eq!(deserialized.enable_auto_fix, custom_config.enable_auto_fix);
    assert_eq!(deserialized.disable_rules, custom_config.disable_rules);
}

/// Test error recovery scenarios
#[tokio::test]
async fn test_error_recovery() {
    let invalid_content = "This is not valid markdown in some way that might cause issues...";

    // Even with potentially problematic content, rumdl should handle gracefully
    let rules = rumdl::rules::all_rules(&rumdl::config::Config::default());
    let result = rumdl::lint(invalid_content, &rules, false);

    // Should not panic or fail catastrophically
    assert!(
        result.is_ok(),
        "Linting should handle edge cases gracefully"
    );
}

/// Test performance with larger documents
#[tokio::test]
async fn test_performance_with_large_document() {
    let start = std::time::Instant::now();

    // Create a reasonably large document
    let mut large_content = String::new();
    large_content.push_str("# Large Document\n\n");

    for i in 1..=100 {
        large_content.push_str(&format!(
            "## Section {}\n\nThis is paragraph {} with some content. ",
            i, i
        ));
        large_content.push_str("Here's some more text to make it substantial. ");
        large_content.push_str("And even more content to test performance.\n\n");

        if i % 10 == 0 {
            large_content.push_str("- List item 1\n- List item 2\n- List item 3\n\n");
        }
    }

    // Test that we can process large documents efficiently
    let rules = rumdl::rules::all_rules(&rumdl::config::Config::default());
    let warnings = rumdl::lint(&large_content, &rules, false).unwrap();

    let elapsed = start.elapsed();
    println!(
        "Processed large document ({} chars) in {:?} with {} warnings",
        large_content.len(),
        elapsed,
        warnings.len()
    );

    // Should complete reasonably quickly (within 2 seconds for this size)
    assert!(
        elapsed < Duration::from_secs(2),
        "Large document processing took too long: {:?}",
        elapsed
    );
}

/// Test rapid editing simulation
#[tokio::test]
async fn test_rapid_editing_simulation() {
    let rules = rumdl::rules::all_rules(&rumdl::config::Config::default());
    let start = std::time::Instant::now();

    // Simulate rapid editing by processing many small changes
    for i in 1..=50 {
        let content = format!("# Document Version {}\n\n{}", i, "Content here. ".repeat(i));

        let warnings = rumdl::lint(&content, &rules, false).unwrap();

        // Convert to diagnostics (simulating LSP diagnostic updates)
        for warning in &warnings {
            let _diagnostic = warning_to_diagnostic(warning);
        }
    }

    let elapsed = start.elapsed();
    println!("Rapid editing simulation completed in {:?}", elapsed);

    // Should handle rapid changes efficiently
    assert!(
        elapsed < Duration::from_secs(1),
        "Rapid editing simulation took too long: {:?}",
        elapsed
    );
}

/// Test workspace-like scenarios
#[tokio::test]
async fn test_workspace_scenarios() {
    // Simulate a workspace with different types of markdown files
    let workspace_files = vec![
        ("README.md", "# Project\n\nMain project documentation."),
        (
            "docs/getting-started.md",
            "# Getting Started\n\n## Installation\n\nRun `npm install`.",
        ),
        (
            "docs/api/endpoints.md",
            "# API Endpoints\n\n### GET /users\n\nReturns users.",
        ),
        (
            "CONTRIBUTING.md",
            "# Contributing\n\n## Guidelines\n\n- Be nice\n- Write tests",
        ),
        (
            "CHANGELOG.md",
            "# Changelog\n\n## [1.0.0] - 2024-01-01\n\n### Added\n- Initial release",
        ),
    ];

    let rules = rumdl::rules::all_rules(&rumdl::config::Config::default());
    let mut total_warnings = 0;
    let file_count = workspace_files.len();

    for (filepath, content) in &workspace_files {
        let warnings = rumdl::lint(content, &rules, false).unwrap();
        total_warnings += warnings.len();

        // Verify each file processes correctly
        for warning in &warnings {
            let diagnostic = warning_to_diagnostic(warning);
            assert!(!diagnostic.message.is_empty());
            assert!(diagnostic.source == Some("rumdl".to_string()));
        }

        println!(
            "File {} processed with {} warnings",
            filepath,
            warnings.len()
        );
    }

    println!(
        "Workspace total: {} warnings across {} files",
        total_warnings, file_count
    );
}

/// Test that diagnostic conversion preserves all necessary information
#[tokio::test]
async fn test_diagnostic_conversion_completeness() {
    let content = "#  Heading with extra space\n\nContent here.";
    let rules = rumdl::rules::all_rules(&rumdl::config::Config::default());
    let warnings = rumdl::lint(content, &rules, false).unwrap();

    for warning in warnings {
        let diagnostic = warning_to_diagnostic(&warning);

        // Verify all important fields are set
        assert!(!diagnostic.message.is_empty());
        assert!(diagnostic.severity.is_some());
        assert_eq!(diagnostic.source, Some("rumdl".to_string()));

        // Check that line/column mapping works correctly
        assert!(diagnostic.range.end.line >= diagnostic.range.start.line);
        assert!(diagnostic.range.start.line < 1000); // Reasonable upper bound
        assert!(diagnostic.range.start.character < 10000); // Reasonable upper bound

        // If warning has a rule name, diagnostic should have a code
        if warning.rule_name.is_some() {
            assert!(diagnostic.code.is_some());
        }
    }
}
