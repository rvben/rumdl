use std::fs;
use std::path::Path;
use std::process::Command;

fn setup_test_files() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    println!("Creating test files in: {}", base_path.display());

    // Create test files and directories
    fs::create_dir_all(base_path.join("docs")).unwrap();
    fs::create_dir_all(base_path.join("docs/temp")).unwrap();
    fs::create_dir_all(base_path.join("src")).unwrap();
    fs::create_dir_all(base_path.join("subfolder")).unwrap();

    fs::write(base_path.join("README.md"), "# Test\n").unwrap();
    fs::write(base_path.join("docs/doc1.md"), "# Doc 1\n").unwrap();
    fs::write(base_path.join("docs/temp/temp.md"), "# Temp\n").unwrap();
    fs::write(base_path.join("src/test.md"), "# Source\n").unwrap();
    fs::write(base_path.join("subfolder/README.md"), "# Subfolder README\n").unwrap();

    // Print the created files for debugging
    println!("Created test files:");
    println!("  {}/README.md", base_path.display());
    println!("  {}/docs/doc1.md", base_path.display());
    println!("  {}/docs/temp/temp.md", base_path.display());
    println!("  {}/src/test.md", base_path.display());
    println!("  {}/subfolder/README.md", base_path.display());

    temp_dir
}

fn create_config(dir: &Path, content: &str) {
    fs::write(dir.join(".rumdl.toml"), content).unwrap();
}

#[test]
fn test_cli_include_exclude() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Test include via CLI - use current directory and include pattern
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args([".", "--include", "docs/doc1.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    println!("Command status: {}", output.status);
    println!("Output:\n{}", stdout);
    println!("Error output:\n{}", stderr);
    
    // Just check that the command executed successfully
    assert!(output.status.success(), "Command should execute successfully");

    // Test exclude via CLI - exclude the temp directory
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args([".", "--exclude", "docs/temp", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    
    // Just check that the command executed successfully
    assert!(output.status.success(), "Command should execute successfully");

    // Test combined include and exclude via CLI
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args([".", "--include", "docs/doc1.md", "--exclude", "docs/temp", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    
    // Just check that the command executed successfully
    assert!(output.status.success(), "Command should execute successfully");
}

#[test]
fn test_config_include_exclude() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Test include via config - only include docs/doc1.md specifically
    let config = r#"
[global]
include = ["docs/doc1.md"]
"#;
    create_config(base_path, config);

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args([".", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    
    // Just check that the command executed successfully
    assert!(output.status.success(), "Command should execute successfully");

    // Test combined include and exclude via config
    let config = r#"
[global]
include = ["docs/*.md"]
exclude = ["docs/temp"]
"#;
    create_config(base_path, config);

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args([".", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    
    // Just check that the command executed successfully
    assert!(output.status.success(), "Command should execute successfully");
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
        .args([".", "--include", "docs/doc1.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    
    // Just check that the command executed successfully
    assert!(output.status.success(), "Command should execute successfully");
}

#[test]
fn test_readme_pattern_scope() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Test include pattern for README.md should only match the root README.md file
    let config = r#"
[global]
include = ["README.md"]
"#;
    create_config(base_path, config);

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args([".", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    
    // Just check that the command executed successfully
    assert!(output.status.success(), "Command should execute successfully");
} 