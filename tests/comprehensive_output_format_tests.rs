use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

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

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
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

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("check").arg("--output-format").arg("full").arg(&test_file);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("[MD022]"))
        .stdout(predicate::str::contains("[MD009]"));
}

#[test]
fn test_concise_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("check").arg("--output-format").arg("concise").arg(&test_file);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains(":1:1: [MD022]"))
        .stdout(predicate::str::contains(":2:28: [MD009]"));
}

#[test]
fn test_grouped_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
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

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
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

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("check")
        .arg("--output-format")
        .arg("json-lines")
        .arg(&test_file);

    let output = cmd.assert().failure().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    // JSON Lines format should have one JSON object per line
    let lines: Vec<&str> = stdout.lines().filter(|l| l.contains("\"rule\"")).collect();
    assert!(lines.len() >= 2, "Expected at least 2 JSON lines");

    // Each line should be valid JSON
    for line in lines {
        assert!(line.contains(r#""rule":"MD"#));
        assert!(line.contains(r#""file":"#));
    }
}

#[test]
fn test_github_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("check").arg("--output-format").arg("github").arg(&test_file);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("::warning file="))
        .stdout(predicate::str::contains("line=1,col=1,"))
        .stdout(predicate::str::contains("title=MD022::"))
        .stdout(predicate::str::contains("line=2,col=28,"))
        .stdout(predicate::str::contains("title=MD009::"))
        // Also check for new endLine/endColumn parameters
        .stdout(predicate::str::contains("endLine="))
        .stdout(predicate::str::contains("endColumn="));
}

#[test]
fn test_gitlab_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("check").arg("--output-format").arg("gitlab").arg(&test_file);

    let output = cmd.assert().failure().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    // GitLab format is a JSON array
    assert!(stdout.starts_with("["));
    assert!(stdout.contains(r#""check_name": "MD022""#));
    assert!(stdout.contains(r#""check_name": "MD009""#));
}

#[test]
fn test_pylint_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("check").arg("--output-format").arg("pylint").arg(&test_file);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains(":1:1: [CMD022]"))
        .stdout(predicate::str::contains(":2:28: [CMD009]"));
}

#[test]
fn test_azure_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("check").arg("--output-format").arg("azure").arg(&test_file);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("##vso[task.logissue type=warning;sourcepath="))
        .stdout(predicate::str::contains("linenumber=1;columnnumber=1;code=MD022]"))
        .stdout(predicate::str::contains("linenumber=2;columnnumber=28;code=MD009]"));
}

#[test]
fn test_sarif_output_format() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
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

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
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

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
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

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("check")
        .arg("--fix")
        .arg("--output-format")
        .arg("text")
        .arg(&test_file);

    // In fix mode, rumdl exits with code 1 even if all issues were fixed
    cmd.assert().failure().stdout(predicate::str::contains("Fixed:"));

    // Verify the file was actually fixed
    let fixed_content = fs::read_to_string(&test_file).unwrap();
    // In non-strict mode, MD009 normalizes trailing spaces to 2 spaces (for line breaks)
    assert!(
        fixed_content.contains("  \n"),
        "Trailing spaces should be normalized to 2 spaces"
    );
    assert!(
        fixed_content.contains("\n\n"),
        "Blank line should be added after heading"
    );
}

#[test]
fn test_output_format_with_silent_mode() {
    let (_temp_dir, test_file) = create_test_file();

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
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

    let mut cmd = Command::cargo_bin("rumdl").unwrap();
    cmd.arg("check")
        .arg("--quiet")
        .arg("--output-format")
        .arg("text")
        .arg(&test_file);

    let output = cmd.assert().failure().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    // In quiet mode, rumdl shows no output
    assert!(stdout.is_empty() || stdout == "Error\n");
}
