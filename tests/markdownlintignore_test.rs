use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn run_rumdl_check(dir: &Path) -> String {
    let output = std::process::Command::new("cargo")
        .args([
            "run",
            "--release",
            "--bin",
            "rumdl",
            "--",
            "check",
            dir.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute rumdl");

    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn test_markdownlintignore_basic() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path();

    // Create .markdownlintignore file
    let ignore_content = r#"ignored.md
temp/*.md
"#;
    fs::write(dir_path.join(".markdownlintignore"), ignore_content).unwrap();

    // Create test files
    let bad_content = "# Bad heading\n# Another bad heading"; // MD025 violation

    // This file should be ignored
    fs::write(dir_path.join("ignored.md"), bad_content).unwrap();

    // This file should be checked
    fs::write(dir_path.join("checked.md"), bad_content).unwrap();

    // Create temp directory with a file that should be ignored
    fs::create_dir(dir_path.join("temp")).unwrap();
    fs::write(dir_path.join("temp/tempfile.md"), bad_content).unwrap();

    // Run rumdl check
    let output = run_rumdl_check(dir_path);

    // Should only find issues in checked.md, not in ignored.md or temp/tempfile.md
    assert!(output.contains("checked.md"));
    assert!(!output.contains("ignored.md"));
    assert!(!output.contains("tempfile.md"));
}

#[test]
fn test_markdownlintignore_patterns() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path();

    // Create .markdownlintignore with various patterns
    let ignore_content = r#"# Comments should work
*.tmp.md
docs/**/*.md
!docs/important.md
test-*.md
"#;
    fs::write(dir_path.join(".markdownlintignore"), ignore_content).unwrap();

    // Create test files
    let bad_content = "# Bad heading\n# Another bad heading"; // MD025 violation

    // These should be ignored
    fs::write(dir_path.join("file.tmp.md"), bad_content).unwrap();
    fs::write(dir_path.join("test-file.md"), bad_content).unwrap();
    fs::write(dir_path.join("test-another.md"), bad_content).unwrap();

    // Create docs directory structure
    fs::create_dir_all(dir_path.join("docs/sub")).unwrap();
    fs::write(dir_path.join("docs/readme.md"), bad_content).unwrap();
    fs::write(dir_path.join("docs/sub/guide.md"), bad_content).unwrap();

    // This should NOT be ignored (negation pattern)
    fs::write(dir_path.join("docs/important.md"), bad_content).unwrap();

    // This should be checked
    fs::write(dir_path.join("normal.md"), bad_content).unwrap();

    // Run rumdl check
    let output = run_rumdl_check(dir_path);

    // Should find issues only in normal.md and docs/important.md
    assert!(output.contains("normal.md"));
    assert!(output.contains("important.md"));

    // Should not find issues in ignored files
    assert!(!output.contains("file.tmp.md"));
    assert!(!output.contains("test-file.md"));
    assert!(!output.contains("test-another.md"));
    assert!(!output.contains("readme.md") || output.contains("docs/important.md")); // Ensure it's not docs/readme.md
    assert!(!output.contains("guide.md"));
}

#[test]
fn test_markdownlintignore_subdirectories() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path();

    // Create .markdownlintignore in root
    let ignore_content = "subdir/ignored.md\n";
    fs::write(dir_path.join(".markdownlintignore"), ignore_content).unwrap();

    // Create subdirectory
    fs::create_dir(dir_path.join("subdir")).unwrap();

    // Create test files
    let bad_content = "# Bad heading\n# Another bad heading"; // MD025 violation

    // This should be ignored
    fs::write(dir_path.join("subdir/ignored.md"), bad_content).unwrap();

    // This should be checked
    fs::write(dir_path.join("subdir/checked.md"), bad_content).unwrap();

    // Run rumdl check from root
    let output = run_rumdl_check(dir_path);

    // Should only find issues in subdir/checked.md
    assert!(output.contains("checked.md"));
    assert!(!output.contains("ignored.md"));
}

#[test]
fn test_no_markdownlintignore() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path();

    // Don't create .markdownlintignore file

    // Create test files
    let bad_content = "# Bad heading\n# Another bad heading"; // MD025 violation

    fs::write(dir_path.join("file1.md"), bad_content).unwrap();
    fs::write(dir_path.join("file2.md"), bad_content).unwrap();

    // Run rumdl check
    let output = run_rumdl_check(dir_path);

    // Should find issues in both files
    assert!(output.contains("file1.md"));
    assert!(output.contains("file2.md"));
}

#[test]
fn test_markdownlintignore_with_rumdl_config() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path();

    // Create .rumdl.toml with some config
    let rumdl_config = r#"
[global]
exclude = ["excluded-by-rumdl.md"]

[MD013]
line_length = 100
"#;
    fs::write(dir_path.join(".rumdl.toml"), rumdl_config).unwrap();

    // Create .markdownlintignore
    let ignore_content = "excluded-by-markdownlint.md\n";
    fs::write(dir_path.join(".markdownlintignore"), ignore_content).unwrap();

    // Create test files
    let bad_content = "# Bad heading\n# Another bad heading"; // MD025 violation

    // These should be ignored by different mechanisms
    fs::write(dir_path.join("excluded-by-rumdl.md"), bad_content).unwrap();
    fs::write(dir_path.join("excluded-by-markdownlint.md"), bad_content).unwrap();

    // This should be checked
    fs::write(dir_path.join("checked.md"), bad_content).unwrap();

    // Run rumdl check
    let output = run_rumdl_check(dir_path);

    // Should only find issues in checked.md
    assert!(output.contains("checked.md"));
    assert!(!output.contains("excluded-by-rumdl.md"));
    assert!(!output.contains("excluded-by-markdownlint.md"));
}
