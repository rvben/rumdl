use rumdl_lib::config::{ConfigError, create_default_config, create_preset_config};
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
    assert!(content.contains("respect-gitignore = true"));
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
    assert!(content.contains("respect-gitignore = true"));

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

#[test]
fn test_create_google_preset_config() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("rumdl.toml");
    let config_path_str = config_path.to_str().unwrap();

    let result = create_preset_config("google", config_path_str);
    assert!(result.is_ok());

    let content = fs::read_to_string(config_path_str).unwrap();

    // Verify Google-specific settings
    assert!(content.contains("[MD003]"));
    assert!(content.contains("style = \"atx\""));
    assert!(content.contains("[MD004]"));
    assert!(content.contains("style = \"dash\""));
    assert!(content.contains("[MD007]"));
    assert!(content.contains("indent = 4"));
    assert!(content.contains("[MD009]"));
    assert!(content.contains("strict = true"));
    assert!(content.contains("[MD013]"));
    assert!(content.contains("line-length = 80"));
    assert!(content.contains("[MD046]"));
    assert!(content.contains("style = \"fenced\""));
    assert!(content.contains("[MD049]"));
    assert!(content.contains("style = \"underscore\""));
    assert!(content.contains("[MD050]"));
    assert!(content.contains("style = \"asterisk\""));

    // Verify Google-specific header comment
    assert!(content.contains("Google"));

    // Verify the config is valid TOML
    let parsed: Result<toml::Value, _> = toml::from_str(&content);
    assert!(parsed.is_ok(), "Google preset config must be valid TOML");
}

#[test]
fn test_create_google_preset_deserializes_into_config() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("rumdl.toml");
    let config_path_str = config_path.to_str().unwrap();

    create_preset_config("google", config_path_str).unwrap();
    let content = fs::read_to_string(config_path_str).unwrap();

    // Verify the Google config can be deserialized into our Config struct
    let config: Result<rumdl_lib::config::Config, _> = toml::from_str(&content);
    assert!(
        config.is_ok(),
        "Google preset must deserialize into Config: {:?}",
        config.err()
    );
}

#[test]
fn test_create_default_preset_config() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("rumdl.toml");
    let config_path_str = config_path.to_str().unwrap();

    let result = create_preset_config("default", config_path_str);
    assert!(result.is_ok());

    let content = fs::read_to_string(config_path_str).unwrap();
    assert!(content.contains("[global]"));
    assert!(content.contains("# rumdl configuration file"));

    // Verify the config is valid TOML
    let parsed: Result<toml::Value, _> = toml::from_str(&content);
    assert!(parsed.is_ok());
}

#[test]
fn test_create_preset_config_unknown_preset() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("rumdl.toml");
    let config_path_str = config_path.to_str().unwrap();

    let result = create_preset_config("nonexistent", config_path_str);
    assert!(result.is_err());
    match result {
        Err(ConfigError::UnknownPreset { name }) => {
            assert_eq!(name, "nonexistent");
        }
        _ => panic!("Expected UnknownPreset error"),
    }
}

#[test]
fn test_create_preset_config_file_exists() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("rumdl.toml");
    let config_path_str = config_path.to_str().unwrap();

    fs::write(config_path_str, "existing").unwrap();

    let result = create_preset_config("google", config_path_str);
    assert!(result.is_err());
    match result {
        Err(ConfigError::FileExists { .. }) => {}
        _ => panic!("Expected FileExists error"),
    }
}

#[test]
fn test_default_preset_matches_create_default_config() {
    let temp_dir = tempdir().unwrap();

    // Create with create_default_config
    let default_path = temp_dir.path().join("default.toml");
    create_default_config(default_path.to_str().unwrap()).unwrap();
    let default_content = fs::read_to_string(&default_path).unwrap();

    // Create with create_preset_config("default", ...)
    let preset_path = temp_dir.path().join("preset.toml");
    create_preset_config("default", preset_path.to_str().unwrap()).unwrap();
    let preset_content = fs::read_to_string(&preset_path).unwrap();

    assert_eq!(
        default_content, preset_content,
        "create_default_config and create_preset_config(\"default\") must produce identical output"
    );
}

#[test]
fn test_create_relaxed_preset_config() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("rumdl.toml");
    let config_path_str = config_path.to_str().unwrap();

    let result = create_preset_config("relaxed", config_path_str);
    assert!(result.is_ok());

    let content = fs::read_to_string(config_path_str).unwrap();

    // Verify relaxed-specific settings
    assert!(content.contains("Relaxed"));
    assert!(content.contains("[global]"));

    // Key relaxed features: disable noisy rules
    assert!(content.contains("MD013"));
    assert!(content.contains("MD033"));
    assert!(content.contains("MD041"));
    assert!(content.contains("disable ="));

    // Uses consistent styles rather than enforcing specific ones
    assert!(content.contains("style = \"consistent\""));

    // Verify the config is valid TOML
    let parsed: Result<toml::Value, _> = toml::from_str(&content);
    assert!(parsed.is_ok(), "Relaxed preset config must be valid TOML");
}

#[test]
fn test_create_relaxed_preset_deserializes_into_config() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("rumdl.toml");
    let config_path_str = config_path.to_str().unwrap();

    create_preset_config("relaxed", config_path_str).unwrap();
    let content = fs::read_to_string(config_path_str).unwrap();

    let config: Result<rumdl_lib::config::Config, _> = toml::from_str(&content);
    assert!(
        config.is_ok(),
        "Relaxed preset must deserialize into Config: {:?}",
        config.err()
    );
}
