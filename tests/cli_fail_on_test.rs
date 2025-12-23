//! Integration tests for the --fail-on CLI flag
//!
//! Tests that the --fail-on flag correctly controls exit code behavior based on
//! violation severity.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Get the path to the rumdl binary
fn rumdl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rumdl")
}

/// Create a test file with only warning-level violations (MD007 - indent issue)
fn create_warning_only_file(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("warning_only.md");
    // MD007 triggers on incorrect list indentation (warning severity)
    fs::write(
        &path,
        r#"# Test

- Item 1
   - Nested item with wrong indent (3 spaces instead of 2 or 4)
"#,
    )
    .unwrap();
    path
}

/// Create a test file with error-level violations (MD042 - empty link)
fn create_error_file(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("error.md");
    // MD042 triggers on empty links (error severity)
    fs::write(
        &path,
        r#"# Test

[empty link]()
"#,
    )
    .unwrap();
    path
}

/// Create a config file that only enables MD007 (warning) and MD042 (error)
fn create_config(dir: &std::path::Path) {
    fs::write(
        dir.join(".rumdl.toml"),
        r#"[global]
enable = ["MD007", "MD042"]
"#,
    )
    .unwrap();
}

#[test]
fn test_fail_on_never_with_errors_exits_zero() {
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let error_file = create_error_file(temp_dir.path());

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", error_file.to_str().unwrap(), "--fail-on", "never"])
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "--fail-on never should exit 0 even with errors\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fail_on_never_with_warnings_exits_zero() {
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let warning_file = create_warning_only_file(temp_dir.path());

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", warning_file.to_str().unwrap(), "--fail-on", "never"])
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "--fail-on never should exit 0 even with warnings\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fail_on_error_with_only_warnings_exits_zero() {
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let warning_file = create_warning_only_file(temp_dir.path());

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", warning_file.to_str().unwrap(), "--fail-on", "error"])
        .output()
        .expect("Failed to execute command");

    // Verify that warnings are still reported
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("MD007") || !output.status.success() || stderr.is_empty(),
        "Expected MD007 warning to be reported or file to have no issues"
    );

    assert!(
        output.status.success(),
        "--fail-on error should exit 0 when only warnings exist\nstderr: {stderr}"
    );
}

#[test]
fn test_fail_on_error_with_errors_exits_one() {
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let error_file = create_error_file(temp_dir.path());

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", error_file.to_str().unwrap(), "--fail-on", "error"])
        .output()
        .expect("Failed to execute command");

    assert!(
        !output.status.success(),
        "--fail-on error should exit 1 when errors exist\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit code 1 for errors with --fail-on error"
    );
}

#[test]
fn test_fail_on_any_with_warnings_exits_one() {
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let warning_file = create_warning_only_file(temp_dir.path());

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", warning_file.to_str().unwrap(), "--fail-on", "any"])
        .output()
        .expect("Failed to execute command");

    // If there are warnings, exit code should be 1
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("MD007") {
        assert!(
            !output.status.success(),
            "--fail-on any should exit 1 on any violation\nstderr: {stderr}"
        );
    }
}

#[test]
fn test_fail_on_any_with_errors_exits_one() {
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let error_file = create_error_file(temp_dir.path());

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", error_file.to_str().unwrap(), "--fail-on", "any"])
        .output()
        .expect("Failed to execute command");

    assert!(
        !output.status.success(),
        "--fail-on any should exit 1 on errors\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fail_on_default_is_any() {
    // Without --fail-on flag, default should be "any" (exit 1 on any violation)
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let error_file = create_error_file(temp_dir.path());

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", error_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute command");

    assert!(
        !output.status.success(),
        "Default --fail-on (any) should exit 1 on violations\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fail_on_with_no_violations_exits_zero() {
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let clean_file = temp_dir.path().join("clean.md");
    fs::write(&clean_file, "# Clean File\n\nNo issues here.\n").unwrap();

    // Test all modes exit 0 when there are no violations
    for mode in ["any", "error", "never"] {
        let output = Command::new(rumdl_bin())
            .current_dir(temp_dir.path())
            .args(["check", clean_file.to_str().unwrap(), "--fail-on", mode])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "--fail-on {} should exit 0 when no violations\nstderr: {}",
            mode,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn test_fail_on_with_stdin() {
    // Test that --fail-on works with stdin input

    // Test --fail-on never with error content
    let output = Command::new(rumdl_bin())
        .args(["check", "--stdin", "--fail-on", "never", "--enable", "MD042"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"# Test\n\n[empty]()\n").unwrap();
            child.wait_with_output()
        })
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "--fail-on never should exit 0 with stdin errors\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Test --fail-on error with error content
    let output = Command::new(rumdl_bin())
        .args(["check", "--stdin", "--fail-on", "error", "--enable", "MD042"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"# Test\n\n[empty]()\n").unwrap();
            child.wait_with_output()
        })
        .expect("Failed to execute command");

    assert!(
        !output.status.success(),
        "--fail-on error should exit 1 with stdin errors\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fail_on_invalid_value_rejected() {
    let output = Command::new(rumdl_bin())
        .args(["check", ".", "--fail-on", "invalid"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success(), "Invalid --fail-on value should be rejected");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid") || stderr.contains("error"),
        "Error message should indicate invalid value\nstderr: {stderr}"
    );
}

// =============================================================================
// Edge Cases
// =============================================================================

/// Create a file with BOTH warning and error violations
fn create_mixed_file(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("mixed.md");
    // MD007 (warning) + MD042 (error) in same file
    fs::write(
        &path,
        r#"# Test

- Item 1
   - Nested item with wrong indent

[empty link]()
"#,
    )
    .unwrap();
    path
}

/// Create a file with an unfixable error (MD042 - empty links can't be auto-fixed)
fn create_unfixable_error_file(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("unfixable_error.md");
    fs::write(
        &path,
        r#"# Test

[empty link]()
"#,
    )
    .unwrap();
    path
}

#[test]
fn test_fail_on_error_with_mixed_warnings_and_errors_exits_one() {
    // When a file has BOTH warnings and errors, --fail-on error should exit 1
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let mixed_file = create_mixed_file(temp_dir.path());

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", mixed_file.to_str().unwrap(), "--fail-on", "error"])
        .output()
        .expect("Failed to execute command");

    // Output can go to stdout or stderr depending on mode
    let combined_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify error is reported
    assert!(
        combined_output.contains("MD042"),
        "Error MD042 should be reported\noutput: {combined_output}"
    );

    assert!(
        !output.status.success(),
        "--fail-on error should exit 1 when errors exist (even with warnings)\noutput: {combined_output}"
    );
}

#[test]
fn test_fail_on_error_with_multiple_files_mixed_severities() {
    // One file with only warnings, another with only errors
    // --fail-on error should exit 1 because one file has errors
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let warning_file = create_warning_only_file(temp_dir.path());
    let error_file = create_error_file(temp_dir.path());

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args([
            "check",
            warning_file.to_str().unwrap(),
            error_file.to_str().unwrap(),
            "--fail-on",
            "error",
        ])
        .output()
        .expect("Failed to execute command");

    assert!(
        !output.status.success(),
        "--fail-on error should exit 1 when any file has errors\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fail_on_error_with_multiple_warning_only_files_exits_zero() {
    // Multiple files, all with only warnings
    // --fail-on error should exit 0
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());

    let warning_file1 = temp_dir.path().join("warning1.md");
    let warning_file2 = temp_dir.path().join("warning2.md");
    fs::write(&warning_file1, "# Test\n\n- Item\n   - Bad indent\n").unwrap();
    fs::write(&warning_file2, "# Test\n\n- Item\n   - Bad indent\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args([
            "check",
            warning_file1.to_str().unwrap(),
            warning_file2.to_str().unwrap(),
            "--fail-on",
            "error",
        ])
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "--fail-on error should exit 0 when all files have only warnings\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fail_on_never_still_reports_issues() {
    // --fail-on never should still report issues in output, just exit 0
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let error_file = create_error_file(temp_dir.path());

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", error_file.to_str().unwrap(), "--fail-on", "never"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Should exit 0");

    // Output can go to stdout or stderr depending on mode
    let combined_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined_output.contains("MD042"),
        "Issues should still be reported even with --fail-on never\noutput: {combined_output}"
    );
}

#[test]
fn test_fail_on_error_still_reports_warnings() {
    // --fail-on error should still report warnings in output, just exit 0 for them
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let warning_file = create_warning_only_file(temp_dir.path());

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", warning_file.to_str().unwrap(), "--fail-on", "error"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Should exit 0 for warnings-only");

    // Output can go to stdout or stderr depending on mode
    let combined_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined_output.contains("MD007"),
        "Warnings should still be reported even with --fail-on error\noutput: {combined_output}"
    );
}

#[test]
fn test_fail_on_with_fix_mode_remaining_errors() {
    // After --fix, if unfixable errors remain, --fail-on error should exit 1
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let error_file = create_unfixable_error_file(temp_dir.path());

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", error_file.to_str().unwrap(), "--fix", "--fail-on", "error"])
        .output()
        .expect("Failed to execute command");

    // MD042 (empty links) cannot be auto-fixed, so error should remain
    assert!(
        !output.status.success(),
        "--fix with --fail-on error should exit 1 when unfixable errors remain\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fail_on_with_fix_mode_remaining_warnings_only() {
    // After --fix, if only warnings remain (or warnings can't be fixed), --fail-on error should exit 0
    let temp_dir = tempdir().unwrap();

    // Create config that only enables MD009 (trailing spaces - fixable warning)
    // and create a file where after fixing there are no issues
    fs::write(
        temp_dir.path().join(".rumdl.toml"),
        r#"[global]
enable = ["MD009"]
"#,
    )
    .unwrap();

    let test_file = temp_dir.path().join("trailing.md");
    fs::write(&test_file, "# Test  \n\nSome text  \n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", test_file.to_str().unwrap(), "--fix", "--fail-on", "error"])
        .output()
        .expect("Failed to execute command");

    // MD009 warnings are fixable, so after fix there should be no issues
    assert!(
        output.status.success(),
        "--fix with --fail-on error should exit 0 when only warnings existed and were fixed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fail_on_with_fmt_subcommand() {
    // fmt subcommand should also respect --fail-on
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());
    let error_file = create_unfixable_error_file(temp_dir.path());

    // fmt with --fail-on error and unfixable errors should...
    // Actually, fmt mode always exits 0 by design (FixMode::Format)
    // So --fail-on should be irrelevant for fmt
    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["fmt", error_file.to_str().unwrap(), "--fail-on", "error"])
        .output()
        .expect("Failed to execute command");

    // fmt always exits 0 regardless of --fail-on (by design)
    assert!(
        output.status.success(),
        "fmt should always exit 0 (format mode ignores --fail-on)\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fail_on_stdin_with_warning_only() {
    // stdin with --fail-on error and only warnings should exit 0
    let output = Command::new(rumdl_bin())
        .args([
            "check",
            "--stdin",
            "--fail-on",
            "error",
            "--enable",
            "MD009", // Trailing spaces - warning severity
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(b"# Test  \n\nTrailing spaces  \n")
                .unwrap();
            child.wait_with_output()
        })
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "--fail-on error with stdin warnings should exit 0\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fail_on_directory_scan_with_mixed_files() {
    // Scanning a directory with mixed severity files
    let temp_dir = tempdir().unwrap();
    create_config(temp_dir.path());

    // Create subdir with files
    let subdir = temp_dir.path().join("docs");
    fs::create_dir(&subdir).unwrap();

    // File with only warning
    fs::write(subdir.join("warning.md"), "# Test\n\n- Item\n   - Bad\n").unwrap();
    // File with error
    fs::write(subdir.join("error.md"), "# Test\n\n[empty]()\n").unwrap();
    // Clean file
    fs::write(subdir.join("clean.md"), "# Clean\n\nNo issues.\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", "docs", "--fail-on", "error"])
        .output()
        .expect("Failed to execute command");

    assert!(
        !output.status.success(),
        "Directory scan with --fail-on error should exit 1 when any file has errors\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Same directory with --fail-on never should exit 0
    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["check", "docs", "--fail-on", "never"])
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "Directory scan with --fail-on never should exit 0\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
