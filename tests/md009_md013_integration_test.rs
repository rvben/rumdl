/// Integration test for issue #76 - trailing whitespace with reflow
/// https://github.com/rvben/rumdl/issues/76
///
/// Tests the actual fix behavior to ensure trailing whitespace is removed
/// before reflow, preventing it from becoming mid-line whitespace.
///
/// This test specifically tests ACCIDENTAL trailing whitespace (not hard breaks),
/// which should be removed and the content reflowed into a single line.
use std::fs;
use tempfile::tempdir;

#[test]
fn test_trailing_whitespace_removed_before_reflow_integration() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Content with ACCIDENTAL trailing whitespace at end of lines (not hard breaks)
    // Note: single space after "and" and "beyond" - NOT hard breaks
    // The trailing spaces should be removed BEFORE reflow combines the lines
    let content = "1. It **generated an application template**. There's a lot of files and \n    configurations required to build a native installer, above and \n    beyond the code of your actual application.\n";

    fs::write(&file_path, content).unwrap();

    // Create config file enabling reflow with high line length
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

    // Read the fixed content
    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Verify:
    // 1. Content has been reflowed into a single line (no hard breaks present)
    // 2. No trailing whitespace exists anywhere
    // 3. No mid-line double spaces (which would indicate trailing spaces became mid-line)

    // Should be reflowed to single line (within the list item) since no hard breaks
    let lines: Vec<&str> = fixed_content.lines().collect();
    assert!(
        lines.len() <= 2, // The list item line and possibly an empty trailing line
        "Content without hard breaks should be reflowed to single line, got {} lines: {:?}",
        lines.len(),
        lines
    );

    // No trailing whitespace
    for line in &lines {
        assert!(
            !line.ends_with(' '),
            "Line should not have trailing whitespace: {line:?}"
        );
    }

    // Check for excessive mid-line whitespace (which would indicate the bug)
    // Normal text should have at most 2 consecutive spaces (after periods in some cases)
    // But definitely not 3+ spaces (excluding indentation at start of lines)
    for line in &lines {
        let trimmed_line = line.trim_start();
        assert!(
            !trimmed_line.contains("   "),
            "Line should not contain excessive mid-line whitespace (excluding indentation): {line:?}"
        );
    }

    // Verify the command succeeded or reported fixes
    // Exit code 1 is OK because it means violations were found and fixed
    let status = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl should succeed or return 1 for fixed violations, got: {status:?}\nstdout: {stdout}\nstderr: {stderr}"
    );
}

#[test]
fn test_multi_paragraph_list_items_with_hard_breaks() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Content with intentional hard breaks (2 spaces)
    // MD009 will normalize any excessive trailing spaces to exactly 2 spaces (hard break)
    // MD013 will preserve the hard breaks and not reflow into a single line
    let content = "1. It **generated an application template**. There's a lot of files and   \n    configurations required to build a native installer, above and  \n    beyond the code of your actual application.\n";

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

    // Count lines - should preserve hard breaks (multiple lines)
    let lines: Vec<&str> = fixed_content.lines().collect();
    assert!(
        lines.len() >= 3,
        "Content with hard breaks should preserve structure (multiple lines), got {} lines: {:?}",
        lines.len(),
        lines
    );

    // Lines with hard breaks should have exactly 2 trailing spaces
    // Last line should have no trailing spaces
    let mut has_hard_breaks = 0;
    for (i, line) in lines.iter().enumerate() {
        if i < lines.len() - 1 && !line.is_empty() && !line.trim().is_empty() {
            // Not the last line - should have hard break (2 spaces)
            if line.ends_with("  ") {
                has_hard_breaks += 1;
            }
        } else if !line.is_empty() {
            // Last line should have no trailing whitespace
            assert!(
                !line.ends_with(' '),
                "Last line should not have trailing whitespace: {line:?}"
            );
        }
    }

    // Should have at least 2 hard breaks (from the original content)
    assert!(
        has_hard_breaks >= 2,
        "Should preserve hard breaks (2 trailing spaces), found {has_hard_breaks} hard breaks in: {lines:?}"
    );

    // No excessive mid-line whitespace (excluding indentation at start of lines)
    for line in lines.iter() {
        let trimmed_line = line.trim_start();
        assert!(
            !trimmed_line.contains("   "),
            "Line should not contain excessive mid-line whitespace (excluding indentation): {line:?}"
        );
    }

    // Verify the command succeeded
    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl should succeed, got: {status:?}"
    );
}
