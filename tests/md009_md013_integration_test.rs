/// Integration test for issue #76 - trailing whitespace with reflow
/// https://github.com/rvben/rumdl/issues/76
///
/// Tests the actual fix behavior to ensure trailing whitespace is removed
/// before reflow, preventing it from becoming mid-line whitespace.
use std::fs;
use tempfile::tempdir;

#[test]
fn test_trailing_whitespace_removed_before_reflow_integration() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Content with trailing whitespace at end of lines (spaces after "and" and "beyond")
    // The trailing spaces should be removed BEFORE reflow combines the lines
    let content = "1. It **generated an application template**. There's a lot of files and   \n    configurations required to build a native installer, above and  \n    beyond the code of your actual application.\n";

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
    // 1. Content has been reflowed into a single line
    // 2. No trailing whitespace exists anywhere
    // 3. No mid-line double spaces (which would indicate trailing spaces became mid-line)

    // Should be reflowed to single line (within the list item)
    let lines: Vec<&str> = fixed_content.lines().collect();
    assert!(
        lines.len() <= 2, // The list item line and possibly a trailing newline
        "Content should be reflowed to a single list item line, got {} lines",
        lines.len()
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
    // But definitely not 3+ spaces
    assert!(
        !fixed_content.contains("   "),
        "Fixed content should not contain excessive mid-line whitespace: {fixed_content:?}"
    );

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
fn test_multi_paragraph_list_items_with_trailing_whitespace() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // The exact example from issue #76 - note the trailing spaces at end of lines
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

    // No trailing whitespace anywhere
    for line in fixed_content.lines() {
        assert!(
            !line.ends_with(' '),
            "No line should have trailing whitespace: {line:?}"
        );
    }

    // No excessive mid-line whitespace
    assert!(
        !fixed_content.contains("   "),
        "Fixed content should not contain excessive mid-line whitespace"
    );

    // Verify the command succeeded
    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl should succeed, got: {status:?}"
    );
}
