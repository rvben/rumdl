/// Test for hard breaks (two trailing spaces) in list items with MD013 reflow
///
/// In Markdown, two trailing spaces indicate a hard line break.
/// This should be preserved even when reflowing list items.
use std::fs;
use tempfile::tempdir;

#[test]
fn test_hard_breaks_preserved_in_list_items() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Content with intentional hard breaks (two trailing spaces)
    let content = "1. First line with hard break  \n    Second line after break\n";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 999999
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    // Run rumdl with fix
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Hard break (two trailing spaces) should be preserved
    // The content should NOT be merged into a single line
    assert!(
        fixed_content.contains("  \n") || fixed_content.contains("  \r\n"),
        "Hard break (two trailing spaces) should be preserved, got: {fixed_content:?}"
    );

    // Verify the command succeeded
    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl should succeed, got: {status:?}"
    );
}

#[test]
fn test_hard_breaks_segment_based_reflow() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Content with hard break - MD013 should use segment-based reflow
    // First segment (line 1) ends with hard break - gets reflowed and hard break preserved
    // Second segment (lines 2-3) has no hard breaks - gets joined and reflowed
    let content = "1. First line  \n    Second line\n    Third line\n";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 50
reflow = true
reflow-mode = "normalize"

# Disable MD009 so we can test MD013's behavior in isolation
[MD009]
br-spaces = 2
"#;
    fs::write(&config_path, config_content).unwrap();

    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Hard break should be preserved after first line
    assert!(
        fixed_content.contains("First line  \n") || fixed_content.contains("First line  \r\n"),
        "Hard break should be preserved after 'First line', got: {fixed_content:?}"
    );

    // Lines after hard break (second segment) should be joined together
    assert!(
        fixed_content.contains("Second line Third line"),
        "Lines in segment after hard break should be joined, got: {fixed_content:?}"
    );
}

#[test]
fn test_hard_breaks_with_crlf_line_endings() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Test Windows-style CRLF line endings with hard breaks
    let content = "1. First line with hard break  \r\n    Second line after break\r\n";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
disable = ["MD009"]

[MD013]
line-length = 999999
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Hard break should be preserved even with CRLF
    assert!(
        fixed_content.contains("  \n") || fixed_content.contains("  \r\n"),
        "Hard break should be preserved with CRLF line endings, got: {fixed_content:?}"
    );

    // Should NOT be reflowed into a single line
    assert!(
        fixed_content.lines().count() >= 2,
        "Should have multiple lines (not reflowed), got: {}",
        fixed_content.lines().count()
    );
}

#[test]
fn test_hard_breaks_preserved_with_no_reflow() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Test that MD013 doesn't modify content when hard breaks are present
    // Even with excessive trailing spaces, MD013 won't normalize them because
    // that's MD009's job. MD013 only normalizes during reflow to prevent
    // creating mid-line spaces.
    let content = "1. Line with hard break  \n    Second line\n";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
disable = ["MD009"]

[MD013]
line-length = 50
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Content should be unchanged because hard break prevents reflow
    assert_eq!(
        fixed_content, content,
        "Content with hard breaks should not be modified by MD013"
    );

    // Hard break should still be present
    assert!(
        fixed_content.contains("  \n") || fixed_content.contains("  \r\n"),
        "Hard break should be preserved"
    );
}

#[test]
fn test_hard_breaks_with_unicode() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Test Unicode characters with hard breaks
    let content = "1. Text with emoji 🎉  \n    Second line with accents café  \n    Third line\n";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
disable = ["MD009"]

[MD013]
line-length = 999999
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Hard breaks should be preserved with Unicode content
    assert!(
        fixed_content.contains("🎉  \n") || fixed_content.contains("🎉  \r\n"),
        "Hard break after emoji should be preserved, got: {fixed_content:?}"
    );

    assert!(
        fixed_content.contains("café  \n") || fixed_content.contains("café  \r\n"),
        "Hard break after accented text should be preserved, got: {fixed_content:?}"
    );

    // Should NOT be reflowed
    assert!(
        fixed_content.lines().count() >= 3,
        "Should have at least 3 lines (not reflowed), got: {}",
        fixed_content.lines().count()
    );
}

#[test]
fn test_nested_lists_with_hard_breaks() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Test nested lists with hard breaks
    let content = "1. Parent item  \n    - Nested item  \n      Continuation\n    - Second nested\n";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
disable = ["MD009"]

[MD013]
line-length = 999999
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Hard breaks in both parent and nested items should be preserved
    assert!(
        fixed_content.contains("Parent item  \n") || fixed_content.contains("Parent item  \r\n"),
        "Parent item hard break should be preserved, got: {fixed_content:?}"
    );

    assert!(
        fixed_content.contains("Nested item  \n") || fixed_content.contains("Nested item  \r\n"),
        "Nested item hard break should be preserved, got: {fixed_content:?}"
    );
}

// Tests for backslash hard breaks (mdformat compatibility)

#[test]
fn test_backslash_hard_breaks_in_list_items() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Content with backslash hard break (mdformat style)
    let content = "- First line with hard break\\\n  Second line after break\n";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 999999
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Backslash hard break should be preserved
    assert!(
        fixed_content.contains("\\\n"),
        "Backslash hard break should be preserved, got: {fixed_content:?}"
    );

    // Should NOT be reflowed into single line
    assert!(
        fixed_content.lines().count() >= 2,
        "Should have multiple lines (not reflowed), got: {}",
        fixed_content.lines().count()
    );

    // Verify the command succeeded
    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl should succeed, got: {status:?}"
    );
}

#[test]
fn test_issue_110_mdformat_compatibility() {
    // Exact test case from issue #110
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = "- [GPT-5 Pro](https://platform.openai.com/docs/models/gpt-5-pro) is available (and expensive!) at $15/$120 per million input/output tokens, maximum 272,000 output tokens.\\\n  OpenAI links to [Background mode](https://platform.openai.com/docs/guides/background), which allows devs to initiate longer-running tasks (GPT-5 Pro, Deep Research, etc.) without concern for timeouts through the use of a poll-able response object.\n";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 80
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // The backslash should be preserved after "tokens."
    assert!(
        fixed_content.contains("tokens.\\\n"),
        "Backslash hard break should be preserved after 'tokens.', got: {fixed_content:?}"
    );

    // The line after backslash should maintain proper list indentation
    assert!(
        fixed_content.contains("  OpenAI links"),
        "Indentation should be preserved after hard break, got: {fixed_content:?}"
    );

    // Verify the command succeeded
    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl should succeed, got: {status:?}"
    );
}

#[test]
fn test_backslash_hard_breaks_with_crlf() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Test Windows-style CRLF line endings with backslash hard breaks
    let content = "1. First line with hard break\\\r\n    Second line after break\r\n";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 999999
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Backslash hard break should be preserved even with CRLF
    assert!(
        fixed_content.contains("\\\n") || fixed_content.contains("\\\r\n"),
        "Backslash hard break should be preserved with CRLF line endings, got: {fixed_content:?}"
    );

    // Should NOT be reflowed into a single line
    assert!(
        fixed_content.lines().count() >= 2,
        "Should have multiple lines (not reflowed), got: {}",
        fixed_content.lines().count()
    );
}

#[test]
fn test_mixed_hard_break_styles() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Test that both styles work in the same document
    let content = "- First item with two spaces  \n  Continuation of first\n- Second item with backslash\\\n  Continuation of second\n";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
disable = ["MD009"]

[MD013]
line-length = 999999
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Two-space hard break should be preserved
    assert!(
        fixed_content.contains("spaces  \n") || fixed_content.contains("spaces  \r\n"),
        "Two-space hard break should be preserved, got: {fixed_content:?}"
    );

    // Backslash hard break should be preserved
    assert!(
        fixed_content.contains("backslash\\\n") || fixed_content.contains("backslash\\\r\n"),
        "Backslash hard break should be preserved, got: {fixed_content:?}"
    );
}

#[test]
fn test_backslash_hard_breaks_with_unicode() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Test Unicode characters with backslash hard breaks
    let content = "1. Text with emoji 🎉\\\n    Second line with accents café\\\n    Third line\n";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 999999
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Backslash hard breaks should be preserved with Unicode content
    assert!(
        fixed_content.contains("🎉\\\n") || fixed_content.contains("🎉\\\r\n"),
        "Backslash hard break after emoji should be preserved, got: {fixed_content:?}"
    );

    assert!(
        fixed_content.contains("café\\\n") || fixed_content.contains("café\\\r\n"),
        "Backslash hard break after accented text should be preserved, got: {fixed_content:?}"
    );

    // Should NOT be reflowed
    assert!(
        fixed_content.lines().count() >= 3,
        "Should have at least 3 lines (not reflowed), got: {}",
        fixed_content.lines().count()
    );
}

#[test]
fn test_backslash_segment_based_reflow() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Content with backslash hard break - MD013 should use segment-based reflow
    let content = "1. First line\\\n    Second line\n    Third line\n";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 50
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Backslash hard break should be preserved after first line
    assert!(
        fixed_content.contains("First line\\\n") || fixed_content.contains("First line\\\r\n"),
        "Backslash hard break should be preserved after 'First line', got: {fixed_content:?}"
    );

    // Lines after hard break (second segment) should be joined together
    assert!(
        fixed_content.contains("Second line Third line"),
        "Lines in segment after hard break should be joined, got: {fixed_content:?}"
    );
}
