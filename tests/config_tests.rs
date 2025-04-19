use rumdl::config::Config; // Ensure Config is imported
use rumdl::rules::*;
use std::fs;
use tempfile::tempdir; // For temporary directory // Add back env import

#[test]
fn test_load_config_file() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let temp_path = temp_dir.path();

    // Create a temporary config file within the temp dir using full path
    let config_path = temp_path.join("test_config.toml");
    let config_content = r#"
[global]
disable = ["MD013"]
enable = ["MD001", "MD003"]
include = ["docs/*.md"]
exclude = [".git"]
ignore_gitignore = false

[MD013]
line_length = 120
code_blocks = false
tables = true
"#;

    fs::write(&config_path, config_content).expect("Failed to write test config file");

    // Test loading the config using the full path
    // Convert PathBuf to &str for load_config if needed, or update load_config signature
    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let config_result = rumdl::config::load_config(Some(config_path_str));
    assert!(
        config_result.is_ok(),
        "Config loading should succeed. Error: {:?}",
        config_result.err()
    );

    let config = config_result.unwrap();

    // Verify global settings
    assert_eq!(config.global.disable, vec!["MD013"]);
    assert_eq!(config.global.enable, vec!["MD001", "MD003"]);
    assert_eq!(config.global.include, vec!["docs/*.md"]);
    assert_eq!(config.global.exclude, vec![".git"]);
    assert!(!config.global.ignore_gitignore);

    // Verify rule-specific settings
    let line_length =
        rumdl::config::get_rule_config_value::<usize>(&config, "MD013", "line_length");
    assert_eq!(line_length, Some(120));

    let code_blocks = rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "code_blocks");
    assert_eq!(code_blocks, Some(false));

    let tables = rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "tables");
    assert_eq!(tables, Some(true));

    // No explicit cleanup needed, tempdir is dropped at end of scope
}

#[test]
fn test_load_nonexistent_config() {
    // Test loading a nonexistent config file
    let config_result = rumdl::config::load_config(Some("nonexistent_config.toml"));
    assert!(
        config_result.is_err(),
        "Loading nonexistent config should fail"
    );

    if let Err(err) = config_result {
        assert!(
            err.to_string().contains("Failed to read config file"),
            "Error message should indicate file reading failure"
        );
    }
}

#[test]
fn test_default_config() {
    // Reverted to simple version: No file I/O, no tempdir, no env calls needed
    let config = Config::default();

    // Check default global settings
    assert!(
        config.global.include.is_empty(),
        "Default include should be empty"
    );
    assert!(
        config.global.exclude.is_empty(),
        "Default exclude should be empty"
    );
    assert!(
        config.global.enable.is_empty(),
        "Default enable should be empty"
    );
    assert!(
        config.global.disable.is_empty(),
        "Default disable should be empty"
    );
    assert!(
        !config.global.ignore_gitignore,
        "Default ignore_gitignore should be false"
    );
    assert!(
        config.global.respect_gitignore,
        "Default respect_gitignore should be true"
    );

    // Check that the default rules map is empty
    assert!(config.rules.is_empty(), "Default rules map should be empty");
}

#[test]
fn test_create_default_config() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let temp_path = temp_dir.path();

    // Define path for default config within the temp dir
    let config_path = temp_path.join("test_default_config.toml");

    // Delete the file first if it exists (shouldn't in temp dir, but good practice)
    if config_path.exists() {
        fs::remove_file(&config_path).expect("Failed to remove existing test file");
    }

    // Create the default config using the full path
    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let result = rumdl::config::create_default_config(config_path_str);
    assert!(
        result.is_ok(),
        "Creating default config should succeed: {:?}",
        result.err()
    );

    // Verify the file exists using the full path
    assert!(
        config_path.exists(),
        "Default config file should exist in temp dir"
    );

    // Load the created config using the full path
    let config_result = rumdl::config::load_config(Some(config_path_str));
    assert!(
        config_result.is_ok(),
        "Loading created config should succeed: {:?}",
        config_result.err()
    );
    // Optional: Add more assertions about the loaded default config content if needed
    // No explicit cleanup needed, tempdir handles it.
}

#[test]
fn test_rule_configuration_application() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let temp_path = temp_dir.path();

    // Create a temporary config file with specific rule settings using full path
    let config_path = temp_path.join("test_rule_config.toml");
    let config_content = r#"
[MD013]
line_length = 150

[MD004]
style = "asterisk"
"#;
    fs::write(&config_path, config_content).expect("Failed to write test config file");

    // Load the config using the full path
    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let config = rumdl::config::load_config(Some(config_path_str)).expect("Failed to load config");

    // Create a test rule with the loaded config
    let mut rules: Vec<Box<dyn rumdl::rule::Rule>> = vec![
        Box::new(MD013LineLength::default()),
        Box::new(MD004UnorderedListStyle::default()),
    ];

    // Apply configuration to rules (similar to apply_rule_configs)
    // For MD013
    if let Some(pos) = rules.iter().position(|r| r.name() == "MD013") {
        let line_length =
            rumdl::config::get_rule_config_value::<usize>(&config, "MD013", "line_length")
                .unwrap_or(80);
        let code_blocks =
            rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "code_blocks")
                .unwrap_or(true);
        let tables = rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "tables")
            .unwrap_or(false);
        let headings = rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "headings")
            .unwrap_or(true);
        let strict = rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "strict")
            .unwrap_or(false);
        rules[pos] = Box::new(MD013LineLength::new(
            line_length,
            code_blocks,
            tables,
            headings,
            strict,
        ));
    }

    // Test with a file that would violate MD013 at 80 chars but not at 150
    let test_content = "# Test\n\nThis is a line that exceeds 80 characters but not 150 characters. It's specifically designed for our test case.";

    // Run the linter with our configured rules
    let warnings = rumdl::lint(test_content, &rules, false).expect("Linting should succeed");

    // Verify no MD013 warnings because line_length is set to 150
    let md013_warnings = warnings
        .iter()
        .filter(|w| w.rule_name == Some("MD013"))
        .count();

    assert_eq!(
        md013_warnings, 0,
        "No MD013 warnings should be generated with line_length 150"
    );

    // No explicit cleanup needed.
}

#[test]
fn test_multiple_rules_configuration() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let temp_path = temp_dir.path();

    // Test that multiple rules can be configured simultaneously
    let config_path = temp_path.join("test_multi_rule_config.toml");
    let config_content = r#"
[global]
disable = []

[MD013]
line_length = 100

[MD046]
style = "fenced"

[MD048]
style = "backtick"
"#;

    fs::write(&config_path, config_content).expect("Failed to write test config file");

    // Load the config
    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let config = rumdl::config::load_config(Some(config_path_str)).expect("Failed to load config");

    // Verify multiple rule configs
    let md013_line_length =
        rumdl::config::get_rule_config_value::<usize>(&config, "MD013", "line_length");
    assert_eq!(md013_line_length, Some(100));

    let md046_style = rumdl::config::get_rule_config_value::<String>(&config, "MD046", "style");
    assert_eq!(md046_style, Some("fenced".to_string()));

    let md048_style = rumdl::config::get_rule_config_value::<String>(&config, "MD048", "style");
    assert_eq!(md048_style, Some("backtick".to_string()));

    // No explicit cleanup needed.
}

#[test]
fn test_invalid_config_format() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let temp_path = temp_dir.path();

    // Test handling invalid TOML
    let config_path = temp_path.join("test_invalid_config.toml");
    let invalid_content = r#"
[global]
disable = ["MD013"
enable = "not_an_array"
"#;

    fs::write(&config_path, invalid_content).expect("Failed to write test invalid config file");

    // Load the invalid config
    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let config_result = rumdl::config::load_config(Some(config_path_str));
    assert!(config_result.is_err(), "Loading invalid config should fail");

    if let Err(err) = config_result {
        assert!(
            err.to_string().contains("Failed to parse TOML"),
            "Error message should indicate TOML parsing failure, got: {}",
            err
        );
    }

    // No explicit cleanup needed.
}

// Integration test that verifies rule behavior changes with configuration
#[test]
fn test_integration_rule_behavior() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let temp_path = temp_dir.path();

    // Test interaction between config and rule behavior within the temp dir
    let config_path = temp_path.join("test_integration_config.toml");
    let config_content = r#"
[MD013]
line_length = 60 # Override default

[MD004]
style = "dash"
"#;
    fs::write(&config_path, config_content).expect("Failed to write integration config file");

    // Load the config
    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let config = rumdl::config::load_config(Some(config_path_str))
        .expect("Failed to load integration config");

    // Test MD013 behavior with line_length = 60
    let mut rules_md013: Vec<Box<dyn rumdl::rule::Rule>> =
        vec![Box::new(MD013LineLength::default())];
    // Apply config specifically for MD013 test
    if let Some(pos) = rules_md013.iter().position(|r| r.name() == "MD013") {
        let line_length =
            rumdl::config::get_rule_config_value::<usize>(&config, "MD013", "line_length")
                .unwrap_or(80);
        rules_md013[pos] = Box::new(MD013LineLength::new(line_length, true, false, true, false));
    }

    let short_content = "# Test\nThis line is short.";
    let long_content =
        "# Test\nThis line is definitely longer than the sixty characters limit we set.";

    let warnings_short = rumdl::lint(short_content, &rules_md013, false).unwrap();
    let warnings_long = rumdl::lint(long_content, &rules_md013, false).unwrap();

    assert!(
        warnings_short.iter().all(|w| w.rule_name != Some("MD013")),
        "MD013 should not trigger for short line with config"
    );
    assert!(
        warnings_long.iter().any(|w| w.rule_name == Some("MD013")),
        "MD013 should trigger for long line with config"
    );

    // Test MD004 behavior with style = "dash"
    // (Similar setup: create rule, apply config, test with relevant content)
    // ... add MD004 test logic here if desired ...
    // No explicit cleanup needed.
}
