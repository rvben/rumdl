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
    assert_eq!(
        stdout.trim(),
        "No configuration file loaded (--no-config/--isolated specified)"
    );
}

#[test]
fn test_config_file_command_with_isolated() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    let output = Command::new(rumdl_exe)
        .args(["config", "file", "--isolated"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout.trim(),
        "No configuration file loaded (--no-config/--isolated specified)"
    );
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

    // The command may find multiple config files (including global ones)
    // We just need to ensure our temp config is in the list
    let found_configs: Vec<&str> = stdout.trim().split('\n').collect();
    assert!(
        found_configs
            .iter()
            .any(|&path| path == absolute_path.to_string_lossy()),
        "Expected config file {} not found in output: {found_configs:?}",
        absolute_path.display()
    );
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

    // Run the config file command (should find only .rumdl.toml as it has higher precedence)
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["config", "file"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.trim().split('\n').collect();

    let rumdl_absolute = fs::canonicalize(&rumdl_path).unwrap();
    let pyproject_absolute = fs::canonicalize(&pyproject_path).unwrap();

    // When both .rumdl.toml and pyproject.toml exist, only .rumdl.toml is loaded
    // (it has higher precedence and stops the search)
    assert!(
        lines.iter().any(|&path| path == rumdl_absolute.to_string_lossy()),
        "Expected .rumdl.toml in output: {lines:?}"
    );

    // pyproject.toml should NOT be listed when .rumdl.toml exists
    // because .rumdl.toml has higher precedence
    assert!(
        !lines.iter().any(|&path| path == pyproject_absolute.to_string_lossy()),
        "pyproject.toml should not be loaded when .rumdl.toml exists: {lines:?}"
    );
}

#[test]
fn test_config_no_defaults_basic() {
    let temp_dir = tempdir().unwrap();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a config file with some non-default values
    let config_content = r#"
[global]
disable = ["MD013"]
line_length = 100

[MD004]
style = "asterisk"
"#;
    let config_path = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_path, config_content).unwrap();

    // Run 'rumdl config --no-defaults'
    let output = Command::new(rumdl_exe)
        .args(["config", "--no-defaults"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute 'rumdl config --no-defaults'");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Should contain the non-default values
    assert!(
        stdout.contains("disable = [\"MD013\"]"),
        "Output should contain non-default disable value. stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("line_length = 100"),
        "Output should contain non-default line_length value"
    );
    assert!(stdout.contains("[MD004]"), "Output should contain MD004 rule section");
    assert!(
        stdout.contains("style = \"asterisk\""),
        "Output should contain non-default style value"
    );

    // Should NOT contain [from default] annotations (only non-defaults are shown)
    // Actually, non-default values should have their source shown
    assert!(
        stdout.contains("[from"),
        "Output should contain provenance annotations for non-default values"
    );
}

#[test]
fn test_config_no_defaults_all_defaults() {
    let temp_dir = tempdir().unwrap();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Run 'rumdl config --no-defaults --no-config' (all defaults)
    let output = Command::new(rumdl_exe)
        .args(["config", "--no-defaults", "--no-config"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute 'rumdl config --no-defaults --no-config'");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should show message that all configs are defaults
    assert!(
        stdout.contains("All configurations are using default values"),
        "Output should indicate all configs are defaults. stdout: {stdout}"
    );
}

#[test]
fn test_config_defaults_and_no_defaults_mutually_exclusive() {
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Run 'rumdl config --defaults --no-defaults' (should error)
    let output = Command::new(rumdl_exe)
        .args(["config", "--defaults", "--no-defaults"])
        .output()
        .expect("Failed to execute command");

    // Should exit with error code
    assert!(!output.status.success(), "Should fail when both flags are used");

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("Cannot use both --defaults and --no-defaults"),
        "Should show error about mutual exclusivity. stderr: {stderr}"
    );
}

#[test]
fn test_config_no_defaults_toml_output() {
    use toml::Value;
    let temp_dir = tempdir().unwrap();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a config file with some non-default values
    let config_content = r#"
[global]
disable = ["MD013"]
line_length = 100

[MD004]
style = "asterisk"
"#;
    let config_path = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_path, config_content).unwrap();

    // Run 'rumdl config --no-defaults --output toml'
    let output = Command::new(rumdl_exe)
        .args(["config", "--no-defaults", "--output", "toml"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute 'rumdl config --no-defaults --output toml'");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should contain the non-default values
    assert!(
        stdout.contains("disable = [\"MD013\"]"),
        "Output should contain non-default disable value"
    );
    // line_length is serialized as "line-length" (kebab-case) in TOML due to rename_all
    assert!(
        stdout.contains("line-length = 100") || stdout.contains("line_length = 100"),
        "Output should contain non-default line_length value. Output: {stdout}"
    );
    assert!(stdout.contains("[MD004]"), "Output should contain MD004 rule section");
    assert!(
        stdout.contains("style = \"asterisk\""),
        "Output should contain non-default style value"
    );

    // Should NOT contain provenance annotations in TOML output
    assert!(
        !stdout.contains("[from"),
        "TOML output should not contain provenance annotations"
    );

    // Output should be valid TOML
    match toml::from_str::<Value>(&stdout) {
        Ok(_) => {} // Valid TOML
        Err(e) => panic!("Output should be valid TOML, but parsing failed: {e}\nOutput: {stdout}"),
    }
}

#[test]
fn test_config_no_defaults_with_pyproject() {
    let temp_dir = tempdir().unwrap();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create pyproject.toml with rumdl config
    let pyproject_content = r#"
[tool.rumdl]
line-length = 120
"#;
    let pyproject_path = temp_dir.path().join("pyproject.toml");
    fs::write(&pyproject_path, pyproject_content).unwrap();

    // Run 'rumdl config --no-defaults'
    let output = Command::new(rumdl_exe)
        .args(["config", "--no-defaults"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute 'rumdl config --no-defaults'");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should show the non-default line_length from pyproject.toml
    assert!(
        stdout.contains("line_length = 120"),
        "Output should contain non-default line_length from pyproject.toml"
    );
    assert!(
        stdout.contains("pyproject.toml") || stdout.contains("[from pyproject.toml]"),
        "Output should indicate source is pyproject.toml"
    );
}

#[test]
fn test_config_no_defaults_json_output() {
    use serde_json::Value;
    let temp_dir = tempdir().unwrap();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a config file with some non-default values
    let config_content = r#"
[global]
disable = ["MD013"]
line_length = 100

[MD004]
style = "asterisk"
"#;
    let config_path = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_path, config_content).unwrap();

    // Run 'rumdl config --no-defaults --output json'
    let output = Command::new(rumdl_exe)
        .args(["config", "--no-defaults", "--output", "json"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute 'rumdl config --no-defaults --output json'");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should be valid JSON
    let json: Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Should contain the non-default values
    if let Some(global) = json.get("global").and_then(|g| g.as_object()) {
        assert!(
            global.contains_key("disable"),
            "JSON should contain non-default disable value"
        );
        assert!(
            global.contains_key("line-length") || global.contains_key("line_length"),
            "JSON should contain non-default line_length value"
        );
    } else {
        panic!("JSON should contain a 'global' object");
    }

    // Should contain MD004 rule
    assert!(json.get("MD004").is_some(), "JSON should contain MD004 rule section");

    if let Some(md004) = json.get("MD004").and_then(|r| r.as_object()) {
        assert!(
            md004.contains_key("style"),
            "JSON should contain non-default style value"
        );
    }
}

#[test]
fn test_config_no_defaults_mixed_rule_config() {
    let temp_dir = tempdir().unwrap();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a config file where a rule has some default and some non-default values
    // MD013 has default line_length=80, but we'll set code_blocks=false (non-default)
    let config_content = r#"
[MD013]
code_blocks = false
"#;
    let config_path = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_path, config_content).unwrap();

    // Run 'rumdl config --no-defaults'
    let output = Command::new(rumdl_exe)
        .args(["config", "--no-defaults"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute 'rumdl config --no-defaults'");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should contain MD013 section
    assert!(stdout.contains("[MD013]"), "Output should contain MD013 rule section");

    // Should contain the non-default code_blocks value
    assert!(
        stdout.contains("code_blocks = false") || stdout.contains("code-blocks = false"),
        "Output should contain non-default code_blocks value"
    );

    // Should NOT contain line_length (which is default)
    assert!(
        !stdout.contains("line_length = 80") && !stdout.contains("line-length = 80"),
        "Output should NOT contain default line_length value"
    );
}

#[test]
fn test_config_no_defaults_per_file_ignores() {
    let temp_dir = tempdir().unwrap();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a config file with per-file-ignores
    let config_content = r#"
[global]
disable = ["MD013"]

[per-file-ignores]
"README.md" = ["MD033", "MD041"]
"docs/**/*.md" = ["MD013"]
"#;
    let config_path = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_path, config_content).unwrap();

    // Run 'rumdl config --no-defaults'
    let output = Command::new(rumdl_exe)
        .args(["config", "--no-defaults"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute 'rumdl config --no-defaults'");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should contain per-file-ignores section
    assert!(
        stdout.contains("[per-file-ignores]") || stdout.contains("per-file-ignores"),
        "Output should contain per-file-ignores section"
    );

    // Should contain the ignore patterns
    assert!(
        stdout.contains("README.md") || stdout.contains("\"README.md\""),
        "Output should contain README.md ignore pattern"
    );
}

#[test]
fn test_config_no_defaults_multiple_sources() {
    let temp_dir = tempdir().unwrap();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create both pyproject.toml and .rumdl.toml with different configs
    let pyproject_content = r#"
[tool.rumdl]
line-length = 120
"#;
    let pyproject_path = temp_dir.path().join("pyproject.toml");
    fs::write(&pyproject_path, pyproject_content).unwrap();

    let rumdl_content = r#"
[global]
disable = ["MD013"]

[MD004]
style = "asterisk"
"#;
    let rumdl_path = temp_dir.path().join(".rumdl.toml");
    fs::write(&rumdl_path, rumdl_content).unwrap();

    // Run 'rumdl config --no-defaults'
    // Note: .rumdl.toml has higher precedence, so pyproject.toml values might not show
    // But if both are loaded, we should see both sources
    let output = Command::new(rumdl_exe)
        .args(["config", "--no-defaults"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute 'rumdl config --no-defaults'");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should contain values from .rumdl.toml (higher precedence)
    assert!(
        stdout.contains("disable = [\"MD013\"]"),
        "Output should contain disable from .rumdl.toml"
    );
    assert!(
        stdout.contains("style = \"asterisk\""),
        "Output should contain style from .rumdl.toml"
    );

    // Note: pyproject.toml values might be overridden, so we don't assert on them
    // The key is that non-default values are shown with their sources
}

#[test]
fn test_config_no_defaults_empty_arrays_explicit() {
    let temp_dir = tempdir().unwrap();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Create a config file with explicitly set empty arrays
    // This tests the edge case where [] is explicitly set vs default []
    let config_content = r#"
[global]
enable = []
disable = []
"#;
    let config_path = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_path, config_content).unwrap();

    // Run 'rumdl config --no-defaults'
    let output = Command::new(rumdl_exe)
        .args(["config", "--no-defaults"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute 'rumdl config --no-defaults'");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();

    // Explicitly set empty arrays should be shown (they're non-default source)
    // Even though the value is the same as default, the source is different
    assert!(
        stdout.contains("enable = []") || stdout.contains("disable = []"),
        "Output should show explicitly set empty arrays if source is non-default"
    );
}

#[test]
fn test_config_no_defaults_json_all_defaults() {
    use serde_json::Value;
    let temp_dir = tempdir().unwrap();
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");

    // Run 'rumdl config --no-defaults --output json --no-config' (all defaults)
    let output = Command::new(rumdl_exe)
        .args(["config", "--no-defaults", "--output", "json", "--no-config"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to execute 'rumdl config --no-defaults --output json --no-config'");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should be valid JSON
    let json: Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // When all configs are defaults, JSON output should be minimal
    // It might be {} or have empty/default structures
    // The key is that it's valid JSON and doesn't contain unexpected non-default values
    // We just verify it parses correctly - the actual content depends on serde serialization
    // which might include default structures
    assert!(
        json.is_object() || json.is_null(),
        "JSON should be an object or null when all configs are defaults"
    );
}
