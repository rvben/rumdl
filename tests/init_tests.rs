use rumdl_lib::config::{ConfigError, create_default_config};
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

    // Verify the file exists and contains the default configuration
    assert!(Path::new(config_path_str).exists());
    let content = fs::read_to_string(config_path_str).unwrap();
    assert!(content.contains("[global]"));
    assert!(content.contains("# rumdl configuration file"));
    assert!(content.contains("exclude ="));
    assert!(content.contains("respect_gitignore = true"));
    assert!(content.contains("# [MD007]"));

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

    // Try to create config file (should error because file exists)
    let result = create_default_config(config_path_str);
    assert!(result.is_err());

    // Verify the error is FileExists
    match result {
        Err(ConfigError::FileExists { .. }) => {}
        _ => panic!("Expected FileExists error"),
    }

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
            Err(ConfigError::IoError { path, .. }) => {
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

    // Verify global section
    assert!(content.contains("[global]"));
    assert!(content.contains("# rumdl configuration file"));
    assert!(content.contains("exclude ="));
    assert!(content.contains("respect_gitignore = true"));

    // Verify some example rule configurations are present (commented out)
    assert!(content.contains("# [MD003]"));
    assert!(content.contains("# [MD004]"));
    assert!(content.contains("# [MD007]"));
    assert!(content.contains("# [MD013]"));
    assert!(content.contains("# [MD044]"));

    // Verify the config is valid TOML
    let parsed_toml: Result<toml::Value, _> = toml::from_str(&content);
    assert!(parsed_toml.is_ok());
}
