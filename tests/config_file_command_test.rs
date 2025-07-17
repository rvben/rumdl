use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_config_file_command_with_explicit_config() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("test.toml");
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a test config file
    let config_content = r#"
[global]
disable = ["MD013"]

[MD004]
style = "asterisk"
"#;
    fs::write(&config_path, config_content).unwrap();

    // Run the config file command with explicit config
    let output = Command::new(rumdl_exe)
        .args(["config", "file", "--config"])
        .arg(&config_path)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let absolute_path = fs::canonicalize(&config_path).unwrap();
    assert_eq!(stdout.trim(), absolute_path.to_string_lossy());
}

#[test]
fn test_config_file_command_with_no_config() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .args(["config", "file", "--no-config"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.trim(), "No configuration file loaded (--no-config specified)");
}

#[test]
fn test_config_file_command_with_nonexistent_config() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .args(["config", "file", "--config", "nonexistent.toml"])
        .output()
        .expect("Failed to execute command");

    // Should exit with code 2 for file not found (tool error)
    assert_eq!(output.status.code(), Some(2), "Expected exit code 2 for file not found");

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Config error"));
    assert!(stderr.contains("Failed to read config file"));
    assert!(stderr.contains("nonexistent.toml"));
}

#[test]
fn test_config_file_command_auto_discovery() {
    let temp_dir = tempdir().unwrap();

    // Create a .rumdl.toml file for auto-discovery
    let config_content = r#"
[global]
disable = ["MD013"]
"#;
    let config_path = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_path, config_content).unwrap();

    // Run the config file command (should auto-discover .rumdl.toml)
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["config", "file"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let absolute_path = fs::canonicalize(&config_path).unwrap();
    assert_eq!(stdout.trim(), absolute_path.to_string_lossy());
}

#[test]
fn test_config_file_command_multiple_files() {
    let temp_dir = tempdir().unwrap();

    // Create both pyproject.toml and .rumdl.toml
    let pyproject_content = r#"
[tool.rumdl]
line-length = 120
"#;
    let pyproject_path = temp_dir.path().join("pyproject.toml");
    fs::write(&pyproject_path, pyproject_content).unwrap();

    let rumdl_content = r#"
[global]
disable = ["MD013"]
"#;
    let rumdl_path = temp_dir.path().join(".rumdl.toml");
    fs::write(&rumdl_path, rumdl_content).unwrap();

    // Run the config file command (should find both files)
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["config", "file"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.trim().split('\n').collect();

    // Should have both files listed
    assert_eq!(lines.len(), 2);

    let pyproject_absolute = fs::canonicalize(&pyproject_path).unwrap();
    let rumdl_absolute = fs::canonicalize(&rumdl_path).unwrap();

    // Check that both paths are present (order might vary)
    let output_paths: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    assert!(output_paths.contains(&pyproject_absolute.to_string_lossy().to_string()));
    assert!(output_paths.contains(&rumdl_absolute.to_string_lossy().to_string()));
}
