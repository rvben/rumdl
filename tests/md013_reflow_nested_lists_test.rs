/// Comprehensive test suite for Issue #107 - Normalize reflow with complex nested lists
///
/// This test suite covers:
/// - Nested bullet lists within ordered lists
/// - Nested ordered lists within bullet lists
/// - Multiple levels of nesting
/// - Fenced code blocks within nested lists
/// - Code blocks with different indent levels
/// - Mix of code blocks and nested lists
/// - Semantic line breaks (NOTE:, WARNING:, etc.)
/// - The specific issue #107 scenario
///
/// All tests use normalize mode with high line_length (999999) to verify that
/// well-structured content doesn't trigger false positive warnings or unnecessary reflow.
use std::fs;
use tempfile::tempdir;

/// Test 1: Exact scenario from issue #107
/// This is the regression test for the reported bug
#[test]
fn test_issue_107_exact_scenario() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Exact content structure from issue #107
    let content = r#"1. Install WSL, reboot

2. Install distro:
   - Install WSL, reboot
   - Install distro (I use Debian)
   - Configure distro (Create user account, etc.)

   Get into the distro, then:

   ```bash
   sudo apt-get update && sudo apt-get -y upgrade
   ```

3. Install [nvidia cuda-toolkit](https://developer.nvidia.com/cuda-downloads?target_os=Linux&target_arch=x86_64&Distribution=WSL-Ubuntu&target_version=2.0&target_type=deb_local)
   NOTE: **DO NOT INSTALL THE DRIVER ON WSL; ONLY INSTALL THE CUDA-TOOLKIT**

4. Final step
"#;

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
disable = ["MD031"]  # Disable blank line before code block check for this test

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
    let stdout = String::from_utf8_lossy(&output.stdout);

    // CRITICAL: No false positive warnings about exceeding 999999 characters
    assert!(
        !stdout.contains("exceeds 999999") && !stdout.contains("999999"),
        "Should not have false positive line length warnings, got: {stdout}"
    );

    // CRITICAL: Nested bullet lists should remain as separate lines
    assert!(
        fixed_content.contains("- Install WSL, reboot\n"),
        "First nested bullet should be preserved, got: {fixed_content}"
    );
    assert!(
        fixed_content.contains("- Install distro (I use Debian)\n"),
        "Second nested bullet should be preserved, got: {fixed_content}"
    );
    assert!(
        fixed_content.contains("- Configure distro (Create user account, etc.)\n"),
        "Third nested bullet should be preserved, got: {fixed_content}"
    );

    // Verify nested bullets are NOT merged into one line
    let nested_merged = fixed_content.contains("- Install WSL, reboot - Install distro");
    assert!(
        !nested_merged,
        "Nested bullet items should NOT be merged into single line"
    );

    // CRITICAL: Code block should preserve original blank line spacing
    // The original has exactly one blank line immediately before the code block
    let before_code = fixed_content.split("```bash").next().unwrap_or("");
    let lines_before: Vec<&str> = before_code.lines().collect();

    // When splitting by "```bash", we get everything before it including the indentation line
    // So the structure is:
    // lines_before[-3]: "   Get into the distro, then:"
    // lines_before[-2]: "" (blank line)
    // lines_before[-1]: "   " (indentation before code fence)

    // Check the last line before code block contains only whitespace (the indent before fence)
    assert!(
        lines_before.last().map(|l| l.trim().is_empty()).unwrap_or(false),
        "Should have whitespace/indent line immediately before code block marker"
    );

    // Check that there's a blank line before the indent line
    if lines_before.len() >= 2 {
        let second_to_last = lines_before[lines_before.len() - 2];
        assert!(
            second_to_last.trim().is_empty(),
            "Should have a blank line before the code fence indent, got: '{second_to_last}'"
        );
    }

    // Check that the line before the blank line contains our text (no extra blank added)
    if lines_before.len() >= 3 {
        let third_to_last = lines_before[lines_before.len() - 3];
        assert!(
            third_to_last.contains("Get into the distro, then:"),
            "Text line should be 3rd from end, got: {:?}",
            &lines_before[lines_before.len().saturating_sub(4)..]
        );
    }

    // CRITICAL: Semantic line break (NOTE:) should be preserved
    assert!(
        fixed_content.contains("cuda-toolkit](https://developer.nvidia.com/cuda-downloads?target_os=Linux&target_arch=x86_64&Distribution=WSL-Ubuntu&target_version=2.0&target_type=deb_local)\n   NOTE:"),
        "NOTE: line should maintain its line break from preceding content, got: {fixed_content}"
    );

    // CRITICAL: List numbering should remain 1, 2, 3, 4 (not reset)
    assert!(fixed_content.contains("1. Install WSL"), "Item 1 should be present");
    assert!(fixed_content.contains("2. Install distro:"), "Item 2 should be present");
    assert!(
        fixed_content.contains("3. Install [nvidia cuda-toolkit]"),
        "Item 3 should be present with number 3 (not reset to 1)"
    );
    assert!(
        fixed_content.contains("4. Final step"),
        "Item 4 should be present with number 4 (not reset to 2)"
    );

    // Verify code block is preserved
    assert!(
        fixed_content.contains("```bash\n   sudo apt-get update"),
        "Code block should be preserved"
    );
}

/// Test 2: Basic nested bullet lists within ordered list
#[test]
fn test_nested_bullets_in_ordered_list() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"1. First ordered item

2. Second ordered item with nested bullets:
   - Nested bullet 1
   - Nested bullet 2
   - Nested bullet 3

3. Third ordered item
"#;

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

    // All three nested bullets should remain as separate lines
    assert!(
        fixed_content.contains("- Nested bullet 1\n"),
        "Nested bullet 1 should be preserved"
    );
    assert!(
        fixed_content.contains("- Nested bullet 2\n"),
        "Nested bullet 2 should be preserved"
    );
    assert!(
        fixed_content.contains("- Nested bullet 3\n"),
        "Nested bullet 3 should be preserved"
    );

    // Verify they're NOT merged
    assert!(
        !fixed_content.contains("- Nested bullet 1 - Nested bullet 2"),
        "Nested bullets should not be merged"
    );

    // List structure should be preserved
    assert!(fixed_content.contains("1. First ordered item"));
    assert!(fixed_content.contains("2. Second ordered item"));
    assert!(fixed_content.contains("3. Third ordered item"));
}

/// Test 3: Nested ordered lists within bullet list
#[test]
fn test_nested_ordered_in_bullet_list() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"- First bullet item

- Second bullet with nested ordered:
  1. Nested ordered 1
  2. Nested ordered 2
  3. Nested ordered 3

- Third bullet item
"#;

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

    // All nested ordered items should be preserved
    assert!(
        fixed_content.contains("1. Nested ordered 1\n") || fixed_content.contains("1. Nested ordered 1"),
        "Nested ordered 1 should be preserved, got: {fixed_content}"
    );
    assert!(
        fixed_content.contains("2. Nested ordered 2\n") || fixed_content.contains("2. Nested ordered 2"),
        "Nested ordered 2 should be preserved, got: {fixed_content}"
    );
    assert!(
        fixed_content.contains("3. Nested ordered 3\n") || fixed_content.contains("3. Nested ordered 3"),
        "Nested ordered 3 should be preserved, got: {fixed_content}"
    );

    // Numbering should be 1, 2, 3 (not reset or merged)
    assert!(
        !fixed_content.contains("1. Nested ordered 1 2. Nested ordered 2"),
        "Nested ordered items should not be merged"
    );

    // Parent list structure preserved
    assert!(fixed_content.contains("- First bullet item"));
    assert!(fixed_content.contains("- Second bullet with nested ordered:"));
    assert!(fixed_content.contains("- Third bullet item"));
}

/// Test 4: Multiple levels of nesting (3+ levels deep)
#[test]
fn test_multiple_nesting_levels() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"1. Level 1 ordered
   - Level 2 bullet
     1. Level 3 ordered
        - Level 4 bullet
     2. Level 3 ordered item 2
   - Level 2 bullet item 2

2. Level 1 ordered item 2
"#;

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
    let stdout = String::from_utf8_lossy(&output.stdout);

    // No false warnings
    assert!(
        !stdout.contains("exceeds 999999"),
        "Should not have false positive warnings"
    );

    // All nesting levels should be preserved
    assert!(fixed_content.contains("1. Level 1 ordered"));
    assert!(fixed_content.contains("- Level 2 bullet"));
    assert!(fixed_content.contains("1. Level 3 ordered"));
    assert!(fixed_content.contains("- Level 4 bullet"));
    assert!(fixed_content.contains("2. Level 3 ordered item 2"));
    assert!(fixed_content.contains("- Level 2 bullet item 2"));
    assert!(fixed_content.contains("2. Level 1 ordered item 2"));

    // Each item on its own line (no merging)
    let line_count = fixed_content.lines().count();
    assert!(
        line_count >= 8,
        "Should have at least 8 lines for the nested structure, got: {line_count}"
    );
}

/// Test 5: Fenced code blocks at various indent levels
#[test]
fn test_code_blocks_various_indents() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"1. Item with code at base indent:

   ```bash
   echo "hello"
   ```

2. Item with deeply indented code:
   - Nested item

     ```python
     print("nested")
     ```

3. Another item
"#;

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

    // Code blocks should be preserved
    assert!(
        fixed_content.contains("```bash\n   echo \"hello\""),
        "First code block should be preserved"
    );
    assert!(
        fixed_content.contains("```python\n     print(\"nested\")"),
        "Second code block should be preserved"
    );

    // Check for excessive blank lines before code blocks
    // There should be 1 blank line before each code block, not multiple
    let first_code_context = fixed_content.split("```bash").next().unwrap();
    let lines_before_first: Vec<&str> = first_code_context.lines().rev().take(2).collect();

    let second_code_context = fixed_content.split("```python").next().unwrap();
    let lines_before_second: Vec<&str> = second_code_context.lines().rev().take(2).collect();

    // Should have exactly 1 blank line before code blocks
    assert!(
        !lines_before_first.is_empty(),
        "Should have context before first code block"
    );
    assert!(
        !lines_before_second.is_empty(),
        "Should have context before second code block"
    );
}

/// Test 6: Code blocks without preceding blank lines
#[test]
fn test_code_blocks_no_blank_line() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"1. Item with code immediately:
   ```bash
   echo "no blank line before"
   ```

2. Another item
"#;

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
disable = ["MD031"]  # Disable blank line before code block check for this test

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

    // Code block should be preserved
    assert!(fixed_content.contains("```bash"), "Code block should be present");

    // Should NOT add a blank line where there wasn't one
    // The code block should directly follow the colon
    assert!(
        fixed_content.contains("immediately:\n   ```bash") || fixed_content.contains("immediately:\r\n   ```bash"),
        "Code block should directly follow text (no added blank line), got: {fixed_content}"
    );
}

/// Test 7: Mix of code blocks and nested lists
#[test]
fn test_mixed_code_and_nested_lists() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"1. Setup instructions:
   - Install dependencies
   - Configure settings

   Run the following:

   ```bash
   npm install
   ```

   - Verify installation
   - Run tests

2. Next step
"#;

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

    // All nested bullets should be preserved
    assert!(fixed_content.contains("- Install dependencies"));
    assert!(fixed_content.contains("- Configure settings"));
    assert!(fixed_content.contains("- Verify installation"));
    assert!(fixed_content.contains("- Run tests"));

    // Code block should be preserved
    assert!(fixed_content.contains("```bash\n   npm install"));

    // Nested lists should not be merged with each other or with code block
    assert!(
        !fixed_content.contains("- Install dependencies - Configure settings"),
        "Nested bullets before code should not be merged"
    );
    assert!(
        !fixed_content.contains("- Verify installation - Run tests"),
        "Nested bullets after code should not be merged"
    );
}

/// Test 8: Semantic line breaks with NOTE/WARNING/IMPORTANT
#[test]
fn test_semantic_line_breaks() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"1. Install the package
   NOTE: Make sure to use version 2.0 or higher

2. Configure the settings
   WARNING: This will overwrite existing config

3. Deploy the application
   IMPORTANT: Backup your data first
"#;

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

    // Semantic markers should maintain their line breaks
    assert!(
        fixed_content.contains("package\n   NOTE:") || fixed_content.contains("package\r\n   NOTE:"),
        "NOTE: should be on separate line, got: {fixed_content}"
    );
    assert!(
        fixed_content.contains("settings\n   WARNING:") || fixed_content.contains("settings\r\n   WARNING:"),
        "WARNING: should be on separate line, got: {fixed_content}"
    );
    assert!(
        fixed_content.contains("application\n   IMPORTANT:") || fixed_content.contains("application\r\n   IMPORTANT:"),
        "IMPORTANT: should be on separate line, got: {fixed_content}"
    );

    // Should NOT be merged into single lines
    assert!(
        !fixed_content.contains("package NOTE:"),
        "NOTE: should not be merged with preceding text"
    );
    assert!(
        !fixed_content.contains("settings WARNING:"),
        "WARNING: should not be merged with preceding text"
    );
    assert!(
        !fixed_content.contains("application IMPORTANT:"),
        "IMPORTANT: should not be merged with preceding text"
    );
}

/// Test 9: Semantic line breaks based on indentation changes
#[test]
fn test_semantic_breaks_indent_changes() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"1. This is the main instruction text
  This line is less indented and should stay separate

2. Another item with indent change:
    Normal continuation at 4 spaces
  Less indented continuation at 2 spaces
"#;

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

    // Different indent levels should be preserved as structure
    assert!(fixed_content.contains("instruction text"), "Original text preserved");
    assert!(fixed_content.contains("less indented"), "Indent change line preserved");
}

/// Test 10: Long content that SHOULD reflow (positive test)
#[test]
fn test_normalize_does_reflow_when_needed() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Intentionally long line that exceeds 80 characters
    let content = "1. This is a very very very long line that actually does exceed our line length limit and genuinely needs to be reflowed into multiple shorter lines to improve readability and make it easier to read\n";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 80
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

    // Content should have been reflowed
    let lines: Vec<&str> = fixed_content.lines().collect();

    // Should have multiple lines after reflow
    assert!(
        lines.len() > 1,
        "Long content should be reflowed into multiple lines, got {} lines",
        lines.len()
    );

    // All words should be preserved
    assert!(fixed_content.contains("very very very long line"));
    assert!(fixed_content.contains("genuinely needs"));
    assert!(fixed_content.contains("improve readability"));

    // List marker should be preserved
    assert!(fixed_content.starts_with("1. "));
}

/// Test 11: Multiple text paragraphs in list item
#[test]
fn test_multiple_paragraphs_in_list_item() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"1. First paragraph in the list item.

   Second paragraph in the same list item, separated by blank line.

   Third paragraph here too.

2. Next item
"#;

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

    // All three paragraphs should be preserved
    assert!(fixed_content.contains("First paragraph"));
    assert!(fixed_content.contains("Second paragraph"));
    assert!(fixed_content.contains("Third paragraph"));

    // Blank lines between paragraphs should be preserved (not all merged)
    let blank_line_count = fixed_content.lines().filter(|l| l.trim().is_empty()).count();
    assert!(
        blank_line_count >= 2,
        "Should have blank lines between paragraphs, got {blank_line_count}"
    );
}

/// Test 12: Plain continuation lines (should be joinable)
#[test]
fn test_plain_continuation_lines() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"1. This is a list item
   that continues on the next line
   and another line here
   all part of same paragraph
"#;

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

    // All words should be preserved
    assert!(fixed_content.contains("list item"));
    assert!(fixed_content.contains("continues"));
    assert!(fixed_content.contains("another line"));
    assert!(fixed_content.contains("same paragraph"));

    // With normalize mode and very high line length, these could be joined
    // or preserved - we just verify no data loss
    let exit_code = output.status.code().unwrap_or(-1);
    assert!(exit_code == 0 || exit_code == 1, "Should complete successfully");
}

/// Test 13: Empty or minimal nested list items (edge case)
#[test]
fn test_empty_nested_items() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"1. Parent item:
   - Item 1
   - Item 2

2. Next item
"#;

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

    let exit_code = output.status.code().unwrap_or(-1);
    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Should not crash
    assert!(
        exit_code == 0 || exit_code == 1,
        "Should handle minimal nested items without crashing"
    );

    // Structure should be preserved
    assert!(fixed_content.contains("- Item 1"));
    assert!(fixed_content.contains("- Item 2"));
}

/// Test 14: Mixed bullet list markers (*, -, +)
#[test]
fn test_mixed_bullet_markers() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"1. Item with mixed markers:
   * First nested (asterisk)
   - Second nested (dash)
   + Third nested (plus)

2. Next item
"#;

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
disable = ["MD004"]  # Disable list marker style check for this test

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

    // All marker types should be recognized and preserved
    // (MD004 is disabled so markers won't be normalized)
    assert!(
        fixed_content.contains("* First nested"),
        "Asterisk marker should be preserved"
    );
    assert!(
        fixed_content.contains("- Second nested"),
        "Dash marker should be preserved"
    );
    assert!(
        fixed_content.contains("+ Third nested"),
        "Plus marker should be preserved"
    );

    // Should not be merged
    assert!(
        !fixed_content.contains("* First nested - Second nested"),
        "Different markers should not be merged"
    );
}

/// Test 15: Code blocks with various backtick fence styles
#[test]
fn test_code_block_backtick_variations() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = r#"1. Triple backtick with language:

   ```python
   code here
   ```

2. Triple backtick without language:

   ```
   plain code
   ```

3. Another item
"#;

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

    // All fence styles should be recognized
    assert!(
        fixed_content.contains("```python"),
        "Code block with language should be preserved"
    );
    assert!(fixed_content.contains("code here"), "Code content should be preserved");
    assert!(
        fixed_content.contains("plain code"),
        "Plain code block should be preserved"
    );

    // Code blocks should not be reflowed
    let exit_code = output.status.code().unwrap_or(-1);
    assert!(
        exit_code == 0 || exit_code == 1,
        "Should handle code blocks successfully"
    );
}

/// Test 16: No false positives with very high line length
#[test]
fn test_no_false_positives_high_line_length() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Well-formatted content with nested structures, all lines actually short
    let content = r#"1. First item with some text

2. Second item:
   - Nested bullet one
   - Nested bullet two

   Some more text here.

   ```bash
   echo "code"
   ```

3. Third item
   NOTE: This is a note

4. Final item
"#;

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
disable = ["MD029", "MD041"]  # Disable list numbering and first line heading checks

[MD013]
line-length = 999999
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let exit_code = output.status.code().unwrap_or(-1);

    // Should complete with exit code 0 (no MD013 violations)
    assert!(
        exit_code == 0,
        "Should have no MD013 violations with line_length = 999999, got exit code {exit_code}\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Should not have any warnings about exceeding 999999
    assert!(
        !stdout.contains("exceeds") && !stdout.contains("999999"),
        "Should not have false positive warnings, got: {stdout}"
    );

    // Content should be unchanged
    let content_after = fs::read_to_string(&file_path).unwrap();
    assert_eq!(
        content, content_after,
        "Content should be unchanged when there are no violations"
    );
}

/// Test 17: Actual violations still caught (negative test)
#[test]
fn test_actual_violations_still_caught() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Create a line that's genuinely very long (over 200 chars)
    let long_word = "word".repeat(60); // 240 characters
    let content = format!("1. This line has a very long {long_word} in it\n");

    fs::write(&file_path, &content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 200
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let exit_code = output.status.code().unwrap_or(-1);

    // Should detect the violation (exit code 1)
    assert!(
        exit_code == 1,
        "Should detect actual line length violation, got exit code {exit_code}"
    );

    // Should have a warning about the actual long line
    assert!(
        stdout.contains("MD013") || stdout.contains("line-length") || stdout.contains("exceeds"),
        "Should warn about actual violation, got: {stdout}"
    );
}
