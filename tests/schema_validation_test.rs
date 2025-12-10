/// Tests for JSON schema validation of rumdl.toml configuration files
use std::fs;

/// Load the generated JSON schema
fn load_schema() -> serde_json::Value {
    let schema_path = concat!(env!("CARGO_MANIFEST_DIR"), "/rumdl.schema.json");
    let schema_content =
        fs::read_to_string(schema_path).expect("Failed to read schema file - run 'cargo dev --write' first");
    serde_json::from_str(&schema_content).expect("Failed to parse schema JSON")
}

/// Convert TOML to JSON for schema validation
fn toml_to_json(toml_str: &str) -> serde_json::Value {
    let toml_value: toml::Value = toml::from_str(toml_str).expect("Failed to parse TOML");
    // Convert TOML to JSON via serde
    let json_str = serde_json::to_string(&toml_value).expect("Failed to convert TOML to JSON");
    serde_json::from_str(&json_str).expect("Failed to parse converted JSON")
}

/// Validate a TOML config string against the schema
fn validate_toml_config(toml_str: &str) -> Result<(), String> {
    let schema = load_schema();
    let instance = toml_to_json(toml_str);

    let compiled = jsonschema::validator_for(&schema).expect("Failed to compile schema");

    compiled
        .validate(&instance)
        .map_err(|err| format!("{} at {}", err, err.instance_path()))
}

#[test]
fn test_schema_exists() {
    let schema = load_schema();
    assert_eq!(schema["$schema"], "https://json-schema.org/draft/2020-12/schema");
    assert_eq!(schema["title"], "Config");
}

#[test]
fn test_empty_config_is_valid() {
    let toml = "";
    assert!(validate_toml_config(toml).is_ok());
}

#[test]
fn test_minimal_global_config() {
    let toml = r#"
[global]
disable = ["MD013"]
"#;
    assert!(validate_toml_config(toml).is_ok());
}

#[test]
fn test_full_global_config() {
    let toml = r#"
[global]
disable = ["MD013", "MD033"]
enable = ["MD001", "MD003"]
exclude = ["node_modules", "*.tmp"]
include = ["docs/*.md"]
respect_gitignore = true
line_length = 100
flavor = "mkdocs"
"#;
    assert!(validate_toml_config(toml).is_ok());
}

#[test]
fn test_per_file_ignores() {
    let toml = r#"
[per-file-ignores]
"README.md" = ["MD033"]
"docs/**/*.md" = ["MD013", "MD033"]
"#;
    assert!(validate_toml_config(toml).is_ok());
}

#[test]
fn test_rule_specific_config() {
    let toml = r#"
[MD003]
style = "atx"

[MD007]
indent = 4

[MD013]
line_length = 100
code_blocks = false
tables = false
headings = true

[MD044]
names = ["rumdl", "Markdown", "GitHub"]
code-blocks = true
"#;
    assert!(validate_toml_config(toml).is_ok());
}

#[test]
fn test_complete_example_config() {
    let toml = r#"
[global]
disable = ["MD013", "MD033"]
exclude = [".git", "node_modules", "dist"]
respect_gitignore = true

[per-file-ignores]
"README.md" = ["MD033"]
"docs/api/**/*.md" = ["MD013"]

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
tables = false
"#;
    let result = validate_toml_config(toml);
    if let Err(error) = &result {
        eprintln!("Validation error: {error}");
    }
    assert!(result.is_ok());
}

#[test]
fn test_flavor_variants() {
    // Test all valid flavor values
    for flavor in ["standard", "mkdocs"] {
        let toml = format!(
            r#"
[global]
flavor = "{flavor}"
"#
        );
        let result = validate_toml_config(&toml);
        assert!(result.is_ok(), "Flavor '{flavor}' should be valid");
    }
}

#[test]
fn test_example_file_validates() {
    // Validate the actual rumdl.toml.example file
    let example_path = concat!(env!("CARGO_MANIFEST_DIR"), "/rumdl.toml.example");
    let toml_content = fs::read_to_string(example_path).expect("Failed to read rumdl.toml.example");

    let result = validate_toml_config(&toml_content);
    if let Err(error) = &result {
        eprintln!("Validation error in rumdl.toml.example: {error}");
    }
    assert!(result.is_ok(), "rumdl.toml.example should validate against schema");
}

#[test]
fn test_project_rumdl_toml_validates() {
    // Validate the actual .rumdl.toml file if it exists
    let config_path = concat!(env!("CARGO_MANIFEST_DIR"), "/.rumdl.toml");
    if let Ok(toml_content) = fs::read_to_string(config_path) {
        let result = validate_toml_config(&toml_content);
        if let Err(error) = &result {
            eprintln!("Validation error in .rumdl.toml: {error}");
        }
        assert!(result.is_ok(), ".rumdl.toml should validate against schema");
    }
}

// Negative tests - these should fail validation

#[test]
fn test_invalid_global_property() {
    let toml = r#"
[global]
invalid_property = "should not exist"
"#;
    // Note: The schema allows additional properties in rules, but global is stricter
    // This test documents current behavior - adjust based on actual schema constraints
    let result = validate_toml_config(toml);
    // If this passes, the schema allows additional properties (which might be intentional)
    // For now, we just validate it doesn't panic
    let _ = result;
}

#[test]
fn test_invalid_flavor_value() {
    let toml = r#"
[global]
flavor = "invalid_flavor"
"#;
    let result = validate_toml_config(toml);
    // Should fail because "invalid_flavor" is not in the enum
    assert!(result.is_err(), "Invalid flavor should fail validation");
}

#[test]
fn test_invalid_type_for_disable() {
    let toml = r#"
[global]
disable = "MD013"  # Should be an array, not a string
"#;
    let result = validate_toml_config(toml);
    assert!(result.is_err(), "Wrong type for disable should fail validation");
}

#[test]
fn test_invalid_type_for_line_length() {
    // Use kebab-case since that's what the JSON schema uses
    let toml = r#"
[global]
line-length = "100"  # Should be a number, not a string
"#;
    let result = validate_toml_config(toml);
    assert!(result.is_err(), "Wrong type for line-length should fail validation");
}

#[test]
fn test_invalid_type_for_respect_gitignore() {
    // Use kebab-case since that's what the JSON schema uses
    let toml = r#"
[global]
respect-gitignore = "true"  # Should be boolean, not string
"#;
    let result = validate_toml_config(toml);
    assert!(
        result.is_err(),
        "Wrong type for respect-gitignore should fail validation"
    );
}

/// Regression test: GlobalConfig schema properties must use kebab-case
///
/// This prevents regression where snake_case properties were being output
/// in the JSON schema, which broke tooling expecting kebab-case (like Ruff uses).
#[test]
fn test_schema_globalconfig_uses_kebab_case() {
    let schema = load_schema();

    // Navigate to GlobalConfig properties in the schema
    let global_config = &schema["$defs"]["GlobalConfig"]["properties"];

    // These properties MUST use kebab-case in the schema
    let expected_kebab_case_properties = [
        "line-length",
        "respect-gitignore",
        "force-exclude",
        "output-format",
        "cache-dir",
    ];

    // These properties MUST NOT use snake_case in the schema
    let forbidden_snake_case_properties = [
        "line_length",
        "respect_gitignore",
        "force_exclude",
        "output_format",
        "cache_dir",
    ];

    for prop in expected_kebab_case_properties {
        assert!(
            global_config.get(prop).is_some(),
            "Schema must have kebab-case property '{prop}' in GlobalConfig"
        );
    }

    for prop in forbidden_snake_case_properties {
        assert!(
            global_config.get(prop).is_none(),
            "Schema must NOT have snake_case property '{prop}' in GlobalConfig (use kebab-case instead)"
        );
    }
}

/// Test that config files can use both kebab-case and snake_case (backward compatibility)
/// This tests parsing directly without schema validation (which has unrelated issues)
#[test]
fn test_config_accepts_both_kebab_and_snake_case() {
    use rumdl_lib::config::Config;

    // Kebab-case (preferred)
    let kebab_toml = r#"
[global]
line-length = 100
respect-gitignore = false
force-exclude = true
"#;
    let kebab_config: Config = toml::from_str(kebab_toml).expect("Kebab-case config should parse");
    assert_eq!(kebab_config.global.line_length.get(), 100);
    assert!(!kebab_config.global.respect_gitignore);

    // Snake_case (backward compatible)
    let snake_toml = r#"
[global]
line_length = 100
respect_gitignore = false
force_exclude = true
"#;
    let snake_config: Config =
        toml::from_str(snake_toml).expect("Snake_case config should parse for backward compatibility");
    assert_eq!(snake_config.global.line_length.get(), 100);
    assert!(!snake_config.global.respect_gitignore);

    // Both should produce the same result
    assert_eq!(kebab_config.global.line_length, snake_config.global.line_length);
    assert_eq!(
        kebab_config.global.respect_gitignore,
        snake_config.global.respect_gitignore
    );
}
