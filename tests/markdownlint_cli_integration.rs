use std::fs::File;
use std::io::Write;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_markdownlint_config_cli_output_matches() {
    // Create a temporary directory
    let dir = tempdir().unwrap();
    let config_path = dir.path().join(".markdownlint.json");

    // Write a sample markdownlint config
    let config_content = r#"{
        "code-block-style": { "style": "fenced" },
        "ul-style": { "style": "dash" }
    }"#;
    let mut file = File::create(&config_path).unwrap();
    file.write_all(config_content.as_bytes()).unwrap();

    // Run the built rumdl CLI binary in the tempdir
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["config", "--output", "toml"])
        .current_dir(&dir)
        .output()
        .expect("Failed to run rumdl CLI");

    assert!(
        output.status.success(),
        "CLI did not exit successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let toml_str = String::from_utf8_lossy(&output.stdout);
    println!("TOML output:\n{toml_str}"); // Print the actual output for debugging

    // Parse the TOML output
    let toml_value: toml::Value = toml::from_str(&toml_str).expect("Failed to parse TOML output");

    // Check that the mapped values are present and correct at the top level
    let md046_table = toml_value.get("MD046").expect("No [MD046] table in output");
    assert_eq!(md046_table["style"].as_str().unwrap(), "fenced");
    let md004_table = toml_value.get("MD004").expect("No [MD004] table in output");
    assert_eq!(md004_table["style"].as_str().unwrap(), "dash");
}

#[test]
fn test_fallback_to_markdownlint_when_pyproject_has_no_rumdl() {
    // Create a temporary directory
    let dir = tempdir().unwrap();
    let pyproject_path = dir.path().join("pyproject.toml");
    let ml_path = dir.path().join(".markdownlint.json");

    // Write a pyproject.toml with no [tool.rumdl] section
    let pyproject_content = r#"[tool.black]
line-length = 88
"#;
    let mut py_file = File::create(&pyproject_path).unwrap();
    py_file.write_all(pyproject_content.as_bytes()).unwrap();

    // Write a sample markdownlint config
    let config_content = r#"{
        "code-block-style": { "style": "fenced" },
        "ul-style": { "style": "dash" }
    }"#;
    let mut ml_file = File::create(&ml_path).unwrap();
    ml_file.write_all(config_content.as_bytes()).unwrap();

    // Run the built rumdl CLI binary in the tempdir
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["config", "--output", "toml"])
        .current_dir(&dir)
        .output()
        .expect("Failed to run rumdl CLI");

    assert!(
        output.status.success(),
        "CLI did not exit successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let toml_str = String::from_utf8_lossy(&output.stdout);
    println!("TOML output:\n{toml_str}"); // For debugging

    // Parse the TOML output
    let toml_value: toml::Value = toml::from_str(&toml_str).expect("Failed to parse TOML output");

    // Check that the mapped values are present and correct at the top level
    let md046_table = toml_value.get("MD046").expect("No [MD046] table in output");
    assert_eq!(md046_table["style"].as_str().unwrap(), "fenced");
    let md004_table = toml_value.get("MD004").expect("No [MD004] table in output");
    assert_eq!(md004_table["style"].as_str().unwrap(), "dash");
}

#[test]
fn test_config_command_prints_source_markdownlint_json() {
    // Create a temporary directory
    let dir = tempdir().unwrap();
    let config_path = dir.path().join(".markdownlint.json");

    // Write a sample markdownlint config
    let config_content = r#"{
        "code-block-style": { "style": "fenced" },
        "ul-style": { "style": "dash" }
    }"#;
    let mut file = File::create(&config_path).unwrap();
    file.write_all(config_content.as_bytes()).unwrap();

    // Run the built rumdl CLI binary in the tempdir WITHOUT --output toml
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("config")
        .current_dir(&dir)
        .output()
        .expect("Failed to run rumdl CLI");

    // Check both stdout and stderr for the message
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("from project config"),
        "Expected output to mention 'from project config', got: {combined}"
    );

    // In the expected output, update the provenance for global config values to [from default]
    // Only rule-specific values set by markdownlint config should show [from project config]
}

#[test]
fn test_invalid_markdownlint_json_prints_helpful_error() {
    // Create a temporary directory
    let dir = tempdir().unwrap();
    let config_path = dir.path().join(".markdownlint.json");

    // Write an invalid markdownlint config (unquoted keys)
    let config_content = r#"{
        code-block-style: { style: "fenced" },
        ul-style: { style: "dash" }
    }"#;
    let mut file = File::create(&config_path).unwrap();
    file.write_all(config_content.as_bytes()).unwrap();

    // Run the built rumdl CLI binary in the tempdir
    // Run 'config get' specifically to trigger the load
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args([
            "config",
            "get",
            "global.exclude",
            "--config",
            config_path.to_str().unwrap(),
        ]) // Provide valid key argument
        .current_dir(&dir)
        .output()
        .expect("Failed to run rumdl CLI");

    // Should exit with code 2 (tool error - config load/parse error)
    assert_eq!(output.status.code(), Some(2), "Expected exit code 2 for parse error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Accept any error message that contains 'Failed to parse JSON' and the filename
    assert!(
        stderr.contains("Failed to parse JSON"),
        "Expected helpful parse error message, got: {stderr}"
    );
    // Check for the filename (not full path, since #291 changed to relative paths)
    assert!(
        stderr.contains(".markdownlint.json"),
        "Error message should include the config filename, got: {stderr}"
    );
}
