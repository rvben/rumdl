use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_flavor_loading() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
flavor = "mkdocs"
disable = ["MD001"]
"#;
    fs::write(&config_path, config_content).unwrap();

    // Load the config
    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Check that flavor was loaded
    assert_eq!(config.global.flavor, MarkdownFlavor::MkDocs);
    assert!(config.is_mkdocs_flavor());
    assert!(config.is_mkdocs_project()); // Test backwards compatibility
    assert_eq!(config.global.disable, vec!["MD001".to_string()]);
}

#[test]
fn test_pyproject_toml_root_level_config() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("pyproject.toml");

    // Create a test pyproject.toml with root-level configuration
    let content = r#"
[tool.rumdl]
line-length = 120
disable = ["MD033"]
enable = ["MD001", "MD004"]
include = ["docs/*.md"]
exclude = ["node_modules"]
respect-gitignore = true
        "#;

    fs::write(&config_path, content).unwrap();

    // Load the config with skip_auto_discovery to avoid environment config files
    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into(); // Convert to plain config for assertions

    // Check global settings
    assert_eq!(config.global.disable, vec!["MD033".to_string()]);
    assert_eq!(config.global.enable, vec!["MD001".to_string(), "MD004".to_string()]);
    // Should now contain only the configured pattern since auto-discovery is disabled
    assert_eq!(config.global.include, vec!["docs/*.md".to_string()]);
    assert_eq!(config.global.exclude, vec!["node_modules".to_string()]);
    assert!(config.global.respect_gitignore);

    // Check line-length was correctly added to MD013
    let line_length = get_rule_config_value::<usize>(&config, "MD013", "line-length");
    assert_eq!(line_length, Some(120));
}

#[test]
fn test_pyproject_toml_snake_case_and_kebab_case() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("pyproject.toml");

    // Test with both kebab-case and snake_case variants
    let content = r#"
[tool.rumdl]
line-length = 150
respect_gitignore = true
        "#;

    fs::write(&config_path, content).unwrap();

    // Load the config with skip_auto_discovery to avoid environment config files
    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into(); // Convert to plain config for assertions

    // Check settings were correctly loaded
    assert!(config.global.respect_gitignore);
    let line_length = get_rule_config_value::<usize>(&config, "MD013", "line-length");
    assert_eq!(line_length, Some(150));
}

#[test]
fn test_md013_key_normalization_in_rumdl_toml() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line_length = 111
line-length = 222
"#;
    fs::write(&config_path, config_content).unwrap();
    // Load the config with skip_auto_discovery to avoid environment config files
    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let rule_cfg = sourced.rules.get("MD013").expect("MD013 rule config should exist");
    // Now we should only get the explicitly configured key
    let keys: Vec<_> = rule_cfg.values.keys().cloned().collect();
    assert_eq!(keys, vec!["line-length"]);
    let val = &rule_cfg.values["line-length"].value;
    assert_eq!(val.as_integer(), Some(222));
    // get_rule_config_value should retrieve the value for both snake_case and kebab-case
    let config: Config = sourced.clone().into_validated_unchecked().into();
    let v1 = get_rule_config_value::<usize>(&config, "MD013", "line_length");
    let v2 = get_rule_config_value::<usize>(&config, "MD013", "line-length");
    assert_eq!(v1, Some(222));
    assert_eq!(v2, Some(222));
}

#[test]
fn test_md013_section_case_insensitivity() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[md013]
line-length = 101

[Md013]
line-length = 102

[MD013]
line-length = 103
"#;
    fs::write(&config_path, config_content).unwrap();
    // Load the config with skip_auto_discovery to avoid environment config files
    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.clone().into_validated_unchecked().into();
    // Only the last section should win, and be present
    let rule_cfg = sourced.rules.get("MD013").expect("MD013 rule config should exist");
    let keys: Vec<_> = rule_cfg.values.keys().cloned().collect();
    assert_eq!(keys, vec!["line-length"]);
    let val = &rule_cfg.values["line-length"].value;
    assert_eq!(val.as_integer(), Some(103));
    let v = get_rule_config_value::<usize>(&config, "MD013", "line-length");
    assert_eq!(v, Some(103));
}

#[test]
fn test_md013_key_snake_and_kebab_case() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line_length = 201
line-length = 202
"#;
    fs::write(&config_path, config_content).unwrap();
    // Load the config with skip_auto_discovery to avoid environment config files
    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.clone().into_validated_unchecked().into();
    let rule_cfg = sourced.rules.get("MD013").expect("MD013 rule config should exist");
    let keys: Vec<_> = rule_cfg.values.keys().cloned().collect();
    assert_eq!(keys, vec!["line-length"]);
    let val = &rule_cfg.values["line-length"].value;
    assert_eq!(val.as_integer(), Some(202));
    let v1 = get_rule_config_value::<usize>(&config, "MD013", "line_length");
    let v2 = get_rule_config_value::<usize>(&config, "MD013", "line-length");
    assert_eq!(v1, Some(202));
    assert_eq!(v2, Some(202));
}

#[test]
fn test_unknown_rule_section_is_ignored() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD999]
foo = 1
bar = 2
[MD013]
line-length = 303
"#;
    fs::write(&config_path, config_content).unwrap();
    // Load the config with skip_auto_discovery to avoid environment config files
    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.clone().into_validated_unchecked().into();
    // MD999 should not be present
    assert!(!sourced.rules.contains_key("MD999"));
    // MD013 should be present and correct
    let v = get_rule_config_value::<usize>(&config, "MD013", "line-length");
    assert_eq!(v, Some(303));
}

#[test]
fn test_invalid_toml_syntax() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    // Invalid TOML with unclosed string
    let config_content = r#"
[MD013]
line-length = "unclosed string
"#;
    fs::write(&config_path, config_content).unwrap();

    let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
    assert!(result.is_err());
    match result.unwrap_err() {
        ConfigError::ParseError(msg) => {
            // The actual error message from toml parser might vary
            assert!(msg.contains("expected") || msg.contains("invalid") || msg.contains("unterminated"));
        }
        _ => panic!("Expected ParseError"),
    }
}

#[test]
fn test_wrong_type_for_config_value() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    // line-length should be a number, not a string
    let config_content = r#"
[MD013]
line-length = "not a number"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // The value should be loaded as a string, not converted
    let rule_config = config.rules.get("MD013").unwrap();
    let value = rule_config.values.get("line-length").unwrap();
    assert!(matches!(value, toml::Value::String(_)));
}

#[test]
fn test_empty_config_file() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    // Empty file
    fs::write(&config_path, "").unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Should have default values
    assert_eq!(config.global.line_length.get(), 80);
    assert!(config.global.respect_gitignore);
    assert!(config.rules.is_empty());
}

#[test]
fn test_malformed_pyproject_toml() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("pyproject.toml");

    // Missing closing bracket
    let content = r#"
[tool.rumdl
line-length = 120
"#;
    fs::write(&config_path, content).unwrap();

    let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
    assert!(result.is_err());
}

#[test]
fn test_conflicting_config_values() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    // Both enable and disable the same rule - these need to be in a global section
    let config_content = r#"
[global]
enable = ["MD013"]
disable = ["MD013"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Conflict resolution: enable wins over disable
    assert!(config.global.enable.contains(&"MD013".to_string()));
    assert!(!config.global.disable.contains(&"MD013".to_string()));
}

#[test]
fn test_invalid_rule_names() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    let config_content = r#"
[global]
enable = ["MD001", "NOT_A_RULE", "md002", "12345"]
disable = ["MD-001", "MD_002"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // All values should be preserved as-is
    assert_eq!(config.global.enable.len(), 4);
    assert_eq!(config.global.disable.len(), 2);
}

#[test]
fn test_deeply_nested_config() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    // This should be ignored as we don't support nested tables within rule configs
    let config_content = r#"
[MD013]
line-length = 100
[MD013.nested]
value = 42
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    let rule_config = config.rules.get("MD013").unwrap();
    assert_eq!(
        rule_config.values.get("line-length").unwrap(),
        &toml::Value::Integer(100)
    );
    // Nested table should not be present
    assert!(!rule_config.values.contains_key("nested"));
}

#[test]
fn test_unicode_in_config() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    let config_content = r#"
[global]
include = ["文档/*.md", "ドキュメント/*.md"]
exclude = ["测试/*", "🚀/*"]

[MD013]
line-length = 80
message = "行太长了 🚨"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    assert_eq!(config.global.include.len(), 2);
    assert_eq!(config.global.exclude.len(), 2);
    assert!(config.global.include[0].contains("文档"));
    assert!(config.global.exclude[1].contains("🚀"));

    let rule_config = config.rules.get("MD013").unwrap();
    let message = rule_config.values.get("message").unwrap();
    if let toml::Value::String(s) = message {
        assert!(s.contains("行太长了"));
        assert!(s.contains("🚨"));
    }
}

#[test]
fn test_extremely_long_values() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    let long_string = "a".repeat(10000);
    let config_content = format!(
        r#"
[global]
exclude = ["{long_string}"]

[MD013]
line-length = 999999999
"#
    );

    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    assert_eq!(config.global.exclude[0].len(), 10000);
    let line_length = get_rule_config_value::<usize>(&config, "MD013", "line-length");
    assert_eq!(line_length, Some(999999999));
}

#[test]
fn test_config_with_comments() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    let config_content = r#"
[global]
# This is a comment
enable = ["MD001"] # Enable MD001
# disable = ["MD002"] # This is commented out

[MD013] # Line length rule
line-length = 100 # Set to 100 characters
# ignored = true # This setting is commented out
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    assert_eq!(config.global.enable, vec!["MD001"]);
    assert!(config.global.disable.is_empty()); // Commented out

    let rule_config = config.rules.get("MD013").unwrap();
    assert_eq!(rule_config.values.len(), 1); // Only line-length
    assert!(!rule_config.values.contains_key("ignored"));
}

#[test]
fn test_arrays_in_rule_config() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    let config_content = r#"
[MD003]
levels = [1, 2, 3]
tags = ["important", "critical"]
mixed = [1, "two", true]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Arrays should now be properly parsed
    let rule_config = config.rules.get("MD003").expect("MD003 config should exist");

    // Check that arrays are present and correctly parsed
    assert!(rule_config.values.contains_key("levels"));
    assert!(rule_config.values.contains_key("tags"));
    assert!(rule_config.values.contains_key("mixed"));

    // Verify array contents
    if let Some(toml::Value::Array(levels)) = rule_config.values.get("levels") {
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], toml::Value::Integer(1));
        assert_eq!(levels[1], toml::Value::Integer(2));
        assert_eq!(levels[2], toml::Value::Integer(3));
    } else {
        panic!("levels should be an array");
    }

    if let Some(toml::Value::Array(tags)) = rule_config.values.get("tags") {
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0], toml::Value::String("important".to_string()));
        assert_eq!(tags[1], toml::Value::String("critical".to_string()));
    } else {
        panic!("tags should be an array");
    }

    if let Some(toml::Value::Array(mixed)) = rule_config.values.get("mixed") {
        assert_eq!(mixed.len(), 3);
        assert_eq!(mixed[0], toml::Value::Integer(1));
        assert_eq!(mixed[1], toml::Value::String("two".to_string()));
        assert_eq!(mixed[2], toml::Value::Boolean(true));
    } else {
        panic!("mixed should be an array");
    }
}

#[test]
fn test_normalize_key_edge_cases() {
    // Rule names
    assert_eq!(normalize_key("MD001"), "MD001");
    assert_eq!(normalize_key("md001"), "MD001");
    assert_eq!(normalize_key("Md001"), "MD001");
    assert_eq!(normalize_key("mD001"), "MD001");

    // Non-rule names
    assert_eq!(normalize_key("line_length"), "line-length");
    assert_eq!(normalize_key("line-length"), "line-length");
    assert_eq!(normalize_key("LINE_LENGTH"), "line-length");
    assert_eq!(normalize_key("respect_gitignore"), "respect-gitignore");

    // Edge cases
    assert_eq!(normalize_key("MD"), "md"); // Too short to be a rule
    assert_eq!(normalize_key("MD00"), "md00"); // Too short
    assert_eq!(normalize_key("MD0001"), "md0001"); // Too long
    assert_eq!(normalize_key("MDabc"), "mdabc"); // Non-digit
    assert_eq!(normalize_key("MD00a"), "md00a"); // Partial digit
    assert_eq!(normalize_key(""), "");
    assert_eq!(normalize_key("_"), "-");
    assert_eq!(normalize_key("___"), "---");
}

#[test]
fn test_missing_config_file() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("nonexistent.toml");

    let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);
    assert!(result.is_err());
    match result.unwrap_err() {
        ConfigError::IoError { .. } => {}
        _ => panic!("Expected IoError for missing file"),
    }
}

#[test]
#[cfg(unix)]
fn test_permission_denied_config() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    fs::write(&config_path, "enable = [\"MD001\"]").unwrap();

    // Remove read permissions
    let mut perms = fs::metadata(&config_path).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&config_path, perms).unwrap();

    let result = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true);

    // Restore permissions for cleanup
    let mut perms = fs::metadata(&config_path).unwrap().permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&config_path, perms).unwrap();

    assert!(result.is_err());
    match result.unwrap_err() {
        ConfigError::IoError { .. } => {}
        _ => panic!("Expected IoError for permission denied"),
    }
}

#[test]
fn test_circular_reference_detection() {
    // This test is more conceptual since TOML doesn't support circular references
    // But we test that deeply nested structures don't cause stack overflow
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    let mut config_content = String::from("[MD001]\n");
    for i in 0..100 {
        config_content.push_str(&format!("key{i} = {i}\n"));
    }

    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    let rule_config = config.rules.get("MD001").unwrap();
    assert_eq!(rule_config.values.len(), 100);
}

#[test]
fn test_special_toml_values() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    let config_content = r#"
[MD001]
infinity = inf
neg_infinity = -inf
not_a_number = nan
datetime = 1979-05-27T07:32:00Z
local_date = 1979-05-27
local_time = 07:32:00
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Some values might not be parsed due to parser limitations
    if let Some(rule_config) = config.rules.get("MD001") {
        // Check special float values if present
        if let Some(toml::Value::Float(f)) = rule_config.values.get("infinity") {
            assert!(f.is_infinite() && f.is_sign_positive());
        }
        if let Some(toml::Value::Float(f)) = rule_config.values.get("neg_infinity") {
            assert!(f.is_infinite() && f.is_sign_negative());
        }
        if let Some(toml::Value::Float(f)) = rule_config.values.get("not_a_number") {
            assert!(f.is_nan());
        }

        // Check datetime values if present
        if let Some(val) = rule_config.values.get("datetime") {
            assert!(matches!(val, toml::Value::Datetime(_)));
        }
        // Note: local_date and local_time might not be parsed by the current implementation
    }
}

#[test]
fn test_default_config_passes_validation() {
    use crate::rules;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_path_str = config_path.to_str().unwrap();

    // Create the default config using the same function that `rumdl init` uses
    create_default_config(config_path_str).unwrap();

    // Load it back as a SourcedConfig
    let sourced = SourcedConfig::load(Some(config_path_str), None).expect("Default config should load successfully");

    // Create the rule registry
    let all_rules = rules::all_rules(&Config::default());
    let registry = RuleRegistry::from_rules(&all_rules);

    // Validate the config
    let warnings = validate_config_sourced(&sourced, &registry);

    // The default config should have no warnings
    if !warnings.is_empty() {
        for warning in &warnings {
            eprintln!("Config validation warning: {}", warning.message);
            if let Some(rule) = &warning.rule {
                eprintln!("  Rule: {rule}");
            }
            if let Some(key) = &warning.key {
                eprintln!("  Key: {key}");
            }
        }
    }
    assert!(
        warnings.is_empty(),
        "Default config from rumdl init should pass validation without warnings"
    );
}

#[test]
fn test_md054_preferred_style_accepts_scalar_form() {
    use crate::rules;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_path, "[MD054]\npreferred-style = \"autolink\"\n").unwrap();

    let sourced = SourcedConfig::load(Some(config_path.to_str().unwrap()), None).expect("Config should load");
    let all_rules = rules::all_rules(&Config::default());
    let registry = RuleRegistry::from_rules(&all_rules);
    let warnings = validate_config_sourced(&sourced, &registry);

    let md054_warnings: Vec<_> = warnings.iter().filter(|w| w.rule.as_deref() == Some("MD054")).collect();
    assert!(
        md054_warnings.is_empty(),
        "Scalar preferred-style should pass validation, got: {md054_warnings:?}"
    );
}

#[test]
fn test_md054_preferred_style_accepts_list_form() {
    use crate::rules;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_path, "[MD054]\npreferred-style = [\"autolink\", \"full\"]\n").unwrap();

    let sourced = SourcedConfig::load(Some(config_path.to_str().unwrap()), None).expect("Config should load");
    let all_rules = rules::all_rules(&Config::default());
    let registry = RuleRegistry::from_rules(&all_rules);
    let warnings = validate_config_sourced(&sourced, &registry);

    let md054_warnings: Vec<_> = warnings.iter().filter(|w| w.rule.as_deref() == Some("MD054")).collect();
    assert!(
        md054_warnings.is_empty(),
        "List preferred-style should pass validation (polymorphic schema), got: {md054_warnings:?}"
    );
}

#[test]
fn test_md054_preferred_style_unknown_key_still_warns() {
    use crate::rules;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_path, "[MD054]\npreferred-styel = \"autolink\"\n").unwrap();

    let sourced = SourcedConfig::load(Some(config_path.to_str().unwrap()), None).expect("Config should load");
    let all_rules = rules::all_rules(&Config::default());
    let registry = RuleRegistry::from_rules(&all_rules);
    let warnings = validate_config_sourced(&sourced, &registry);

    let unknown_key_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule.as_deref() == Some("MD054") && w.message.contains("Unknown option"))
        .collect();
    assert!(
        !unknown_key_warnings.is_empty(),
        "Polymorphic schema must still detect typos in key names; got warnings: {warnings:?}"
    );
}

#[test]
fn test_enabled_key_valid_for_any_rule() {
    use crate::rules;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    // MD070 has no config struct — test that enabled is accepted anyway
    std::fs::write(&config_path, "[MD070]\nenabled = true\n").unwrap();

    let sourced = SourcedConfig::load(Some(config_path.to_str().unwrap()), None).expect("Config should load");
    let all_rules = rules::all_rules(&Config::default());
    let registry = RuleRegistry::from_rules(&all_rules);
    let warnings = validate_config_sourced(&sourced, &registry);

    let enabled_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.key.as_deref() == Some("enabled"))
        .collect();
    assert!(
        enabled_warnings.is_empty(),
        "'enabled' should be valid for any rule, got warnings: {enabled_warnings:?}"
    );
}

#[test]
fn test_per_file_ignores_config_parsing() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[per-file-ignores]
"README.md" = ["MD033"]
"docs/**/*.md" = ["MD013", "MD033"]
"test/*.md" = ["MD041"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Verify per-file-ignores was loaded
    assert_eq!(config.per_file_ignores.len(), 3);
    assert_eq!(
        config.per_file_ignores.get("README.md"),
        Some(&vec!["MD033".to_string()])
    );
    assert_eq!(
        config.per_file_ignores.get("docs/**/*.md"),
        Some(&vec!["MD013".to_string(), "MD033".to_string()])
    );
    assert_eq!(
        config.per_file_ignores.get("test/*.md"),
        Some(&vec!["MD041".to_string()])
    );
}

#[test]
fn test_per_file_ignores_glob_matching() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[per-file-ignores]
"README.md" = ["MD033"]
"docs/**/*.md" = ["MD013"]
"**/test_*.md" = ["MD041"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Test exact match
    let ignored = config.get_ignored_rules_for_file(&PathBuf::from("README.md"));
    assert!(ignored.contains("MD033"));
    assert_eq!(ignored.len(), 1);

    // Test glob pattern matching
    let ignored = config.get_ignored_rules_for_file(&PathBuf::from("docs/api/overview.md"));
    assert!(ignored.contains("MD013"));
    assert_eq!(ignored.len(), 1);

    // Test recursive glob pattern
    let ignored = config.get_ignored_rules_for_file(&PathBuf::from("tests/fixtures/test_example.md"));
    assert!(ignored.contains("MD041"));
    assert_eq!(ignored.len(), 1);

    // Test non-matching path
    let ignored = config.get_ignored_rules_for_file(&PathBuf::from("other/file.md"));
    assert!(ignored.is_empty());
}

#[test]
fn test_per_file_ignores_pyproject_toml() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("pyproject.toml");
    let config_content = r#"
[tool.rumdl]
[tool.rumdl.per-file-ignores]
"README.md" = ["MD033", "MD013"]
"generated/*.md" = ["MD041"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Verify per-file-ignores was loaded from pyproject.toml
    assert_eq!(config.per_file_ignores.len(), 2);
    assert_eq!(
        config.per_file_ignores.get("README.md"),
        Some(&vec!["MD033".to_string(), "MD013".to_string()])
    );
    assert_eq!(
        config.per_file_ignores.get("generated/*.md"),
        Some(&vec!["MD041".to_string()])
    );
}

#[test]
fn test_per_file_ignores_multiple_patterns_match() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[per-file-ignores]
"docs/**/*.md" = ["MD013"]
"**/api/*.md" = ["MD033"]
"docs/api/overview.md" = ["MD041"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // File matches multiple patterns - should get union of all rules
    let ignored = config.get_ignored_rules_for_file(&PathBuf::from("docs/api/overview.md"));
    assert_eq!(ignored.len(), 3);
    assert!(ignored.contains("MD013"));
    assert!(ignored.contains("MD033"));
    assert!(ignored.contains("MD041"));
}

#[test]
fn test_per_file_ignores_rule_name_normalization() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[per-file-ignores]
"README.md" = ["md033", "MD013", "Md041"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // All rule names should be normalized to uppercase
    let ignored = config.get_ignored_rules_for_file(&PathBuf::from("README.md"));
    assert_eq!(ignored.len(), 3);
    assert!(ignored.contains("MD033"));
    assert!(ignored.contains("MD013"));
    assert!(ignored.contains("MD041"));
}

#[test]
fn test_per_file_ignores_invalid_glob_pattern() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[per-file-ignores]
"[invalid" = ["MD033"]
"valid/*.md" = ["MD013"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Invalid pattern should be skipped, valid pattern should work
    let ignored = config.get_ignored_rules_for_file(&PathBuf::from("valid/test.md"));
    assert!(ignored.contains("MD013"));

    // Invalid pattern should not cause issues
    let ignored2 = config.get_ignored_rules_for_file(&PathBuf::from("[invalid"));
    assert!(ignored2.is_empty());
}

#[test]
fn test_per_file_ignores_empty_section() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
disable = ["MD001"]

[per-file-ignores]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Empty per-file-ignores should work fine
    assert_eq!(config.per_file_ignores.len(), 0);
    let ignored = config.get_ignored_rules_for_file(&PathBuf::from("README.md"));
    assert!(ignored.is_empty());
}

#[test]
fn test_per_file_ignores_with_underscores_in_pyproject() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("pyproject.toml");
    let config_content = r#"
[tool.rumdl]
[tool.rumdl.per_file_ignores]
"README.md" = ["MD033"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Should support both per-file-ignores and per_file_ignores
    assert_eq!(config.per_file_ignores.len(), 1);
    assert_eq!(
        config.per_file_ignores.get("README.md"),
        Some(&vec!["MD033".to_string()])
    );
}

#[test]
fn test_per_file_ignores_absolute_path_matching() {
    // Regression test for issue #208: per-file-ignores should work with absolute paths
    // This is critical for GitHub Actions which uses absolute paths like $GITHUB_WORKSPACE
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    // Create a subdirectory and file to match against
    let github_dir = temp_dir.path().join(".github");
    fs::create_dir_all(&github_dir).unwrap();
    let test_file = github_dir.join("pull_request_template.md");
    fs::write(&test_file, "Test content").unwrap();

    let config_content = r#"
[per-file-ignores]
".github/pull_request_template.md" = ["MD041"]
"docs/**/*.md" = ["MD013"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Test with absolute path (like GitHub Actions would use)
    let absolute_path = test_file.canonicalize().unwrap();
    let ignored = config.get_ignored_rules_for_file(&absolute_path);
    assert!(
        ignored.contains("MD041"),
        "Should match absolute path {absolute_path:?} against relative pattern"
    );
    assert_eq!(ignored.len(), 1);

    // Also verify relative path still works
    let relative_path = PathBuf::from(".github/pull_request_template.md");
    let ignored = config.get_ignored_rules_for_file(&relative_path);
    assert!(ignored.contains("MD041"), "Should match relative path");
}

// ==========================================
// Per-File-Flavor Tests
// ==========================================

#[test]
fn test_per_file_flavor_config_parsing() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[per-file-flavor]
"docs/**/*.md" = "mkdocs"
"**/*.mdx" = "mdx"
"**/*.qmd" = "quarto"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Verify per-file-flavor was loaded
    assert_eq!(config.per_file_flavor.len(), 3);
    assert_eq!(
        config.per_file_flavor.get("docs/**/*.md"),
        Some(&MarkdownFlavor::MkDocs)
    );
    assert_eq!(config.per_file_flavor.get("**/*.mdx"), Some(&MarkdownFlavor::MDX));
    assert_eq!(config.per_file_flavor.get("**/*.qmd"), Some(&MarkdownFlavor::Quarto));
}

#[test]
fn test_per_file_flavor_glob_matching() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[per-file-flavor]
"docs/**/*.md" = "mkdocs"
"**/*.mdx" = "mdx"
"components/**/*.md" = "mdx"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Test mkdocs flavor for docs directory
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/api/overview.md"));
    assert_eq!(flavor, MarkdownFlavor::MkDocs);

    // Test mdx flavor for .mdx extension
    let flavor = config.get_flavor_for_file(&PathBuf::from("src/components/Button.mdx"));
    assert_eq!(flavor, MarkdownFlavor::MDX);

    // Test mdx flavor for components directory
    let flavor = config.get_flavor_for_file(&PathBuf::from("components/Button/README.md"));
    assert_eq!(flavor, MarkdownFlavor::MDX);

    // Test non-matching path falls back to standard
    let flavor = config.get_flavor_for_file(&PathBuf::from("README.md"));
    assert_eq!(flavor, MarkdownFlavor::Standard);
}

#[test]
fn test_per_file_flavor_pyproject_toml() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("pyproject.toml");
    let config_content = r#"
[tool.rumdl]
[tool.rumdl.per-file-flavor]
"docs/**/*.md" = "mkdocs"
"**/*.mdx" = "mdx"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Verify per-file-flavor was loaded from pyproject.toml
    assert_eq!(config.per_file_flavor.len(), 2);
    assert_eq!(
        config.per_file_flavor.get("docs/**/*.md"),
        Some(&MarkdownFlavor::MkDocs)
    );
    assert_eq!(config.per_file_flavor.get("**/*.mdx"), Some(&MarkdownFlavor::MDX));
}

#[test]
fn test_per_file_flavor_first_match_wins() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    // Order matters - first match wins (IndexMap preserves order)
    let config_content = r#"
[per-file-flavor]
"docs/internal/**/*.md" = "quarto"
"docs/**/*.md" = "mkdocs"
"**/*.md" = "standard"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // More specific pattern should match first
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/internal/secret.md"));
    assert_eq!(flavor, MarkdownFlavor::Quarto);

    // Less specific pattern for other docs
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/public/readme.md"));
    assert_eq!(flavor, MarkdownFlavor::MkDocs);

    // Fallback to least specific pattern
    let flavor = config.get_flavor_for_file(&PathBuf::from("other/file.md"));
    assert_eq!(flavor, MarkdownFlavor::Standard);
}

#[test]
fn test_per_file_flavor_overrides_global_flavor() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
flavor = "mkdocs"

[per-file-flavor]
"**/*.mdx" = "mdx"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Per-file-flavor should override global flavor
    let flavor = config.get_flavor_for_file(&PathBuf::from("components/Button.mdx"));
    assert_eq!(flavor, MarkdownFlavor::MDX);

    // Non-matching files should use global flavor
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/readme.md"));
    assert_eq!(flavor, MarkdownFlavor::MkDocs);
}

#[test]
fn test_per_file_flavor_empty_map() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
disable = ["MD001"]

[per-file-flavor]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Empty per-file-flavor should fall back to auto-detection
    let flavor = config.get_flavor_for_file(&PathBuf::from("README.md"));
    assert_eq!(flavor, MarkdownFlavor::Standard);

    // MDX files should auto-detect
    let flavor = config.get_flavor_for_file(&PathBuf::from("test.mdx"));
    assert_eq!(flavor, MarkdownFlavor::MDX);
}

#[test]
fn test_per_file_flavor_with_underscores() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("pyproject.toml");
    let config_content = r#"
[tool.rumdl]
[tool.rumdl.per_file_flavor]
"docs/**/*.md" = "mkdocs"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Should support both per-file-flavor and per_file_flavor
    assert_eq!(config.per_file_flavor.len(), 1);
    assert_eq!(
        config.per_file_flavor.get("docs/**/*.md"),
        Some(&MarkdownFlavor::MkDocs)
    );
}

#[test]
fn test_per_file_flavor_absolute_path_matching() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");

    // Create a subdirectory and file to match against
    let docs_dir = temp_dir.path().join("docs");
    fs::create_dir_all(&docs_dir).unwrap();
    let test_file = docs_dir.join("guide.md");
    fs::write(&test_file, "Test content").unwrap();

    let config_content = r#"
[per-file-flavor]
"docs/**/*.md" = "mkdocs"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Test with absolute path
    let absolute_path = test_file.canonicalize().unwrap();
    let flavor = config.get_flavor_for_file(&absolute_path);
    assert_eq!(
        flavor,
        MarkdownFlavor::MkDocs,
        "Should match absolute path {absolute_path:?} against relative pattern"
    );

    // Also verify relative path still works
    let relative_path = PathBuf::from("docs/guide.md");
    let flavor = config.get_flavor_for_file(&relative_path);
    assert_eq!(flavor, MarkdownFlavor::MkDocs, "Should match relative path");
}

#[test]
fn test_per_file_flavor_all_flavors() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[per-file-flavor]
"standard/**/*.md" = "standard"
"mkdocs/**/*.md" = "mkdocs"
"mdx/**/*.md" = "mdx"
"quarto/**/*.md" = "quarto"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // All four flavors should be loadable
    assert_eq!(config.per_file_flavor.len(), 4);
    assert_eq!(
        config.per_file_flavor.get("standard/**/*.md"),
        Some(&MarkdownFlavor::Standard)
    );
    assert_eq!(
        config.per_file_flavor.get("mkdocs/**/*.md"),
        Some(&MarkdownFlavor::MkDocs)
    );
    assert_eq!(config.per_file_flavor.get("mdx/**/*.md"), Some(&MarkdownFlavor::MDX));
    assert_eq!(
        config.per_file_flavor.get("quarto/**/*.md"),
        Some(&MarkdownFlavor::Quarto)
    );
}

#[test]
fn test_per_file_flavor_invalid_glob_pattern() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    // Include an invalid glob pattern with unclosed bracket
    let config_content = r#"
[per-file-flavor]
"[invalid" = "mkdocs"
"valid/**/*.md" = "mdx"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Invalid pattern should be skipped, valid pattern should still work
    let flavor = config.get_flavor_for_file(&PathBuf::from("valid/test.md"));
    assert_eq!(flavor, MarkdownFlavor::MDX);

    // Non-matching should fall back to Standard
    let flavor = config.get_flavor_for_file(&PathBuf::from("other/test.md"));
    assert_eq!(flavor, MarkdownFlavor::Standard);
}

#[test]
fn test_per_file_flavor_paths_with_spaces() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[per-file-flavor]
"my docs/**/*.md" = "mkdocs"
"src/**/*.md" = "mdx"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Paths with spaces should match
    let flavor = config.get_flavor_for_file(&PathBuf::from("my docs/guide.md"));
    assert_eq!(flavor, MarkdownFlavor::MkDocs);

    // Regular path
    let flavor = config.get_flavor_for_file(&PathBuf::from("src/README.md"));
    assert_eq!(flavor, MarkdownFlavor::MDX);
}

#[test]
fn test_per_file_flavor_deeply_nested_paths() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[per-file-flavor]
"a/b/c/d/e/**/*.md" = "quarto"
"a/b/**/*.md" = "mkdocs"
"**/*.md" = "standard"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // 5-level deep path should match most specific pattern first
    let flavor = config.get_flavor_for_file(&PathBuf::from("a/b/c/d/e/f/deep.md"));
    assert_eq!(flavor, MarkdownFlavor::Quarto);

    // 3-level deep path
    let flavor = config.get_flavor_for_file(&PathBuf::from("a/b/c/test.md"));
    assert_eq!(flavor, MarkdownFlavor::MkDocs);

    // Root level
    let flavor = config.get_flavor_for_file(&PathBuf::from("root.md"));
    assert_eq!(flavor, MarkdownFlavor::Standard);
}

#[test]
fn test_per_file_flavor_complex_overlapping_patterns() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    // Complex pattern order testing - tests that IndexMap preserves TOML order
    let config_content = r#"
[per-file-flavor]
"docs/api/*.md" = "mkdocs"
"docs/**/*.mdx" = "mdx"
"docs/**/*.md" = "quarto"
"**/*.md" = "standard"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // docs/api/*.md should match first
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/api/reference.md"));
    assert_eq!(flavor, MarkdownFlavor::MkDocs);

    // docs/api/nested/file.md should NOT match docs/api/*.md (no **), but match docs/**/*.md
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/api/nested/file.md"));
    assert_eq!(flavor, MarkdownFlavor::Quarto);

    // .mdx in docs should match docs/**/*.mdx
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/components/Button.mdx"));
    assert_eq!(flavor, MarkdownFlavor::MDX);

    // .md outside docs should match **/*.md
    let flavor = config.get_flavor_for_file(&PathBuf::from("src/README.md"));
    assert_eq!(flavor, MarkdownFlavor::Standard);
}

#[test]
fn test_per_file_flavor_extension_detection_interaction() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    // Test that per-file-flavor pattern can override extension-based auto-detection
    let config_content = r#"
[per-file-flavor]
"legacy/**/*.mdx" = "standard"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // .mdx file in legacy dir should use pattern override (standard), not auto-detect (mdx)
    let flavor = config.get_flavor_for_file(&PathBuf::from("legacy/old.mdx"));
    assert_eq!(flavor, MarkdownFlavor::Standard);

    // .mdx file elsewhere should auto-detect as MDX
    let flavor = config.get_flavor_for_file(&PathBuf::from("src/component.mdx"));
    assert_eq!(flavor, MarkdownFlavor::MDX);
}

#[test]
fn test_per_file_flavor_standard_alias_none() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    // Test that "none" works as alias for "standard"
    let config_content = r#"
[per-file-flavor]
"plain/**/*.md" = "none"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // "none" should resolve to Standard
    let flavor = config.get_flavor_for_file(&PathBuf::from("plain/test.md"));
    assert_eq!(flavor, MarkdownFlavor::Standard);
}

#[test]
fn test_per_file_flavor_brace_expansion() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    // Test brace expansion in glob patterns
    let config_content = r#"
[per-file-flavor]
"docs/**/*.{md,mdx}" = "mkdocs"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Should match .md files
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/guide.md"));
    assert_eq!(flavor, MarkdownFlavor::MkDocs);

    // Should match .mdx files
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/component.mdx"));
    assert_eq!(flavor, MarkdownFlavor::MkDocs);
}

#[test]
fn test_per_file_flavor_single_star_vs_double_star() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    // Test difference between * (single level) and ** (recursive)
    let config_content = r#"
[per-file-flavor]
"docs/*.md" = "mkdocs"
"src/**/*.md" = "mdx"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Single * matches only direct children
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/README.md"));
    assert_eq!(flavor, MarkdownFlavor::MkDocs);

    // Single * does NOT match nested files
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/api/index.md"));
    assert_eq!(flavor, MarkdownFlavor::Standard); // fallback

    // Double ** matches recursively
    let flavor = config.get_flavor_for_file(&PathBuf::from("src/components/Button.md"));
    assert_eq!(flavor, MarkdownFlavor::MDX);

    let flavor = config.get_flavor_for_file(&PathBuf::from("src/README.md"));
    assert_eq!(flavor, MarkdownFlavor::MDX);
}

#[test]
fn test_per_file_flavor_question_mark_wildcard() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    // Test ? wildcard (matches single character)
    let config_content = r#"
[per-file-flavor]
"docs/v?.md" = "mkdocs"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // ? matches single character
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/v1.md"));
    assert_eq!(flavor, MarkdownFlavor::MkDocs);

    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/v2.md"));
    assert_eq!(flavor, MarkdownFlavor::MkDocs);

    // ? does NOT match multiple characters
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/v10.md"));
    assert_eq!(flavor, MarkdownFlavor::Standard);

    // ? does NOT match zero characters
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/v.md"));
    assert_eq!(flavor, MarkdownFlavor::Standard);
}

#[test]
fn test_per_file_flavor_character_class() {
    use std::path::PathBuf;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    // Test character class [abc]
    let config_content = r#"
[per-file-flavor]
"docs/[abc].md" = "mkdocs"
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Should match a, b, or c
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/a.md"));
    assert_eq!(flavor, MarkdownFlavor::MkDocs);

    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/b.md"));
    assert_eq!(flavor, MarkdownFlavor::MkDocs);

    // Should NOT match d
    let flavor = config.get_flavor_for_file(&PathBuf::from("docs/d.md"));
    assert_eq!(flavor, MarkdownFlavor::Standard);
}

// ==========================================
// Path normalization robustness tests
// (regression: per-file-flavor / per-file-ignores must work even when
// `project_root` was not discovered, as long as the file lives under CWD.
// This mirrors how rumdl is invoked from CI runners, language servers,
// and editors that pass absolute paths through the API.)
// ==========================================

/// Create an absolute file path inside the given temp dir by creating
/// the parent directories and an empty file at `rel`, then canonicalizing.
fn make_file(temp: &tempfile::TempDir, rel: &str) -> std::path::PathBuf {
    let abs = temp.path().join(rel);
    fs::create_dir_all(abs.parent().unwrap()).unwrap();
    fs::write(&abs, "").unwrap();
    abs.canonicalize().unwrap()
}

#[test]
fn test_normalize_match_path_uses_project_root() {
    // Happy path: project_root is set, file is under it. Result is the
    // path relative to project_root, regardless of where cwd points.
    let temp = tempdir().unwrap();
    let cwd = tempdir().unwrap(); // unrelated cwd
    let file = make_file(&temp, "docs/guide.md");
    let root = temp.path().canonicalize().unwrap();

    let result = super::types::normalize_match_path(&file, Some(&root), Some(cwd.path()));
    assert_eq!(result.as_ref(), std::path::Path::new("docs/guide.md"));
}

#[test]
fn test_normalize_match_path_falls_back_to_cwd_when_project_root_none() {
    // The actual fix: when project_root is None but the file is under cwd,
    // the result must be the path relative to cwd.
    let temp = tempdir().unwrap();
    let file = make_file(&temp, "docs/guide.md");
    let cwd = temp.path().canonicalize().unwrap();

    let result = super::types::normalize_match_path(&file, None, Some(&cwd));
    assert_eq!(result.as_ref(), std::path::Path::new("docs/guide.md"));
}

#[test]
fn test_normalize_match_path_falls_back_to_cwd_when_project_root_unrelated() {
    // When project_root is set but the file lives outside it (e.g. when the
    // user invokes rumdl on a file outside the configured project), fall back
    // to cwd-relative matching rather than blindly using the raw absolute path.
    let temp = tempdir().unwrap();
    let elsewhere = tempdir().unwrap();
    let file = make_file(&temp, "docs/guide.md");
    let cwd = temp.path().canonicalize().unwrap();
    let unrelated_root = elsewhere.path().canonicalize().unwrap();

    let result = super::types::normalize_match_path(&file, Some(&unrelated_root), Some(&cwd));
    assert_eq!(result.as_ref(), std::path::Path::new("docs/guide.md"));
}

#[test]
fn test_normalize_match_path_relative_path_passthrough() {
    // A relative path needs no normalization regardless of project_root or cwd.
    let temp = tempdir().unwrap();
    let result = super::types::normalize_match_path(
        std::path::Path::new("docs/guide.md"),
        Some(temp.path()),
        Some(temp.path()),
    );
    assert_eq!(result.as_ref(), std::path::Path::new("docs/guide.md"));
}

#[test]
fn test_normalize_match_path_nonexistent_file_passthrough() {
    // Editor/LSP buffers may reference a path that does not exist on disk yet,
    // so canonicalize() will fail. Such relative paths must still be matchable.
    let result = super::types::normalize_match_path(std::path::Path::new("docs/draft.md"), None, None);
    assert_eq!(result.as_ref(), std::path::Path::new("docs/draft.md"));
}

#[test]
fn test_normalize_match_path_outside_cwd_returns_raw_path() {
    // Path is absolute and lives nowhere we can map to relative form.
    // Returning the raw path is the safe fallback — a relative glob pattern
    // simply won't match it, which is the desired behavior.
    let outside = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let file = make_file(&outside, "docs/elsewhere.md");
    let cwd_path = cwd.path().canonicalize().unwrap();

    let result = super::types::normalize_match_path(&file, None, Some(&cwd_path));
    assert_eq!(result.as_ref(), file.as_path());
}

#[test]
fn test_normalize_match_path_silent_fallback_when_project_root_and_cwd_both_unrelated() {
    // Comprehensive silent-fallback case: file lives outside BOTH project_root
    // and cwd. The function must return the raw absolute path so the
    // downstream glob simply doesn't match — never panic, never short-circuit
    // to a wrong relative form.
    let project = tempdir().unwrap();
    let working = tempdir().unwrap();
    let elsewhere = tempdir().unwrap();
    let file = make_file(&elsewhere, "docs/orphan.md");
    let project_root = project.path().canonicalize().unwrap();
    let cwd = working.path().canonicalize().unwrap();

    let result = super::types::normalize_match_path(&file, Some(&project_root), Some(&cwd));
    assert_eq!(
        result.as_ref(),
        file.as_path(),
        "silent fallback must return the raw absolute path verbatim",
    );
}

#[test]
fn test_per_file_flavor_matches_absolute_path_with_project_root_only_no_cwd() {
    // End-to-end: the public API must wire normalize_match_path correctly so
    // that an absolute path under project_root resolves to the override flavor.
    let temp = tempdir().unwrap();
    let file = make_file(&temp, "docs/guide.md");

    let mut per_file_flavor = indexmap::IndexMap::new();
    per_file_flavor.insert("docs/**/*.md".to_string(), MarkdownFlavor::MkDocs);
    let config = Config {
        per_file_flavor,
        project_root: Some(temp.path().canonicalize().unwrap()),
        ..Default::default()
    };

    let flavor = config.get_flavor_for_file(&file);
    assert_eq!(flavor, MarkdownFlavor::MkDocs);
}

#[test]
#[serial_test::serial]
fn test_per_file_flavor_matches_absolute_path_with_cwd_fallback() {
    // End-to-end: when project_root is None, an absolute path under cwd must
    // resolve via the cwd fallback path. This is the scenario from #591.
    // Mutates global cwd, so #[serial_test::serial] guards parallel races.
    let temp = tempdir().unwrap();
    let file = make_file(&temp, "docs/guide.md");
    let cwd = temp.path().canonicalize().unwrap();

    let mut per_file_flavor = indexmap::IndexMap::new();
    per_file_flavor.insert("docs/**/*.md".to_string(), MarkdownFlavor::MkDocs);
    let config = Config {
        per_file_flavor,
        project_root: None,
        ..Default::default()
    };

    let prev_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&cwd).unwrap();
    let result = std::panic::catch_unwind(|| config.get_flavor_for_file(&file));
    std::env::set_current_dir(&prev_cwd).unwrap();
    let flavor = result.unwrap();

    assert_eq!(flavor, MarkdownFlavor::MkDocs);
}

#[test]
#[serial_test::serial]
fn test_per_file_ignores_matches_absolute_path_with_cwd_fallback() {
    // Sibling end-to-end test for the per-file-ignores path, which uses the
    // same normalize_match_path helper. An absolute file path under cwd must
    // resolve the rule list correctly when project_root is None.
    use std::collections::HashMap;

    let temp = tempdir().unwrap();
    let file = make_file(&temp, "docs/guide.md");
    let cwd = temp.path().canonicalize().unwrap();

    let mut per_file_ignores = HashMap::new();
    per_file_ignores.insert("docs/**/*.md".to_string(), vec!["MD013".to_string()]);
    let config = Config {
        per_file_ignores,
        project_root: None,
        ..Default::default()
    };

    let prev_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&cwd).unwrap();
    let result = std::panic::catch_unwind(|| config.get_ignored_rules_for_file(&file));
    std::env::set_current_dir(&prev_cwd).unwrap();
    let ignored = result.unwrap();

    assert!(
        ignored.contains("MD013"),
        "MD013 should be ignored for docs/guide.md via cwd fallback. Got: {ignored:?}",
    );
}

#[test]
fn test_normalize_match_path_globset_round_trip() {
    // The full pipeline (normalize → globset match) must produce a relative
    // path that a forward-slash glob can match. On Windows this additionally
    // exercises UNC-prefix stripping (canonicalize() returns `\\?\C:\...`)
    // and backslash → forward-slash normalization in globset; on Unix the
    // canonical path is already free of UNC and uses forward slashes.
    let temp = tempdir().unwrap();
    let file = make_file(&temp, "docs/guide.md");
    let root = temp.path().canonicalize().unwrap();

    let result = super::types::normalize_match_path(&file, Some(&root), None);
    assert!(result.is_relative(), "expected relative path, got {result:?}");

    let glob = globset::GlobBuilder::new("docs/**/*.md")
        .literal_separator(true)
        .build()
        .unwrap()
        .compile_matcher();
    assert!(
        glob.is_match(result.as_ref()),
        "globset must match {result:?} against `docs/**/*.md`",
    );
}

#[test]
fn test_canonical_project_root_cache_returns_stable_reference() {
    // The cache contract is: subsequent calls return a borrow of the same
    // stored `PathBuf`, not a freshly canonicalized one. We verify this
    // structurally by comparing pointers — deterministic, no timing.
    let temp = tempdir().unwrap();
    let config = Config {
        project_root: Some(temp.path().to_path_buf()),
        ..Config::default()
    };

    let first: *const std::path::Path = config.canonical_project_root().expect("project_root canonicalizes");
    let second: *const std::path::Path = config.canonical_project_root().expect("cache hit");

    assert!(
        std::ptr::eq(first, second),
        "cached lookup must return a borrow of the stored PathBuf, not a fresh canonicalization",
    );
}

#[test]
fn test_first_call_warn_else_debug_returns_warn_then_debug() {
    use std::sync::OnceLock;

    let latch: OnceLock<()> = OnceLock::new();

    assert_eq!(
        super::types::first_call_warn_else_debug(&latch),
        log::Level::Warn,
        "first call must surface at warn level",
    );
    assert_eq!(
        super::types::first_call_warn_else_debug(&latch),
        log::Level::Debug,
        "second call must downgrade to debug",
    );
    assert_eq!(
        super::types::first_call_warn_else_debug(&latch),
        log::Level::Debug,
        "subsequent calls remain at debug",
    );
}

#[test]
fn test_format_silent_fallback_message_renders_paths_with_display_formatting() {
    // Paths must appear via Display (not Debug) so the diagnostic doesn't
    // contain stray quote characters that came from Debug's PathBuf impl.
    use std::path::PathBuf;

    let file = PathBuf::from("/elsewhere/notes/draft.md");
    let root = PathBuf::from("/projects/myrepo");
    let cwd = PathBuf::from("/tmp/build");

    let msg = super::types::format_silent_fallback_message(&file, Some(&root), Some(&cwd));

    assert_eq!(
        msg,
        "Per-file glob patterns will not match /elsewhere/notes/draft.md: \
         file is outside project_root (/projects/myrepo) and cwd (/tmp/build)",
        "exact message format is part of the diagnostic contract; got: {msg}",
    );
    assert!(
        !msg.contains('"'),
        "Display formatting must not emit Debug-style quotes; got: {msg}",
    );
}

#[test]
fn test_format_silent_fallback_message_renders_unset_root_and_cwd_explicitly() {
    // When project_root and/or cwd are unavailable the diagnostic must
    // surface that explicitly as "(unset)" rather than leaking Rust's
    // `None` Debug representation.
    use std::path::PathBuf;

    let file = PathBuf::from("/anywhere/file.md");
    let msg = super::types::format_silent_fallback_message(&file, None, None);

    assert_eq!(
        msg,
        "Per-file glob patterns will not match /anywhere/file.md: \
         file is outside project_root (<unset>) and cwd (<unset>)",
    );
    assert!(
        !msg.contains("None"),
        "must not surface Rust's Debug `None`; got: {msg}"
    );
}

#[test]
fn test_format_silent_fallback_message_renders_partial_unset() {
    // Mixed Some/None must render each independently — the placeholder
    // should appear only where it's actually unset.
    use std::path::PathBuf;

    let file = PathBuf::from("/file.md");
    let root = PathBuf::from("/projects/repo");

    let only_root = super::types::format_silent_fallback_message(&file, Some(&root), None);
    assert!(only_root.contains("project_root (/projects/repo)"), "got: {only_root}");
    assert!(only_root.contains("cwd (<unset>)"), "got: {only_root}");

    let only_cwd = super::types::format_silent_fallback_message(&file, None, Some(&root));
    assert!(only_cwd.contains("project_root (<unset>)"), "got: {only_cwd}");
    assert!(only_cwd.contains("cwd (/projects/repo)"), "got: {only_cwd}");
}

#[test]
fn test_first_call_warn_else_debug_independent_latches() {
    // Each latch tracks its own first-call state. A fresh latch must not
    // be influenced by a different latch having already been set.
    use std::sync::OnceLock;

    let latch_a: OnceLock<()> = OnceLock::new();
    let latch_b: OnceLock<()> = OnceLock::new();

    assert_eq!(super::types::first_call_warn_else_debug(&latch_a), log::Level::Warn);
    assert_eq!(super::types::first_call_warn_else_debug(&latch_a), log::Level::Debug);

    assert_eq!(
        super::types::first_call_warn_else_debug(&latch_b),
        log::Level::Warn,
        "latch_b is independent of latch_a",
    );
}

#[test]
fn test_canonical_project_root_cache_shared_across_clones() {
    // `Config: Clone` and the cache is wrapped in `Arc<OnceLock<_>>` so a
    // value computed by any clone is observable to all. Verify that:
    // a clone created BEFORE the cache is populated still sees the cached
    // value once the original populates it.
    let temp = tempdir().unwrap();
    let root = temp.path().to_path_buf();

    let original = Config {
        project_root: Some(root.clone()),
        ..Config::default()
    };
    let clone_before_init = original.clone();

    // Populate via the original.
    let canonical = original
        .canonical_project_root()
        .expect("project_root canonicalizes")
        .to_path_buf();

    // The pre-init clone must observe the same cached value without
    // re-canonicalizing, because both share the same `Arc<OnceLock<_>>`.
    let observed = clone_before_init
        .canonical_project_root()
        .expect("clone observes cached value");
    assert_eq!(observed, canonical.as_path());
}

#[test]
fn test_generate_json_schema() {
    use schemars::schema_for;
    use std::env;

    let schema = schema_for!(Config);
    let schema_json = serde_json::to_string_pretty(&schema).expect("Failed to serialize schema");

    // Write schema to file if RUMDL_UPDATE_SCHEMA env var is set
    if env::var("RUMDL_UPDATE_SCHEMA").is_ok() {
        let schema_path = env::current_dir().unwrap().join("rumdl.schema.json");
        fs::write(&schema_path, &schema_json).expect("Failed to write schema file");
        println!("Schema written to: {}", schema_path.display());
    }

    // Basic validation that schema was generated
    assert!(schema_json.contains("\"title\": \"Config\""));
    assert!(schema_json.contains("\"global\""));
    assert!(schema_json.contains("\"per-file-ignores\""));
}

#[test]
fn test_markdown_flavor_schema_matches_fromstr() {
    // Extract enum values from the actual generated schema
    // This ensures the test stays in sync with the schema automatically
    use schemars::schema_for;

    let schema = schema_for!(MarkdownFlavor);
    let schema_json = serde_json::to_value(&schema).expect("Failed to serialize schema");

    // Extract enum values from schema
    let enum_values = schema_json
        .get("enum")
        .expect("Schema should have 'enum' field")
        .as_array()
        .expect("enum should be an array");

    assert!(!enum_values.is_empty(), "Schema enum should not be empty");

    // Verify all schema enum values are parseable by FromStr
    for value in enum_values {
        let str_value = value.as_str().expect("enum value should be a string");
        let result = str_value.parse::<MarkdownFlavor>();
        assert!(
            result.is_ok(),
            "Schema value '{str_value}' should be parseable by FromStr but got: {:?}",
            result.err()
        );
    }

    // Also verify the aliases in FromStr that aren't in schema (empty string, none)
    for alias in ["", "none"] {
        let result = alias.parse::<MarkdownFlavor>();
        assert!(result.is_ok(), "FromStr alias '{alias}' should be parseable");
    }
}

#[test]
fn test_project_config_is_standalone() {
    // Ruff model: Project config is standalone, user config is NOT merged
    // This ensures reproducibility across machines and CI/local consistency
    let temp_dir = tempdir().unwrap();

    // Create a fake user config directory
    // Note: user_configuration_path_impl adds /rumdl to the config dir
    let user_config_dir = temp_dir.path().join("user_config");
    let rumdl_config_dir = user_config_dir.join("rumdl");
    fs::create_dir_all(&rumdl_config_dir).unwrap();
    let user_config_path = rumdl_config_dir.join("rumdl.toml");

    // User config disables MD013 and MD041
    let user_config_content = r#"
[global]
disable = ["MD013", "MD041"]
line-length = 100
"#;
    fs::write(&user_config_path, user_config_content).unwrap();

    // Create a project config that enables MD001
    let project_config_path = temp_dir.path().join("project").join("pyproject.toml");
    fs::create_dir_all(project_config_path.parent().unwrap()).unwrap();
    let project_config_content = r#"
[tool.rumdl]
enable = ["MD001"]
"#;
    fs::write(&project_config_path, project_config_content).unwrap();

    // Load config with explicit project path, passing user_config_dir
    let sourced = SourcedConfig::load_with_discovery_impl(
        Some(project_config_path.to_str().unwrap()),
        None,
        false,
        Some(&user_config_dir),
        None,
    )
    .unwrap();

    let config: Config = sourced.into_validated_unchecked().into();

    // User config settings should NOT be present (Ruff model: project is standalone)
    assert!(
        !config.global.disable.contains(&"MD013".to_string()),
        "User config should NOT be merged with project config"
    );
    assert!(
        !config.global.disable.contains(&"MD041".to_string()),
        "User config should NOT be merged with project config"
    );

    // Project config settings should be applied
    assert!(
        config.global.enable.contains(&"MD001".to_string()),
        "Project config enabled rules should be applied"
    );
}

#[serial_test::serial]
#[test]
fn test_user_config_as_fallback_when_no_project_config() {
    // Ruff model: User config is used as fallback when no project config exists
    use std::env;

    let temp_dir = tempdir().unwrap();
    let original_dir = env::current_dir().unwrap();

    // Create a fake user config directory
    let user_config_dir = temp_dir.path().join("user_config");
    let rumdl_config_dir = user_config_dir.join("rumdl");
    fs::create_dir_all(&rumdl_config_dir).unwrap();
    let user_config_path = rumdl_config_dir.join("rumdl.toml");

    // User config with specific settings
    let user_config_content = r#"
[global]
disable = ["MD013", "MD041"]
line-length = 88
"#;
    fs::write(&user_config_path, user_config_content).unwrap();

    // Create a project directory WITHOUT any config
    let project_dir = temp_dir.path().join("project_no_config");
    fs::create_dir_all(&project_dir).unwrap();

    // Change to project directory
    env::set_current_dir(&project_dir).unwrap();

    // Load config - should use user config as fallback
    let sourced = SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir), None).unwrap();

    let config: Config = sourced.into_validated_unchecked().into();

    // User config should be loaded as fallback
    assert!(
        config.global.disable.contains(&"MD013".to_string()),
        "User config should be loaded as fallback when no project config"
    );
    assert!(
        config.global.disable.contains(&"MD041".to_string()),
        "User config should be loaded as fallback when no project config"
    );
    assert_eq!(
        config.global.line_length.get(),
        88,
        "User config line-length should be loaded as fallback"
    );

    env::set_current_dir(original_dir).unwrap();
}

#[serial_test::serial]
#[test]
fn test_user_config_fallback_supports_extends() {
    // User fallback config should support extends chains
    use std::env;

    let temp_dir = tempdir().unwrap();
    let original_dir = env::current_dir().unwrap();

    // Create a fake user config directory
    let user_config_dir = temp_dir.path().join("user_config");
    let rumdl_config_dir = user_config_dir.join("rumdl");
    fs::create_dir_all(&rumdl_config_dir).unwrap();

    // Base config in user config directory
    let base_config_path = rumdl_config_dir.join("base.toml");
    fs::write(
        &base_config_path,
        r#"
[global]
disable = ["MD013"]
line-length = 92
"#,
    )
    .unwrap();

    // User fallback config extends base config
    let user_config_path = rumdl_config_dir.join("rumdl.toml");
    fs::write(
        &user_config_path,
        r#"extends = "base.toml"

[global]
extend-disable = ["MD033"]
"#,
    )
    .unwrap();

    // Create a project directory WITHOUT any config
    let project_dir = temp_dir.path().join("project_no_config");
    fs::create_dir_all(&project_dir).unwrap();

    // Change to project directory
    env::set_current_dir(&project_dir).unwrap();

    // Load config - should use user config as fallback and resolve extends
    let sourced = SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir), None).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Inherited from base config
    assert!(config.global.disable.contains(&"MD013".to_string()));
    assert_eq!(config.global.line_length.get(), 92);
    // Added by child fallback config
    assert!(config.global.extend_disable.contains(&"MD033".to_string()));

    env::set_current_dir(original_dir).unwrap();
}

#[serial_test::serial]
#[test]
fn test_home_dotfile_used_when_no_xdg_or_project_config() {
    // ~/.rumdl.toml is honored as a final fallback when neither a project config
    // nor a platform user-config file exists.
    use std::env;

    let temp_dir = tempdir().unwrap();
    let original_dir = env::current_dir().unwrap();

    // Fake $HOME containing only a dotfile rumdl config
    let fake_home = temp_dir.path().join("home");
    fs::create_dir_all(&fake_home).unwrap();
    fs::write(
        fake_home.join(".rumdl.toml"),
        r#"
[global]
disable = ["MD041"]
line-length = 77
"#,
    )
    .unwrap();

    // Empty XDG config dir: present but contains no rumdl files
    let user_config_dir = temp_dir.path().join("user_config_empty");
    fs::create_dir_all(user_config_dir.join("rumdl")).unwrap();

    // Project dir without any config
    let project_dir = temp_dir.path().join("project_no_config");
    fs::create_dir_all(&project_dir).unwrap();
    env::set_current_dir(&project_dir).unwrap();

    let sourced =
        SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir), Some(&fake_home)).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    assert!(
        config.global.disable.contains(&"MD041".to_string()),
        "~/.rumdl.toml should be loaded as fallback when no project or XDG config exists"
    );
    assert_eq!(
        config.global.line_length.get(),
        77,
        "line-length from ~/.rumdl.toml should apply"
    );

    env::set_current_dir(original_dir).unwrap();
}

#[serial_test::serial]
#[test]
fn test_home_rumdl_toml_used_when_no_dotfile_present() {
    // ~/rumdl.toml (no leading dot) is also honored, after ~/.rumdl.toml.
    use std::env;

    let temp_dir = tempdir().unwrap();
    let original_dir = env::current_dir().unwrap();

    let fake_home = temp_dir.path().join("home");
    fs::create_dir_all(&fake_home).unwrap();
    fs::write(fake_home.join("rumdl.toml"), "[global]\ndisable = [\"MD013\"]\n").unwrap();

    let user_config_dir = temp_dir.path().join("user_config_empty");
    fs::create_dir_all(user_config_dir.join("rumdl")).unwrap();

    let project_dir = temp_dir.path().join("project_no_config");
    fs::create_dir_all(&project_dir).unwrap();
    env::set_current_dir(&project_dir).unwrap();

    let sourced =
        SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir), Some(&fake_home)).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    assert!(
        config.global.disable.contains(&"MD013".to_string()),
        "~/rumdl.toml should be loaded when ~/.rumdl.toml is absent"
    );

    env::set_current_dir(original_dir).unwrap();
}

#[serial_test::serial]
#[test]
fn test_xdg_config_wins_over_home_dotfile() {
    // When both XDG (~/.config/rumdl/...) and ~/.rumdl.toml exist, XDG wins.
    // This preserves backwards-compatible behavior for users who already configured
    // the platform user-config directory.
    use std::env;

    let temp_dir = tempdir().unwrap();
    let original_dir = env::current_dir().unwrap();

    // XDG config: disables MD013
    let user_config_dir = temp_dir.path().join("user_config");
    let rumdl_config_dir = user_config_dir.join("rumdl");
    fs::create_dir_all(&rumdl_config_dir).unwrap();
    fs::write(rumdl_config_dir.join("rumdl.toml"), "[global]\ndisable = [\"MD013\"]\n").unwrap();

    // Home dotfile: would disable MD041 if used
    let fake_home = temp_dir.path().join("home");
    fs::create_dir_all(&fake_home).unwrap();
    fs::write(fake_home.join(".rumdl.toml"), "[global]\ndisable = [\"MD041\"]\n").unwrap();

    let project_dir = temp_dir.path().join("project_no_config");
    fs::create_dir_all(&project_dir).unwrap();
    env::set_current_dir(&project_dir).unwrap();

    let sourced =
        SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir), Some(&fake_home)).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    assert!(
        config.global.disable.contains(&"MD013".to_string()),
        "XDG config should take precedence over ~/.rumdl.toml"
    );
    assert!(
        !config.global.disable.contains(&"MD041".to_string()),
        "Home dotfile should be ignored when XDG config exists"
    );

    env::set_current_dir(original_dir).unwrap();
}

#[serial_test::serial]
#[test]
fn test_project_config_wins_over_home_dotfile() {
    // Project config is standalone -- a home dotfile must not bleed in.
    use std::env;

    let temp_dir = tempdir().unwrap();
    let original_dir = env::current_dir().unwrap();

    let fake_home = temp_dir.path().join("home");
    fs::create_dir_all(&fake_home).unwrap();
    fs::write(
        fake_home.join(".rumdl.toml"),
        "[global]\ndisable = [\"MD041\"]\nline-length = 77\n",
    )
    .unwrap();

    // Empty XDG dir
    let user_config_dir = temp_dir.path().join("user_config_empty");
    fs::create_dir_all(user_config_dir.join("rumdl")).unwrap();

    // Project with its own config
    let project_dir = temp_dir.path().join("project_with_config");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(
        project_dir.join(".rumdl.toml"),
        "[global]\nenable = [\"MD001\"]\nline-length = 120\n",
    )
    .unwrap();
    env::set_current_dir(&project_dir).unwrap();

    let sourced =
        SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir), Some(&fake_home)).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    assert!(
        config.global.enable.contains(&"MD001".to_string()),
        "Project config should be loaded"
    );
    assert!(
        !config.global.disable.contains(&"MD041".to_string()),
        "Home dotfile must NOT bleed into project (project is standalone)"
    );
    assert_eq!(
        config.global.line_length.get(),
        120,
        "Project line-length wins, not home dotfile's"
    );

    env::set_current_dir(original_dir).unwrap();
}

#[serial_test::serial]
#[test]
fn test_home_dotfile_supports_extends() {
    // ~/.rumdl.toml must support `extends` chains -- a relative `extends` resolves
    // against the dotfile's parent directory ($HOME), and the inherited config is
    // merged with UserConfig precedence just like the XDG path.
    use std::env;

    let temp_dir = tempdir().unwrap();
    let original_dir = env::current_dir().unwrap();

    // Fake $HOME with a base config and a child that extends it
    let fake_home = temp_dir.path().join("home");
    fs::create_dir_all(&fake_home).unwrap();
    fs::write(
        fake_home.join("base.toml"),
        r#"
[global]
disable = ["MD013"]
line-length = 92
"#,
    )
    .unwrap();
    fs::write(
        fake_home.join(".rumdl.toml"),
        r#"extends = "base.toml"

[global]
extend-disable = ["MD033"]
"#,
    )
    .unwrap();

    let user_config_dir = temp_dir.path().join("user_config_empty");
    fs::create_dir_all(user_config_dir.join("rumdl")).unwrap();

    let project_dir = temp_dir.path().join("project_no_config");
    fs::create_dir_all(&project_dir).unwrap();
    env::set_current_dir(&project_dir).unwrap();

    let sourced =
        SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir), Some(&fake_home)).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Inherited from base.toml via extends
    assert!(
        config.global.disable.contains(&"MD013".to_string()),
        "extends from ~/.rumdl.toml should pull base.toml's disable list"
    );
    assert_eq!(
        config.global.line_length.get(),
        92,
        "extends from ~/.rumdl.toml should pull base.toml's line-length"
    );
    // Added by the home dotfile itself
    assert!(
        config.global.extend_disable.contains(&"MD033".to_string()),
        "child fragment in ~/.rumdl.toml should still apply on top of extends"
    );

    env::set_current_dir(original_dir).unwrap();
}

#[serial_test::serial]
#[test]
fn test_home_dotfile_picked_up_over_rumdl_toml() {
    // ~/.rumdl.toml takes precedence over ~/rumdl.toml when both exist.
    use std::env;

    let temp_dir = tempdir().unwrap();
    let original_dir = env::current_dir().unwrap();

    let fake_home = temp_dir.path().join("home");
    fs::create_dir_all(&fake_home).unwrap();
    fs::write(fake_home.join(".rumdl.toml"), "[global]\ndisable = [\"MD013\"]\n").unwrap();
    fs::write(fake_home.join("rumdl.toml"), "[global]\ndisable = [\"MD041\"]\n").unwrap();

    let user_config_dir = temp_dir.path().join("user_config_empty");
    fs::create_dir_all(user_config_dir.join("rumdl")).unwrap();

    let project_dir = temp_dir.path().join("project_no_config");
    fs::create_dir_all(&project_dir).unwrap();
    env::set_current_dir(&project_dir).unwrap();

    let sourced =
        SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir), Some(&fake_home)).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    assert!(
        config.global.disable.contains(&"MD013".to_string()),
        ".rumdl.toml (dotfile) should win over rumdl.toml in $HOME"
    );
    assert!(
        !config.global.disable.contains(&"MD041".to_string()),
        "rumdl.toml in $HOME should be ignored when .rumdl.toml is present"
    );

    env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_typestate_validate_method() {
    use tempfile::tempdir;

    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    // Create config with an unknown rule option to trigger a validation warning
    let config_content = r#"
[global]
enable = ["MD001"]

[MD013]
line_length = 80
unknown_option = true
"#;
    std::fs::write(&config_path, config_content).expect("Failed to write config");

    // Load config - this returns SourcedConfig<ConfigLoaded>
    let loaded = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
        .expect("Should load config");

    // Create a rule registry for validation
    let default_config = Config::default();
    let all_rules = crate::rules::all_rules(&default_config);
    let registry = RuleRegistry::from_rules(&all_rules);

    // Validate - this transitions to SourcedConfig<ConfigValidated>
    let validated = loaded.validate(&registry).expect("Should validate config");

    // Check that validation warnings were captured for the unknown option
    // Note: The validation checks rule options against the rule's schema
    let has_unknown_option_warning = validated
        .validation_warnings
        .iter()
        .any(|w| w.message.contains("unknown_option") || w.message.contains("Unknown option"));

    // Print warnings for debugging if assertion fails
    if !has_unknown_option_warning {
        for w in &validated.validation_warnings {
            eprintln!("Warning: {}", w.message);
        }
    }
    assert!(
        has_unknown_option_warning,
        "Should have warning for unknown option. Got {} warnings: {:?}",
        validated.validation_warnings.len(),
        validated
            .validation_warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>()
    );

    // Now we can convert to Config (this would be a compile error with ConfigLoaded)
    let config: Config = validated.into();

    // Verify the config values are correct
    assert!(config.global.enable.contains(&"MD001".to_string()));
}

#[test]
fn test_typestate_validate_into_convenience_method() {
    use tempfile::tempdir;

    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let config_path = temp_dir.path().join("test.toml");

    let config_content = r#"
[global]
enable = ["MD022"]

[MD022]
lines_above = 2
"#;
    std::fs::write(&config_path, config_content).expect("Failed to write config");

    let loaded = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
        .expect("Should load config");

    let default_config = Config::default();
    let all_rules = crate::rules::all_rules(&default_config);
    let registry = RuleRegistry::from_rules(&all_rules);

    // Use the convenience method that validates and converts in one step
    let (config, warnings) = loaded.validate_into(&registry).expect("Should validate and convert");

    // Should have no warnings for valid config
    assert!(warnings.is_empty(), "Should have no warnings for valid config");

    // Config should be usable
    assert!(config.global.enable.contains(&"MD022".to_string()));
}

#[test]
fn test_resolve_rule_name_canonical() {
    // Canonical IDs should resolve to themselves
    assert_eq!(resolve_rule_name("MD001"), "MD001");
    assert_eq!(resolve_rule_name("MD013"), "MD013");
    assert_eq!(resolve_rule_name("MD069"), "MD069");
}

#[test]
fn test_resolve_rule_name_aliases() {
    // Aliases should resolve to canonical IDs
    assert_eq!(resolve_rule_name("heading-increment"), "MD001");
    assert_eq!(resolve_rule_name("line-length"), "MD013");
    assert_eq!(resolve_rule_name("no-bare-urls"), "MD034");
    assert_eq!(resolve_rule_name("ul-style"), "MD004");
}

#[test]
fn test_resolve_rule_name_case_insensitive() {
    // Case should not matter
    assert_eq!(resolve_rule_name("HEADING-INCREMENT"), "MD001");
    assert_eq!(resolve_rule_name("Heading-Increment"), "MD001");
    assert_eq!(resolve_rule_name("md001"), "MD001");
    assert_eq!(resolve_rule_name("MD001"), "MD001");
}

#[test]
fn test_resolve_rule_name_underscore_to_hyphen() {
    // Underscores should be converted to hyphens
    assert_eq!(resolve_rule_name("heading_increment"), "MD001");
    assert_eq!(resolve_rule_name("line_length"), "MD013");
    assert_eq!(resolve_rule_name("no_bare_urls"), "MD034");
}

#[test]
fn test_resolve_rule_name_unknown() {
    // Unknown names should fall back to normalization
    assert_eq!(resolve_rule_name("custom-rule"), "custom-rule");
    assert_eq!(resolve_rule_name("CUSTOM_RULE"), "custom-rule");
    assert_eq!(resolve_rule_name("md999"), "MD999"); // Looks like an MD rule
}

#[test]
fn test_resolve_rule_names_basic() {
    let result = resolve_rule_names("MD001,line-length,heading-increment");
    assert!(result.contains("MD001"));
    assert!(result.contains("MD013")); // line-length
    // Note: heading-increment also resolves to MD001, so set should contain MD001 and MD013
    assert_eq!(result.len(), 2);
}

#[test]
fn test_resolve_rule_names_with_whitespace() {
    let result = resolve_rule_names("  MD001 , line-length , MD034  ");
    assert!(result.contains("MD001"));
    assert!(result.contains("MD013"));
    assert!(result.contains("MD034"));
    assert_eq!(result.len(), 3);
}

#[test]
fn test_resolve_rule_names_empty_entries() {
    let result = resolve_rule_names("MD001,,MD013,");
    assert!(result.contains("MD001"));
    assert!(result.contains("MD013"));
    assert_eq!(result.len(), 2);
}

#[test]
fn test_resolve_rule_names_empty_string() {
    let result = resolve_rule_names("");
    assert!(result.is_empty());
}

#[test]
fn test_resolve_rule_names_mixed() {
    // Mix of canonical IDs, aliases, and unknown
    let result = resolve_rule_names("MD001,line-length,custom-rule");
    assert!(result.contains("MD001"));
    assert!(result.contains("MD013"));
    assert!(result.contains("custom-rule"));
    assert_eq!(result.len(), 3);
}

// =========================================================================
// Unit tests for is_valid_rule_name() and validate_cli_rule_names()
// =========================================================================

#[test]
fn test_is_valid_rule_name_canonical() {
    // Valid canonical rule IDs
    assert!(is_valid_rule_name("MD001"));
    assert!(is_valid_rule_name("MD013"));
    assert!(is_valid_rule_name("MD041"));
    assert!(is_valid_rule_name("MD069"));

    // Case insensitive
    assert!(is_valid_rule_name("md001"));
    assert!(is_valid_rule_name("Md001"));
    assert!(is_valid_rule_name("mD001"));
}

#[test]
fn test_is_valid_rule_name_aliases() {
    // Valid aliases
    assert!(is_valid_rule_name("line-length"));
    assert!(is_valid_rule_name("heading-increment"));
    assert!(is_valid_rule_name("no-bare-urls"));
    assert!(is_valid_rule_name("ul-style"));

    // Case insensitive
    assert!(is_valid_rule_name("LINE-LENGTH"));
    assert!(is_valid_rule_name("Line-Length"));

    // Underscore variant
    assert!(is_valid_rule_name("line_length"));
    assert!(is_valid_rule_name("ul_style"));
}

#[test]
fn test_is_valid_rule_name_special_all() {
    assert!(is_valid_rule_name("all"));
    assert!(is_valid_rule_name("ALL"));
    assert!(is_valid_rule_name("All"));
    assert!(is_valid_rule_name("aLl"));
}

#[test]
fn test_is_valid_rule_name_invalid() {
    // Non-existent rules
    assert!(!is_valid_rule_name("MD000"));
    assert!(!is_valid_rule_name("MD002")); // gap in numbering
    assert!(!is_valid_rule_name("MD006")); // gap in numbering
    assert!(!is_valid_rule_name("MD999"));
    assert!(!is_valid_rule_name("MD100"));

    // Invalid formats
    assert!(!is_valid_rule_name(""));
    assert!(!is_valid_rule_name("INVALID"));
    assert!(!is_valid_rule_name("not-a-rule"));
    assert!(!is_valid_rule_name("random-text"));
    assert!(!is_valid_rule_name("abc"));

    // Edge cases
    assert!(!is_valid_rule_name("MD"));
    assert!(!is_valid_rule_name("MD1"));
    assert!(!is_valid_rule_name("MD12"));
}

#[test]
fn test_validate_cli_rule_names_valid() {
    // All valid - should return no warnings
    let warnings = validate_cli_rule_names(
        Some("MD001,MD013"),
        Some("line-length"),
        Some("heading-increment"),
        Some("all"),
        None,
        None,
    );
    assert!(warnings.is_empty(), "Expected no warnings for valid rules");
}

#[test]
fn test_validate_cli_rule_names_invalid() {
    // Invalid rule in --enable
    let warnings = validate_cli_rule_names(Some("abc"), None, None, None, None, None);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("Unknown rule in --enable: abc"));

    // Invalid rule in --disable
    let warnings = validate_cli_rule_names(None, Some("xyz"), None, None, None, None);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("Unknown rule in --disable: xyz"));

    // Invalid rule in --extend-enable
    let warnings = validate_cli_rule_names(None, None, Some("nonexistent"), None, None, None);
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0]
            .message
            .contains("Unknown rule in --extend-enable: nonexistent")
    );

    // Invalid rule in --extend-disable
    let warnings = validate_cli_rule_names(None, None, None, Some("fake-rule"), None, None);
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0]
            .message
            .contains("Unknown rule in --extend-disable: fake-rule")
    );

    // Invalid rule in --fixable
    let warnings = validate_cli_rule_names(None, None, None, None, Some("not-a-rule"), None);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("Unknown rule in --fixable: not-a-rule"));

    // Invalid rule in --unfixable
    let warnings = validate_cli_rule_names(None, None, None, None, None, Some("bogus"));
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("Unknown rule in --unfixable: bogus"));
}

#[test]
fn test_validate_cli_rule_names_mixed() {
    // Mix of valid and invalid
    let warnings = validate_cli_rule_names(Some("MD001,abc,MD003"), None, None, None, None, None);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("abc"));
}

#[test]
fn test_validate_cli_rule_names_suggestions() {
    // Typo should suggest correction
    let warnings = validate_cli_rule_names(Some("line-lenght"), None, None, None, None, None);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("did you mean"));
    assert!(warnings[0].message.contains("line-length"));
}

#[test]
fn test_validate_cli_rule_names_none() {
    // All None - should return no warnings
    let warnings = validate_cli_rule_names(None, None, None, None, None, None);
    assert!(warnings.is_empty());
}

#[test]
fn test_validate_cli_rule_names_empty_string() {
    // Empty strings should produce no warnings
    let warnings = validate_cli_rule_names(Some(""), Some(""), Some(""), Some(""), Some(""), Some(""));
    assert!(warnings.is_empty());
}

#[test]
fn test_validate_cli_rule_names_whitespace() {
    // Whitespace handling
    let warnings = validate_cli_rule_names(Some("  MD001  ,  MD013  "), None, None, None, None, None);
    assert!(warnings.is_empty(), "Whitespace should be trimmed");
}

#[test]
fn test_validate_cli_rule_names_fixable_valid() {
    // Valid fixable and unfixable rules
    let warnings = validate_cli_rule_names(None, None, None, None, Some("MD001,MD013"), Some("MD040"));
    assert!(
        warnings.is_empty(),
        "Expected no warnings for valid fixable/unfixable rules"
    );
}

#[test]
fn test_all_implemented_rules_have_aliases() {
    // This test ensures we don't forget to add aliases when adding new rules.
    // If this test fails, add the missing rule to RULE_ALIAS_MAP in config.rs
    // with both the canonical entry (e.g., "MD071" => "MD071") and an alias
    // (e.g., "BLANK-LINE-AFTER-FRONTMATTER" => "MD071").

    // Get all implemented rules from the rules module
    let config = crate::config::Config::default();
    let all_rules = crate::rules::all_rules(&config);

    let mut missing_rules = Vec::new();
    for rule in &all_rules {
        let rule_name = rule.name();
        // Check if the canonical entry exists in RULE_ALIAS_MAP
        if resolve_rule_name_alias(rule_name).is_none() {
            missing_rules.push(rule_name.to_string());
        }
    }

    assert!(
        missing_rules.is_empty(),
        "The following rules are missing from RULE_ALIAS_MAP: {:?}\n\
             Add entries like:\n\
             - Canonical: \"{}\" => \"{}\"\n\
             - Alias: \"RULE-NAME-HERE\" => \"{}\"",
        missing_rules,
        missing_rules.first().unwrap_or(&"MDxxx".to_string()),
        missing_rules.first().unwrap_or(&"MDxxx".to_string()),
        missing_rules.first().unwrap_or(&"MDxxx".to_string()),
    );
}

// ==================== to_relative_display_path Tests ====================

#[test]
fn test_relative_path_in_cwd() {
    // Create a temp file in the current directory
    let cwd = std::env::current_dir().unwrap();
    let test_path = cwd.join("test_file.md");
    fs::write(&test_path, "test").unwrap();

    let result = super::to_relative_display_path(test_path.to_str().unwrap());

    // Should be relative (just the filename)
    assert_eq!(result, "test_file.md");

    // Cleanup
    fs::remove_file(&test_path).unwrap();
}

#[test]
fn test_relative_path_in_subdirectory() {
    // Create a temp file in a subdirectory
    let cwd = std::env::current_dir().unwrap();
    let subdir = cwd.join("test_subdir_for_relative_path");
    fs::create_dir_all(&subdir).unwrap();
    let test_path = subdir.join("test_file.md");
    fs::write(&test_path, "test").unwrap();

    let result = super::to_relative_display_path(test_path.to_str().unwrap());

    // Should be relative path with subdirectory
    assert_eq!(result, "test_subdir_for_relative_path/test_file.md");

    // Cleanup
    fs::remove_file(&test_path).unwrap();
    fs::remove_dir(&subdir).unwrap();
}

#[test]
fn test_relative_path_outside_cwd_returns_original() {
    // Use a path that's definitely outside CWD (root level)
    let outside_path = "/tmp/definitely_not_in_cwd_test.md";

    let result = super::to_relative_display_path(outside_path);

    // Can't make relative to CWD, should return original
    // (unless CWD happens to be /tmp, which is unlikely in tests)
    let cwd = std::env::current_dir().unwrap();
    if !cwd.starts_with("/tmp") {
        assert_eq!(result, outside_path);
    }
}

#[test]
fn test_relative_path_already_relative() {
    // Already relative path that doesn't exist
    let relative_path = "some/relative/path.md";

    let result = super::to_relative_display_path(relative_path);

    // Should return original since it can't be canonicalized
    assert_eq!(result, relative_path);
}

#[test]
fn test_relative_path_with_dot_components() {
    // Path with . and .. components
    let cwd = std::env::current_dir().unwrap();
    let test_path = cwd.join("test_dot_component.md");
    fs::write(&test_path, "test").unwrap();

    // Create path with redundant ./
    let dotted_path = cwd.join(".").join("test_dot_component.md");
    let result = super::to_relative_display_path(dotted_path.to_str().unwrap());

    // Should resolve to clean relative path
    assert_eq!(result, "test_dot_component.md");

    // Cleanup
    fs::remove_file(&test_path).unwrap();
}

#[test]
fn test_relative_path_empty_string() {
    let result = super::to_relative_display_path("");

    // Empty string should return empty string
    assert_eq!(result, "");
}

// ───── `enable = []` semantics ─────

#[test]
fn test_empty_enable_list_is_explicit_rumdl_toml() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
enable = []
disable = ["MD013"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();

    // enable = [] should be treated as explicitly set (not Default)
    assert_ne!(
        sourced.global.enable.source,
        ConfigSource::Default,
        "Empty enable = [] should change source from Default (it was explicitly set)"
    );

    let config: Config = sourced.into_validated_unchecked().into();

    // enable should be empty and explicit → disables all rules
    assert!(config.global.enable.is_empty());
    assert!(config.global.enable_is_explicit);

    // disable should still be parsed
    assert_eq!(config.global.disable, vec!["MD013".to_string()]);
}

#[test]
fn test_empty_enable_list_is_explicit_pyproject() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("pyproject.toml");
    let config_content = r#"
[tool.rumdl]
enable = []
disable = ["MD033"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();

    // enable = [] should be treated as explicitly set
    assert_ne!(
        sourced.global.enable.source,
        ConfigSource::Default,
        "Empty enable = [] in pyproject.toml should change source from Default"
    );
}

#[test]
fn test_enable_all_keyword_rumdl_toml() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
enable = ["ALL"]
disable = ["MD013"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // enable should contain "ALL"
    assert!(config.global.enable.iter().any(|s| s.eq_ignore_ascii_case("all")));
    // disable should still be parsed
    assert_eq!(config.global.disable, vec!["MD013".to_string()]);
}

#[test]
fn test_enable_all_keyword_pyproject() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("pyproject.toml");
    let config_content = r#"
[tool.rumdl]
enable = ["ALL"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    assert!(config.global.enable.iter().any(|s| s.eq_ignore_ascii_case("all")));
}

#[test]
fn test_nonempty_enable_list_still_works_rumdl_toml() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".rumdl.toml");
    let config_content = r#"
[global]
enable = ["MD001", "MD003"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();

    // Non-empty enable list should change source from Default
    assert_ne!(
        sourced.global.enable.source,
        ConfigSource::Default,
        "Non-empty enable list should override Default source"
    );

    let config: Config = sourced.into_validated_unchecked().into();
    assert_eq!(config.global.enable.len(), 2);
    assert!(config.global.enable.contains(&"MD001".to_string()));
    assert!(config.global.enable.contains(&"MD003".to_string()));
}

#[test]
fn test_nonempty_enable_list_still_works_pyproject() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("pyproject.toml");
    let config_content = r#"
[tool.rumdl]
enable = ["MD001", "MD003"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true).unwrap();

    assert_ne!(
        sourced.global.enable.source,
        ConfigSource::Default,
        "Non-empty enable list in pyproject.toml should override Default source"
    );

    let config: Config = sourced.into_validated_unchecked().into();
    assert_eq!(config.global.enable.len(), 2);
}

// ==================== extends tests ====================

#[test]
fn test_extends_basic_inheritance() {
    // Parent config disables MD013, child extends it without overriding disable
    let temp_dir = tempdir().unwrap();

    let parent_path = temp_dir.path().join("parent.toml");
    fs::write(
        &parent_path,
        r#"
[global]
disable = ["MD013"]
line-length = 120
"#,
    )
    .unwrap();

    let child_path = temp_dir.path().join(".rumdl.toml");
    fs::write(
        &child_path,
        format!(
            r#"extends = "{}"

[global]
extend-disable = ["MD036"]
"#,
            parent_path.display()
        ),
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(child_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Parent's disable should be inherited
    assert!(
        config.global.disable.contains(&"MD013".to_string()),
        "Parent's disable should be inherited"
    );
    // Child's extend-disable should be present
    assert!(
        config.global.extend_disable.contains(&"MD036".to_string()),
        "Child's extend-disable should be present"
    );
    // Parent's line-length should be inherited
    assert_eq!(config.global.line_length.get(), 120);
}

#[test]
fn test_extends_child_overrides_parent() {
    // Child explicitly sets disable, which replaces parent's disable
    let temp_dir = tempdir().unwrap();

    let parent_path = temp_dir.path().join("parent.toml");
    fs::write(
        &parent_path,
        r#"
[global]
disable = ["MD013", "MD033"]
"#,
    )
    .unwrap();

    let child_path = temp_dir.path().join(".rumdl.toml");
    fs::write(
        &child_path,
        format!(
            r#"extends = "{}"

[global]
disable = ["MD041"]
"#,
            parent_path.display()
        ),
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(child_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Child's disable replaces parent's
    assert_eq!(config.global.disable, vec!["MD041".to_string()]);
}

#[test]
fn test_extends_additive_extend_enable() {
    // Both parent and child have extend-enable — values should accumulate
    let temp_dir = tempdir().unwrap();

    let parent_path = temp_dir.path().join("parent.toml");
    fs::write(
        &parent_path,
        r#"
[global]
extend-enable = ["MD060"]
"#,
    )
    .unwrap();

    let child_path = temp_dir.path().join(".rumdl.toml");
    fs::write(
        &child_path,
        format!(
            r#"extends = "{}"

[global]
extend-enable = ["MD063"]
"#,
            parent_path.display()
        ),
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(child_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Both extend-enable values should be present (union semantics)
    assert!(
        config.global.extend_enable.contains(&"MD060".to_string()),
        "Parent's extend-enable should be preserved"
    );
    assert!(
        config.global.extend_enable.contains(&"MD063".to_string()),
        "Child's extend-enable should be added"
    );
}

#[test]
fn test_extends_chain_three_levels() {
    // A extends B extends C — all three contribute settings
    let temp_dir = tempdir().unwrap();

    let grandparent_path = temp_dir.path().join("grandparent.toml");
    fs::write(
        &grandparent_path,
        r#"
[global]
line-length = 80
extend-enable = ["MD060"]
"#,
    )
    .unwrap();

    let parent_path = temp_dir.path().join("parent.toml");
    fs::write(
        &parent_path,
        format!(
            r#"extends = "{}"

[global]
extend-enable = ["MD063"]
"#,
            grandparent_path.display()
        ),
    )
    .unwrap();

    let child_path = temp_dir.path().join(".rumdl.toml");
    fs::write(
        &child_path,
        format!(
            r#"extends = "{}"

[global]
extend-disable = ["MD013"]
"#,
            parent_path.display()
        ),
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(child_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Grandparent's line-length should be inherited through chain
    assert_eq!(config.global.line_length.get(), 80);
    // Both grandparent and parent's extend-enable should accumulate
    assert!(config.global.extend_enable.contains(&"MD060".to_string()));
    assert!(config.global.extend_enable.contains(&"MD063".to_string()));
    // Child's extend-disable
    assert!(config.global.extend_disable.contains(&"MD013".to_string()));
}

#[test]
fn test_extends_circular_detection() {
    // A extends B, B extends A → should error
    let temp_dir = tempdir().unwrap();

    let a_path = temp_dir.path().join("a.toml");
    let b_path = temp_dir.path().join("b.toml");

    fs::write(
        &a_path,
        format!(
            r#"extends = "{}"

[global]
disable = ["MD013"]
"#,
            b_path.display()
        ),
    )
    .unwrap();

    fs::write(
        &b_path,
        format!(
            r#"extends = "{}"

[global]
disable = ["MD033"]
"#,
            a_path.display()
        ),
    )
    .unwrap();

    let result = SourcedConfig::load_with_discovery(Some(a_path.to_str().unwrap()), None, true);
    assert!(result.is_err(), "Circular extends should produce an error");
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("Circular extends") || err_msg.contains("circular"),
        "Error should mention circular: {err_msg}"
    );
}

#[test]
fn test_extends_self_reference() {
    // A extends A → circular error
    let temp_dir = tempdir().unwrap();

    let a_path = temp_dir.path().join("a.toml");
    fs::write(
        &a_path,
        format!(
            r#"extends = "{}"

[global]
disable = ["MD013"]
"#,
            a_path.display()
        ),
    )
    .unwrap();

    let result = SourcedConfig::load_with_discovery(Some(a_path.to_str().unwrap()), None, true);
    assert!(result.is_err(), "Self-referencing extends should produce an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Circular extends") || err_msg.contains("circular"),
        "Error should mention circular: {err_msg}"
    );
}

#[test]
fn test_extends_depth_limit() {
    // Create a chain of 12 configs (exceeds limit of 10)
    let temp_dir = tempdir().unwrap();

    let mut paths = Vec::new();
    for i in 0..12 {
        paths.push(temp_dir.path().join(format!("config_{i}.toml")));
    }

    // Write the leaf config (no extends)
    fs::write(
        &paths[11],
        r#"
[global]
disable = ["MD013"]
"#,
    )
    .unwrap();

    // Write configs 1-10, each extending the next
    for i in (0..11).rev() {
        fs::write(
            &paths[i],
            format!(
                r#"extends = "{}"

[global]
extend-disable = ["MD{:03}"]
"#,
                paths[i + 1].display(),
                i + 1
            ),
        )
        .unwrap();
    }

    let result = SourcedConfig::load_with_discovery(Some(paths[0].to_str().unwrap()), None, true);
    assert!(result.is_err(), "Deep extends chain should produce an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("maximum depth") || err_msg.contains("depth"),
        "Error should mention depth: {err_msg}"
    );
}

#[test]
fn test_extends_relative_path() {
    // Child in subdirectory extends parent using relative path
    let temp_dir = tempdir().unwrap();
    let sub_dir = temp_dir.path().join("subdir");
    fs::create_dir(&sub_dir).unwrap();

    let parent_path = temp_dir.path().join("parent.toml");
    fs::write(
        &parent_path,
        r#"
[global]
disable = ["MD013"]
"#,
    )
    .unwrap();

    let child_path = sub_dir.join(".rumdl.toml");
    fs::write(
        &child_path,
        r#"extends = "../parent.toml"

[global]
extend-disable = ["MD033"]
"#,
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(child_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Parent's disable inherited via relative path
    assert!(config.global.disable.contains(&"MD013".to_string()));
    // Child's extend-disable
    assert!(config.global.extend_disable.contains(&"MD033".to_string()));
}

#[test]
fn test_extends_missing_file() {
    let temp_dir = tempdir().unwrap();

    let child_path = temp_dir.path().join(".rumdl.toml");
    fs::write(
        &child_path,
        r#"extends = "nonexistent.toml"

[global]
disable = ["MD013"]
"#,
    )
    .unwrap();

    let result = SourcedConfig::load_with_discovery(Some(child_path.to_str().unwrap()), None, true);
    assert!(result.is_err(), "Missing extends target should produce an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found") || err_msg.contains("nonexistent"),
        "Error should mention file not found: {err_msg}"
    );
}

#[test]
fn test_extends_pyproject_toml() {
    // pyproject.toml with extends at [tool.rumdl] level
    let temp_dir = tempdir().unwrap();

    let parent_path = temp_dir.path().join("parent.toml");
    fs::write(
        &parent_path,
        r#"
[global]
disable = ["MD013"]
"#,
    )
    .unwrap();

    let child_path = temp_dir.path().join("pyproject.toml");
    fs::write(
        &child_path,
        format!(
            r#"
[tool.rumdl]
extends = "{}"
extend-disable = ["MD033"]
"#,
            parent_path.display()
        ),
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(child_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Parent's disable inherited
    assert!(config.global.disable.contains(&"MD013".to_string()));
    // Child's extend-disable
    assert!(config.global.extend_disable.contains(&"MD033".to_string()));
}

#[test]
fn test_extends_pyproject_child_overrides_rumdl_parent() {
    // pyproject child should override parent replace-fields from extended rumdl config
    let temp_dir = tempdir().unwrap();

    let parent_path = temp_dir.path().join("parent.toml");
    fs::write(
        &parent_path,
        r#"
[global]
disable = ["MD013", "MD033"]
"#,
    )
    .unwrap();

    let child_path = temp_dir.path().join("pyproject.toml");
    fs::write(
        &child_path,
        format!(
            r#"
[tool.rumdl]
extends = "{}"
disable = ["MD041"]
"#,
            parent_path.display()
        ),
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(child_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Child's disable should replace parent's disable
    assert_eq!(config.global.disable, vec!["MD041".to_string()]);
}

#[test]
fn test_extends_rule_specific_override() {
    // Parent sets MD007 indent to 4, child overrides to 2
    let temp_dir = tempdir().unwrap();

    let parent_path = temp_dir.path().join("parent.toml");
    fs::write(
        &parent_path,
        r#"
[MD007]
indent = 4
"#,
    )
    .unwrap();

    let child_path = temp_dir.path().join(".rumdl.toml");
    fs::write(
        &child_path,
        format!(
            r#"extends = "{}"

[MD007]
indent = 2
"#,
            parent_path.display()
        ),
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(child_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Child's rule config should override parent's
    let indent_val = get_rule_config_value::<i64>(&config, "MD007", "indent");
    assert_eq!(indent_val, Some(2), "Child should override parent's MD007 indent");
}

#[test]
fn test_extends_rule_inherited_when_not_overridden() {
    // Parent sets MD007 indent to 4, child does not set MD007 at all
    let temp_dir = tempdir().unwrap();

    let parent_path = temp_dir.path().join("parent.toml");
    fs::write(
        &parent_path,
        r#"
[MD007]
indent = 4
"#,
    )
    .unwrap();

    let child_path = temp_dir.path().join(".rumdl.toml");
    fs::write(
        &child_path,
        format!(
            r#"extends = "{}"

[global]
disable = ["MD013"]
"#,
            parent_path.display()
        ),
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(child_path.to_str().unwrap()), None, true).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    // Parent's rule config should be inherited
    let indent_val = get_rule_config_value::<i64>(&config, "MD007", "indent");
    assert_eq!(indent_val, Some(4), "Parent's MD007 indent should be inherited");
}

#[test]
fn test_extends_loaded_files_tracking() {
    // Verify that both parent and child appear in loaded_files
    let temp_dir = tempdir().unwrap();

    let parent_path = temp_dir.path().join("parent.toml");
    fs::write(
        &parent_path,
        r#"
[global]
disable = ["MD013"]
"#,
    )
    .unwrap();

    let child_path = temp_dir.path().join(".rumdl.toml");
    fs::write(
        &child_path,
        format!(
            r#"extends = "{}"

[global]
extend-disable = ["MD033"]
"#,
            parent_path.display()
        ),
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(child_path.to_str().unwrap()), None, true).unwrap();

    // Both files should appear in loaded_files
    assert!(
        sourced.loaded_files.len() >= 2,
        "Both parent and child should be in loaded_files, got: {:?}",
        sourced.loaded_files
    );
    assert!(
        sourced.loaded_files.iter().any(|f| f.contains("parent.toml")),
        "parent.toml should be in loaded_files"
    );
    assert!(
        sourced.loaded_files.iter().any(|f| f.contains(".rumdl.toml")),
        ".rumdl.toml should be in loaded_files"
    );
}

#[test]
fn test_extends_base_values_propagate_when_child_silent() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("base.toml"), "[global]\ndisable = [\"MD013\"]\n").unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "extends = \"base.toml\"\n").unwrap();

    let sourced = SourcedConfig::load_with_discovery_impl(
        Some(dir.path().join(".rumdl.toml").to_str().unwrap()),
        None,
        true,
        None,
        None,
    )
    .unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    assert_eq!(config.global.disable, vec!["MD013".to_string()]);
}

#[test]
fn test_extends_child_disable_replaces_base() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("base.toml"), "[global]\ndisable = [\"MD013\"]\n").unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "extends = \"base.toml\"\n[global]\ndisable = [\"MD001\"]\n",
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery_impl(
        Some(dir.path().join(".rumdl.toml").to_str().unwrap()),
        None,
        true,
        None,
        None,
    )
    .unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    assert_eq!(config.global.disable, vec!["MD001".to_string()]);
}

#[test]
fn test_extends_three_level_chain_propagates_from_root() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("root.toml"), "[global]\ndisable = [\"MD013\"]\n").unwrap();
    fs::write(dir.path().join("middle.toml"), "extends = \"root.toml\"\n").unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "extends = \"middle.toml\"\n").unwrap();

    let sourced = SourcedConfig::load_with_discovery_impl(
        Some(dir.path().join(".rumdl.toml").to_str().unwrap()),
        None,
        true,
        None,
        None,
    )
    .unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    assert_eq!(config.global.disable, vec!["MD013".to_string()]);
}

#[test]
fn test_extends_rule_config_inherits_from_base() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("base.toml"), "[MD013]\nline-length = 120\n").unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "extends = \"base.toml\"\n").unwrap();

    let sourced = SourcedConfig::load_with_discovery_impl(
        Some(dir.path().join(".rumdl.toml").to_str().unwrap()),
        None,
        true,
        None,
        None,
    )
    .unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    let line_length = get_rule_config_value::<usize>(&config, "MD013", "line-length");
    assert_eq!(line_length, Some(120));
}

#[test]
fn test_extends_child_rule_config_overrides_base() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("base.toml"), "[MD013]\nline-length = 100\n").unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "extends = \"base.toml\"\n[MD013]\nline-length = 160\n",
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery_impl(
        Some(dir.path().join(".rumdl.toml").to_str().unwrap()),
        None,
        true,
        None,
        None,
    )
    .unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    let line_length = get_rule_config_value::<usize>(&config, "MD013", "line-length");
    assert_eq!(line_length, Some(160));
}

#[test]
fn test_extends_enable_wins_over_inherited_disable() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("base.toml"),
        "[global]\ndisable = [\"MD013\", \"MD001\"]\n",
    )
    .unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "extends = \"base.toml\"\n[global]\nenable = [\"MD001\"]\n",
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery_impl(
        Some(dir.path().join(".rumdl.toml").to_str().unwrap()),
        None,
        true,
        None,
        None,
    )
    .unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    assert!(
        !config.global.disable.contains(&"MD001".to_string()),
        "MD001 should not be disabled when explicitly enabled"
    );
    assert!(
        config.global.disable.contains(&"MD013".to_string()),
        "MD013 should still be disabled (only MD001 was re-enabled)"
    );
}

#[test]
fn test_extends_cycle_returns_error() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.toml"), "extends = \"b.toml\"\n").unwrap();
    fs::write(dir.path().join("b.toml"), "extends = \"a.toml\"\n").unwrap();

    let result = SourcedConfig::load_with_discovery_impl(
        Some(dir.path().join("a.toml").to_str().unwrap()),
        None,
        true,
        None,
        None,
    );

    assert!(
        matches!(result, Err(ConfigError::CircularExtends { .. })),
        "Expected CircularExtends error, got: {result:?}"
    );
}

#[test]
fn test_extends_missing_file_returns_error() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "extends = \"nonexistent.toml\"\n").unwrap();

    let result = SourcedConfig::load_with_discovery_impl(
        Some(dir.path().join(".rumdl.toml").to_str().unwrap()),
        None,
        true,
        None,
        None,
    );

    assert!(
        matches!(result, Err(ConfigError::ExtendsNotFound { .. })),
        "Expected ExtendsNotFound error, got: {result:?}"
    );
}

#[test]
fn test_extends_depth_limit_returns_error() {
    let dir = tempdir().unwrap();
    // Build MAX_EXTENDS_DEPTH + 1 levels so the loader hits the depth guard.
    // Mirrors MAX_EXTENDS_DEPTH = 10 from src/config/loading.rs.
    let max_depth: usize = 10;
    fs::write(dir.path().join("level_0.toml"), "[global]\n").unwrap();
    for i in 1..=max_depth {
        fs::write(
            dir.path().join(format!("level_{i}.toml")),
            format!("extends = \"level_{}.toml\"\n", i - 1),
        )
        .unwrap();
    }

    let result = SourcedConfig::load_with_discovery_impl(
        Some(dir.path().join(format!("level_{max_depth}.toml")).to_str().unwrap()),
        None,
        true,
        None,
        None,
    );

    assert!(
        matches!(result, Err(ConfigError::ExtendsDepthExceeded { .. })),
        "Expected ExtendsDepthExceeded error, got: {result:?}"
    );
}

#[serial_test::serial]
#[test]
fn test_user_config_loaded_alongside_markdownlint_config() {
    // When a markdownlint project config is discovered, the user config
    // must also be loaded as a base layer so rumdl-specific settings apply.
    use std::env;

    let temp_dir = tempdir().unwrap();
    let original_dir = env::current_dir().unwrap();

    // User config sets a rumdl-specific setting (flavor) that markdownlint cannot express
    let user_config_dir = temp_dir.path().join("user_config");
    let rumdl_config_dir = user_config_dir.join("rumdl");
    fs::create_dir_all(&rumdl_config_dir).unwrap();
    fs::write(rumdl_config_dir.join("rumdl.toml"), "[global]\nflavor = \"mkdocs\"\n").unwrap();

    // Project directory has a .markdownlint.yaml that disables MD013
    let project_dir = temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join(".markdownlint.yaml"), "MD013: false\n").unwrap();

    env::set_current_dir(&project_dir).unwrap();

    let sourced = SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir), None).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    env::set_current_dir(&original_dir).unwrap();

    // Markdownlint config setting must apply
    assert!(
        config.global.disable.contains(&"MD013".to_string()),
        "Markdownlint config should disable MD013, got disable={:?}",
        config.global.disable
    );

    // User config setting must also apply (rumdl-specific, not expressible in markdownlint format)
    assert_eq!(
        config.global.flavor,
        MarkdownFlavor::MkDocs,
        "User config flavor should be loaded alongside markdownlint project config"
    );
}

#[serial_test::serial]
#[test]
fn test_user_config_settings_apply_when_markdownlint_present() {
    // User config settings that markdownlint does not override must still apply
    // after the fix (user config is loaded as a base layer).
    use std::env;

    let temp_dir = tempdir().unwrap();
    let original_dir = env::current_dir().unwrap();

    // User config sets a non-default line-length
    let user_config_dir = temp_dir.path().join("user_config2");
    let rumdl_config_dir = user_config_dir.join("rumdl");
    fs::create_dir_all(&rumdl_config_dir).unwrap();
    fs::write(rumdl_config_dir.join("rumdl.toml"), "[global]\nline-length = 200\n").unwrap();

    // Project directory has a .markdownlint.yaml that does NOT set line-length
    let project_dir = temp_dir.path().join("project2");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join(".markdownlint.yaml"), "default: true\n").unwrap();

    env::set_current_dir(&project_dir).unwrap();

    let sourced = SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir), None).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    env::set_current_dir(&original_dir).unwrap();

    // Without the fix: user config never loaded → line-length stays at default (80)
    // With the fix: user config loaded → line-length = 200
    assert_eq!(
        config.global.line_length.get(),
        200,
        "User config line-length should apply when markdownlint project config is present"
    );
}

#[serial_test::serial]
#[test]
fn test_markdownlint_config_overrides_user_config_on_conflict() {
    // When user config and markdownlint project config set the same field,
    // the markdownlint config (ProjectConfig, precedence 3) must win over
    // user config (UserConfig, precedence 1) via merge_override.
    //
    // Scenario: user wants MD001 disabled; the project's markdownlint config
    // disables MD013 instead. The project's disable list replaces the user's.
    use std::env;

    let temp_dir = tempdir().unwrap();
    let original_dir = env::current_dir().unwrap();

    let user_config_dir = temp_dir.path().join("user_config3");
    let rumdl_config_dir = user_config_dir.join("rumdl");
    fs::create_dir_all(&rumdl_config_dir).unwrap();
    fs::write(rumdl_config_dir.join("rumdl.toml"), "[global]\ndisable = [\"MD001\"]\n").unwrap();

    // Markdownlint config disables MD013, does not mention MD001
    let project_dir = temp_dir.path().join("project3");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join(".markdownlint.yaml"), "MD013: false\n").unwrap();

    env::set_current_dir(&project_dir).unwrap();

    let sourced = SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir), None).unwrap();
    let config: Config = sourced.into_validated_unchecked().into();

    env::set_current_dir(&original_dir).unwrap();

    // Markdownlint disable list has higher precedence and replaces the user config's list
    assert!(
        config.global.disable.contains(&"MD013".to_string()),
        "Markdownlint config should disable MD013, got disable={:?}",
        config.global.disable
    );
    assert!(
        !config.global.disable.contains(&"MD001".to_string()),
        "Markdownlint config's disable list replaces user config's; MD001 should not be disabled, got disable={:?}",
        config.global.disable
    );
}

#[serial_test::serial]
#[test]
fn test_user_config_applies_when_markdownlint_config_is_malformed() {
    // When the discovered markdownlint config fails to parse, the user config
    // that was already loaded as a base layer must still apply.
    use std::env;

    let temp_dir = tempdir().unwrap();
    let original_dir = env::current_dir().unwrap();

    let user_config_dir = temp_dir.path().join("user_config_malformed");
    let rumdl_config_dir = user_config_dir.join("rumdl");
    fs::create_dir_all(&rumdl_config_dir).unwrap();
    fs::write(rumdl_config_dir.join("rumdl.toml"), "[global]\nflavor = \"obsidian\"\n").unwrap();

    let project_dir = temp_dir.path().join("project_malformed");
    fs::create_dir_all(&project_dir).unwrap();
    // Unclosed YAML mapping — guaranteed parse failure
    fs::write(project_dir.join(".markdownlint.yaml"), "{ not: [valid yaml\n").unwrap();

    env::set_current_dir(&project_dir).unwrap();

    let result = SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir), None);

    env::set_current_dir(&original_dir).unwrap();

    // Load must succeed — a bad markdownlint file is not a fatal error
    let config: Config = result
        .expect("load_with_discovery_impl should succeed even with malformed markdownlint config")
        .into_validated_unchecked()
        .into();

    // User config flavor must still apply because it was loaded before the parse attempt
    assert_eq!(
        config.global.flavor,
        MarkdownFlavor::Obsidian,
        "User config flavor should apply when markdownlint config is malformed"
    );
}
