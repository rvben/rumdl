use rumdl::config::{Config, GlobalConfig, RuleConfig};
use rumdl::rule::Rule;
use rumdl::rules::*;
use std::collections::HashMap;
use std::fs;

fn create_test_config() -> Config {
    let mut rules = HashMap::new();

    // Add MD013 config
    let mut md013_values = HashMap::new();
    md013_values.insert("line_length".to_string(), toml::Value::Integer(120));
    md013_values.insert("code_blocks".to_string(), toml::Value::Boolean(false));
    md013_values.insert("headings".to_string(), toml::Value::Boolean(true));
    let md013_config = RuleConfig {
        values: md013_values,
    };
    rules.insert("MD013".to_string(), md013_config);

    // Add MD004 config
    let mut md004_values = HashMap::new();
    md004_values.insert(
        "style".to_string(),
        toml::Value::String("asterisk".to_string()),
    );
    let md004_config = RuleConfig {
        values: md004_values,
    };
    rules.insert("MD004".to_string(), md004_config);

    Config {
        global: GlobalConfig {
            disable: vec![],
            enable: vec![],
            include: vec![],
            exclude: vec![],
            ignore_gitignore: false,
            respect_gitignore: true,
        },
        rules,
    }
}

// Helper function to apply configuration to specific rules
fn apply_rule_configs(rules: &mut Vec<Box<dyn Rule>>, config: &Config) {
    for rule in rules.iter_mut() {
        let rule_name = rule.name();

        // Apply MD013 configuration
        if rule_name == "MD013" {
            let line_length =
                rumdl::config::get_rule_config_value::<u64>(config, "MD013", "line_length")
                    .map(|v| v as usize)
                    .unwrap_or(80);
            let code_blocks =
                rumdl::config::get_rule_config_value::<bool>(config, "MD013", "code_blocks")
                    .unwrap_or(true);
            let tables = rumdl::config::get_rule_config_value::<bool>(config, "MD013", "tables")
                .unwrap_or(false);
            let headings =
                rumdl::config::get_rule_config_value::<bool>(config, "MD013", "headings")
                    .unwrap_or(true);
            let strict = rumdl::config::get_rule_config_value::<bool>(config, "MD013", "strict")
                .unwrap_or(false);

            *rule = Box::new(MD013LineLength::new(
                line_length,
                code_blocks,
                tables,
                headings,
                strict,
            ));
        }

        // Apply MD004 configuration
        if rule_name == "MD004" {
            let style = rumdl::config::get_rule_config_value::<String>(config, "MD004", "style")
                .unwrap_or_else(|| "consistent".to_string());
            let ul_style = match style.as_str() {
                "asterisk" => {
                    rumdl::rules::md004_unordered_list_style::UnorderedListStyle::Asterisk
                }
                "plus" => rumdl::rules::md004_unordered_list_style::UnorderedListStyle::Plus,
                "dash" => rumdl::rules::md004_unordered_list_style::UnorderedListStyle::Dash,
                _ => rumdl::rules::md004_unordered_list_style::UnorderedListStyle::Consistent,
            };
            *rule = Box::new(MD004UnorderedListStyle::new(ul_style));
        }
    }
}

#[test]
fn test_apply_rule_configs() {
    // Create test rules
    let mut rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD013LineLength::default()),
        Box::new(MD004UnorderedListStyle::default()),
        Box::new(MD001HeadingIncrement::default()), // Not configured
    ];

    // Create a test config
    let config = create_test_config();

    // Apply configs to rules
    apply_rule_configs(&mut rules, &config);

    // Test content that would trigger different behaviors based on config
    let test_content = r#"# Heading

This is a line that exceeds the default 80 characters but is less than the configured 120 characters.

* Item 1
- Item 2
+ Item 3
"#;

    // Run the linter with modified rules
    let warnings = rumdl::lint(test_content, &rules, false).expect("Linting should succeed");

    // Check MD013 behavior - should not trigger on >80 but <120 chars
    let md013_warnings = warnings
        .iter()
        .filter(|w| w.rule_name == Some("MD013"))
        .count();
    assert_eq!(
        md013_warnings, 0,
        "MD013 should not trigger with line_length 120"
    );

    // Check MD004 behavior - should warn on dash and plus (not asterisk)
    let md004_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name == Some("MD004"))
        .collect();
    assert_eq!(
        md004_warnings.len(),
        2,
        "MD004 should trigger twice (dash and plus markers)"
    );

    // Make sure the non-configured rule (MD001) still works normally
    let md001_warnings = warnings
        .iter()
        .filter(|w| w.rule_name == Some("MD001"))
        .count();
    assert_eq!(
        md001_warnings, 0,
        "MD001 should not trigger on this content"
    );
}

#[test]
fn test_config_priority() {
    // Test that rule-specific configs override defaults

    // Create test rules with defaults
    let mut rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD013LineLength::default()), // Default line_length: 80
    ];

    // Create config with different line_length
    let mut config = create_test_config(); // line_length: 120

    // Apply configs
    apply_rule_configs(&mut rules, &config);

    // Test with a line that's 100 chars (exceeds default but within config)
    let line_100_chars = "# Test\n\n".to_owned() + &"A".repeat(98); // 98 A's + 2 chars for "# " = 100 chars

    // Run linting with our custom rule
    let warnings = rumdl::lint(&line_100_chars, &rules, false).expect("Linting should succeed");

    // Should not trigger MD013 because config value is 120
    let md013_warnings = warnings
        .iter()
        .filter(|w| w.rule_name == Some("MD013"))
        .count();
    assert_eq!(
        md013_warnings, 0,
        "MD013 should not trigger with configured line_length 120"
    );

    // Now change config to 50 chars
    let mut md013_values = HashMap::new();
    md013_values.insert("line_length".to_string(), toml::Value::Integer(50));
    let md013_config = RuleConfig {
        values: md013_values,
    };
    config.rules.insert("MD013".to_string(), md013_config);

    // Re-apply configs
    apply_rule_configs(&mut rules, &config);

    // Should now trigger MD013
    let warnings = rumdl::lint(&line_100_chars, &rules, false).expect("Linting should succeed");
    let md013_warnings = warnings
        .iter()
        .filter(|w| w.rule_name == Some("MD013"))
        .count();
    assert_eq!(
        md013_warnings, 1,
        "MD013 should trigger with configured line_length 50"
    );
}

#[test]
fn test_partial_rule_config() {
    // Test that partial configurations only override specified fields

    // Create a rule with known defaults
    let mut rules: Vec<Box<dyn Rule>> =
        vec![Box::new(MD013LineLength::new(80, true, false, true, false))];

    // Create config with only line_length specified
    let mut rules_map = HashMap::new();
    let mut md013_values = HashMap::new();
    md013_values.insert("line_length".to_string(), toml::Value::Integer(100));
    // Note: code_blocks not specified, should keep default value
    let md013_config = RuleConfig {
        values: md013_values,
    };
    rules_map.insert("MD013".to_string(), md013_config);

    let config = Config {
        global: GlobalConfig::default(),
        rules: rules_map,
    };

    // Apply configs
    apply_rule_configs(&mut rules, &config);

    // Test with a regular line that exceeds 80 chars but not 100 chars
    let test_content = "This is a regular line that is longer than 80 characters but shorter than 100 characters in length.";

    // Run linting with our custom rule
    let warnings = rumdl::lint(test_content, &rules, false).expect("Linting should succeed");

    // Should NOT trigger MD013 because line_length is set to 100
    let md013_warnings = warnings
        .iter()
        .filter(|w| w.rule_name == Some("MD013"))
        .count();
    assert_eq!(
        md013_warnings, 0,
        "MD013 should not trigger with line_length 100"
    );

    // Now update config to set line_length to 60
    let mut rules_map = HashMap::new();
    let mut md013_values = HashMap::new();
    md013_values.insert("line_length".to_string(), toml::Value::Integer(60));
    let md013_config = RuleConfig {
        values: md013_values,
    };
    rules_map.insert("MD013".to_string(), md013_config);

    let config = Config {
        global: GlobalConfig::default(),
        rules: rules_map,
    };

    // Re-apply configs
    apply_rule_configs(&mut rules, &config);

    // Run linting with our custom rule
    let warnings = rumdl::lint(test_content, &rules, false).expect("Linting should succeed");

    // Now should trigger MD013 because line_length is less than the line length
    let md013_warnings = warnings
        .iter()
        .filter(|w| w.rule_name == Some("MD013"))
        .count();
    assert_eq!(
        md013_warnings, 1,
        "MD013 should trigger with line_length 60"
    );
}

// The remaining tests require get_enabled_rules which is in main.rs and isn't exposed.
// We'll use a simplified version of these tests instead.

#[test]
fn test_config_enable_disable() {
    // Create a config file with rules enabled and disabled
    let config_path = "test_enabled_rules_config.toml";
    let config_content = r#"
[global]
disable = ["MD013"]
enable = ["MD004", "MD001"]

[MD004]
style = "dash"
"#;

    fs::write(config_path, config_content).expect("Failed to write test config file");

    // Load the config
    let config = rumdl::config::load_config(Some(config_path)).expect("Failed to load config");

    // Create our rules (similar to what get_enabled_rules would do)
    let mut rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD001HeadingIncrement::default()),
        Box::new(MD004UnorderedListStyle::default()),
        Box::new(MD013LineLength::default()),
    ];

    // Apply configurations
    apply_rule_configs(&mut rules, &config);

    // Apply config disable first (this would be done by get_enabled_rules)
    rules.retain(|rule| !config.global.disable.iter().any(|name| name == rule.name()));

    // MD013 should be filtered out by the disable directive
    let has_md013 = rules.iter().any(|r| r.name() == "MD013");
    assert!(!has_md013, "MD013 should be disabled");

    // MD001 and MD004 should still be present
    let has_md001 = rules.iter().any(|r| r.name() == "MD001");
    let has_md004 = rules.iter().any(|r| r.name() == "MD004");
    assert!(has_md001, "MD001 should still be enabled");
    assert!(has_md004, "MD004 should still be enabled");

    // Check that MD004 has the right style configuration
    let test_content = r#"
# Test
* Item with asterisk
"#;

    let warnings = rumdl::lint(test_content, &rules, false).expect("Linting should succeed");
    let md004_warnings = warnings
        .iter()
        .filter(|w| w.rule_name == Some("MD004"))
        .count();

    assert_eq!(
        md004_warnings, 1,
        "MD004 should flag asterisk when style is set to dash"
    );

    // Clean up
    fs::remove_file(config_path).expect("Failed to remove test config file");
}

#[test]
fn test_disable_all_override() {
    // Test that explicitly enabled rules override "disable all"

    // Create a config file that disables all rules but enables MD013
    let config_path = "test_enabled_override.toml";
    let config_content = r#"
[global]
disable = ["all"]
enable = ["MD013"]

[MD013]
line_length = 100
"#;

    fs::write(config_path, config_content).expect("Failed to write test config file");

    // Load the config
    let config = rumdl::config::load_config(Some(config_path)).expect("Failed to load config");

    // Create our rules (similar to what get_enabled_rules would do)
    let mut rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD001HeadingIncrement::default()),
        Box::new(MD004UnorderedListStyle::default()),
        Box::new(MD013LineLength::default()),
    ];

    // Apply configurations
    apply_rule_configs(&mut rules, &config);

    // Apply disable first
    if config.global.disable.contains(&"all".to_string()) {
        rules.clear(); // Disable all rules
    } else {
        rules.retain(|rule| !config.global.disable.iter().any(|name| name == rule.name()));
    }

    // Then apply enable (this allows enable to override disable)
    if !config.global.enable.is_empty() {
        // Add back any explicitly enabled rules
        if rules.is_empty() {
            // If all rules were disabled, we need to add back the ones that are explicitly enabled
            for rule_name in &config.global.enable {
                match rule_name.as_str() {
                    "MD013" => rules.push(Box::new(MD013LineLength::default())),
                    "MD001" => rules.push(Box::new(MD001HeadingIncrement::default())),
                    "MD004" => rules.push(Box::new(MD004UnorderedListStyle::default())),
                    _ => {}
                }
            }

            // Reapply configurations after adding rules back
            apply_rule_configs(&mut rules, &config);
        } else {
            // Only keep explicitly enabled rules
            rules.retain(|rule| config.global.enable.iter().any(|name| name == rule.name()));
        }
    }

    // Only MD013 should be enabled
    assert_eq!(rules.len(), 1, "Should only have one rule enabled");
    assert_eq!(
        rules[0].name(),
        "MD013",
        "The only enabled rule should be MD013"
    );

    // Test content that would be flagged by MD001/MD004 but not by MD013 with line_length 100
    let test_content =
        "#Test\n\n".to_owned() + &"A".repeat(90) + "\n\n## Some heading\n\n* Item 1\n+ Item 2";
    let warnings = rumdl::lint(&test_content, &rules, false).expect("Linting should succeed");

    // Verify no warnings (MD013 has line_length 100 and the line is 90 chars)
    assert!(warnings.is_empty(), "No warnings should be generated because only MD013 is enabled and the line is under 100 chars");

    // Clean up
    fs::remove_file(config_path).expect("Failed to remove test config file");
}
