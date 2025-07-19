use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn setup_test_file() -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create a test file that will trigger multiple rules
    fs::write(
        base_path.join("test.md"),
        "# Test Header\n\n### Skip Level Header\n\nThis is a very long line that exceeds the typical line length limit for markdown files and should trigger MD013\n\n<html>HTML content</html>\n",
    )
    .unwrap();

    temp_dir
}

#[test]
fn test_enable_disable_precedence() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: --enable with --disable should have disable win
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args([
            "check",
            "test.md",
            "--enable",
            "MD001",
            "--disable",
            "MD001",
            "--verbose",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Enable+Disable output: {stdout}");

    // Should show no enabled rules (MD001 disabled)
    assert!(stdout.contains("Enabled rules:"));
    assert!(
        !stdout.contains("MD001"),
        "MD001 should be disabled when both --enable and --disable are specified"
    );
    assert!(output.status.success(), "Command should succeed with no issues");
}

#[test]
fn test_enable_only() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: --enable only should enable only specified rules
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--enable", "MD001", "--verbose"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Enable only output: {stdout}");

    // Should show only MD001 enabled
    assert!(stdout.contains("MD001"), "MD001 should be enabled");
    assert!(
        !stdout.contains("MD013"),
        "MD013 should not be enabled when only MD001 is specified"
    );
    assert!(
        !stdout.contains("MD033"),
        "MD033 should not be enabled when only MD001 is specified"
    );

    // Should detect the MD001 violation (skipping heading level)
    assert!(!output.status.success(), "Command should fail due to MD001 violation");
    assert!(stdout.contains("MD001"), "Should show MD001 violation in output");
}

#[test]
fn test_disable_only() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: --disable only should disable specified rules
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--disable", "MD001", "--verbose"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Disable only output: {stdout}");

    // Should show all rules except MD001
    assert!(!stdout.contains("MD001 (Heading levels"), "MD001 should be disabled");
    assert!(stdout.contains("MD013"), "MD013 should be enabled");
    assert!(stdout.contains("MD033"), "MD033 should be enabled");

    // Should detect violations but not MD001
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{stdout}{stderr}");
    assert!(!combined_output.contains("MD001"), "Should not report MD001 violations");
}

#[test]
fn test_extend_enable() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: --extend-enable should add rules to default set
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--extend-enable", "MD001", "--verbose"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Extend enable output: {stdout}");

    // Should show all default rules plus MD001
    assert!(stdout.contains("MD001"), "MD001 should be enabled via extend-enable");
    assert!(
        stdout.contains("MD013"),
        "MD013 should still be enabled (default behavior)"
    );
    assert!(
        stdout.contains("MD033"),
        "MD033 should still be enabled (default behavior)"
    );
}

#[test]
fn test_extend_disable() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: --extend-disable should remove rules from default set
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--extend-disable", "MD001", "--verbose"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Extend disable output: {stdout}");

    // Should show all default rules except MD001
    assert!(
        !stdout.contains("MD001 (Heading levels"),
        "MD001 should be disabled via extend-disable"
    );
    assert!(stdout.contains("MD013"), "MD013 should still be enabled");
    assert!(stdout.contains("MD033"), "MD033 should still be enabled");
}

#[test]
fn test_multiple_rules() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: Multiple rules in enable/disable
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args([
            "check",
            "test.md",
            "--enable",
            "MD001,MD013",
            "--disable",
            "MD013",
            "--verbose",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Multiple rules output: {stdout}");

    // Should show only MD001 enabled (MD013 disabled by --disable)
    assert!(stdout.contains("MD001"), "MD001 should be enabled");
    assert!(!stdout.contains("MD013"), "MD013 should be disabled by --disable flag");
    assert!(
        !stdout.contains("MD033"),
        "MD033 should not be enabled when only MD001,MD013 specified"
    );
}

#[test]
fn test_extend_flags_with_disable() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: --extend-enable with --disable should have disable win
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args([
            "check",
            "test.md",
            "--extend-enable",
            "MD001",
            "--disable",
            "MD001",
            "--verbose",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Extend enable with disable output: {stdout}");

    // Should show all default rules except MD001 (disabled by --disable)
    assert!(
        !stdout.contains("MD001 (Heading levels"),
        "MD001 should be disabled by --disable even when extended"
    );
    assert!(stdout.contains("MD013"), "MD013 should still be enabled");
    assert!(stdout.contains("MD033"), "MD033 should still be enabled");
}

#[test]
fn test_case_insensitive_rules() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: Case insensitive rule names
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args([
            "check",
            "test.md",
            "--enable",
            "md001",
            "--disable",
            "MD001",
            "--verbose",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Case insensitive output: {stdout}");

    // Should work with case insensitive matching
    assert!(
        !stdout.contains("MD001 (Heading levels"),
        "MD001 should be disabled even with case mismatch"
    );
    assert!(output.status.success(), "Command should succeed with no issues");
}

#[test]
fn test_flag_combinations() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: Complex flag combination
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args([
            "check",
            "test.md",
            "--extend-enable",
            "MD001",
            "--extend-disable",
            "MD013",
            "--disable",
            "MD033",
            "--verbose",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Complex combination output: {stdout}");

    // Should show: all default rules + MD001 - MD013 - MD033
    assert!(stdout.contains("MD001"), "MD001 should be enabled via extend-enable");
    assert!(!stdout.contains("MD013"), "MD013 should be disabled via extend-disable");
    assert!(!stdout.contains("MD033"), "MD033 should be disabled via --disable");
}
