//! Tests for CLI rule alias support (Issue #242) and unknown rule validation (Issue #243)
//!
//! Verifies that CLI flags --enable, --disable, --extend-enable, and --extend-disable
//! accept both rule IDs (MD001) and human-readable aliases (heading-increment).
//! Also verifies that unknown rule names produce warnings.

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

// ============================================================================
// Issue #243: Unknown rule validation tests
// ============================================================================

#[test]
fn test_unknown_rule_in_enable_produces_warning() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: --enable with unknown rule should produce warning
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--enable", "abc"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("Unknown rule stderr: {stderr}");

    assert!(
        stderr.contains("[cli warning]"),
        "Should produce a CLI warning for unknown rule"
    );
    assert!(
        stderr.contains("Unknown rule in --enable: abc"),
        "Warning should mention the unknown rule name"
    );
}

#[test]
fn test_unknown_rule_in_disable_produces_warning() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--disable", "xyz"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("Unknown rule in --disable: xyz"),
        "Should warn about unknown rule in --disable"
    );
}

#[test]
fn test_unknown_rule_in_extend_enable_produces_warning() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--extend-enable", "nonexistent"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("Unknown rule in --extend-enable: nonexistent"),
        "Should warn about unknown rule in --extend-enable"
    );
}

#[test]
fn test_unknown_rule_in_extend_disable_produces_warning() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--extend-disable", "fake-rule"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("Unknown rule in --extend-disable: fake-rule"),
        "Should warn about unknown rule in --extend-disable"
    );
}

#[test]
fn test_unknown_rule_suggests_similar() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: typo in rule name should suggest correct spelling
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--enable", "line-lenght"]) // typo: lenght
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("Typo suggestion stderr: {stderr}");

    assert!(
        stderr.contains("did you mean"),
        "Should suggest similar rule name for typos"
    );
    assert!(
        stderr.contains("line-length"),
        "Should suggest 'line-length' for 'line-lenght'"
    );
}

#[test]
fn test_mixed_valid_and_invalid_rules() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: mix of valid and invalid rules
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--enable", "MD001,abc,MD003", "--verbose"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should warn about 'abc'
    assert!(
        stderr.contains("Unknown rule in --enable: abc"),
        "Should warn about unknown rule 'abc'"
    );

    // Should still enable valid rules
    assert!(
        stdout.contains("MD001"),
        "MD001 should still be enabled despite invalid rule in list"
    );
    assert!(
        stdout.contains("MD003"),
        "MD003 should still be enabled despite invalid rule in list"
    );
}

#[test]
fn test_special_all_value_is_valid() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: "all" special value should not produce warning
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--disable", "all"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("[cli warning]"),
        "'all' should be a valid special value and not produce warnings"
    );
}

#[test]
fn test_silent_flag_suppresses_cli_warnings() {
    let temp_dir = setup_test_file();
    let base_path = temp_dir.path();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Test: --silent should suppress CLI warnings
    let output = Command::new(rumdl_exe)
        .current_dir(base_path)
        .args(["check", "test.md", "--enable", "unknown-rule", "--silent"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("[cli warning]"),
        "--silent should suppress CLI warnings"
    );
}

/// Test that inline config comments with unknown rules produce warnings
#[test]
fn test_inline_config_unknown_rule_warning() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let test_file = temp_dir.path().join("test.md");
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a file with an inline config comment using an unknown rule
    fs::write(
        &test_file,
        r#"# Test
<!-- rumdl-disable nonexistent-rule -->
Some content
"#,
    )
    .expect("Failed to write test file");

    let output = Command::new(rumdl_exe)
        .args(["check", "--no-cache", test_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("Inline config warning stderr: {stderr}");

    assert!(
        stderr.contains("[inline config warning]"),
        "Should produce an inline config warning for unknown rule"
    );
    assert!(
        stderr.contains("nonexistent-rule"),
        "Warning should mention the unknown rule name"
    );
}

/// Test that inline config comments with valid rules don't produce warnings
#[test]
fn test_inline_config_valid_rule_no_warning() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let test_file = temp_dir.path().join("test.md");
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a file with an inline config comment using a valid rule
    fs::write(
        &test_file,
        r#"# Test
<!-- rumdl-disable MD013 -->
Some content
"#,
    )
    .expect("Failed to write test file");

    let output = Command::new(rumdl_exe)
        .args(["check", "--no-cache", test_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("[inline config warning]"),
        "Should not produce inline config warning for valid rule MD013"
    );
}

/// Test that inline config warnings are suppressed with --silent
#[test]
fn test_inline_config_warning_silent() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let test_file = temp_dir.path().join("test.md");
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a file with an inline config comment using an unknown rule
    fs::write(
        &test_file,
        r#"# Test
<!-- rumdl-disable nonexistent-rule -->
Some content
"#,
    )
    .expect("Failed to write test file");

    let output = Command::new(rumdl_exe)
        .args(["check", "--no-cache", "--silent", test_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("[inline config warning]"),
        "--silent should suppress inline config warnings"
    );
}

/// Test that inline config warning includes did-you-mean suggestion
#[test]
fn test_inline_config_warning_suggestion() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let test_file = temp_dir.path().join("test.md");
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a file with an inline config comment using a typo of a valid rule
    fs::write(
        &test_file,
        r#"# Test
<!-- rumdl-disable MD00 -->
Some content
"#,
    )
    .expect("Failed to write test file");

    let output = Command::new(rumdl_exe)
        .args(["check", "--no-cache", test_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("Suggestion warning stderr: {stderr}");

    assert!(
        stderr.contains("[inline config warning]"),
        "Should produce an inline config warning"
    );
    assert!(stderr.contains("did you mean"), "Warning should include a suggestion");
}

/// Test that inline config warnings work via stdin
#[test]
fn test_inline_config_warning_stdin() {
    use std::io::Write;
    use std::process::Stdio;

    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let content = r#"# Test
<!-- rumdl-disable nonexistent-rule -->
Some content
"#;

    let mut child = Command::new(rumdl_exe)
        .args(["check", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn command");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin.write_all(content.as_bytes()).expect("Failed to write to stdin");
    }

    let output = child.wait_with_output().expect("Failed to read output");
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("Stdin inline config warning stderr: {stderr}");

    assert!(
        stderr.contains("[inline config warning]"),
        "Should produce an inline config warning via stdin"
    );
    assert!(
        stderr.contains("nonexistent-rule"),
        "Warning should mention the unknown rule name"
    );
}

/// Test that configure-file comments are validated
#[test]
fn test_inline_config_configure_file_warning() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let test_file = temp_dir.path().join("test.md");
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a file with a configure-file comment using an unknown rule
    fs::write(
        &test_file,
        r#"# Test
<!-- rumdl-configure-file { "nonexistent_rule": { "enabled": true } } -->
Some content
"#,
    )
    .expect("Failed to write test file");

    let output = Command::new(rumdl_exe)
        .args(["check", "--no-cache", test_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("Configure-file warning stderr: {stderr}");

    assert!(
        stderr.contains("[inline config warning]"),
        "Should produce an inline config warning for configure-file"
    );
    assert!(
        stderr.contains("nonexistent_rule"),
        "Warning should mention the unknown rule name"
    );
    assert!(
        stderr.contains("configure-file"),
        "Warning should mention configure-file comment type"
    );
}

/// Test that markdownlint-* variants also produce warnings for unknown rules
#[test]
fn test_inline_config_markdownlint_variant_warning() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let test_file = temp_dir.path().join("test.md");
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a file with a markdownlint-disable comment using an unknown rule
    fs::write(
        &test_file,
        r#"# Test
<!-- markdownlint-disable nonexistent-rule -->
Some content
"#,
    )
    .expect("Failed to write test file");

    let output = Command::new(rumdl_exe)
        .args(["check", "--no-cache", test_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("Markdownlint variant warning stderr: {stderr}");

    assert!(
        stderr.contains("[inline config warning]"),
        "Should produce an inline config warning for markdownlint-* variant"
    );
    assert!(
        stderr.contains("nonexistent-rule"),
        "Warning should mention the unknown rule name"
    );
}
