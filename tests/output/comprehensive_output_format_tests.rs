use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

/// Summary strings that must never appear in machine-readable output
const SUMMARY_STRINGS: &[&str] = &["Issues:", "Success:", "Fixed:", "Run `rumdl fmt`", "No issues found"];

fn assert_no_summary_text(stdout: &str, format_name: &str) {
    for s in SUMMARY_STRINGS {
        assert!(
            !stdout.contains(s),
            "{format_name} format should not contain '{s}', got:\n{stdout}"
        );
    }
}

fn create_test_file() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");

    let content = format!(
        r#"# Test Heading
Content with trailing space{}
## Second heading
More content
"#,
        "   " // Add trailing spaces programmatically to trigger MD009
    );

    fs::write(&test_file, content).unwrap();
    (temp_dir, test_file)
}

#[test]
fn test_text_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--output-format").arg("text").arg(&test_file);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("[MD022]"))
        .stdout(predicate::str::contains("[MD009]"))
        .stdout(predicate::str::contains("[*]")); // fixable indicator
}

#[test]
fn test_full_output_format_alias() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--output-format").arg("full").arg(&test_file);

    // Full format shows rule names without brackets and includes source context
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("MD022"))
        .stdout(predicate::str::contains("MD009"))
        .stdout(predicate::str::contains("-->"));
}

#[test]
fn test_concise_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--output-format").arg("concise").arg(&test_file);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains(":1:1: [MD022]"))
        .stdout(predicate::str::contains(":2:28: [MD009]"));
}

#[test]
fn test_grouped_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--output-format").arg("grouped").arg(&test_file);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("test.md:"))
        .stdout(predicate::str::contains("MD022:"))
        .stdout(predicate::str::contains("MD009:"))
        .stdout(predicate::str::contains("1:1 Expected"))
        .stdout(predicate::str::contains("2:28"))
        .stdout(predicate::str::contains("trailing spaces"));
}

#[test]
fn test_json_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--output-format").arg("json").arg(&test_file);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains(r#""rule": "MD022""#))
        .stdout(predicate::str::contains(r#""rule": "MD009""#))
        .stdout(predicate::str::contains(r#""line": 1,"#))
        .stdout(predicate::str::contains(r#""line": 2,"#));
}

#[test]
fn test_json_lines_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg("--output-format")
        .arg("json-lines")
        .arg(&test_file);

    let output = cmd.assert().failure().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    // Every non-empty line must be valid JSON — no summary text allowed
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert!(lines.len() >= 2, "Expected at least 2 JSON lines");

    for line in lines {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
        assert!(parsed.is_ok(), "Non-JSON line found in json-lines output: {line}");
        let json = parsed.unwrap();
        assert!(json.get("rule").is_some(), "Missing 'rule' field in JSON line");
        assert!(json.get("file").is_some(), "Missing 'file' field in JSON line");
    }
}

#[test]
fn test_github_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--output-format").arg("github").arg(&test_file);

    let output = cmd.assert().failure().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    // Verify expected annotations are present
    assert!(stdout.contains("::warning file=") || stdout.contains("::error file="));
    assert!(stdout.contains("title=MD022::"));
    assert!(stdout.contains("title=MD009::"));
    assert!(stdout.contains("endLine="));
    assert!(stdout.contains("endColumn="));

    // No human-readable summary lines in machine-readable output
    assert!(
        !stdout.contains("Issues:"),
        "GitHub format should not contain summary text"
    );
    assert!(
        !stdout.contains("Run `rumdl fmt`"),
        "GitHub format should not contain fix hint"
    );
}

#[test]
fn test_gitlab_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--output-format").arg("gitlab").arg(&test_file);

    let output = cmd.assert().failure().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    // GitLab format is a JSON array
    assert!(stdout.starts_with('['));
    assert!(stdout.contains(r#""check_name": "MD022""#));
    assert!(stdout.contains(r#""check_name": "MD009""#));
}

#[test]
fn test_pylint_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--output-format").arg("pylint").arg(&test_file);

    let output = cmd.assert().failure().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    assert!(stdout.contains(":1:1: [CMD022]"));
    assert!(stdout.contains(":2:28: [CMD009]"));
    assert_no_summary_text(&stdout, "pylint");
}

#[test]
fn test_azure_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--output-format").arg("azure").arg(&test_file);

    let output = cmd.assert().failure().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    assert!(stdout.contains("##vso[task.logissue type=warning;sourcepath="));
    assert!(stdout.contains("linenumber=1;columnnumber=1;code=MD022]"));
    assert!(stdout.contains("linenumber=2;columnnumber=28;code=MD009]"));
    assert_no_summary_text(&stdout, "azure");
}

#[test]
fn test_sarif_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--output-format").arg("sarif").arg(&test_file);

    let output = cmd.assert().failure().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    // SARIF format is a JSON object with specific structure
    assert!(stdout.contains(
        r#""$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json""#
    ));
    assert!(stdout.contains(r#""runs": ["#));
    assert!(stdout.contains(r#""results": ["#));
}

#[test]
fn test_junit_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--output-format").arg("junit").arg(&test_file);

    let output = cmd.assert().failure().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    // JUnit format is XML
    assert!(stdout.contains(r#"<?xml version="1.0" encoding="UTF-8"?>"#));
    assert!(stdout.contains(r#"<testsuites"#));
    assert!(stdout.contains(r#"<testcase"#));
    assert!(stdout.contains(r#"<failure"#));
}

#[test]
fn test_invalid_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg("--output-format")
        .arg("invalid_format")
        .arg(&test_file);

    cmd.assert()
        .code(2)  // Should exit with code 2 for invalid command argument
        .stderr(predicate::str::contains("invalid value 'invalid_format'"));
}

#[test]
fn test_output_format_with_fix_mode() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create content with fixable issues
    let content = "# Test\nContent with trailing space   \n";

    fs::write(&test_file, content).unwrap();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg("--fix")
        .arg("--output-format")
        .arg("text")
        .arg(&test_file);

    // In fix mode, rumdl exits with code 0 if all issues were fixed
    cmd.assert().success().stdout(predicate::str::contains("Fixed:"));

    // Verify the file was actually fixed
    let fixed_content = fs::read_to_string(&test_file).unwrap();
    // MD009 removes trailing spaces that don't match br_spaces (default 2)
    // Since the content had 3 spaces, they should be removed
    assert!(!fixed_content.contains("   "), "Trailing spaces (3) should be removed");
    assert!(
        fixed_content.contains("\n\n"),
        "Blank line should be added after heading"
    );
}

#[test]
fn test_output_format_with_silent_mode() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg("--silent")
        .arg("--output-format")
        .arg("text")
        .arg(&test_file);

    cmd.assert().failure().stdout(predicate::str::is_empty());
}

#[test]
fn test_output_format_with_quiet_mode() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg("--silent")
        .arg("--output-format")
        .arg("text")
        .arg(&test_file);

    let output = cmd.assert().failure().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    // In silent mode, rumdl shows no output
    assert!(stdout.is_empty() || stdout == "Error\n");
}

/// Verify that ALL machine-readable formats produce zero summary text on clean files.
/// A clean file should produce empty stdout and exit 0.
#[test]
fn test_machine_readable_formats_no_summary_on_clean_file() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("clean.md");
    fs::write(&test_file, "# Clean heading\n\nSome content.\n").unwrap();

    for format in &["json-lines", "github", "pylint", "azure"] {
        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.arg("check").arg("--output-format").arg(format).arg(&test_file);

        let output = cmd.assert().success().get_output().stdout.clone();
        let stdout = String::from_utf8_lossy(&output);

        assert!(
            stdout.trim().is_empty(),
            "{format} format should produce empty stdout on clean file, got: {stdout}"
        );
    }
}

/// Verify that ALL machine-readable formats produce zero summary text in fix mode.
#[test]
fn test_machine_readable_formats_no_summary_in_fix_mode() {
    for format in &["json-lines", "github", "pylint", "azure"] {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("fixable.md");
        fs::write(&test_file, "# Test\nContent with trailing space   \n").unwrap();

        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.arg("check")
            .arg("--fix")
            .arg("--output-format")
            .arg(format)
            .arg(&test_file);

        let output = cmd.assert().get_output().stdout.clone();
        let stdout = String::from_utf8_lossy(&output);

        assert_no_summary_text(&stdout, format);
    }
}

/// Verify json-lines output is strictly one valid JSON object per line,
/// with no interleaved text, for files with multiple warnings.
#[test]
fn test_json_lines_strict_validity_multiple_warnings() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("multi.md");
    // Trigger multiple rules: MD022 (blank lines around headings), MD009 (trailing spaces),
    // MD047 (files should end with newline)
    let content = format!(
        "# Heading\nContent with trailing space{}\n## Another\nMore content",
        "   "
    );
    fs::write(&test_file, content).unwrap();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg("--output-format")
        .arg("json-lines")
        .arg(&test_file);

    let output = cmd.assert().failure().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert!(
        lines.len() >= 3,
        "Expected at least 3 warnings, got {}:\n{stdout}",
        lines.len()
    );

    for (i, line) in lines.iter().enumerate() {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
        assert!(parsed.is_ok(), "Line {i} is not valid JSON: {line}");
    }
}

/// Verify batch formats (json, gitlab, sarif, junit) also produce no summary text.
/// These were already suppressed via needs_collection, but verify explicitly.
#[test]
fn test_batch_formats_no_summary_text() {
    let (_temp_dir, test_file) = create_test_file();

    for format in &["json", "gitlab", "sarif", "junit"] {
        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.arg("check").arg("--output-format").arg(format).arg(&test_file);

        let output = cmd.assert().failure().get_output().stdout.clone();
        let stdout = String::from_utf8_lossy(&output);

        assert_no_summary_text(&stdout, format);
    }
}
