use std::fs;
use std::path::Path;
use rumdl::rules::*; // Import rules for testing

#[test]
fn test_load_config_file() {
    // Create a temporary config file
    let config_path = "test_config.toml";
    let config_content = r#"
[global]
disable = ["MD013"]
enable = ["MD001", "MD003"]
include = ["docs/*.md"]
exclude = [".git"]
respect_gitignore = true

[MD013]
line_length = 120
code_blocks = false
tables = true
"#;
    
    fs::write(config_path, config_content).expect("Failed to write test config file");
    
    // Test loading the config
    let config_result = rumdl::config::load_config(Some(config_path));
    assert!(config_result.is_ok(), "Config loading should succeed");
    
    let config = config_result.unwrap();
    
    // Verify global settings
    assert_eq!(config.global.disable, vec!["MD013"]);
    assert_eq!(config.global.enable, vec!["MD001", "MD003"]);
    assert_eq!(config.global.include, vec!["docs/*.md"]);
    assert_eq!(config.global.exclude, vec![".git"]);
    assert!(config.global.respect_gitignore);
    
    // Verify rule-specific settings
    let line_length = rumdl::config::get_rule_config_value::<usize>(&config, "MD013", "line_length");
    assert_eq!(line_length, Some(120));
    
    let code_blocks = rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "code_blocks");
    assert_eq!(code_blocks, Some(false));
    
    let tables = rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "tables");
    assert_eq!(tables, Some(true));
    
    // Clean up
    fs::remove_file(config_path).expect("Failed to remove test config file");
}

#[test]
fn test_load_nonexistent_config() {
    // Test loading a nonexistent config file
    let config_result = rumdl::config::load_config(Some("nonexistent_config.toml"));
    assert!(config_result.is_err(), "Loading nonexistent config should fail");
    
    if let Err(err) = config_result {
        assert!(err.to_string().contains("Failed to read config file"), 
                "Error message should indicate file reading failure");
    }
}

#[test]
fn test_default_config() {
    // Test default config when no file is specified
    let config_result = rumdl::config::load_config(None);
    
    // When no config is found, we should get a default config
    assert!(config_result.is_ok(), "Default config should be returned");
    
    let config = config_result.unwrap();
    assert!(config.global.disable.is_empty());
    assert!(config.global.enable.is_empty());
    // ... other default values
}

#[test]
fn test_create_default_config() {
    // Test creating a default config file
    let config_path = "test_default_config.toml";
    
    // Delete the file first if it exists
    if Path::new(config_path).exists() {
        fs::remove_file(config_path).expect("Failed to remove existing test file");
    }
    
    // Create the default config
    let result = rumdl::config::create_default_config(config_path);
    assert!(result.is_ok(), "Creating default config should succeed");
    
    // Verify the file exists
    assert!(Path::new(config_path).exists(), "Default config file should exist");
    
    // Load the created config
    let config_result = rumdl::config::load_config(Some(config_path));
    assert!(config_result.is_ok(), "Loading created config should succeed");
    
    // Clean up
    fs::remove_file(config_path).expect("Failed to remove test config file");
}

#[test]
fn test_rule_configuration_application() {
    // Create a temporary config file with specific rule settings
    let config_path = "test_rule_config.toml";
    let config_content = r#"
[MD013]
line_length = 150

[MD004]
style = "asterisk"
"#;
    
    fs::write(config_path, config_content).expect("Failed to write test config file");
    
    // Load the config
    let config = rumdl::config::load_config(Some(config_path)).expect("Failed to load config");
    
    // Create a test rule with the loaded config
    let mut rules: Vec<Box<dyn rumdl::rule::Rule>> = vec![
        Box::new(MD013LineLength::default()),
        Box::new(MD004UnorderedListStyle::default())
    ];
    
    // Apply configuration to rules (similar to apply_rule_configs)
    // For MD013
    if let Some(pos) = rules.iter().position(|r| r.name() == "MD013") {
        let line_length = rumdl::config::get_rule_config_value::<usize>(&config, "MD013", "line_length")
            .unwrap_or(80);
        let code_blocks = rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "code_blocks")
            .unwrap_or(true);
        let tables = rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "tables")
            .unwrap_or(false);
        let headings = rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "headings")
            .unwrap_or(true);
        let strict = rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "strict")
            .unwrap_or(false);
            
        rules[pos] = Box::new(MD013LineLength::new(line_length, code_blocks, tables, headings, strict));
    }
    
    // Test with a file that would violate MD013 at 80 chars but not at 150
    let test_content = "# Test\n\nThis is a line that exceeds 80 characters but not 150 characters. It's specifically designed for our test case.";
    
    // Run the linter with our configured rules
    let warnings = rumdl::lint(test_content, &rules).expect("Linting should succeed");
    
    // Verify no MD013 warnings because line_length is set to 150
    let md013_warnings = warnings.iter()
        .filter(|w| w.rule_name == Some("MD013"))
        .count();
    
    assert_eq!(md013_warnings, 0, "No MD013 warnings should be generated with line_length 150");
    
    // Clean up
    fs::remove_file(config_path).expect("Failed to remove test config file");
}

#[test]
fn test_multiple_rules_configuration() {
    // Test that multiple rules can be configured simultaneously
    let config_path = "test_multi_rule_config.toml";
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
    
    fs::write(config_path, config_content).expect("Failed to write test config file");
    
    // Load the config
    let config = rumdl::config::load_config(Some(config_path)).expect("Failed to load config");
    
    // Verify multiple rule configs
    let md013_line_length = rumdl::config::get_rule_config_value::<usize>(&config, "MD013", "line_length");
    assert_eq!(md013_line_length, Some(100));
    
    let md046_style = rumdl::config::get_rule_config_value::<String>(&config, "MD046", "style");
    assert_eq!(md046_style, Some("fenced".to_string()));
    
    let md048_style = rumdl::config::get_rule_config_value::<String>(&config, "MD048", "style");
    assert_eq!(md048_style, Some("backtick".to_string()));
    
    // Clean up
    fs::remove_file(config_path).expect("Failed to remove test config file");
}

#[test]
fn test_invalid_config_format() {
    // Test handling invalid TOML
    let config_path = "test_invalid_config.toml";
    let invalid_content = r#"
[global]
disable = ["MD013"
enable = "not_an_array"
"#;
    
    fs::write(config_path, invalid_content).expect("Failed to write test invalid config file");
    
    // Load the invalid config
    let config_result = rumdl::config::load_config(Some(config_path));
    assert!(config_result.is_err(), "Loading invalid config should fail");
    
    if let Err(err) = config_result {
        assert!(err.to_string().contains("Failed to parse TOML"), 
                "Error message should indicate TOML parsing failure");
    }
    
    // Clean up
    fs::remove_file(config_path).expect("Failed to remove test config file");
}

// Integration test that verifies rule behavior changes with configuration
#[test]
fn test_integration_rule_behavior() {
    // Create a test file with known contents that would trigger rules
    let test_file_path = "test_integration.md";
    let test_content = r#"# Test Heading

This is a test paragraph with a line that exceeds 80 characters but not 120 characters right here.

```
let code_block = "without language specification";
```

* Item 1
- Item 2
+ Item 3
"#;
    
    fs::write(test_file_path, test_content).expect("Failed to write test file");
    
    // Create a config file that:
    // 1. Sets MD013 line_length to 120 (would normally trigger at 80)
    // 2. Sets MD004 style to "asterisk" (should warn about dash and plus)
    let config_path = "test_integration_config.toml";
    let config_content = r#"
[MD013]
line_length = 120

[MD004]
style = "asterisk"
"#;
    
    fs::write(config_path, config_content).expect("Failed to write test config file");
    
    // Load the config
    let config = rumdl::config::load_config(Some(config_path)).expect("Failed to load config");
    
    // Set up rules with configuration applied
    let mut rules: Vec<Box<dyn rumdl::rule::Rule>> = vec![
        Box::new(MD013LineLength::default()),
        Box::new(MD004UnorderedListStyle::default())
    ];
    
    // Apply MD013 config
    if let Some(pos) = rules.iter().position(|r| r.name() == "MD013") {
        let line_length = rumdl::config::get_rule_config_value::<usize>(&config, "MD013", "line_length")
            .unwrap_or(80);
        rules[pos] = Box::new(MD013LineLength::new(line_length, true, false, true, false));
    }
    
    // Apply MD004 config
    if let Some(pos) = rules.iter().position(|r| r.name() == "MD004") {
        let style = rumdl::config::get_rule_config_value::<String>(&config, "MD004", "style")
            .unwrap_or_else(|| "consistent".to_string());
        let ul_style = match style.as_str() {
            "asterisk" => rumdl::rules::md004_unordered_list_style::UnorderedListStyle::Asterisk,
            "plus" => rumdl::rules::md004_unordered_list_style::UnorderedListStyle::Plus,
            "dash" => rumdl::rules::md004_unordered_list_style::UnorderedListStyle::Dash,
            _ => rumdl::rules::md004_unordered_list_style::UnorderedListStyle::Consistent,
        };
        rules[pos] = Box::new(MD004UnorderedListStyle::new(ul_style));
    }
    
    // Run the linter
    let test_content = fs::read_to_string(test_file_path).expect("Failed to read test file");
    let warnings = rumdl::lint(&test_content, &rules).expect("Linting should succeed");
    
    // Verify results
    // 1. There should be no MD013 warnings (line is under 120 chars)
    let md013_warnings = warnings.iter()
        .filter(|w| w.rule_name == Some("MD013"))
        .count();
    assert_eq!(md013_warnings, 0, "No MD013 warnings should be generated with line_length 120");
    
    // 2. There should be MD004 warnings for dash and plus markers
    let md004_warnings = warnings.iter()
        .filter(|w| w.rule_name == Some("MD004"))
        .count();
    assert_eq!(md004_warnings, 2, "Two MD004 warnings should be generated for dash and plus markers");
    
    // Clean up
    fs::remove_file(test_file_path).expect("Failed to remove test file");
    fs::remove_file(config_path).expect("Failed to remove test config file");
} 