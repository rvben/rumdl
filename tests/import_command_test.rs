use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use tempfile::tempdir;

/// Test that `rumdl import` does not generate a trailing blank line in the output file.
/// Regression test for https://github.com/rvben/rumdl/issues/433
#[test]
fn test_import_no_trailing_blank_line_in_file() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");

    let input = temp_dir.path().join("input.json");
    fs::write(&input, r#"{"MD003": {"style": "atx"}, "MD046": {"style": "fenced"}}"#).expect("Failed to write input");

    let output = temp_dir.path().join("output.toml");

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("import")
        .arg(input.to_str().unwrap())
        .arg("--output")
        .arg(output.to_str().unwrap());

    cmd.assert().success();

    let content = fs::read_to_string(&output).expect("Failed to read output");

    // File must end with exactly one newline, not a blank line
    assert!(
        content.ends_with('\n'),
        "Output file should end with a newline, got: {content:?}"
    );
    assert!(
        !content.ends_with("\n\n"),
        "Output file should not have a trailing blank line, got: {content:?}"
    );
}

/// Test that `rumdl import --dry-run` does not produce a trailing blank line.
#[test]
fn test_import_dry_run_no_trailing_blank_line() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");

    let input = temp_dir.path().join("input.json");
    fs::write(&input, r#"{"MD003": {"style": "atx"}, "MD046": {"style": "fenced"}}"#).expect("Failed to write input");

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("import").arg(input.to_str().unwrap()).arg("--dry-run");

    let output = cmd.output().expect("Failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.ends_with('\n'),
        "Dry-run output should end with a newline, got: {stdout:?}"
    );
    assert!(
        !stdout.ends_with("\n\n"),
        "Dry-run output should not have a trailing blank line, got: {stdout:?}"
    );
}

/// Test that `rumdl import --format json --dry-run` does not produce a trailing blank line.
/// Regression: changing println! to print! in dry-run broke JSON output (no trailing newline).
#[test]
fn test_import_json_dry_run_no_trailing_blank_line() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");

    let input = temp_dir.path().join("input.json");
    fs::write(&input, r#"{"MD003": {"style": "atx"}}"#).expect("Failed to write input");

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("import")
        .arg(input.to_str().unwrap())
        .arg("--format")
        .arg("json")
        .arg("--dry-run");

    let output = cmd.output().expect("Failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.ends_with('\n'),
        "JSON dry-run output should end with a newline, got: {stdout:?}"
    );
    assert!(
        !stdout.ends_with("\n\n"),
        "JSON dry-run output should not have a trailing blank line, got: {stdout:?}"
    );
}

/// Test that sections are still separated by blank lines (formatting preserved).
#[test]
fn test_import_sections_separated_by_blank_lines() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");

    let input = temp_dir.path().join("input.json");
    fs::write(&input, r#"{"MD003": {"style": "atx"}, "MD046": {"style": "fenced"}}"#).expect("Failed to write input");

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("import").arg(input.to_str().unwrap()).arg("--dry-run");

    let output = cmd.output().expect("Failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Both rule sections should appear
    assert!(stdout.contains("[MD003]"), "Output should contain [MD003] section");
    assert!(stdout.contains("[MD046]"), "Output should contain [MD046] section");

    // There should be a blank line between sections (before each non-first header)
    assert!(
        stdout.contains("\n\n[MD046]"),
        "Output should have blank lines between sections, got: {stdout:?}"
    );
}
