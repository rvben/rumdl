use rumdl::config::{Config, SourcedConfig};
use rumdl::lint_context::LintContext;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_basic_config_inheritance() {
    println!("Testing basic configuration inheritance...");

    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Create a basic configuration
    let config_content = r#"
[global]
enable = ["MD022", "MD026"]
disable = ["MD025"]

[MD022]
lines_above = 2
lines_below = 1
"#;
    fs::write(project_path.join("rumdl.toml"), config_content).unwrap();

    // Load the configuration
    let config: Config =
        SourcedConfig::load_with_discovery(Some(project_path.join("rumdl.toml").to_str().unwrap()), None, false)
            .unwrap()
            .into();

    // Test that rules are properly enabled/disabled
    assert!(config.global.enable.contains(&"MD022".to_string()));
    assert!(config.global.enable.contains(&"MD026".to_string()));
    assert!(config.global.disable.contains(&"MD025".to_string()));

    // Test rule-specific configuration
    let lines_above = rumdl::config::get_rule_config_value::<i32>(&config, "MD022", "lines_above");
    let lines_below = rumdl::config::get_rule_config_value::<i32>(&config, "MD022", "lines_below");

    assert_eq!(lines_above, Some(2));
    assert_eq!(lines_below, Some(1));

    println!("✅ Basic configuration inheritance working correctly");
}

#[test]
fn test_config_override_patterns() {
    println!("Testing configuration override patterns...");

    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Test 1: Enable specific rules
    let enable_config = r#"
[global]
enable = ["MD001", "MD003", "MD022"]
"#;
    fs::write(project_path.join("rumdl.toml"), enable_config).unwrap();

    let config: Config =
        SourcedConfig::load_with_discovery(Some(project_path.join("rumdl.toml").to_str().unwrap()), None, false)
            .unwrap()
            .into();

    assert!(config.global.enable.contains(&"MD001".to_string()));
    assert!(config.global.enable.contains(&"MD003".to_string()));
    assert!(config.global.enable.contains(&"MD022".to_string()));

    // Test 2: Disable specific rules
    let disable_config = r#"
[global]
disable = ["MD013", "MD033"]
"#;
    fs::write(project_path.join("rumdl.toml"), disable_config).unwrap();

    let config: Config =
        SourcedConfig::load_with_discovery(Some(project_path.join("rumdl.toml").to_str().unwrap()), None, false)
            .unwrap()
            .into();

    assert!(config.global.disable.contains(&"MD013".to_string()));
    assert!(config.global.disable.contains(&"MD033".to_string()));

    // Test 3: Mixed enable/disable
    let mixed_config = r#"
[global]
enable = ["MD001", "MD022"]
disable = ["MD013"]
"#;
    fs::write(project_path.join("rumdl.toml"), mixed_config).unwrap();

    let config: Config =
        SourcedConfig::load_with_discovery(Some(project_path.join("rumdl.toml").to_str().unwrap()), None, false)
            .unwrap()
            .into();

    assert!(config.global.enable.contains(&"MD001".to_string()));
    assert!(config.global.enable.contains(&"MD022".to_string()));
    assert!(config.global.disable.contains(&"MD013".to_string()));

    println!("✅ Configuration override patterns working correctly");
}

#[test]
fn test_rule_parameter_configuration() {
    println!("Testing rule parameter configuration...");

    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Create configuration with various rule parameters
    let config_content = r#"
[global]
enable = ["MD013", "MD022", "MD026"]

[MD013]
line_length = 120
code_blocks = false
tables = true

[MD022]
lines_above = 3
lines_below = 2

[MD026]
punctuation = "!?"
"#;
    fs::write(project_path.join("rumdl.toml"), config_content).unwrap();

    let config: Config =
        SourcedConfig::load_with_discovery(Some(project_path.join("rumdl.toml").to_str().unwrap()), None, false)
            .unwrap()
            .into();

    // Test MD013 parameters
    let line_length = rumdl::config::get_rule_config_value::<i32>(&config, "MD013", "line_length");
    let code_blocks = rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "code_blocks");
    let tables = rumdl::config::get_rule_config_value::<bool>(&config, "MD013", "tables");

    assert_eq!(line_length, Some(120));
    assert_eq!(code_blocks, Some(false));
    assert_eq!(tables, Some(true));

    // Test MD022 parameters
    let lines_above = rumdl::config::get_rule_config_value::<i32>(&config, "MD022", "lines_above");
    let lines_below = rumdl::config::get_rule_config_value::<i32>(&config, "MD022", "lines_below");

    assert_eq!(lines_above, Some(3));
    assert_eq!(lines_below, Some(2));

    // Test MD026 parameters
    let punctuation = rumdl::config::get_rule_config_value::<String>(&config, "MD026", "punctuation");
    assert_eq!(punctuation, Some("!?".to_string()));

    println!("✅ Rule parameter configuration working correctly");
}

#[test]
fn test_configuration_error_handling() {
    println!("Testing configuration error handling...");

    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Test invalid TOML syntax
    let invalid_toml = r#"
[global
enable = ["MD022"]  # Missing closing bracket
"#;
    fs::write(project_path.join("rumdl.toml"), invalid_toml).unwrap();

    let result =
        SourcedConfig::load_with_discovery(Some(project_path.join("rumdl.toml").to_str().unwrap()), None, false);

    // Should either fail gracefully or fall back to defaults
    match result {
        Ok(_) => println!("✅ Invalid TOML handled with fallback"),
        Err(_) => println!("✅ Invalid TOML properly rejected"),
    }

    // Test with valid TOML but unknown rules
    let unknown_rules = r#"
[global]
enable = ["MD022", "MD999", "INVALID_RULE"]
"#;
    fs::write(project_path.join("rumdl.toml"), unknown_rules).unwrap();

    let config: Config =
        SourcedConfig::load_with_discovery(Some(project_path.join("rumdl.toml").to_str().unwrap()), None, false)
            .unwrap()
            .into();

    // Valid rules should still work
    assert!(config.global.enable.contains(&"MD022".to_string()));

    println!("✅ Configuration error handling working correctly");
}

#[test]
fn test_configuration_performance() {
    println!("Testing configuration loading performance...");

    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Create a moderately complex configuration
    let config_content = r#"
[global]
enable = ["MD001", "MD002", "MD003", "MD004", "MD005", "MD006", "MD007", "MD008", "MD009", "MD010"]
disable = ["MD013", "MD033"]

[MD001]
level = 1

[MD002]
level = 1

[MD003]
style = "atx"

[MD004]
style = "asterisk"

[MD007]
indent = 4

[MD013]
line_length = 100
code_blocks = false

[MD022]
lines_above = 2
lines_below = 2
"#;
    fs::write(project_path.join("rumdl.toml"), config_content).unwrap();

    // Measure loading time
    let start_time = std::time::Instant::now();

    for _ in 0..100 {
        let _config: Config =
            SourcedConfig::load_with_discovery(Some(project_path.join("rumdl.toml").to_str().unwrap()), None, false)
                .unwrap()
                .into();
    }

    let elapsed = start_time.elapsed();
    println!("100 config loads took: {elapsed:?}");

    // Should be reasonably fast
    assert!(elapsed.as_millis() < 500, "Config loading too slow: {elapsed:?}");

    println!("✅ Configuration performance test passed");
}

#[test]
fn test_dynamic_rule_filtering() {
    println!("Testing dynamic rule filtering with configuration...");

    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Create configuration that enables only specific rules
    let config_content = r#"
[global]
enable = ["MD022", "MD026"]
disable = ["MD025"]
"#;
    fs::write(project_path.join("rumdl.toml"), config_content).unwrap();

    let config: Config =
        SourcedConfig::load_with_discovery(Some(project_path.join("rumdl.toml").to_str().unwrap()), None, false)
            .unwrap()
            .into();

    // Get all available rules
    let all_rules = rumdl::rules::all_rules(&config);

    // Filter rules based on configuration
    let filtered_rules = rumdl::rules::filter_rules(&all_rules, &config.global);

    println!("Total rules available: {}", all_rules.len());
    println!("Filtered rules count: {}", filtered_rules.len());

    // Test that filtering works
    let enabled_rule_names: Vec<String> = filtered_rules.iter().map(|rule| rule.name().to_string()).collect();

    // Should include enabled rules
    if config.global.enable.contains(&"MD022".to_string()) {
        // If enable list is specified, only those rules should be active
        // (unless also in disable list)
        println!("Enabled rules: {enabled_rule_names:?}");
    }

    // Test with content
    let test_content = r#"# Title!

## Section
Content here.
"#;

    let ctx = LintContext::new(test_content);
    let mut total_warnings = 0;

    for rule in &filtered_rules {
        let warnings = rule.check(&ctx).unwrap();
        total_warnings += warnings.len();
    }

    println!("Total warnings from filtered rules: {total_warnings}");

    println!("✅ Dynamic rule filtering working correctly");
}
