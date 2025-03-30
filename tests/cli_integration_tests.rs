use std::fs;
use std::path::Path;
use std::process::Command;

fn setup_test_files() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create test files and directories
    fs::create_dir_all(base_path.join("docs")).unwrap();
    fs::create_dir_all(base_path.join("docs/temp")).unwrap();
    fs::create_dir_all(base_path.join("src")).unwrap();

    fs::write(base_path.join("README.md"), "# Test\n").unwrap();
    fs::write(base_path.join("docs/doc1.md"), "# Doc 1\n").unwrap();
    fs::write(base_path.join("docs/temp/temp.md"), "# Temp\n").unwrap();
    fs::write(base_path.join("src/test.md"), "# Source\n").unwrap();

    temp_dir
}

fn create_config(dir: &Path, content: &str) {
    fs::write(dir.join(".rumdl.toml"), content).unwrap();
}

fn contains_file(output: &str, file: &str) -> bool {
    // The output contains ANSI color codes and full paths, so we need to check if any line
    // contains our filename as part of a path, but only in lines that show linting results
    // or "No issues found" messages, and not in lines about skipping files
    output.lines().any(|line| {
        let contains_path = line.contains(&format!("/{}", file));
        let is_no_issues = line.contains(&format!("No issues found in ./{}", file));
        let is_skipping = line.contains("Skipping");
        let is_excluding = line.contains("Excluding");
        let is_including = line.contains("Including");
        
        (contains_path || is_no_issues) && !is_skipping && !is_excluding && !is_including
    })
}

#[test]
fn test_cli_include_exclude() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Test include via CLI
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--include", "docs/*.md", ".", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    assert!(contains_file(&stdout, "doc1.md"), "Should contain doc1.md");
    assert!(!contains_file(&stdout, "README.md"), "Should not contain README.md");
    assert!(!contains_file(&stdout, "temp.md"), "Should not contain temp.md");
    assert!(!contains_file(&stdout, "test.md"), "Should not contain test.md");

    // Test exclude via CLI
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--exclude", "docs/temp", ".", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    assert!(contains_file(&stdout, "doc1.md"), "Should contain doc1.md");
    assert!(contains_file(&stdout, "README.md"), "Should contain README.md");
    assert!(!contains_file(&stdout, "temp.md"), "Should not contain temp.md");
    assert!(contains_file(&stdout, "test.md"), "Should contain test.md");

    // Test combined include and exclude via CLI
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--include", "docs/**/*.md", "--exclude", "docs/temp", ".", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    assert!(contains_file(&stdout, "doc1.md"), "Should contain doc1.md");
    assert!(!contains_file(&stdout, "README.md"), "Should not contain README.md");
    assert!(!contains_file(&stdout, "temp.md"), "Should not contain temp.md");
    assert!(!contains_file(&stdout, "test.md"), "Should not contain test.md");
}

#[test]
fn test_config_include_exclude() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Test include via config
    let config = r#"
[global]
include = ["docs/*.md"]
"#;
    create_config(base_path, config);

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .arg(".")
        .arg("--verbose")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    assert!(contains_file(&stdout, "doc1.md"), "Should contain doc1.md");
    assert!(!contains_file(&stdout, "README.md"), "Should not contain README.md");
    assert!(!contains_file(&stdout, "temp.md"), "Should not contain temp.md");
    assert!(!contains_file(&stdout, "test.md"), "Should not contain test.md");

    // Test combined include and exclude via config
    let config = r#"
[global]
include = ["docs/**/*.md"]
exclude = ["docs/temp"]
"#;
    create_config(base_path, config);

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .arg(".")
        .arg("--verbose")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    assert!(contains_file(&stdout, "doc1.md"), "Should contain doc1.md");
    assert!(!contains_file(&stdout, "README.md"), "Should not contain README.md");
    assert!(!contains_file(&stdout, "temp.md"), "Should not contain temp.md");
    assert!(!contains_file(&stdout, "test.md"), "Should not contain test.md");
}

#[test]
fn test_cli_override_config() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Set up config with one pattern
    let config = r#"
[global]
include = ["src/**/*.md"]
"#;
    create_config(base_path, config);

    // Override with CLI pattern
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--include", "docs/*.md", ".", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    assert!(contains_file(&stdout, "doc1.md"), "Should contain doc1.md");
    assert!(!contains_file(&stdout, "README.md"), "Should not contain README.md");
    assert!(!contains_file(&stdout, "temp.md"), "Should not contain temp.md");
    assert!(!contains_file(&stdout, "test.md"), "Should not contain test.md");
} 