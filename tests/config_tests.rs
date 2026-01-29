use rumdl_lib::config::Config; // Ensure Config is imported
use rumdl_lib::config::RuleRegistry;
use rumdl_lib::rules::*;
use serial_test::serial;
use std::fs;
use tempfile::tempdir; // For temporary directory // Add back env import // Ensure SourcedConfig is imported

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

[MD013]
line_length = 120
code_blocks = false
tables = true
"#;

    fs::write(&config_path, config_content).expect("Failed to write test config file");

    // Test loading the config using the full path
    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let sourced_result = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path_str), None, true);
    assert!(
        sourced_result.is_ok(),
        "SourcedConfig loading should succeed. Error: {:?}",
        sourced_result.err()
    );

    let config: Config = sourced_result.unwrap().into_validated_unchecked().into();

    // Verify global settings
    assert_eq!(config.global.disable, vec!["MD013"]);
    assert_eq!(config.global.enable, vec!["MD001", "MD003"]);
    assert_eq!(config.global.include, vec!["docs/*.md"]);
    assert_eq!(config.global.exclude, vec![".git"]);
    assert!(config.global.respect_gitignore);

    // Verify rule-specific settings
    let line_length = rumdl_lib::config::get_rule_config_value::<usize>(&config, "MD013", "line_length");
    assert_eq!(line_length, Some(120));

    let code_blocks = rumdl_lib::config::get_rule_config_value::<bool>(&config, "MD013", "code_blocks");
    assert_eq!(code_blocks, Some(false));

    let tables = rumdl_lib::config::get_rule_config_value::<bool>(&config, "MD013", "tables");
    assert_eq!(tables, Some(true));

    // No explicit cleanup needed, tempdir is dropped at end of scope
}

#[test]
fn test_load_nonexistent_config() {
    // Test loading a nonexistent config file using SourcedConfig::load
    let sourced_result =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some("nonexistent_config.toml"), None, true);
    assert!(sourced_result.is_err(), "Loading nonexistent config should fail");

    if let Err(err) = sourced_result {
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
    assert!(config.global.include.is_empty(), "Default include should be empty");
    assert!(config.global.exclude.is_empty(), "Default exclude should be empty");
    assert!(config.global.enable.is_empty(), "Default enable should be empty");
    assert!(config.global.disable.is_empty(), "Default disable should be empty");
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
    let result = rumdl_lib::config::create_default_config(config_path_str);
    assert!(
        result.is_ok(),
        "Creating default config should succeed: {:?}",
        result.err()
    );

    // Verify the file exists using the full path
    assert!(config_path.exists(), "Default config file should exist in temp dir");

    // Load the created config using SourcedConfig::load
    let sourced_result = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path_str), None, true);
    assert!(
        sourced_result.is_ok(),
        "Loading created config should succeed: {:?}",
        sourced_result.err()
    );
    // Convert to Config if needed for further assertions
    // let config: Config = sourced_result.unwrap().into_validated_unchecked().into();
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

    // Load the config using SourcedConfig::load
    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let sourced_config = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path_str), None, true)
        .expect("Failed to load sourced config");
    // Convert to Config for rule application logic
    let config: Config = sourced_config.into_validated_unchecked().into();

    // Create a test rule with the loaded config
    let mut rules: Vec<Box<dyn rumdl_lib::rule::Rule>> = vec![
        Box::new(MD013LineLength::default()),
        Box::new(MD004UnorderedListStyle::new(UnorderedListStyle::Consistent)),
    ];

    // Apply configuration to rules (similar to apply_rule_configs)
    // For MD013
    if let Some(pos) = rules.iter().position(|r| r.name() == "MD013") {
        let line_length =
            rumdl_lib::config::get_rule_config_value::<usize>(&config, "MD013", "line_length").unwrap_or(80);
        let code_blocks =
            rumdl_lib::config::get_rule_config_value::<bool>(&config, "MD013", "code_blocks").unwrap_or(true);
        let tables = rumdl_lib::config::get_rule_config_value::<bool>(&config, "MD013", "tables").unwrap_or(false);
        let headings = rumdl_lib::config::get_rule_config_value::<bool>(&config, "MD013", "headings").unwrap_or(true);
        let strict = rumdl_lib::config::get_rule_config_value::<bool>(&config, "MD013", "strict").unwrap_or(false);
        rules[pos] = Box::new(MD013LineLength::new(line_length, code_blocks, tables, headings, strict));
    }

    // Test with a file that would violate MD013 at 80 chars but not at 150
    let test_content = "# Test\n\nThis is a line that exceeds 80 characters but not 150 characters. It's specifically designed for our test case.";

    // Run the linter with our configured rules
    let warnings = rumdl_lib::lint(
        test_content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .expect("Linting should succeed");

    // Verify no MD013 warnings because line_length is set to 150
    let md013_warnings = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD013"))
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

    // Load the config using SourcedConfig::load
    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let sourced_config = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path_str), None, true)
        .expect("Failed to load sourced config");
    // Convert to Config for rule verification
    let config: Config = sourced_config.into_validated_unchecked().into();

    // Verify multiple rule configs
    let md013_line_length = rumdl_lib::config::get_rule_config_value::<usize>(&config, "MD013", "line_length");
    assert_eq!(md013_line_length, Some(100));

    let md046_style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD046", "style");
    assert_eq!(md046_style, Some("fenced".to_string()));

    let md048_style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD048", "style");
    assert_eq!(md048_style, Some("backtick".to_string()));

    // No explicit cleanup needed.
}

#[test]
fn test_invalid_config_format() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let temp_path = temp_dir.path();

    // Create a temporary config file with invalid TOML syntax
    let config_path = temp_path.join("invalid_config.toml");
    let invalid_config_content = r#"
[global]
disable = ["MD013" # Missing closing bracket
"#;
    fs::write(&config_path, invalid_config_content).expect("Failed to write invalid config file");

    // Attempt to load the invalid config using SourcedConfig::load
    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let sourced_result = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path_str), None, true);
    assert!(sourced_result.is_err(), "Loading invalid config should fail");

    if let Err(err) = sourced_result {
        assert!(
            err.to_string().contains("Failed to parse TOML"),
            "Error message should indicate parsing failure: {err}"
        );
    }
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

    // Load config using SourcedConfig::load
    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let sourced_config = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path_str), None, true)
        .expect("Failed to load integration config");
    let config: Config = sourced_config.into_validated_unchecked().into(); // Convert for use

    // Test MD013 behavior with line_length = 60
    let mut rules_md013: Vec<Box<dyn rumdl_lib::rule::Rule>> = vec![Box::new(MD013LineLength::default())];
    // Apply config specifically for MD013 test
    if let Some(pos) = rules_md013.iter().position(|r| r.name() == "MD013") {
        let line_length =
            rumdl_lib::config::get_rule_config_value::<usize>(&config, "MD013", "line_length").unwrap_or(80);
        rules_md013[pos] = Box::new(MD013LineLength::new(line_length, true, false, true, false));
    }

    let short_content = "# Test\nThis line is short.";
    let long_content = "# Test\nThis line is definitely longer than the sixty characters limit we set.";

    let warnings_short = rumdl_lib::lint(
        short_content,
        &rules_md013,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();
    let warnings_long = rumdl_lib::lint(
        long_content,
        &rules_md013,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    assert!(
        warnings_short.iter().all(|w| w.rule_name.as_deref() != Some("MD013")),
        "MD013 should not trigger for short line with config"
    );
    assert!(
        warnings_long.iter().any(|w| w.rule_name.as_deref() == Some("MD013")),
        "MD013 should trigger for long line with config"
    );

    // Test MD004 behavior with style = "dash"
    // (Similar setup: create rule, apply config, test with relevant content)
    // ... add MD004 test logic here if desired ...
    // No explicit cleanup needed.
}

#[test]
fn test_config_validation_unknown_rule() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("unknown_rule.toml");
    let config_content = r#"[UNKNOWN_RULE]"#;
    fs::write(&config_path, config_content).unwrap();
    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("config should load successfully"); // Use load
    let rules = rumdl_lib::all_rules(&rumdl_lib::config::Config::default()); // Use all_rules instead of get_rules
    let registry = RuleRegistry::from_rules(&rules);
    let warnings = rumdl_lib::config::validate_config_sourced(&sourced, &registry); // Use validate_config_sourced
    // Unknown rules should generate a validation warning
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("Unknown rule"));
    assert!(warnings[0].message.contains("UNKNOWN_RULE"));
}

#[test]
fn test_config_validation_unknown_option() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("unknown_option.toml");
    let config_content = r#"[MD013]
unknown_opt = true"#;
    fs::write(&config_path, config_content).unwrap();
    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("config should load successfully"); // Use load
    let rules = rumdl_lib::all_rules(&rumdl_lib::config::Config::default()); // Use all_rules instead of get_rules
    let registry = RuleRegistry::from_rules(&rules);
    let warnings = rumdl_lib::config::validate_config_sourced(&sourced, &registry); // Use validate_config_sourced
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("Unknown option"));
}

#[test]
fn test_config_validation_type_mismatch() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("type_mismatch.toml");
    let config_content = r#"[MD013]
line_length = "not a number""#;
    fs::write(&config_path, config_content).unwrap();
    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("config should load successfully"); // Use load
    let rules = rumdl_lib::all_rules(&rumdl_lib::config::Config::default()); // Use all_rules instead of get_rules
    let registry = RuleRegistry::from_rules(&rules);
    let warnings = rumdl_lib::config::validate_config_sourced(&sourced, &registry); // Use validate_config_sourced
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("Type mismatch"));
}

#[test]
fn test_config_validation_unknown_global_option() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("unknown_global.toml");
    let config_content = r#"[global]
unknown_global = true"#;
    fs::write(&config_path, config_content).unwrap();
    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("config should load successfully");
    let rules = rumdl_lib::all_rules(&rumdl_lib::config::Config::default());
    let registry = RuleRegistry::from_rules(&rules);
    let warnings = rumdl_lib::config::validate_config_sourced(&sourced, &registry);

    // Should detect the unknown global key "unknown_global"
    let global_warnings = warnings.iter().filter(|w| w.rule.is_none()).count();
    assert_eq!(
        global_warnings, 1,
        "Expected 1 unknown global option warning for 'unknown_global'"
    );

    // Verify the warning message contains "unknown_global" or "unknown-global"
    let has_unknown_key_warning = warnings
        .iter()
        .any(|w| w.message.contains("unknown_global") || w.message.contains("unknown-global"));
    assert!(
        has_unknown_key_warning,
        "Expected warning about unknown_global, got: {warnings:?}"
    );
}

#[test]
fn test_pyproject_toml_root_level_config() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let temp_path = temp_dir.path();

    // Create a temporary config file with specific rule settings using full path
    let config_path = temp_path.join("pyproject.toml");
    // Content for the pyproject.toml file (using [tool.rumdl])
    let config_content = r#"
[tool.rumdl]
line-length = 120
disable = ["MD033"]
enable = ["MD001", "MD004"]
include = ["docs/*.md"]
exclude = ["node_modules"]
respect-gitignore = true

# Rule-specific settings to ensure they are picked up too
[tool.rumdl.MD007]
indent = 2
"#;

    // Write the content to pyproject.toml in the temp dir
    fs::write(&config_path, config_content).expect("Failed to write test pyproject.toml");

    // Load the config using the explicit path to the temp file
    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let sourced_config = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path_str), None, true)
        .expect("Failed to load sourced config from explicit path");

    let config: Config = sourced_config.into_validated_unchecked().into(); // Convert to plain config for assertions

    // Check global settings (expect normalized keys)
    assert_eq!(config.global.disable, vec!["MD033".to_string()]);
    assert_eq!(config.global.enable, vec!["MD001".to_string(), "MD004".to_string()]);
    assert_eq!(config.global.include, vec!["docs/*.md".to_string()]);
    assert_eq!(config.global.exclude, vec!["node_modules".to_string()]);
    assert!(config.global.respect_gitignore);

    // Verify rule-specific settings for MD013 (implicit via line-length)
    let line_length = rumdl_lib::config::get_rule_config_value::<usize>(&config, "MD013", "line-length");
    assert_eq!(line_length, Some(120));

    // Verify rule-specific settings for MD007 (explicit)
    let indent = rumdl_lib::config::get_rule_config_value::<usize>(&config, "MD007", "indent");
    assert_eq!(indent, Some(2));

    // No explicit cleanup needed, tempdir handles it.
}

#[cfg(test)]
mod config_file_parsing_tests {

    use rumdl_lib::config::SourcedConfig;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_json_file_detection_and_parsing() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");

        // Valid JSON config
        let config_content = r#"{
            "MD004": { "style": "dash" },
            "MD013": { "line_length": 100 }
        }"#;
        fs::write(&config_path, config_content).unwrap();

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
        assert!(result.is_ok(), "Valid JSON config should load successfully");

        let config: rumdl_lib::config::Config = result.unwrap().into_validated_unchecked().into();
        let md004_style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD004", "style");
        assert_eq!(md004_style, Some("dash".to_string()));
    }

    #[test]
    fn test_invalid_json_syntax_error() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("invalid.json");

        // Invalid JSON syntax - unquoted key
        let config_content = r#"{ MD004: { "style": "dash" } }"#;
        fs::write(&config_path, config_content).unwrap();

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
        assert!(result.is_err(), "Invalid JSON should fail to parse");

        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Failed to parse JSON"),
            "Error should mention JSON parsing: {error_msg}"
        );
        assert!(
            error_msg.contains("key must be a string"),
            "Error should be specific about the issue: {error_msg}"
        );
    }

    #[test]
    fn test_yaml_file_detection_and_parsing() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.yaml");

        // Valid YAML config
        let config_content = r#"
MD004:
  style: dash
MD013:
  line_length: 100
"#;
        fs::write(&config_path, config_content).unwrap();

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
        assert!(result.is_ok(), "Valid YAML config should load successfully");

        let config: rumdl_lib::config::Config = result.unwrap().into_validated_unchecked().into();
        let md004_style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD004", "style");
        assert_eq!(md004_style, Some("dash".to_string()));
    }

    #[test]
    fn test_invalid_yaml_syntax_error() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("invalid.yaml");

        // Invalid YAML syntax - incorrect indentation/structure
        let config_content = r#"
MD004:
  style: dash
  invalid: - syntax
"#;
        fs::write(&config_path, config_content).unwrap();

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
        assert!(result.is_err(), "Invalid YAML should fail to parse");

        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Failed to parse YAML"),
            "Error should mention YAML parsing: {error_msg}"
        );
    }

    #[test]
    fn test_toml_file_detection_and_parsing() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        // Valid TOML config
        let config_content = r#"
[MD004]
style = "dash"

[MD013]
line_length = 100
"#;
        fs::write(&config_path, config_content).unwrap();

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
        assert!(result.is_ok(), "Valid TOML config should load successfully");

        let config: rumdl_lib::config::Config = result.unwrap().into_validated_unchecked().into();
        let md004_style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD004", "style");
        assert_eq!(md004_style, Some("dash".to_string()));
    }

    #[test]
    fn test_invalid_toml_syntax_error() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("invalid.toml");

        // Invalid TOML syntax - missing value
        let config_content = r#"
[MD004]
style = "dash"
invalid_key =
"#;
        fs::write(&config_path, config_content).unwrap();

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
        assert!(result.is_err(), "Invalid TOML should fail to parse");

        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Failed to parse TOML"),
            "Error should mention TOML parsing: {error_msg}"
        );
        assert!(
            error_msg.contains("string values must be quoted") || error_msg.contains("invalid string"),
            "Error should describe the specific issue: {error_msg}"
        );
    }

    #[test]
    fn test_markdownlint_json_file_detection() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".markdownlint.json");

        // Valid markdownlint JSON config
        let config_content = r#"{
            "MD004": { "style": "asterisk" },
            "line-length": { "line_length": 120 }
        }"#;
        fs::write(&config_path, config_content).unwrap();

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
        assert!(result.is_ok(), "Valid markdownlint JSON should load successfully");

        let config: rumdl_lib::config::Config = result.unwrap().into_validated_unchecked().into();
        let md004_style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD004", "style");
        assert_eq!(md004_style, Some("asterisk".to_string()));
    }

    #[test]
    fn test_markdownlint_yaml_file_detection() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join(".markdownlint.yml");

        // Valid markdownlint YAML config
        let config_content = r#"
MD004:
  style: plus
line-length:
  line_length: 90
"#;
        fs::write(&config_path, config_content).unwrap();

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
        assert!(result.is_ok(), "Valid markdownlint YAML should load successfully");

        let config: rumdl_lib::config::Config = result.unwrap().into_validated_unchecked().into();
        let md004_style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD004", "style");
        assert_eq!(md004_style, Some("plus".to_string()));
    }

    #[test]
    fn test_file_not_found_error() {
        let result = SourcedConfig::load_with_discovery(Some("/nonexistent/config.json"), None, true);
        assert!(result.is_err(), "Nonexistent file should fail to load");

        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Failed to read config file"),
            "Error should mention file reading failure: {error_msg}"
        );
        assert!(
            error_msg.contains("No such file or directory"),
            "Error should mention specific I/O error: {error_msg}"
        );
    }

    #[test]
    fn test_different_file_extensions_use_correct_parsers() {
        let temp_dir = tempdir().unwrap();

        // Test that .json files get JSON parsing even if content is invalid
        let json_path = temp_dir.path().join("test.json");
        fs::write(&json_path, r#"{ invalid: json }"#).unwrap();
        let json_result = SourcedConfig::load_with_discovery(Some(json_path.to_str().unwrap()), None, true);
        assert!(json_result.is_err());
        assert!(json_result.unwrap_err().to_string().contains("Failed to parse JSON"));

        // Test that .yaml files get YAML parsing even if content is invalid
        let yaml_path = temp_dir.path().join("test.yaml");
        fs::write(&yaml_path, "invalid: - yaml").unwrap();
        let yaml_result = SourcedConfig::load_with_discovery(Some(yaml_path.to_str().unwrap()), None, true);
        assert!(yaml_result.is_err());
        assert!(yaml_result.unwrap_err().to_string().contains("Failed to parse YAML"));

        // Test that .toml files get TOML parsing
        let toml_path = temp_dir.path().join("test.toml");
        fs::write(&toml_path, "invalid = ").unwrap();
        let toml_result = SourcedConfig::load_with_discovery(Some(toml_path.to_str().unwrap()), None, true);
        assert!(toml_result.is_err());
        assert!(toml_result.unwrap_err().to_string().contains("Failed to parse TOML"));

        // Test that unknown extensions default to TOML parsing
        let unknown_path = temp_dir.path().join("test.config");
        fs::write(&unknown_path, "invalid = ").unwrap();
        let unknown_result = SourcedConfig::load_with_discovery(Some(unknown_path.to_str().unwrap()), None, true);
        assert!(unknown_result.is_err());
        assert!(unknown_result.unwrap_err().to_string().contains("Failed to parse TOML"));
    }

    #[test]
    fn test_jsonc_file_support() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.jsonc");

        // Valid JSONC with comments (should be parsed as JSON)
        let config_content = r#"{
            // This is a comment
            "MD004": { "style": "dash" }
        }"#;
        fs::write(&config_path, config_content).unwrap();

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
        // Note: This might fail if our JSON parser doesn't support comments
        // If it fails, that's actually expected behavior - JSONC requires special handling
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("Failed to parse JSON"),
                "JSONC parsing should attempt JSON first"
            );
        }
    }

    #[test]
    fn test_mixed_valid_and_invalid_config_values() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("mixed.json");

        // Valid JSON structure but with some invalid config values
        let config_content = r#"{
            "MD004": { "style": "valid_dash_style", "invalid_option": "should_be_ignored" },
            "MD013": { "line_length": "not_a_number" },
            "UNKNOWN_RULE": { "some_option": "value" }
        }"#;
        fs::write(&config_path, config_content).unwrap();

        let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
        // Config should load successfully but invalid values should be handled gracefully
        assert!(result.is_ok(), "Config with invalid values should still load");

        // Could add validation tests here if we implement config validation warnings
    }

    #[test]
    fn test_cli_integration_config_error_messages() {
        use std::process::Command;

        let temp_dir = tempdir().unwrap();

        // Use the standard Cargo environment variable for the binary path
        let binary_path = env!("CARGO_BIN_EXE_rumdl");

        // Test JSON syntax error via CLI (without --no-config so config is actually loaded)
        let json_path = temp_dir.path().join("invalid.json");
        fs::write(&json_path, r#"{ invalid: "json" }"#).unwrap();

        let output = Command::new(binary_path)
            .args(["check", "--config", json_path.to_str().unwrap(), "README.md"])
            .output()
            .expect("Failed to execute command");

        // Should exit with code 2 for configuration error
        assert_eq!(
            output.status.code(),
            Some(2),
            "Expected exit code 2 for invalid JSON config"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined_output = format!("{stderr}{stdout}");
        assert!(
            combined_output.contains("Failed to parse JSON") || combined_output.contains("Config error"),
            "CLI should show JSON parsing error: stderr='{stderr}' stdout='{stdout}'"
        );

        // Test YAML syntax error via CLI
        let yaml_path = temp_dir.path().join("invalid.yaml");
        fs::write(&yaml_path, "invalid: - yaml").unwrap();

        let output = Command::new(binary_path)
            .args(["check", "--config", yaml_path.to_str().unwrap(), "README.md"])
            .output()
            .expect("Failed to execute command");

        // Should exit with code 2 for configuration error
        assert_eq!(
            output.status.code(),
            Some(2),
            "Expected exit code 2 for invalid YAML config"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined_output = format!("{stderr}{stdout}");
        assert!(
            combined_output.contains("Failed to parse YAML") || combined_output.contains("Config error"),
            "CLI should show YAML parsing error: stderr='{stderr}' stdout='{stdout}'"
        );

        // Test file not found error via CLI
        let output = Command::new(binary_path)
            .args(["check", "--config", "/nonexistent/config.json", "README.md"])
            .output()
            .expect("Failed to execute command");

        // Should exit with code 2 for file not found
        assert_eq!(
            output.status.code(),
            Some(2),
            "Expected exit code 2 for nonexistent config file"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined_output = format!("{stderr}{stdout}");
        assert!(
            combined_output.contains("Failed to read config file") || combined_output.contains("Config error"),
            "CLI should show file reading error: stderr='{stderr}' stdout='{stdout}'"
        );
    }

    #[test]
    fn test_no_config_flag_bypasses_config_loading() {
        use std::process::Command;

        let temp_dir = tempdir().unwrap();

        // Use the standard Cargo environment variable for the binary path
        let binary_path = env!("CARGO_BIN_EXE_rumdl");

        // Create an invalid config file in auto-discovery location
        let invalid_config_path = temp_dir.path().join(".rumdl.toml");
        fs::write(&invalid_config_path, "invalid = [toml syntax").unwrap();

        // Create a simple test markdown file
        let md_path = temp_dir.path().join("test.md");
        fs::write(&md_path, "# Test\n\nSome content.\n").unwrap();

        // Test that --no-config bypasses config auto-discovery and succeeds
        // even with invalid config in the directory
        let output = Command::new(binary_path)
            .args(["check", "--no-config", md_path.to_str().unwrap()])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute command");

        // Should succeed because --no-config bypasses the invalid config
        assert!(
            output.status.success(),
            "Command with --no-config should succeed even with invalid config in directory. stderr='{}' stdout='{}'",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
    }

    #[test]
    fn test_config_and_no_config_flags_conflict() {
        use std::process::Command;

        let temp_dir = tempdir().unwrap();

        // Use the standard Cargo environment variable for the binary path
        let binary_path = env!("CARGO_BIN_EXE_rumdl");

        // Create a valid config file
        let config_path = temp_dir.path().join(".rumdl.toml");
        fs::write(&config_path, "[global]\n").unwrap();

        // Create a simple test markdown file
        let md_path = temp_dir.path().join("test.md");
        fs::write(&md_path, "# Test\n").unwrap();

        // Test that --config and --no-config together fail fast
        let output = Command::new(binary_path)
            .args([
                "check",
                "--config",
                config_path.to_str().unwrap(),
                "--no-config",
                md_path.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute command");

        // Should fail with a conflict error
        assert!(
            !output.status.success(),
            "Command with both --config and --no-config should fail"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("cannot be used with"),
            "Error should mention flag conflict. stderr='{stderr}'"
        );
    }

    #[test]
    fn test_auto_discovery_vs_explicit_config() {
        let temp_dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();

        // Change to temp directory for auto-discovery test
        std::env::set_current_dir(&temp_dir).unwrap();

        // Create a .markdownlint.json file for auto-discovery
        let auto_config_content = r#"{ "MD004": { "style": "asterisk" } }"#;
        fs::write(".markdownlint.json", auto_config_content).unwrap();

        // Test auto-discovery (should find .markdownlint.json)
        let auto_result = SourcedConfig::load_with_discovery(None, None, false);
        assert!(auto_result.is_ok(), "Auto-discovery should find .markdownlint.json");

        let auto_config: rumdl_lib::config::Config = auto_result.unwrap().into_validated_unchecked().into();
        let auto_style = rumdl_lib::config::get_rule_config_value::<String>(&auto_config, "MD004", "style");
        assert_eq!(auto_style, Some("asterisk".to_string()));

        // Create explicit config with different value
        let explicit_path = temp_dir.path().join("explicit.json");
        let explicit_config_content = r#"{ "MD004": { "style": "dash" } }"#;
        fs::write(&explicit_path, explicit_config_content).unwrap();

        // Test explicit config (should override auto-discovery)
        let explicit_result = SourcedConfig::load_with_discovery(Some(explicit_path.to_str().unwrap()), None, false);
        assert!(explicit_result.is_ok(), "Explicit config should load successfully");

        let explicit_config: rumdl_lib::config::Config = explicit_result.unwrap().into_validated_unchecked().into();
        let explicit_style = rumdl_lib::config::get_rule_config_value::<String>(&explicit_config, "MD004", "style");
        assert_eq!(explicit_style, Some("dash".to_string()));

        // Test skip auto-discovery (should not find .markdownlint.json)
        let skip_result = SourcedConfig::load_with_discovery(None, None, true);
        assert!(skip_result.is_ok(), "Skip auto-discovery should succeed");

        let skip_config: rumdl_lib::config::Config = skip_result.unwrap().into_validated_unchecked().into();
        let skip_style = rumdl_lib::config::get_rule_config_value::<String>(&skip_config, "MD004", "style");
        assert_eq!(skip_style, None, "Skip auto-discovery should not load any config");

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }
}

#[test]
#[serial(cwd)]
fn test_user_configuration_discovery() {
    use std::env;

    let original_dir = env::current_dir().unwrap();

    // Create temporary directories
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path().join("project");
    let config_dir = temp_dir.path().join("config");
    let rumdl_config_dir = config_dir.join("rumdl");

    fs::create_dir_all(&project_dir).unwrap();
    fs::create_dir_all(&rumdl_config_dir).unwrap();

    // Create user config file
    let user_config_path = rumdl_config_dir.join("rumdl.toml");
    let user_config_content = r#"
[global]
line-length = 88
disable = ["MD041"]

[MD007]
indent = 4
"#;
    fs::write(&user_config_path, user_config_content).unwrap();

    // Change to project directory (which has no config)
    env::set_current_dir(&project_dir).unwrap();

    // Test that user config is loaded when no project config exists
    // Pass the config_dir directly instead of setting XDG_CONFIG_HOME
    let sourced = rumdl_lib::config::SourcedConfig::load_with_discovery_impl(None, None, false, Some(&config_dir))
        .expect("Should load user config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify user config was loaded
    assert_eq!(
        config.global.line_length.get(),
        88,
        "Should load line-length from user config"
    );
    assert_eq!(
        config.global.disable,
        vec!["MD041"],
        "Should load disabled rules from user config"
    );

    // Verify rule-specific settings
    let indent = rumdl_lib::config::get_rule_config_value::<usize>(&config, "MD007", "indent");
    assert_eq!(indent, Some(4), "Should load MD007 indent from user config");

    // Now create a project config
    let project_config_path = project_dir.join(".rumdl.toml");
    let project_config_content = r#"
[global]
line-length = 100

[MD007]
indent = 2
"#;
    fs::write(&project_config_path, project_config_content).unwrap();

    // Test that project config takes precedence over user config
    let sourced_with_project =
        rumdl_lib::config::SourcedConfig::load_with_discovery_impl(None, None, false, Some(&config_dir))
            .expect("Should load project config");

    let config_with_project: Config = sourced_with_project.into_validated_unchecked().into();

    // Verify project config takes precedence
    assert_eq!(
        config_with_project.global.line_length.get(),
        100,
        "Project config should override user config"
    );
    let project_indent = rumdl_lib::config::get_rule_config_value::<usize>(&config_with_project, "MD007", "indent");
    assert_eq!(
        project_indent,
        Some(2),
        "Project MD007 config should override user config"
    );

    // Restore original environment
    env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial(cwd)]
fn test_user_configuration_file_precedence() {
    use std::env;

    let original_dir = env::current_dir().unwrap();

    // Create temporary directories
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path().join("project");
    let config_dir = temp_dir.path().join("config");
    let rumdl_config_dir = config_dir.join("rumdl");

    fs::create_dir_all(&project_dir).unwrap();
    fs::create_dir_all(&rumdl_config_dir).unwrap();

    // Create multiple user config files to test precedence
    // .rumdl.toml (highest precedence)
    let dot_rumdl_path = rumdl_config_dir.join(".rumdl.toml");
    fs::write(
        &dot_rumdl_path,
        r#"[global]
line-length = 77"#,
    )
    .unwrap();

    // rumdl.toml (middle precedence)
    let rumdl_path = rumdl_config_dir.join("rumdl.toml");
    fs::write(
        &rumdl_path,
        r#"[global]
line-length = 88"#,
    )
    .unwrap();

    // pyproject.toml (lowest precedence)
    let pyproject_path = rumdl_config_dir.join("pyproject.toml");
    fs::write(
        &pyproject_path,
        r#"[tool.rumdl.global]
line-length = 99"#,
    )
    .unwrap();

    // Change to project directory (which has no config)
    env::set_current_dir(&project_dir).unwrap();

    // Test that .rumdl.toml is loaded first - pass config_dir directly
    let sourced = rumdl_lib::config::SourcedConfig::load_with_discovery_impl(None, None, false, Some(&config_dir))
        .expect("Should load user config");

    let config: Config = sourced.into_validated_unchecked().into();
    assert_eq!(
        config.global.line_length.get(),
        77,
        ".rumdl.toml should have highest precedence"
    );

    // Remove .rumdl.toml and test again
    fs::remove_file(&dot_rumdl_path).unwrap();

    let sourced2 = rumdl_lib::config::SourcedConfig::load_with_discovery_impl(None, None, false, Some(&config_dir))
        .expect("Should load user config");

    let config2: Config = sourced2.into_validated_unchecked().into();
    assert_eq!(
        config2.global.line_length.get(),
        88,
        "rumdl.toml should be loaded when .rumdl.toml is absent"
    );

    // Remove rumdl.toml and test again
    fs::remove_file(&rumdl_path).unwrap();

    let sourced3 = rumdl_lib::config::SourcedConfig::load_with_discovery_impl(None, None, false, Some(&config_dir))
        .expect("Should load user config");

    let config3: Config = sourced3.into_validated_unchecked().into();
    assert_eq!(
        config3.global.line_length.get(),
        99,
        "pyproject.toml should be loaded when other configs are absent"
    );

    // Restore original environment
    env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_cache_dir_config() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let temp_path = temp_dir.path();

    // Test with kebab-case
    let config_path = temp_path.join("test_cache_dir.toml");
    let config_content = r#"
[global]
cache-dir = "/custom/cache/path"
"#;

    fs::write(&config_path, config_content).expect("Failed to write test config file");

    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let sourced = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path_str), None, true)
        .expect("Should load config successfully");

    let config: rumdl_lib::config::Config = sourced.into_validated_unchecked().into();
    assert!(config.global.cache_dir.is_some(), "cache_dir should be set from config");
    assert_eq!(
        config.global.cache_dir.as_ref().unwrap(),
        "/custom/cache/path",
        "cache_dir should match the configured value"
    );

    // Test with snake_case
    let config_path2 = temp_path.join("test_cache_dir_snake.toml");
    let config_content2 = r#"
[global]
cache_dir = "/another/cache/path"
"#;

    fs::write(&config_path2, config_content2).expect("Failed to write test config file");

    let config_path2_str = config_path2.to_str().expect("Path should be valid UTF-8");
    let sourced2 = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path2_str), None, true)
        .expect("Should load config successfully");

    let config2: rumdl_lib::config::Config = sourced2.into_validated_unchecked().into();
    assert!(
        config2.global.cache_dir.is_some(),
        "cache_dir should be set from config with snake_case"
    );
    assert_eq!(
        config2.global.cache_dir.as_ref().unwrap(),
        "/another/cache/path",
        "cache_dir should match the configured value with snake_case"
    );

    // Test default (no cache_dir specified)
    let config_path3 = temp_path.join("test_no_cache_dir.toml");
    let config_content3 = r#"
[global]
line-length = 100
"#;

    fs::write(&config_path3, config_content3).expect("Failed to write test config file");

    let config_path3_str = config_path3.to_str().expect("Path should be valid UTF-8");
    let sourced3 = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path3_str), None, true)
        .expect("Should load config successfully");

    let config3: rumdl_lib::config::Config = sourced3.into_validated_unchecked().into();
    assert!(
        config3.global.cache_dir.is_none(),
        "cache_dir should be None when not configured"
    );
}

#[test]
fn test_cache_enabled_config() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let temp_path = temp_dir.path();

    // Test with cache = false
    let config_path = temp_path.join("test_cache_disabled.toml");
    let config_content = r#"
[global]
cache = false
"#;

    fs::write(&config_path, config_content).expect("Failed to write test config file");

    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let sourced = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path_str), None, true)
        .expect("Should load config successfully");

    let config: rumdl_lib::config::Config = sourced.into_validated_unchecked().into();
    assert!(!config.global.cache, "cache should be false when configured as false");

    // Test with cache = true (explicit)
    let config_path2 = temp_path.join("test_cache_enabled.toml");
    let config_content2 = r#"
[global]
cache = true
"#;

    fs::write(&config_path2, config_content2).expect("Failed to write test config file");

    let config_path2_str = config_path2.to_str().expect("Path should be valid UTF-8");
    let sourced2 = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path2_str), None, true)
        .expect("Should load config successfully");

    let config2: rumdl_lib::config::Config = sourced2.into_validated_unchecked().into();
    assert!(config2.global.cache, "cache should be true when configured as true");

    // Test default (no cache specified - should default to true)
    let config_path3 = temp_path.join("test_no_cache_setting.toml");
    let config_content3 = r#"
[global]
line-length = 100
"#;

    fs::write(&config_path3, config_content3).expect("Failed to write test config file");

    let config_path3_str = config_path3.to_str().expect("Path should be valid UTF-8");
    let sourced3 = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path3_str), None, true)
        .expect("Should load config successfully");

    let config3: rumdl_lib::config::Config = sourced3.into_validated_unchecked().into();
    assert!(config3.global.cache, "cache should default to true when not configured");
}

/// Tests for project root detection and cache placement (issue #159)
mod project_root_tests {
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_project_root_with_git_at_root() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        // Create structure: $ROOT/.git + $ROOT/.rumdl.toml + $ROOT/docs/file.md
        fs::create_dir(temp_path.join(".git")).expect("Failed to create .git");
        fs::write(temp_path.join(".rumdl.toml"), "[global]").expect("Failed to write config");
        fs::create_dir(temp_path.join("docs")).expect("Failed to create docs");
        fs::write(temp_path.join("docs/test.md"), "# Test").expect("Failed to write test.md");

        // Load config from project root
        let config_path = temp_path.join(".rumdl.toml");
        let sourced =
            rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
                .expect("Should load config");

        // Project root should be temp_path (where .git is)
        assert!(sourced.project_root.is_some(), "project_root should be set");
        let project_root = sourced.project_root.unwrap();
        assert_eq!(
            project_root.canonicalize().unwrap(),
            temp_path.canonicalize().unwrap(),
            "project_root should be at .git location"
        );
    }

    #[test]
    fn test_project_root_with_config_in_subdirectory() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        // Create structure: $ROOT/.git + $ROOT/.config/.rumdl.toml + $ROOT/docs/file.md
        fs::create_dir(temp_path.join(".git")).expect("Failed to create .git");
        fs::create_dir(temp_path.join(".config")).expect("Failed to create .config");
        fs::write(temp_path.join(".config/.rumdl.toml"), "[global]").expect("Failed to write config");
        fs::create_dir(temp_path.join("docs")).expect("Failed to create docs");
        fs::write(temp_path.join("docs/test.md"), "# Test").expect("Failed to write test.md");

        // Load config from .config/
        let config_path = temp_path.join(".config/.rumdl.toml");
        let sourced =
            rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
                .expect("Should load config");

        // Project root should STILL be temp_path (where .git is), not .config/
        assert!(sourced.project_root.is_some(), "project_root should be set");
        let project_root = sourced.project_root.unwrap();
        assert_eq!(
            project_root.canonicalize().unwrap(),
            temp_path.canonicalize().unwrap(),
            "project_root should be at .git location, not config location"
        );
    }

    #[test]
    fn test_project_root_without_git() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        // Create structure: $ROOT/.config/.rumdl.toml (no .git)
        fs::create_dir(temp_path.join(".config")).expect("Failed to create .config");
        fs::write(temp_path.join(".config/.rumdl.toml"), "[global]").expect("Failed to write config");
        fs::create_dir(temp_path.join("docs")).expect("Failed to create docs");
        fs::write(temp_path.join("docs/test.md"), "# Test").expect("Failed to write test.md");

        // Load config from .config/
        let config_path = temp_path.join(".config/.rumdl.toml");
        let sourced =
            rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
                .expect("Should load config");

        // Project root should be .config/ (config location as fallback)
        assert!(sourced.project_root.is_some(), "project_root should be set");
        let project_root = sourced.project_root.unwrap();
        assert_eq!(
            project_root.canonicalize().unwrap(),
            temp_path.join(".config").canonicalize().unwrap(),
            "project_root should be at config location when no .git found"
        );
    }

    #[test]
    fn test_project_root_with_auto_discovery() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        // Create structure: $ROOT/.git + $ROOT/.rumdl.toml + $ROOT/docs/deep/nested/
        fs::create_dir(temp_path.join(".git")).expect("Failed to create .git");
        fs::write(temp_path.join(".rumdl.toml"), "[global]").expect("Failed to write config");
        fs::create_dir_all(temp_path.join("docs/deep/nested")).expect("Failed to create nested dirs");
        fs::write(temp_path.join("docs/deep/nested/test.md"), "# Test").expect("Failed to write test.md");

        // Change to nested directory and load config with auto-discovery
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(temp_path.join("docs/deep/nested")).expect("Failed to change dir");

        let sourced =
            rumdl_lib::config::SourcedConfig::load_with_discovery(None, None, false).expect("Should discover config");

        // Restore original directory
        std::env::set_current_dir(original_dir).expect("Failed to restore dir");

        // Project root should be temp_path (where .git is), even when running from nested dir
        assert!(
            sourced.project_root.is_some(),
            "project_root should be set with auto-discovery"
        );
        let project_root = sourced.project_root.unwrap();
        assert_eq!(
            project_root.canonicalize().unwrap(),
            temp_path.canonicalize().unwrap(),
            "project_root should be at .git location even from nested directory"
        );
    }

    #[test]
    fn test_cache_dir_resolves_to_project_root() {
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        // Create structure with .git
        fs::create_dir(temp_path.join(".git")).expect("Failed to create .git");
        fs::write(temp_path.join(".rumdl.toml"), "[global]").expect("Failed to write config");

        let config_path = temp_path.join(".rumdl.toml");
        let sourced =
            rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
                .expect("Should load config");

        // Simulate main.rs cache resolution logic
        let cache_dir_from_config = sourced
            .global
            .cache_dir
            .as_ref()
            .map(|sv| std::path::PathBuf::from(&sv.value));
        let project_root = sourced.project_root.clone();

        let mut cache_dir = cache_dir_from_config.unwrap_or_else(|| std::path::PathBuf::from(".rumdl_cache"));

        // Resolve relative to project root (this is the fix for #159)
        if cache_dir.is_relative()
            && let Some(root) = project_root
        {
            cache_dir = root.join(cache_dir);
        }

        // Cache should be at project root, not CWD
        assert_eq!(
            cache_dir.parent().unwrap().canonicalize().unwrap(),
            temp_path.canonicalize().unwrap(),
            "cache directory should be anchored to project root"
        );
    }

    #[test]
    fn test_config_dir_discovery() {
        // Test that .config/rumdl.toml is discovered when no root-level config exists
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        // Create structure with .git and .config/rumdl.toml (no root-level config)
        fs::create_dir(temp_path.join(".git")).expect("Failed to create .git");
        fs::create_dir(temp_path.join(".config")).expect("Failed to create .config");
        fs::write(
            temp_path.join(".config/rumdl.toml"),
            r#"
[global]
line-length = 42
"#,
        )
        .expect("Failed to write config");

        // Change to the temp directory and test auto-discovery
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(temp_path).expect("Failed to change dir");

        let sourced = rumdl_lib::config::SourcedConfig::load_with_discovery(None, None, false)
            .expect("Should discover .config/rumdl.toml");

        // Restore original directory
        std::env::set_current_dir(original_dir).expect("Failed to restore dir");

        let config: rumdl_lib::config::Config = sourced.into_validated_unchecked().into();
        assert_eq!(
            config.global.line_length.get(),
            42,
            ".config/rumdl.toml should be discovered"
        );
    }

    #[test]
    fn test_config_dir_precedence() {
        // Test that .rumdl.toml takes precedence over .config/rumdl.toml
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_path = temp_dir.path();

        // Create both root-level and .config configs
        fs::create_dir(temp_path.join(".git")).expect("Failed to create .git");
        fs::write(
            temp_path.join(".rumdl.toml"),
            r#"
[global]
line-length = 100
"#,
        )
        .expect("Failed to write root config");

        fs::create_dir(temp_path.join(".config")).expect("Failed to create .config");
        fs::write(
            temp_path.join(".config/rumdl.toml"),
            r#"
[global]
line-length = 42
"#,
        )
        .expect("Failed to write .config config");

        // Change to the temp directory and test auto-discovery
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(temp_path).expect("Failed to change dir");

        let sourced =
            rumdl_lib::config::SourcedConfig::load_with_discovery(None, None, false).expect("Should discover config");

        // Restore original directory
        std::env::set_current_dir(original_dir).expect("Failed to restore dir");

        let config: rumdl_lib::config::Config = sourced.into_validated_unchecked().into();
        assert_eq!(
            config.global.line_length.get(),
            100,
            ".rumdl.toml should take precedence over .config/rumdl.toml"
        );
    }
}

// ====================================
// Rule Name Alias Support Tests
// ====================================

#[test]
fn test_rumdl_toml_rule_section_with_aliases() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    // Test using rule name aliases in section headers
    let config_content = r#"
[ul-style]
style = "dash"

[ol-prefix]
style = "ordered"

[line-length]
line-length = 100
code-blocks = false
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config with aliases");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify that aliases were resolved to canonical names
    let ul_style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD004", "style");
    assert_eq!(
        ul_style,
        Some("dash".to_string()),
        "ul-style alias should resolve to MD004"
    );

    let ol_style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD029", "style");
    assert_eq!(
        ol_style,
        Some("ordered".to_string()),
        "ol-prefix alias should resolve to MD029"
    );

    let line_length = rumdl_lib::config::get_rule_config_value::<usize>(&config, "MD013", "line-length");
    assert_eq!(line_length, Some(100), "line-length alias should resolve to MD013");

    let code_blocks = rumdl_lib::config::get_rule_config_value::<bool>(&config, "MD013", "code-blocks");
    assert_eq!(code_blocks, Some(false), "code-blocks config should work with alias");
}

#[test]
fn test_rumdl_toml_enable_disable_with_aliases() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    // Test using aliases in enable/disable arrays
    let config_content = r#"
[global]
enable = ["ul-style", "ol-prefix", "line-length"]
disable = ["no-bare-urls", "hr-style"]
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify that aliases were resolved in enable/disable arrays
    assert!(
        config.global.enable.contains(&"MD004".to_string()),
        "ul-style should be resolved to MD004 in enable"
    );
    assert!(
        config.global.enable.contains(&"MD029".to_string()),
        "ol-prefix should be resolved to MD029 in enable"
    );
    assert!(
        config.global.enable.contains(&"MD013".to_string()),
        "line-length should be resolved to MD013 in enable"
    );

    assert!(
        config.global.disable.contains(&"MD034".to_string()),
        "no-bare-urls should be resolved to MD034 in disable"
    );
    assert!(
        config.global.disable.contains(&"MD035".to_string()),
        "hr-style should be resolved to MD035 in disable"
    );
}

#[test]
fn test_rumdl_toml_per_file_ignores_with_aliases() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    let config_content = r#"
[per-file-ignores]
"docs/*.md" = ["ul-style", "line-length"]
"README.md" = ["no-bare-urls"]
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify that aliases were resolved in per-file-ignores
    let docs_rules = config.per_file_ignores.get("docs/*.md");
    assert!(docs_rules.is_some(), "docs/*.md pattern should exist");
    assert!(
        docs_rules.unwrap().contains(&"MD004".to_string()),
        "ul-style should be resolved to MD004"
    );
    assert!(
        docs_rules.unwrap().contains(&"MD013".to_string()),
        "line-length should be resolved to MD013"
    );

    let readme_rules = config.per_file_ignores.get("README.md");
    assert!(readme_rules.is_some(), "README.md pattern should exist");
    assert!(
        readme_rules.unwrap().contains(&"MD034".to_string()),
        "no-bare-urls should be resolved to MD034"
    );
}

#[test]
fn test_pyproject_toml_rule_section_with_aliases() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("pyproject.toml");

    // Test using dot notation for nested sections
    let config_content = r#"
[tool.rumdl.ul-style]
style = "dash"

[tool.rumdl.ol-prefix]
style = "ordered"

[tool.rumdl.line-length]
line-length = 100
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load pyproject.toml with aliases");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify that aliases were resolved
    let ul_style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD004", "style");
    assert_eq!(
        ul_style,
        Some("dash".to_string()),
        "ul-style alias should resolve to MD004 in pyproject.toml"
    );

    let ol_style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD029", "style");
    assert_eq!(
        ol_style,
        Some("ordered".to_string()),
        "ol-prefix alias should resolve to MD029 in pyproject.toml"
    );

    // Note: line-length config may work differently in pyproject.toml due to parsing
    // Let's test with canonical name as well
    let line_length = rumdl_lib::config::get_rule_config_value::<usize>(&config, "MD013", "line-length");
    // This test verifies section 3 handling of [tool.rumdl.alias] format
    assert_eq!(
        line_length,
        Some(100),
        "line-length alias should resolve to MD013 in pyproject.toml (section 3)"
    );
}

#[test]
fn test_pyproject_toml_enable_disable_with_aliases() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("pyproject.toml");

    let config_content = r#"
[tool.rumdl]
enable = ["ul-style", "ol-prefix"]
disable = ["no-bare-urls", "line-length"]
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify aliases were resolved
    assert!(
        config.global.enable.contains(&"MD004".to_string()),
        "ul-style should be resolved to MD004"
    );
    assert!(
        config.global.enable.contains(&"MD029".to_string()),
        "ol-prefix should be resolved to MD029"
    );
    assert!(
        config.global.disable.contains(&"MD034".to_string()),
        "no-bare-urls should be resolved to MD034"
    );
    assert!(
        config.global.disable.contains(&"MD013".to_string()),
        "line-length should be resolved to MD013"
    );
}

#[test]
fn test_mixed_canonical_and_alias_names() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    // Test mixing canonical names and aliases
    let config_content = r#"
[global]
enable = ["MD001", "ul-style", "MD013", "ol-prefix"]

[MD004]
style = "asterisk"

[line-length]
line-length = 120
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify both canonical and alias names work
    assert!(config.global.enable.contains(&"MD001".to_string()));
    assert!(config.global.enable.contains(&"MD004".to_string()));
    assert!(config.global.enable.contains(&"MD013".to_string()));
    assert!(config.global.enable.contains(&"MD029".to_string()));

    // Verify rule configs work with both
    let ul_style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD004", "style");
    assert_eq!(ul_style, Some("asterisk".to_string()));

    let line_length = rumdl_lib::config::get_rule_config_value::<usize>(&config, "MD013", "line-length");
    assert_eq!(line_length, Some(120));
}

#[test]
fn test_fuzzy_matching_suggests_aliases() {
    // Test that fuzzy matching suggests aliases, not just canonical names
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    // Typo in alias name: "ul-sytle" instead of "ul-style"
    let config_content = r#"
[ul-sytle]
style = "dash"
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let rules = rumdl_lib::all_rules(&rumdl_lib::config::Config::default());
    let registry = RuleRegistry::from_rules(&rules);
    let warnings = rumdl_lib::config::validate_config_sourced(&sourced, &registry);

    // Debug: print warnings
    for (i, warning) in warnings.iter().enumerate() {
        println!("Warning {}: {}", i, warning.message);
    }

    // Should have 1 warning for unknown rule
    assert_eq!(warnings.len(), 1, "Should have 1 validation warning");

    // The warning should suggest the correct alias "ul-style" in lowercase
    assert!(
        warnings[0].message.contains("ul-sytle"),
        "Warning should mention the typo: {}",
        warnings[0].message
    );
    assert!(
        warnings[0].message.contains("ul-style"),
        "Warning should suggest the correct alias in lowercase: {}",
        warnings[0].message
    );
}

#[test]
fn test_md007_style_explicit_from_config_file() {
    // Test that style_explicit is correctly set when loading from config file
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    // Config with explicit style setting
    let config_content = r#"
[MD007]
indent = 4
style = "fixed"
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify indent is loaded
    let indent = rumdl_lib::config::get_rule_config_value::<u8>(&config, "MD007", "indent");
    assert_eq!(indent, Some(4), "indent should be 4");

    // Verify style is loaded
    let style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD007", "style");
    assert_eq!(style, Some("fixed".to_string()), "style should be fixed");
}

#[test]
fn test_md007_indent_only_config() {
    // Test that indent-only config (no explicit style) works correctly
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    // Config with only indent setting (no style)
    let config_content = r#"
[MD007]
indent = 4
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify indent is loaded
    let indent = rumdl_lib::config::get_rule_config_value::<u8>(&config, "MD007", "indent");
    assert_eq!(indent, Some(4), "indent should be 4");

    // Verify style is NOT in the config (should allow auto-detection)
    let style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD007", "style");
    assert!(
        style.is_none(),
        "style should not be set when only indent is configured"
    );
}

#[test]
fn test_md073_reads_indent_from_md007_config() {
    use rumdl_lib::rule::Rule;

    // Test that MD073 reads indent from MD007 config when not explicitly set
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    let config_content = r#"
[MD007]
indent = 4

[MD073]
enabled = true
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Create MD073 rule using from_config
    let rule = MD073TocValidation::from_config(&config);
    let rule = rule
        .as_any()
        .downcast_ref::<MD073TocValidation>()
        .expect("Should downcast to MD073TocValidation");

    // MD073 should have read indent=4 from MD007
    assert_eq!(rule.indent, 4, "MD073 should read indent from MD007 config");
}

#[test]
fn test_md073_explicit_indent_overrides_md007() {
    use rumdl_lib::rule::Rule;

    // Test that MD073's explicit indent overrides MD007's indent
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    let config_content = r#"
[MD007]
indent = 4

[MD073]
enabled = true
indent = 3
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Create MD073 rule using from_config
    let rule = MD073TocValidation::from_config(&config);
    let rule = rule
        .as_any()
        .downcast_ref::<MD073TocValidation>()
        .expect("Should downcast to MD073TocValidation");

    // MD073's explicit indent=3 should override MD007's indent=4
    assert_eq!(rule.indent, 3, "MD073 explicit indent should override MD007");
}

#[test]
fn test_severity_config_toml() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    let config_content = r#"
[MD001]
severity = "warning"

[MD013]
severity = "error"
line_length = 120
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify severity overrides are stored correctly
    assert_eq!(
        config.get_rule_severity("MD001"),
        Some(rumdl_lib::rule::Severity::Warning),
        "MD001 should have Warning severity"
    );
    assert_eq!(
        config.get_rule_severity("MD013"),
        Some(rumdl_lib::rule::Severity::Error),
        "MD013 should have Error severity"
    );

    // Verify other config still works
    let line_length = rumdl_lib::config::get_rule_config_value::<usize>(&config, "MD013", "line_length");
    assert_eq!(line_length, Some(120));
}

#[test]
fn test_severity_case_insensitive() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    let config_content = r#"
[MD001]
severity = "ERROR"

[MD003]
severity = "Warning"

[MD004]
severity = "error"

[MD005]
severity = "warning"
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify all case variations work
    assert_eq!(
        config.get_rule_severity("MD001"),
        Some(rumdl_lib::rule::Severity::Error)
    );
    assert_eq!(
        config.get_rule_severity("MD003"),
        Some(rumdl_lib::rule::Severity::Warning)
    );
    assert_eq!(
        config.get_rule_severity("MD004"),
        Some(rumdl_lib::rule::Severity::Error)
    );
    assert_eq!(
        config.get_rule_severity("MD005"),
        Some(rumdl_lib::rule::Severity::Warning)
    );
}

#[test]
fn test_severity_pyproject_toml() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("pyproject.toml");

    let config_content = r#"
[tool.rumdl]
MD001 = { severity = "warning" }
MD013 = { severity = "error", line_length = 100 }

[tool.rumdl.MD003]
severity = "error"
style = "atx"
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify severity overrides work in both formats
    assert_eq!(
        config.get_rule_severity("MD001"),
        Some(rumdl_lib::rule::Severity::Warning)
    );
    assert_eq!(
        config.get_rule_severity("MD013"),
        Some(rumdl_lib::rule::Severity::Error)
    );
    assert_eq!(
        config.get_rule_severity("MD003"),
        Some(rumdl_lib::rule::Severity::Error)
    );

    // Verify other config still works
    let line_length = rumdl_lib::config::get_rule_config_value::<usize>(&config, "MD013", "line_length");
    assert_eq!(line_length, Some(100));

    let style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD003", "style");
    assert_eq!(style.as_deref(), Some("atx"));
}

#[test]
fn test_severity_unknown_rule_validation() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    let config_content = r#"
[MD999]
severity = "error"

[MD001]
severity = "warning"
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let all_rules = rumdl_lib::rules::all_rules(&rumdl_lib::config::Config::default());
    let registry = RuleRegistry::from_rules(&all_rules);
    let validated = sourced.validate(&registry).expect("Validation should succeed");

    // Should have a warning about unknown rule MD999
    assert!(
        validated
            .validation_warnings
            .iter()
            .any(|w| w.message.contains("MD999") && w.message.contains("nknown")),
        "Should warn about unknown rule MD999"
    );

    let config: Config = validated.into();

    // MD001 should still work
    assert_eq!(
        config.get_rule_severity("MD001"),
        Some(rumdl_lib::rule::Severity::Warning)
    );
}

#[test]
fn test_severity_with_rule_aliases() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    // Use an alias (heading-increment is alias for MD001)
    let config_content = r#"
[heading-increment]
severity = "error"
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Severity should be stored under canonical name MD001
    assert_eq!(
        config.get_rule_severity("MD001"),
        Some(rumdl_lib::rule::Severity::Error),
        "Severity set via alias should be stored under canonical name"
    );
}

#[test]
fn test_md007_indent_explicit_do_what_i_mean() {
    // Test issue #273: "Do What I Mean" behavior
    // When indent is explicitly set but style is not, the rule should use fixed style
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;

    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    // Config with only indent setting (no style) - "Do What I Mean" case
    let config_content = r#"
[MD007]
indent = 4
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Create MD007 rule using from_config (which sets indent_explicit)
    let rule = MD007ULIndent::from_config(&config);

    // Test 1: 4-space indentation should be valid (fixed style behavior)
    let valid_content = "* Item 1\n    * Item 2\n        * Item 3";
    let ctx = LintContext::new(valid_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).expect("Rule check should succeed");
    assert!(
        result.is_empty(),
        "With indent=4 explicit, 4-space indentation should be valid. Got: {result:?}"
    );

    // Test 2: 2-space indentation should be invalid (expected 4)
    let invalid_content = "* Item 1\n  * Item 2\n    * Item 3";
    let ctx = LintContext::new(invalid_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).expect("Rule check should succeed");
    assert!(
        !result.is_empty(),
        "With indent=4 explicit, 2-space indentation should be flagged"
    );
    assert!(
        result[0].message.contains("Expected 4 spaces"),
        "Warning should say expected 4 spaces, got: {}",
        result[0].message
    );
}

#[test]
fn test_md007_explicit_text_aligned_overrides_indent() {
    // When both indent and style are explicitly set, style wins
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;

    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    // Config with both indent AND explicit text-aligned style
    let config_content = r#"
[MD007]
indent = 4
style = "text-aligned"
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");

    let config: Config = sourced.into_validated_unchecked().into();

    // Create MD007 rule using from_config
    let rule = MD007ULIndent::from_config(&config);

    // With explicit text-aligned style, 2-space indentation should be valid
    // (text-aligned ignores indent setting and aligns with parent text)
    let content = "* Item 1\n  * Item 2\n    * Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).expect("Rule check should succeed");
    assert!(
        result.is_empty(),
        "With explicit text-aligned style, 2-space indentation should be valid. Got: {result:?}"
    );
}

/// Comprehensive test that verifies ALL GlobalConfig fields are properly wired
/// through the config parsing and merging system.
///
/// This test catches the "silent failure" problem where adding a new config key
/// compiles fine but fails at runtime because one of the 7 required steps was missed.
///
/// If this test fails, check ARCHITECTURE-IMPROVEMENTS.md for the 7-step checklist.
#[test]
#[allow(deprecated)]
fn test_global_config_all_fields_roundtrip() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("complete_config.toml");

    // Set ALL GlobalConfig fields to non-default values
    let config_content = r#"
[global]
# Vec fields - non-empty
enable = ["MD001", "MD003"]
disable = ["MD013", "MD041"]
include = ["docs/**/*.md", "README.md"]
exclude = ["node_modules/**", "vendor/**"]
fixable = ["MD009", "MD010"]
unfixable = ["MD033"]

# Boolean fields - opposite of default
respect-gitignore = false
cache = false
force-exclude = true

# Option/scalar fields - set to non-default values
line-length = 120
output-format = "json"
cache-dir = "/custom/cache/path"
flavor = "mkdocs"
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let sourced = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path_str), None, true)
        .expect("Should load config successfully");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify ALL Vec<String> fields
    assert_eq!(
        config.global.enable,
        vec!["MD001", "MD003"],
        "enable field should be populated"
    );
    assert_eq!(
        config.global.disable,
        vec!["MD013", "MD041"],
        "disable field should be populated"
    );
    assert_eq!(
        config.global.include,
        vec!["docs/**/*.md", "README.md"],
        "include field should be populated"
    );
    assert_eq!(
        config.global.exclude,
        vec!["node_modules/**", "vendor/**"],
        "exclude field should be populated"
    );
    assert_eq!(
        config.global.fixable,
        vec!["MD009", "MD010"],
        "fixable field should be populated"
    );
    assert_eq!(
        config.global.unfixable,
        vec!["MD033"],
        "unfixable field should be populated"
    );

    // Verify boolean fields (checking they have non-default values)
    assert!(
        !config.global.respect_gitignore,
        "respect_gitignore should be false (non-default)"
    );
    assert!(!config.global.cache, "cache should be false (non-default)");
    assert!(
        config.global.force_exclude,
        "force_exclude should be true (non-default)"
    );

    // Verify Option/scalar fields
    assert_eq!(config.global.line_length.get(), 120, "line_length should be 120");
    assert_eq!(
        config.global.output_format.as_deref(),
        Some("json"),
        "output_format should be 'json'"
    );
    assert_eq!(
        config.global.cache_dir.as_deref(),
        Some("/custom/cache/path"),
        "cache_dir should be '/custom/cache/path'"
    );
    assert_eq!(
        config.global.flavor,
        rumdl_lib::config::MarkdownFlavor::MkDocs,
        "flavor should be MkDocs"
    );
}

/// Test that pyproject.toml also properly handles all GlobalConfig fields
#[test]
#[allow(deprecated)]
fn test_global_config_all_fields_pyproject_toml() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("pyproject.toml");

    // Set ALL GlobalConfig fields via pyproject.toml
    let config_content = r#"
[tool.rumdl]
enable = ["MD002", "MD004"]
disable = ["MD014", "MD042"]
include = ["src/**/*.md"]
exclude = [".git/**"]
fixable = ["MD011"]
unfixable = ["MD034"]
respect-gitignore = false
cache = false
force-exclude = true
line-length = 100
output-format = "pylint"
cache-dir = "/pyproject/cache"
flavor = "quarto"
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    let config_path_str = config_path.to_str().expect("Path should be valid UTF-8");
    let sourced = rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path_str), None, true)
        .expect("Should load pyproject.toml successfully");

    let config: Config = sourced.into_validated_unchecked().into();

    // Verify key fields round-trip through pyproject.toml
    assert_eq!(config.global.enable, vec!["MD002", "MD004"]);
    assert_eq!(config.global.disable, vec!["MD014", "MD042"]);
    assert_eq!(config.global.include, vec!["src/**/*.md"]);
    assert_eq!(config.global.exclude, vec![".git/**"]);
    assert_eq!(config.global.fixable, vec!["MD011"]);
    assert_eq!(config.global.unfixable, vec!["MD034"]);
    assert!(!config.global.respect_gitignore);
    assert!(!config.global.cache);
    assert!(config.global.force_exclude);
    assert_eq!(config.global.line_length.get(), 100);
    assert_eq!(config.global.output_format.as_deref(), Some("pylint"));
    assert_eq!(config.global.cache_dir.as_deref(), Some("/pyproject/cache"));
    assert_eq!(config.global.flavor, rumdl_lib::config::MarkdownFlavor::Quarto);
}

/// Test for issue #296: per-file-ignores requires brace expansion for multiple files.
/// Comma-separated patterns like "A.md,B.md" don't match individual files;
/// users must use brace expansion "{A.md,B.md}" instead.
#[test]
fn test_per_file_ignores_brace_expansion_required() {
    use std::path::PathBuf;

    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join(".rumdl.toml");

    // Test 1: Comma-separated pattern (without braces) - should NOT match individual files
    // This is the exact pattern from issue #296
    let config_content = r#"
[per-file-ignores]
"AGENTS.md,README.md" = ["MD033"]
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    // Comma-separated pattern should NOT match individual files
    let ignored_agents = config.get_ignored_rules_for_file(&PathBuf::from("AGENTS.md"));
    assert!(
        ignored_agents.is_empty(),
        "Pattern 'AGENTS.md,README.md' should NOT match 'AGENTS.md' (commas are literal in glob patterns)"
    );

    let ignored_readme = config.get_ignored_rules_for_file(&PathBuf::from("README.md"));
    assert!(
        ignored_readme.is_empty(),
        "Pattern 'AGENTS.md,README.md' should NOT match 'README.md' (commas are literal in glob patterns)"
    );

    // But it WOULD match a file literally named "AGENTS.md,README.md" (edge case)
    let ignored_literal = config.get_ignored_rules_for_file(&PathBuf::from("AGENTS.md,README.md"));
    assert!(
        ignored_literal.contains("MD033"),
        "Pattern 'AGENTS.md,README.md' should match literal filename 'AGENTS.md,README.md'"
    );

    // Test 2: Brace expansion pattern - SHOULD match individual files
    let config_content = r#"
[per-file-ignores]
"{AGENTS.md,README.md}" = ["MD033"]
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    // Brace expansion pattern SHOULD match individual files
    let ignored_agents = config.get_ignored_rules_for_file(&PathBuf::from("AGENTS.md"));
    assert!(
        ignored_agents.contains("MD033"),
        "Pattern '{{AGENTS.md,README.md}}' should match 'AGENTS.md'"
    );

    let ignored_readme = config.get_ignored_rules_for_file(&PathBuf::from("README.md"));
    assert!(
        ignored_readme.contains("MD033"),
        "Pattern '{{AGENTS.md,README.md}}' should match 'README.md'"
    );

    // Should NOT match the literal comma-separated name
    let ignored_literal = config.get_ignored_rules_for_file(&PathBuf::from("AGENTS.md,README.md"));
    assert!(
        ignored_literal.is_empty(),
        "Brace pattern should NOT match literal 'AGENTS.md,README.md'"
    );
}

/// Test brace expansion edge cases for per-file-ignores patterns.
#[test]
fn test_per_file_ignores_brace_expansion_edge_cases() {
    use std::path::PathBuf;

    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join(".rumdl.toml");

    // Edge case 1: Multiple commas without braces
    let config_content = r#"
[per-file-ignores]
"a.md,b.md,c.md" = ["MD033"]
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    // None of the individual files should match
    assert!(config.get_ignored_rules_for_file(&PathBuf::from("a.md")).is_empty());
    assert!(config.get_ignored_rules_for_file(&PathBuf::from("b.md")).is_empty());
    assert!(config.get_ignored_rules_for_file(&PathBuf::from("c.md")).is_empty());

    // Edge case 2: Brace expansion with wildcards
    let config_content = r#"
[per-file-ignores]
"{*.md,*.txt}" = ["MD013"]
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    assert!(
        config
            .get_ignored_rules_for_file(&PathBuf::from("test.md"))
            .contains("MD013")
    );
    assert!(
        config
            .get_ignored_rules_for_file(&PathBuf::from("test.txt"))
            .contains("MD013")
    );
    assert!(config.get_ignored_rules_for_file(&PathBuf::from("test.rs")).is_empty());

    // Edge case 3: Comma in directory path (no braces) - should be treated literally
    let config_content = r#"
[per-file-ignores]
"path/with,comma/file.md" = ["MD033"]
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    // Should only match the literal path with comma
    assert!(
        config
            .get_ignored_rules_for_file(&PathBuf::from("path/with,comma/file.md"))
            .contains("MD033")
    );
    assert!(
        config
            .get_ignored_rules_for_file(&PathBuf::from("path/with/file.md"))
            .is_empty()
    );

    // Edge case 4: Brace at end of pattern (partial filename match)
    let config_content = r#"
[per-file-ignores]
"README.{md,txt}" = ["MD041"]
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    assert!(
        config
            .get_ignored_rules_for_file(&PathBuf::from("README.md"))
            .contains("MD041")
    );
    assert!(
        config
            .get_ignored_rules_for_file(&PathBuf::from("README.txt"))
            .contains("MD041")
    );
    assert!(
        config
            .get_ignored_rules_for_file(&PathBuf::from("README.rst"))
            .is_empty()
    );
}

/// Test that patterns with both comma and braces work correctly (no false warning).
#[test]
fn test_per_file_ignores_brace_expansion_no_false_warning() {
    use std::path::PathBuf;

    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join(".rumdl.toml");

    // Pattern with braces should work and not trigger warning
    let config_content = r#"
[per-file-ignores]
"{docs,guides}/**/*.md" = ["MD013"]
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    // Pattern should correctly match files in either directory
    let ignored_docs = config.get_ignored_rules_for_file(&PathBuf::from("docs/file.md"));
    assert!(ignored_docs.contains("MD013"), "Should match docs/file.md");

    let ignored_guides = config.get_ignored_rules_for_file(&PathBuf::from("guides/file.md"));
    assert!(ignored_guides.contains("MD013"), "Should match guides/file.md");

    // Should NOT match other directories
    let ignored_other = config.get_ignored_rules_for_file(&PathBuf::from("other/file.md"));
    assert!(ignored_other.is_empty(), "Should NOT match other/file.md");
}

/// Test per-file-ignores brace expansion works correctly in pyproject.toml.
#[test]
fn test_per_file_ignores_brace_expansion_pyproject() {
    use std::path::PathBuf;

    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("pyproject.toml");

    // Test with pyproject.toml format
    let config_content = r#"
[tool.rumdl.per-file-ignores]
"{AGENTS.md,README.md}" = ["MD033"]
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    // Brace expansion should work in pyproject.toml
    let ignored_agents = config.get_ignored_rules_for_file(&PathBuf::from("AGENTS.md"));
    assert!(
        ignored_agents.contains("MD033"),
        "Brace expansion should work in pyproject.toml for AGENTS.md"
    );

    let ignored_readme = config.get_ignored_rules_for_file(&PathBuf::from("README.md"));
    assert!(
        ignored_readme.contains("MD033"),
        "Brace expansion should work in pyproject.toml for README.md"
    );

    // Test comma-separated (without braces) in pyproject.toml - should NOT match
    let config_content = r#"
[tool.rumdl.per-file-ignores]
"AGENTS.md,README.md" = ["MD033"]
"#;
    fs::write(&config_path, config_content).expect("Failed to write config");

    let sourced =
        rumdl_lib::config::SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
            .expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    // Comma-separated should NOT match individual files
    assert!(
        config
            .get_ignored_rules_for_file(&PathBuf::from("AGENTS.md"))
            .is_empty(),
        "Comma pattern in pyproject.toml should NOT match AGENTS.md"
    );
    assert!(
        config
            .get_ignored_rules_for_file(&PathBuf::from("README.md"))
            .is_empty(),
        "Comma pattern in pyproject.toml should NOT match README.md"
    );
}
