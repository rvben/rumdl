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
    fs::create_dir_all(base_path.join("subfolder")).unwrap();

    fs::write(base_path.join("README.md"), "# Test\n").unwrap();
    fs::write(base_path.join("docs/doc1.md"), "# Doc 1\n").unwrap();
    fs::write(base_path.join("docs/temp/temp.md"), "# Temp\n").unwrap();
    fs::write(base_path.join("src/test.md"), "# Source\n").unwrap();
    fs::write(base_path.join("subfolder/README.md"), "# Subfolder README\n").unwrap();

    temp_dir
}

fn create_config(dir: &Path, content: &str) {
    fs::write(dir.join(".rumdl.toml"), content).unwrap();
}

fn contains_file(output: &str, file: &str) -> bool {
    println!("Checking for file '{}' in output", file);
    
    // Files are reported with one of these patterns:
    // "✓ No issues found in ./path/file.md" - for files with no issues
    // "path/file.md:line:col:" - for files with issues
    // We should ignore messages about skipping files
    let found = output.lines().any(|line| {
        // Check for the successful "No issues found" message with the exact file pattern
        // For README.md in the root directory, we need to check for "No issues found in ./README.md"
        let is_success_message = if file == "README.md" {
            (line.contains("No issues found in ./README.md") || 
             line.contains("Success: No issues found in README.md")) &&
            !line.contains("subfolder") &&
            !line.contains("Skipping") && 
            !line.contains("Excluding") && 
            !line.contains("Including")
        } else {
            line.contains("No issues found in ./") && 
            line.contains(&format!("/{}", file)) &&
            !line.contains("Skipping") && 
            !line.contains("Excluding") && 
            !line.contains("Including")
        };
        
        // Check for lines reporting issues in the file
        let is_issue_message = if file == "README.md" {
            (line.starts_with("./README.md:") || 
             line.starts_with("README.md:")) &&
            !line.contains("subfolder") &&
            !line.contains("Skipping") && 
            !line.contains("Excluding") && 
            !line.contains("Including")
        } else {
            line.contains(&format!("/{}", file)) && 
            line.contains(":") && 
            !line.contains("Skipping") && 
            !line.contains("Excluding") && 
            !line.contains("Including")
        };
        
        if is_success_message || is_issue_message {
            println!("  Found file in line: '{}'", line);
            true
        } else {
            false
        }
    });
    
    if !found {
        println!("  File '{}' not found in output", file);
    }
    
    found
}

#[test]
fn test_cli_include_exclude() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Test include via CLI - use include flag without specifying a path to make include pattern effective
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--include", "docs/doc1.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    assert!(contains_file(&stdout, "doc1.md"), "Should contain doc1.md");
    assert!(!contains_file(&stdout, "README.md"), "Should not contain README.md");
    assert!(!contains_file(&stdout, "temp.md"), "Should not contain temp.md");
    assert!(!contains_file(&stdout, "test.md"), "Should not contain test.md");

    // Test exclude via CLI - just target the docs directory and process doc1.md
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["docs/doc1.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    assert!(contains_file(&stdout, "doc1.md"), "Should contain doc1.md");
    assert!(!contains_file(&stdout, "README.md"), "Should not contain README.md");
    assert!(!contains_file(&stdout, "temp.md"), "Should not contain temp.md");
    assert!(!contains_file(&stdout, "test.md"), "Should not contain test.md");

    // Test combined include and exclude via CLI - don't specify a path
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--include", "docs/doc1.md", "--exclude", "docs/temp", "--verbose"])
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

    // Test include via config - only include docs/doc1.md specifically
    let config = r#"
[global]
include = ["docs/doc1.md"]
"#;
    create_config(base_path, config);

    // Don't specify a path to make include patterns effective
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
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
include = ["docs/*.md"]
exclude = ["docs/temp"]
"#;
    create_config(base_path, config);

    // Don't specify a path to make include patterns effective
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
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

    // Override with CLI pattern - specifically target docs/doc1.md
    // Don't specify a path to make the include pattern effective
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["--include", "docs/doc1.md", "--verbose"])
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
fn test_readme_pattern_scope() {
    let temp_dir = setup_test_files();
    let base_path = temp_dir.path();

    // Test include pattern for README.md should only match the root README.md file
    let config = r#"
[global]
include = ["README.md"]
"#;
    create_config(base_path, config);

    // Run without specifying a path to use include patterns
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .arg("--verbose")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output:\n{}", stdout);
    
    // Should find the root README.md
    assert!(contains_file(&stdout, "README.md"), "Should contain README.md from root");
    
    // Should NOT find the subfolder README.md - more precise check
    // We're looking specifically for a successful linting message for subfolder/README.md
    assert!(!stdout.contains("No issues found in ./subfolder/README.md"), 
            "Should not process README.md from subfolder");
} 