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
    let sourced = SourcedConfig::load_with_discovery_impl(None, None, false, Some(&user_config_dir)).unwrap();

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
    );
    assert!(warnings.is_empty(), "Expected no warnings for valid rules");
}

#[test]
fn test_validate_cli_rule_names_invalid() {
    // Invalid rule in --enable
    let warnings = validate_cli_rule_names(Some("abc"), None, None, None);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("Unknown rule in --enable: abc"));

    // Invalid rule in --disable
    let warnings = validate_cli_rule_names(None, Some("xyz"), None, None);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("Unknown rule in --disable: xyz"));

    // Invalid rule in --extend-enable
    let warnings = validate_cli_rule_names(None, None, Some("nonexistent"), None);
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0]
            .message
            .contains("Unknown rule in --extend-enable: nonexistent")
    );

    // Invalid rule in --extend-disable
    let warnings = validate_cli_rule_names(None, None, None, Some("fake-rule"));
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0]
            .message
            .contains("Unknown rule in --extend-disable: fake-rule")
    );
}

#[test]
fn test_validate_cli_rule_names_mixed() {
    // Mix of valid and invalid
    let warnings = validate_cli_rule_names(Some("MD001,abc,MD003"), None, None, None);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("abc"));
}

#[test]
fn test_validate_cli_rule_names_suggestions() {
    // Typo should suggest correction
    let warnings = validate_cli_rule_names(Some("line-lenght"), None, None, None);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("did you mean"));
    assert!(warnings[0].message.contains("line-length"));
}

#[test]
fn test_validate_cli_rule_names_none() {
    // All None - should return no warnings
    let warnings = validate_cli_rule_names(None, None, None, None);
    assert!(warnings.is_empty());
}

#[test]
fn test_validate_cli_rule_names_empty_string() {
    // Empty strings should produce no warnings
    let warnings = validate_cli_rule_names(Some(""), Some(""), Some(""), Some(""));
    assert!(warnings.is_empty());
}

#[test]
fn test_validate_cli_rule_names_whitespace() {
    // Whitespace handling
    let warnings = validate_cli_rule_names(Some("  MD001  ,  MD013  "), None, None, None);
    assert!(warnings.is_empty(), "Whitespace should be trimmed");
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
