use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_statistics_flag() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test_stats.md");

    // Create a file with various violations
    let content = r#"# Heading 1
Content immediately after heading
## Heading 2
* item 1
+ item 2
- item 3
This line has trailing spaces
### This is a very long heading that definitely exceeds the default line length limit of eighty characters
"#;

    fs::write(&test_file, content).unwrap();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--statistics").arg(&test_file);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("Rule Violation Statistics:"))
        .stdout(predicate::str::contains("Rule"))
        .stdout(predicate::str::contains("Violations"))
        .stdout(predicate::str::contains("Fixable"))
        .stdout(predicate::str::contains("Percentage"))
        .stdout(predicate::str::contains("MD022"))
        .stdout(predicate::str::contains("MD004"))
        .stdout(predicate::str::contains("Total"))
        // Verify table structure
        .stdout(predicate::str::contains("--------------------------------------------------"));
}

#[test]
fn test_statistics_with_no_issues() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("clean.md");

    // Create a clean file with no violations
    let content = r#"# Heading

Content with proper spacing.

## Another Heading

More content.
"#;

    fs::write(&test_file, content).unwrap();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--statistics").arg(&test_file);

    cmd.assert()
        .success()
        // Should not show statistics when there are no issues
        .stdout(predicate::str::contains("Rule Violation Statistics:").not());
}

#[test]
fn test_statistics_multiple_files() {
    let temp_dir = tempdir().unwrap();

    // Create multiple files with violations
    let file1 = temp_dir.path().join("file1.md");
    fs::write(&file1, "# Heading\nNo space after heading").unwrap();

    let file2 = temp_dir.path().join("file2.md");
    fs::write(&file2, "* item 1\n+ item 2\n- item 3").unwrap();

    let file3 = temp_dir.path().join("file3.md");
    fs::write(&file3, "Trailing spaces  \nMore content").unwrap();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--statistics").arg(temp_dir.path());

    cmd.assert()
        .failure()
        // Should show combined statistics for all files
        .stdout(predicate::str::contains("Rule Violation Statistics:"))
        .stdout(predicate::str::contains("Total"))
        .stdout(predicate::str::contains("--------------------------------------------------"));
}

#[test]
fn test_statistics_with_quiet_mode() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    fs::write(&test_file, "# Heading\nNo space").unwrap();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check").arg("--statistics").arg("--silent").arg(&test_file);

    cmd.assert()
        .failure()
        // Statistics should not be shown in silent mode
        .stdout(predicate::str::is_empty());
}
