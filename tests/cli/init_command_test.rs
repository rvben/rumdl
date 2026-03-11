#[cfg(test)]
mod init_command_tests {
    use assert_cmd::cargo::cargo_bin_cmd;

    use rumdl_lib::config;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_init_command_creates_config_file() {
        // Create a temporary directory for the test
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        // Change to the temporary directory
        std::env::set_current_dir(temp_path).expect("Failed to change to temporary directory");

        // Ensure the config file doesn't exist
        let config_path = temp_path.join(".rumdl.toml");
        if config_path.exists() {
            fs::remove_file(&config_path).expect("Failed to remove existing config file");
        }

        // Run the init command
        let mut cmd = cargo_bin_cmd!("rumdl");
        let assert = cmd.arg("init").assert();

        // Check that the command succeeded
        assert
            .success()
            .stdout(predicates::str::contains("Created default configuration file"));

        // Check that the config file was created
        assert!(config_path.exists());

        // Check that the config file contains expected content
        let config_content = fs::read_to_string(config_path).expect("Failed to read config file");
        assert!(config_content.contains("[global]"));
        assert!(config_content.contains("exclude ="));
    }

    #[test]
    fn test_create_default_config_fails_if_file_exists() {
        // Create a temporary directory for the test
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        // Create a config file
        let config_path = temp_path.join(".rumdl.toml");
        fs::write(&config_path, "# Existing config").expect("Failed to create config file");

        // Verify the file exists
        assert!(
            config_path.exists(),
            "Config file was not created properly for the test"
        );

        // Try to create the config file again
        let result = config::create_default_config(config_path.to_str().unwrap());

        // Check that the function returned an error
        assert!(result.is_err());

        // Check that the error message contains the expected text
        match result {
            Err(err) => {
                let err_string = err.to_string();
                assert!(err_string.contains("Configuration file already exists"));
            }
            Ok(_) => panic!("Expected an error but got Ok"),
        }

        // Check that the config file was not modified
        let config_content = fs::read_to_string(config_path).expect("Failed to read config file");
        assert_eq!(config_content, "# Existing config");
    }

    #[test]
    fn test_init_output_is_valid_configuration() {
        // Create a temporary directory for the test
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();
        let config_path = temp_path.join(".rumdl.toml");

        // Run the init command in the temp directory
        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.current_dir(temp_path).arg("init").assert().success();

        // Read the generated config file
        let config_content = fs::read_to_string(&config_path).expect("Failed to read config file");

        // Parse the config to verify it's valid TOML
        let toml_value: toml::Value = toml::from_str(&config_content).expect("Generated config is not valid TOML");

        // Verify it can be deserialized into our Config struct
        let _config: config::Config =
            toml::from_str(&config_content).expect("Generated config cannot be deserialized into Config struct");

        // Verify some expected structure
        assert!(
            toml_value.get("global").is_some(),
            "Config should have a [global] section"
        );
    }

    #[test]
    fn test_init_output_can_be_used_by_linter() {
        // Create a temporary directory for the test
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        // Run the init command
        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.current_dir(temp_path).arg("init").assert().success();

        // Create a simple test markdown file
        let test_md = temp_path.join("test.md");
        fs::write(&test_md, "# Hello\n\nThis is a test.\n").expect("Failed to write test file");

        // Run rumdl check with the generated config
        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.current_dir(temp_path)
            .arg("check")
            .arg("test.md")
            .assert()
            .success();
    }

    #[test]
    fn test_init_with_preset_google() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        let mut cmd = cargo_bin_cmd!("rumdl");
        let assert = cmd.current_dir(temp_path).args(["init", "--preset", "google"]).assert();

        assert
            .success()
            .stdout(predicates::str::contains("Created google configuration file"));

        let config_path = temp_path.join(".rumdl.toml");
        assert!(config_path.exists());

        let content = fs::read_to_string(config_path).expect("Failed to read config");
        assert!(content.contains("Google"));
        assert!(content.contains("[MD003]"));
    }

    #[test]
    fn test_init_with_output_path() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();
        let custom_path = temp_path.join("custom.toml");

        let mut cmd = cargo_bin_cmd!("rumdl");
        let assert = cmd
            .current_dir(temp_path)
            .args(["init", "--output", custom_path.to_str().unwrap()])
            .assert();

        assert.success();
        assert!(custom_path.exists());
    }

    #[test]
    fn test_init_with_preset_and_output() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();
        let custom_path = temp_path.join("google.toml");

        let mut cmd = cargo_bin_cmd!("rumdl");
        let assert = cmd
            .current_dir(temp_path)
            .args(["init", "--preset", "google", "--output", custom_path.to_str().unwrap()])
            .assert();

        assert.success();
        assert!(custom_path.exists());

        let content = fs::read_to_string(custom_path).expect("Failed to read config");
        assert!(content.contains("[MD003]"));
        assert!(content.contains("style = \"atx\""));
    }

    #[test]
    fn test_init_preset_default_matches_init() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        // Create with --preset default
        let preset_path = temp_path.join("preset.toml");
        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.current_dir(temp_path)
            .args(["init", "--preset", "default", "--output", preset_path.to_str().unwrap()])
            .assert()
            .success();

        // Create with plain init
        let default_path = temp_path.join("default.toml");
        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.current_dir(temp_path)
            .args(["init", "--output", default_path.to_str().unwrap()])
            .assert()
            .success();

        let preset_content = fs::read_to_string(preset_path).unwrap();
        let default_content = fs::read_to_string(default_path).unwrap();
        assert_eq!(preset_content, default_content);
    }

    #[test]
    fn test_init_pyproject_with_preset_google() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.current_dir(temp_path)
            .args(["init", "--pyproject", "--preset", "google"])
            .assert()
            .success();

        let config_path = temp_path.join("pyproject.toml");
        assert!(config_path.exists());

        let content = fs::read_to_string(config_path).expect("Failed to read config");
        assert!(content.contains("[tool.rumdl]"));
        assert!(content.contains("[tool.rumdl.MD003]"));
        assert!(content.contains("style = \"atx\""));
        assert!(content.contains("[tool.rumdl.MD013]"));
        assert!(content.contains("line-length = 80"));
    }

    #[test]
    fn test_init_google_preset_can_be_used_by_linter() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        // Create Google preset config
        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.current_dir(temp_path)
            .args(["init", "--preset", "google"])
            .assert()
            .success();

        // Create a simple test markdown file that complies with Google style
        let test_md = temp_path.join("test.md");
        fs::write(&test_md, "# Hello\n\nThis is a test.\n").expect("Failed to write test file");

        // Run rumdl check with the generated config
        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.current_dir(temp_path)
            .arg("check")
            .arg("test.md")
            .assert()
            .success();
    }

    #[test]
    fn test_init_with_preset_relaxed() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        let mut cmd = cargo_bin_cmd!("rumdl");
        let assert = cmd
            .current_dir(temp_path)
            .args(["init", "--preset", "relaxed"])
            .assert();

        assert
            .success()
            .stdout(predicates::str::contains("Created relaxed configuration file"));

        let config_path = temp_path.join(".rumdl.toml");
        assert!(config_path.exists());

        let content = fs::read_to_string(config_path).expect("Failed to read config");
        assert!(content.contains("Relaxed"));
        assert!(content.contains("disable ="));
    }

    #[test]
    fn test_init_relaxed_preset_can_be_used_by_linter() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        // Create relaxed preset config
        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.current_dir(temp_path)
            .args(["init", "--preset", "relaxed"])
            .assert()
            .success();

        // Create a test markdown file - relaxed should not complain about missing first heading
        // or inline HTML since those rules are disabled
        let test_md = temp_path.join("test.md");
        fs::write(&test_md, "Some text without a heading.\n\n<div>HTML content</div>\n")
            .expect("Failed to write test file");

        // Run rumdl check with the generated config - should pass since MD041 and MD033 are disabled
        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.current_dir(temp_path)
            .arg("check")
            .arg("test.md")
            .assert()
            .success();
    }

    #[test]
    fn test_init_pyproject_with_preset_relaxed() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.current_dir(temp_path)
            .args(["init", "--pyproject", "--preset", "relaxed"])
            .assert()
            .success();

        let config_path = temp_path.join("pyproject.toml");
        assert!(config_path.exists());

        let content = fs::read_to_string(config_path).expect("Failed to read config");
        assert!(content.contains("[tool.rumdl]"));
        assert!(content.contains("MD013"));
        assert!(content.contains("MD033"));
        assert!(content.contains("style = \"consistent\""));
    }

    #[test]
    fn test_init_pyproject_conflicts_with_output() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        let mut cmd = cargo_bin_cmd!("rumdl");
        cmd.current_dir(temp_path)
            .args(["init", "--pyproject", "--output", "custom.toml"])
            .assert()
            .failure()
            .stderr(predicates::str::contains("cannot be used with"));
    }
}
