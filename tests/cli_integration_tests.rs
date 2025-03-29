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

#[test]
fn test_cli_include_exclude() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Test include via CLI
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--include", "docs/*.md", "."])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("doc1.md"));
    assert!(!stdout.contains("README.md"));
    assert!(!stdout.contains("temp.md"));
    assert!(!stdout.contains("test.md"));

    // Test exclude via CLI
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--exclude", "docs/temp", "."])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("doc1.md"));
    assert!(stdout.contains("README.md"));
    assert!(!stdout.contains("temp.md"));
    assert!(stdout.contains("test.md"));

    // Test combined include and exclude via CLI
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--include", "docs/**/*.md", "--exclude", "docs/temp", "."])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("doc1.md"));
    assert!(!stdout.contains("README.md"));
    assert!(!stdout.contains("temp.md"));
    assert!(!stdout.contains("test.md"));
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
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("doc1.md"));
    assert!(!stdout.contains("README.md"));
    assert!(!stdout.contains("temp.md"));
    assert!(!stdout.contains("test.md"));

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
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("doc1.md"));
    assert!(!stdout.contains("README.md"));
    assert!(!stdout.contains("temp.md"));
    assert!(!stdout.contains("test.md"));
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
        .args(["--include", "docs/*.md", "."])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("doc1.md"));
    assert!(!stdout.contains("README.md"));
    assert!(!stdout.contains("temp.md"));
    assert!(!stdout.contains("test.md"));
} 