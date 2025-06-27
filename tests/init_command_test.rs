#[cfg(test)]
mod init_command_tests {
    use assert_cmd::prelude::*;

    use rumdl::config;
    use std::fs;
    use std::process::Command;
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
        let mut cmd = Command::cargo_bin("rumdl").expect("Failed to find binary");
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
}
