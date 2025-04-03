use rumdl::init::{create_default_config, InitError};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_create_default_config_new_file() {
    // Create a temporary directory that will be automatically cleaned up
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("rumdl.toml");
    let config_path_str = config_path.to_str().unwrap();

    // Create a new config file
    let result = create_default_config(config_path_str);
    assert!(result.is_ok());
    assert!(result.unwrap()); // Should return true for new file

    // Verify the file exists and contains the default configuration
    assert!(Path::new(config_path_str).exists());
    let content = fs::read_to_string(config_path_str).unwrap();
    assert!(content.contains("[general]"));
    assert!(content.contains("line_length = 80"));
    assert!(content.contains("[rules]"));
    assert!(content.contains("[rules.MD007]"));
    assert!(content.contains("indent = 2"));

    // Cleanup is handled automatically by tempdir
}

#[test]
fn test_create_default_config_existing_file() {
    // Create a temporary directory
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("rumdl.toml");
    let config_path_str = config_path.to_str().unwrap();

    // Create a dummy file first
    fs::write(config_path_str, "dummy content").unwrap();

    // Try to create config file (should not overwrite existing file)
    let result = create_default_config(config_path_str);
    assert!(result.is_ok());
    assert!(!result.unwrap()); // Should return false for existing file

    // Verify the file still contains the original content
    let content = fs::read_to_string(config_path_str).unwrap();
    assert_eq!(content, "dummy content");
}

#[test]
fn test_create_default_config_permission_error() {
    if cfg!(unix) {
        // Skip this test on Windows as permission model is different
        // Create a temporary directory with no write permissions
        let temp_dir = tempdir().unwrap();
        let unwritable_dir = temp_dir.path().join("unwritable");
        fs::create_dir(&unwritable_dir).unwrap();

        // On Unix, set directory permissions to read-only (no write access)
        use std::os::unix::fs::PermissionsExt;
        let read_only = fs::Permissions::from_mode(0o555);
        fs::set_permissions(&unwritable_dir, read_only).unwrap();

        // Try to create config file in read-only directory
        let config_path = unwritable_dir.join("rumdl.toml");
        let config_path_str = config_path.to_str().unwrap();

        let result = create_default_config(config_path_str);
        assert!(result.is_err());
        match result {
            Err(InitError::IoError { path, .. }) => {
                assert_eq!(path, config_path_str);
            }
            _ => panic!("Expected IoError variant"),
        }
    }
}

#[test]
fn test_create_default_config_content_validation() {
    // Create a temporary directory
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("rumdl.toml");
    let config_path_str = config_path.to_str().unwrap();

    // Create the config file
    let result = create_default_config(config_path_str);
    assert!(result.is_ok());

    // Read the content and verify all expected sections are present
    let content = fs::read_to_string(config_path_str).unwrap();

    // Verify general section
    assert!(content.contains("[general]"));
    assert!(content.contains("line_length = 80"));

    // Verify rules section
    assert!(content.contains("[rules]"));
    assert!(content.contains("disabled = []"));

    // Verify specific rule configurations
    assert!(content.contains("[rules.MD007]"));
    assert!(content.contains("indent = 2"));

    assert!(content.contains("[rules.MD013]"));
    assert!(content.contains("code_blocks = true"));
    assert!(content.contains("tables = false"));
    assert!(content.contains("headings = true"));
    assert!(content.contains("strict = false"));

    assert!(content.contains("[rules.MD022]"));
    assert!(content.contains("lines_above = 1"));
    assert!(content.contains("lines_below = 1"));

    assert!(content.contains("[rules.MD024]"));
    assert!(content.contains("allow_different_nesting = true"));

    assert!(content.contains("[rules.MD029]"));
    assert!(content.contains("style = \"one\""));

    assert!(content.contains("[rules.MD035]"));
    assert!(content.contains("style = \"---\""));

    assert!(content.contains("[rules.MD048]"));
    assert!(content.contains("style = \"```\""));

    assert!(content.contains("[rules.MD049]"));
    assert!(content.contains("style = \"*\""));

    assert!(content.contains("[rules.MD050]"));
    assert!(content.contains("style = \"**\""));

    // Verify the config is valid TOML
    let parsed_toml: Result<toml::Value, _> = toml::from_str(&content);
    assert!(parsed_toml.is_ok());
}
