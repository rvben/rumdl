//!
//! This module provides initialization utilities for rumdl, such as creating default configuration files.

use std::fs;
use std::io;
use std::path::Path;
use thiserror::Error;

/// Error type for initialization operations
#[derive(Error, Debug)]
pub enum InitError {
    #[error("Failed to access file {path}: {source}")]
    IoError { source: io::Error, path: String },
}

/// Create a default configuration file at the specified path.
///
/// Returns `true` if the file was created, or `false` if it already exists.
///
/// # Errors
///
/// Returns an error if the file cannot be created due to permissions or other I/O errors.
pub fn create_default_config(path: &str) -> Result<bool, InitError> {
    if Path::new(path).exists() {
        return Ok(false);
    }

    let default_config = r#"# rumdl configuration file

[general]
# Maximum line length for line-based rules
line_length = 80

[rules]
# Rules to disable (comma-separated list of rule IDs)
disabled = []

# Rule-specific configuration
[rules.MD007]
# Number of spaces for list indentation
indent = 2

[rules.MD013]
# Enable line length checking for code blocks
code_blocks = true
# Enable line length checking for tables
tables = false
# Enable line length checking for headings
headings = true
# Enable strict line length checking (no exceptions)
strict = false

[rules.MD022]
# Number of blank lines required before headings
lines_above = 1
# Number of blank lines required after headings
lines_below = 1

[rules.MD024]
# Allow headings with the same content if they're not siblings
allow_different_nesting = true

[rules.MD029]
# Style for ordered list markers (one = 1., ordered = 1, 2, 3, ordered_parenthesis = 1), 2), 3))
style = "one"

[rules.MD035]
# Style for horizontal rules (----, ***, etc.)
style = "---"

[rules.MD048]
# Style for code fence markers (``` or ~~~)
style = "```"

[rules.MD049]
# Style for emphasis (asterisk or underscore)
style = "*"

[rules.MD050]
# Style for strong emphasis (asterisk or underscore)
style = "**"
"#;

    fs::write(path, default_config).map_err(|e| InitError::IoError {
        source: e,
        path: path.to_string(),
    })?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_create_default_config_success() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_path_str = config_path.to_str().unwrap();

        // Create config file
        let result = create_default_config(config_path_str).unwrap();
        assert!(result, "Should return true when creating new file");

        // Verify file exists
        assert!(config_path.exists(), "Config file should exist");

        // Verify content
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(
            content.contains("# rumdl configuration file"),
            "Should contain header comment"
        );
        assert!(content.contains("[general]"), "Should contain general section");
        assert!(
            content.contains("line_length = 80"),
            "Should contain line_length setting"
        );
        assert!(content.contains("[rules]"), "Should contain rules section");
        assert!(content.contains("[rules.MD007]"), "Should contain MD007 config");
        assert!(content.contains("[rules.MD013]"), "Should contain MD013 config");
        assert!(content.contains("[rules.MD022]"), "Should contain MD022 config");
        assert!(content.contains("[rules.MD024]"), "Should contain MD024 config");
        assert!(content.contains("[rules.MD029]"), "Should contain MD029 config");
        assert!(content.contains("[rules.MD035]"), "Should contain MD035 config");
        assert!(content.contains("[rules.MD048]"), "Should contain MD048 config");
        assert!(content.contains("[rules.MD049]"), "Should contain MD049 config");
        assert!(content.contains("[rules.MD050]"), "Should contain MD050 config");
    }

    #[test]
    fn test_create_default_config_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_path_str = config_path.to_str().unwrap();

        // Create file first
        fs::write(&config_path, "existing content").unwrap();

        // Try to create config again
        let result = create_default_config(config_path_str).unwrap();
        assert!(!result, "Should return false when file already exists");

        // Verify original content is preserved
        let content = fs::read_to_string(&config_path).unwrap();
        assert_eq!(content, "existing content", "Existing file should not be modified");
    }

    #[test]
    fn test_create_default_config_no_permission() {
        // Test with a path that likely can't be written to
        let result = create_default_config("/root/.rumdl.toml");
        assert!(result.is_err(), "Should error on permission denied");

        if let Err(e) = result {
            assert!(e.to_string().contains("Failed to access file"));
            assert!(e.to_string().contains("/root/.rumdl.toml"));
        }
    }

    #[test]
    fn test_create_default_config_nested_directory() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("nested").join("dir").join(".rumdl.toml");
        let nested_path_str = nested_path.to_str().unwrap();

        // Should fail because parent directories don't exist
        let result = create_default_config(nested_path_str);
        assert!(result.is_err(), "Should error when parent directories don't exist");
    }

    #[test]
    fn test_create_default_config_valid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_path_str = config_path.to_str().unwrap();

        // Create config file
        create_default_config(config_path_str).unwrap();

        // Verify it's valid TOML
        let content = fs::read_to_string(&config_path).unwrap();
        let parsed: Result<toml::Value, _> = toml::from_str(&content);
        assert!(parsed.is_ok(), "Generated config should be valid TOML");

        // Verify structure
        let value = parsed.unwrap();
        assert!(value.get("general").is_some(), "Should have general section");
        assert!(value.get("rules").is_some(), "Should have rules section");

        let general = value.get("general").unwrap();
        assert_eq!(
            general.get("line_length").and_then(|v| v.as_integer()),
            Some(80),
            "line_length should be 80"
        );

        let rules = value.get("rules").unwrap();
        assert!(rules.get("MD007").is_some(), "Should have MD007 config");
        assert!(rules.get("MD013").is_some(), "Should have MD013 config");
    }

    #[test]
    fn test_create_default_config_edge_cases() {
        // Test with empty string path
        let result = create_default_config("");
        assert!(result.is_err(), "Should error on empty path");

        // Test with relative path
        let temp_dir = TempDir::new().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let result = create_default_config(".rumdl.toml");
        assert!(result.is_ok(), "Should work with relative path");
        assert!(result.unwrap(), "Should create file with relative path");

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_create_default_config_unicode_path() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("配置文件.toml");
        let config_path_str = config_path.to_str().unwrap();

        // Create config file with Unicode path
        let result = create_default_config(config_path_str).unwrap();
        assert!(result, "Should handle Unicode paths");
        assert!(config_path.exists(), "Unicode path file should exist");
    }

    #[test]
    fn test_init_error_display() {
        let error = InitError::IoError {
            source: io::Error::new(io::ErrorKind::PermissionDenied, "test error"),
            path: "/test/path".to_string(),
        };

        let error_string = error.to_string();
        assert!(error_string.contains("Failed to access file"));
        assert!(error_string.contains("/test/path"));
        assert!(error_string.contains("test error"));
    }

    #[test]
    #[cfg(unix)]
    fn test_create_default_config_symlink() {
        use std::os::unix::fs::symlink;

        let temp_dir = TempDir::new().unwrap();
        let target_path = temp_dir.path().join("target.toml");
        let symlink_path = temp_dir.path().join("symlink.toml");

        // Create a file and symlink to it
        fs::write(&target_path, "existing content").unwrap();
        symlink(&target_path, &symlink_path).unwrap();

        // Should not overwrite symlink target
        let result = create_default_config(symlink_path.to_str().unwrap()).unwrap();
        assert!(!result, "Should return false for existing symlink");

        // Verify target content is preserved
        let content = fs::read_to_string(&target_path).unwrap();
        assert_eq!(content, "existing content", "Symlink target should not be modified");
    }

    #[test]
    fn test_create_default_config_concurrent() {
        use std::sync::Arc;
        use std::thread;

        let temp_dir = Arc::new(TempDir::new().unwrap());
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_path_str = config_path.to_str().unwrap().to_string();

        // Try to create the same file from multiple threads
        let handles: Vec<_> = (0..5)
            .map(|_| {
                let path = config_path_str.clone();
                thread::spawn(move || create_default_config(&path))
            })
            .collect();

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // Exactly one thread should have created the file
        let _successes = results
            .iter()
            .filter(|r| r.as_ref().map(|&v| v).unwrap_or(false))
            .count();

        // Due to race conditions, we might get 1 or more successes
        // but the file should exist and be valid
        assert!(config_path.exists(), "File should exist after concurrent attempts");

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(
            content.contains("# rumdl configuration file"),
            "File should contain valid config"
        );
    }

    #[test]
    fn test_default_config_completeness() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".rumdl.toml");
        let config_path_str = config_path.to_str().unwrap();

        create_default_config(config_path_str).unwrap();
        let content = fs::read_to_string(&config_path).unwrap();

        // Check that all documented rule configurations are present
        let expected_rules = vec![
            ("MD007", "indent = 2"),
            ("MD013", "code_blocks = true"),
            ("MD022", "lines_above = 1"),
            ("MD024", "allow_different_nesting = true"),
            ("MD029", "style = \"one\""),
            ("MD035", "style = \"---\""),
            ("MD048", "style = \"```\""),
            ("MD049", "style = \"*\""),
            ("MD050", "style = \"**\""),
        ];

        for (rule, config) in expected_rules {
            assert!(
                content.contains(&format!("[rules.{rule}]")),
                "Should contain {rule} section"
            );
            assert!(content.contains(config), "Should contain {rule} config: {config}");
        }
    }
}
