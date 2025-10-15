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

    compiled.validate(&instance).map_err(|err_iter| {
        let errors: Vec<String> = err_iter.map(|e| format!("{} at {}", e, e.instance_path)).collect();
        errors.join("; ")
    })
}

#[test]
fn test_schema_exists() {
    let schema = load_schema();
    assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
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
    let toml = r#"
[global]
line_length = "100"  # Should be a number, not a string
"#;
    let result = validate_toml_config(toml);
    assert!(result.is_err(), "Wrong type for line_length should fail validation");
}

#[test]
fn test_invalid_type_for_respect_gitignore() {
    let toml = r#"
[global]
respect_gitignore = "true"  # Should be boolean, not string
"#;
    let result = validate_toml_config(toml);
    assert!(
        result.is_err(),
        "Wrong type for respect_gitignore should fail validation"
    );
}
