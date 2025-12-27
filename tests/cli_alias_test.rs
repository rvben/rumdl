//! Tests for CLI rule alias support (Issue #242)
//!
//! Verifies that CLI flags --enable, --disable, --extend-enable, and --extend-disable
//! accept both rule IDs (MD001) and human-readable aliases (heading-increment).

use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn setup_test_file() -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create a test file that triggers MD001 (heading-increment) and MD013 (line-length)
    fs::write(
        base_path.join("test.md"),
        "# Test Header\n\n### Skip Level Header\n\nThis is a very long line that exceeds the typical line length limit and should trigger the line-length rule when enabled with proper configuration\n",
    )
    .unwrap();

    temp_dir
}

#[test]
fn test_enable_with_alias() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: --enable with alias should work
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--enable", "heading-increment", "--verbose"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Enable with alias output: {stdout}");

    // Should show MD001 enabled (resolved from heading-increment alias)
    assert!(
        stdout.contains("MD001"),
        "MD001 should be enabled via heading-increment alias"
    );
    // Should NOT show MD013 since we only enabled MD001
    assert!(
        !stdout.contains("MD013"),
        "MD013 should not be enabled when only heading-increment is specified"
    );
}

#[test]
fn test_enable_with_mixed_ids_and_aliases() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: --enable with mixed IDs and aliases
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--enable", "MD001,line-length", "--verbose"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Enable with mixed IDs and aliases: {stdout}");

    // Should show both MD001 and MD013 enabled
    assert!(stdout.contains("MD001"), "MD001 should be enabled");
    assert!(
        stdout.contains("MD013"),
        "MD013 should be enabled via line-length alias"
    );
}

#[test]
fn test_disable_with_alias() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: --disable with alias should work
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--disable", "heading-increment", "--verbose"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Disable with alias output: {stdout}");

    // Should NOT show MD001 in enabled rules
    let enabled_section = stdout
        .lines()
        .skip_while(|l| !l.contains("Enabled rules:"))
        .take_while(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !enabled_section.contains("MD001"),
        "MD001 should be disabled via heading-increment alias"
    );
}

#[test]
fn test_enable_with_case_insensitive_alias() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: aliases should be case-insensitive
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--enable", "HEADING-INCREMENT", "--verbose"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Enable with uppercase alias: {stdout}");

    // Should resolve uppercase alias to MD001
    assert!(
        stdout.contains("MD001"),
        "MD001 should be enabled via uppercase HEADING-INCREMENT alias"
    );
}

#[test]
fn test_enable_with_underscore_alias() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: underscores should be converted to hyphens
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--enable", "heading_increment", "--verbose"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Enable with underscore alias: {stdout}");

    // Should resolve underscore alias to MD001
    assert!(
        stdout.contains("MD001"),
        "MD001 should be enabled via heading_increment alias (underscore variant)"
    );
}

#[test]
fn test_extend_enable_with_alias() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a config that enables only MD033
    fs::write(base_path.join(".rumdl.toml"), "[global]\nenable = [\"MD033\"]\n").unwrap();

    // Test: --extend-enable with alias should add to config
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--extend-enable", "heading-increment", "--verbose"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Extend-enable with alias: {stdout}");

    // Should show both MD033 (from config) and MD001 (from extend-enable alias)
    assert!(
        stdout.contains("MD001"),
        "MD001 should be enabled via extend-enable heading-increment alias"
    );
    assert!(stdout.contains("MD033"), "MD033 should still be enabled from config");
}

#[test]
fn test_extend_disable_with_alias() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: --extend-disable with alias should disable rule
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--extend-disable", "heading-increment", "--verbose"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Extend-disable with alias: {stdout}");

    // Should NOT show MD001 in enabled rules
    let enabled_section = stdout
        .lines()
        .skip_while(|l| !l.contains("Enabled rules:"))
        .take_while(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !enabled_section.contains("MD001"),
        "MD001 should be disabled via extend-disable heading-increment alias"
    );
}

#[test]
fn test_multiple_aliases_comma_separated() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: multiple aliases in comma-separated list
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args([
            "check",
            "test.md",
            "--enable",
            "heading-increment,line-length,no-bare-urls",
            "--verbose",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Multiple aliases: {stdout}");

    // Should resolve all three aliases
    assert!(
        stdout.contains("MD001"),
        "MD001 should be enabled via heading-increment"
    );
    assert!(stdout.contains("MD013"), "MD013 should be enabled via line-length");
    assert!(stdout.contains("MD034"), "MD034 should be enabled via no-bare-urls");
}

#[test]
fn test_enable_disable_with_aliases() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: --enable with --disable using aliases
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args([
            "check",
            "test.md",
            "--enable",
            "heading-increment,line-length",
            "--disable",
            "line-length",
            "--verbose",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Enable+Disable with aliases: {stdout}");

    // Should show MD001 but NOT MD013 (disabled via alias)
    assert!(stdout.contains("MD001"), "MD001 should be enabled");

    // Check enabled rules section specifically
    let enabled_section = stdout
        .lines()
        .skip_while(|l| !l.contains("Enabled rules:"))
        .take_while(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !enabled_section.contains("MD013"),
        "MD013 should be disabled via line-length alias in --disable"
    );
}
