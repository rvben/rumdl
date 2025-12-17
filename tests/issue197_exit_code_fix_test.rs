// Test for Issue #197: rumdl check --fix should return exit code 0 when all violations are fixed
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_issue197_exit_code_after_all_fixes() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create a file with a fixable issue (MD007 - list indentation)
    // This will be fixed by --fix
    fs::write(
        &test_file,
        "# Heading\n\n- list item\n    - nested item (4 spaces, should be 2)\n",
    )
    .unwrap();

    // Create config to set MD007 indent to 2
    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_file, "[MD007]\nindent = 2\n").unwrap();

    // Run rumdl check --fix
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(test_file.to_str().unwrap())
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let exit_code = output.status.code().unwrap_or(-1);

    // Verify the fix was applied
    assert!(
        stdout.contains("[fixed]") || stdout.contains("Fixed:"),
        "Should show that issues were fixed. stdout: {stdout}\nstderr: {stderr}"
    );

    // Verify exit code is 0 when all issues are fixed
    assert_eq!(
        exit_code, 0,
        "Exit code should be 0 when all issues are fixed. stdout: {stdout}\nstderr: {stderr}\nexit_code: {exit_code}"
    );

    // Verify the message shows all issues were fixed
    assert!(
        stdout.contains("Fixed:") && (stdout.contains("Fixed 1/1") || stdout.contains("Fixed: 1/1")),
        "Should show 'Fixed: Fixed 1/1 issues' message. stdout: {stdout}"
    );
}

#[test]
fn test_issue197_exit_code_with_remaining_issues() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create a file with both fixable and unfixable issues
    // MD007 (fixable) and MD041 (unfixable - first line must be heading)
    fs::write(
        &test_file,
        "This is not a heading (MD041 violation - unfixable)\n\n- list item\n    - nested item (MD007 violation - fixable)\n",
    )
    .unwrap();

    // Create config to set MD007 indent to 2
    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_file, "[MD007]\nindent = 2\n").unwrap();

    // Run rumdl check --fix
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(test_file.to_str().unwrap())
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let exit_code = output.status.code().unwrap_or(-1);

    // Verify exit code is 1 when some issues remain (unfixable)
    assert_eq!(
        exit_code, 1,
        "Exit code should be 1 when unfixable issues remain. stdout: {stdout}\nstderr: {stderr}\nexit_code: {exit_code}"
    );
}
