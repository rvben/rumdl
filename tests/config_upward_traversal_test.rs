use std::fs;
use std::os::unix::fs as unix_fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_config_upward_traversal() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create nested directory structure
    let nested_dir = project_dir.join("subdir").join("nested");
    fs::create_dir_all(&nested_dir).unwrap();

    // Create config at project root
    let config_content = r#"
[global]
line-length = 120
disable = ["MD013", "MD033"]
"#;
    let config_path = project_dir.join(".rumdl.toml");
    fs::write(&config_path, config_content).unwrap();

    // Create a test markdown file in nested directory
    let test_file = nested_dir.join("test.md");
    fs::write(&test_file, "# Test\n\nThis is a very long line that exceeds 80 characters but should not trigger MD013 due to parent config.\n").unwrap();

    // Run rumdl from nested directory
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "test.md"])
        .current_dir(&nested_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();

    // MD013 should be disabled by parent config
    assert!(!stderr.contains("MD013"), "MD013 should be disabled by parent config");
    assert!(!stderr.contains("Line length"), "Line length warning should not appear");
}

#[test]
fn test_config_stops_at_git_boundary() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create nested directory structure with .git in middle
    let subdir = project_dir.join("subdir");
    let nested_dir = subdir.join("nested");
    fs::create_dir_all(&nested_dir).unwrap();

    // Create .git directory in subdir (boundary)
    fs::create_dir(subdir.join(".git")).unwrap();

    // Create config at project root (should not be found)
    let config_content = r#"
[global]
disable = ["MD013"]
"#;
    fs::write(project_dir.join(".rumdl.toml"), config_content).unwrap();

    // Create a test markdown file
    let test_file = nested_dir.join("test.md");
    fs::write(&test_file, "# Test\n\nThis is a very long line that exceeds 80 characters and should trigger MD013 because config is not found.\n").unwrap();

    // Run rumdl from nested directory
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "test.md"])
        .current_dir(&nested_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Check both stdout and stderr for the MD013 message
    let combined = format!("{stdout}{stderr}");

    // MD013 should trigger because config is not found (stopped at .git)
    assert!(
        combined.contains("MD013") || combined.contains("Line length"),
        "MD013 should trigger because traversal stopped at .git boundary. Output: {combined}"
    );
}

#[test]
fn test_isolated_flag_ignores_all_configs() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create config that disables MD013
    let config_content = r#"
[global]
disable = ["MD013"]
"#;
    fs::write(project_dir.join(".rumdl.toml"), config_content).unwrap();

    // Create a test markdown file
    let test_file = project_dir.join("test.md");
    fs::write(&test_file, "# Test\n\nThis is a very long line that exceeds 80 characters and should trigger MD013 when using --isolated flag.\n").unwrap();

    // Run with --isolated flag
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "test.md", "--isolated"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Check both stdout and stderr for the MD013 message
    let combined = format!("{stdout}{stderr}");

    // MD013 should trigger despite config because --isolated is used
    assert!(
        combined.contains("MD013") || combined.contains("Line length"),
        "MD013 should trigger with --isolated flag. Output: {combined}"
    );
}

#[test]
fn test_config_precedence_order() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create both pyproject.toml and .rumdl.toml
    let pyproject_content = r#"
[tool.rumdl]
line-length = 120
"#;
    fs::write(project_dir.join("pyproject.toml"), pyproject_content).unwrap();

    let rumdl_content = r#"
[global]
line-length = 100
"#;
    fs::write(project_dir.join(".rumdl.toml"), rumdl_content).unwrap();

    // Check which config is loaded
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["config", "file"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8(output.stdout).unwrap();

    // .rumdl.toml should take precedence
    assert!(
        stdout.contains(".rumdl.toml"),
        ".rumdl.toml should take precedence over pyproject.toml"
    );
    assert!(
        !stdout.contains("pyproject.toml"),
        "pyproject.toml should not be loaded when .rumdl.toml exists"
    );
}

#[test]
#[cfg(unix)]
fn test_symlinked_config_is_followed() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create a real config file
    let real_config_content = r#"
[global]
disable = ["MD013", "MD033"]
"#;
    let real_config_path = project_dir.join("real-config.toml");
    fs::write(&real_config_path, real_config_content).unwrap();

    // Create a symlink to it
    let symlink_path = project_dir.join(".rumdl.toml");
    unix_fs::symlink(&real_config_path, &symlink_path).unwrap();

    // Create a test markdown file
    let test_file = project_dir.join("test.md");
    fs::write(&test_file, "# Test\n\nThis is a very long line that exceeds 80 characters and MD013 should be disabled by symlinked config.\n").unwrap();

    // Run rumdl - should follow the symlink
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "test.md"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let combined = format!("{stdout}{stderr}");

    // MD013 should be disabled by symlinked config (following Ruff's behavior)
    assert!(
        !combined.contains("MD013"),
        "MD013 should be disabled by symlinked config"
    );
    assert!(
        !combined.contains("Line length"),
        "Line length warning should not appear"
    );
}

#[test]
fn test_markdownlint_yaml_upward_traversal() {
    // Issue #193: .markdownlint.yaml should be discovered via upward traversal
    // just like .rumdl.toml
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create nested directory structure
    let nested_dir = project_dir.join("path").join("to");
    fs::create_dir_all(&nested_dir).unwrap();

    // Create .markdownlint.yaml at project root (not in nested dir)
    let config_content = r#"
MD013:
  line_length: 200
  code_blocks: false
"#;
    fs::write(project_dir.join(".markdownlint.yaml"), config_content).unwrap();

    // Create a test markdown file in nested directory
    let test_file = nested_dir.join("file.md");
    fs::write(
        &test_file,
        "# Test\n\nThis is a line that is about 100 characters long and should not trigger MD013 due to parent config setting line_length to 200.\n",
    )
    .unwrap();

    // Run rumdl from nested directory, checking the file
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "file.md"])
        .current_dir(&nested_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let combined = format!("{stdout}{stderr}");

    // MD013 should NOT trigger because line_length=200 from parent config
    assert!(
        !combined.contains("MD013"),
        "MD013 should not trigger - .markdownlint.yaml at repo root should be discovered. Output: {combined}"
    );
}
