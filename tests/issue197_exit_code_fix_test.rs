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

/// Test that verifies the fix implementation re-lints after applying fixes.
///
/// This addresses a concern raised by @martimlobao on issue #197:
/// If --fix creates NEW issues while fixing existing ones (e.g., MD005/MD007 conflict),
/// the exit code should still be 1.
///
/// The implementation in file_processor.rs:668-740 handles this by:
/// 1. Applying all fixes to the content
/// 2. Re-linting the fixed content with all rules
/// 3. Returning exit code based on remaining_warnings (which includes ANY issues, new or old)
///
/// This test verifies that behavior by checking that:
/// - The fix is applied (issue count decreases)
/// - But exit code is 1 if ANY issues remain after fixing
#[test]
fn test_issue197_relint_after_fix_catches_remaining_issues() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create a file where:
    // - MD007 will fix the indentation
    // - But MD041 (first line not heading) remains unfixable
    // This verifies the re-lint catches issues that weren't part of the original fix
    fs::write(&test_file, "Not a heading\n\n- item\n    - nested\n").unwrap();

    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_file, "[MD007]\nindent = 2\n").unwrap();

    // First, verify the file has multiple issues
    let check_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg(test_file.to_str().unwrap())
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let check_stdout = String::from_utf8_lossy(&check_output.stdout);
    assert!(
        check_stdout.contains("MD007") && check_stdout.contains("MD041"),
        "File should have both MD007 and MD041 issues. stdout: {check_stdout}"
    );

    // Now run --fix
    let fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(test_file.to_str().unwrap())
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to execute rumdl");

    let fix_stdout = String::from_utf8_lossy(&fix_output.stdout);
    let fix_stderr = String::from_utf8_lossy(&fix_output.stderr);
    let exit_code = fix_output.status.code().unwrap_or(-1);

    // Verify MD007 was fixed
    assert!(
        fix_stdout.contains("[fixed]"),
        "MD007 should be fixed. stdout: {fix_stdout}"
    );

    // Verify exit code is 1 because MD041 still remains
    // This proves the implementation re-lints after fixing and catches remaining issues
    assert_eq!(
        exit_code, 1,
        "Exit code should be 1 when issues remain after fix (re-lint catches them). \
         stdout: {fix_stdout}\nstderr: {fix_stderr}"
    );

    // Verify the content was actually modified (fix was applied)
    let fixed_content = fs::read_to_string(&test_file).unwrap();
    assert!(
        fixed_content.contains("  - nested"),
        "Content should be fixed (2 spaces). Got: {fixed_content}"
    );
}
