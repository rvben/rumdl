/// Serde-based configuration system for rules
///
/// This module provides a modern, type-safe configuration system inspired by Ruff's approach.
/// It eliminates manual TOML construction and provides automatic serialization/deserialization.
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Trait for rule configurations
pub trait RuleConfig: Serialize + DeserializeOwned + Default + Clone {
    /// The rule name (e.g., "MD009")
    const RULE_NAME: &'static str;
}

/// Helper to load rule configuration from the global config
///
/// This function will emit warnings to stderr if the configuration is invalid,
/// helping users identify and fix configuration errors.
pub fn load_rule_config<T: RuleConfig>(config: &crate::config::Config) -> T {
    config
        .rules
        .get(T::RULE_NAME)
        .and_then(|rule_config| {
            // Build the TOML table with backwards compatibility mappings
            let mut table = toml::map::Map::new();

            for (k, v) in rule_config.values.iter() {
                // No manual mapping needed - serde aliases handle this
                table.insert(k.clone(), v.clone());
            }

            let toml_table = toml::Value::Table(table);

            // Deserialize directly from TOML, which preserves serde attributes
            match toml_table.try_into::<T>() {
                Ok(config) => Some(config),
                Err(e) => {
                    // Emit a warning about the invalid configuration
                    eprintln!("Warning: Invalid configuration for rule {}: {}", T::RULE_NAME, e);
                    eprintln!("Using default values for rule {}.", T::RULE_NAME);
                    eprintln!("Hint: Check the documentation for valid configuration values.");

                    None
                }
            }
        })
        .unwrap_or_default()
}

/// Convert JSON value to TOML value for default config generation
pub fn json_to_toml_value(json_val: &serde_json::Value) -> Option<toml::Value> {
    match json_val {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(b) => Some(toml::Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(toml::Value::Integer(i))
            } else {
                n.as_f64().map(toml::Value::Float)
            }
        }
        serde_json::Value::String(s) => Some(toml::Value::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let toml_arr: Vec<_> = arr.iter().filter_map(json_to_toml_value).collect();
            Some(toml::Value::Array(toml_arr))
        }
        serde_json::Value::Object(obj) => {
            let mut toml_table = toml::map::Map::new();
            for (k, v) in obj {
                if let Some(toml_v) = json_to_toml_value(v) {
                    toml_table.insert(k.clone(), toml_v);
                }
            }
            Some(toml::Value::Table(toml_table))
        }
    }
}

/// Check if a key looks like a rule name (MD### format)
///
/// Rule names must start with "MD" (case-insensitive) followed by digits.
pub fn is_rule_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.starts_with("MD") && upper.len() >= 4 && upper[2..].chars().all(|c| c.is_ascii_digit())
}

/// Result of converting JSON to RuleConfig, with any warnings
#[derive(Debug, Default)]
pub struct RuleConfigConversion {
    /// The converted rule configuration
    pub config: Option<crate::config::RuleConfig>,
    /// Warnings about invalid or ignored values
    pub warnings: Vec<String>,
}

/// Convert a JSON rule configuration to an internal RuleConfig
///
/// Supports all rule configuration options including:
/// - `severity`: "error", "warning", or "info"
/// - Any rule-specific options (converted from JSON to TOML values)
///
/// Returns `None` if the JSON value is not an object.
pub fn json_to_rule_config(json_value: &serde_json::Value) -> Option<crate::config::RuleConfig> {
    json_to_rule_config_with_warnings(json_value).config
}

/// Convert a JSON rule configuration to an internal RuleConfig, collecting warnings
///
/// Like `json_to_rule_config`, but also returns warnings for invalid values.
/// Use this when you want to report configuration issues to the user.
pub fn json_to_rule_config_with_warnings(json_value: &serde_json::Value) -> RuleConfigConversion {
    use std::collections::BTreeMap;

    let mut result = RuleConfigConversion::default();

    let Some(obj) = json_value.as_object() else {
        result.warnings.push(format!(
            "Expected object for rule config, got {}",
            json_type_name(json_value)
        ));
        return result;
    };

    let mut values = BTreeMap::new();
    let mut severity = None;

    for (key, val) in obj {
        // Handle severity specially
        if key == "severity" {
            if let Some(s) = val.as_str() {
                match s.to_lowercase().as_str() {
                    "error" => severity = Some(crate::rule::Severity::Error),
                    "warning" => severity = Some(crate::rule::Severity::Warning),
                    "info" => severity = Some(crate::rule::Severity::Info),
                    _ => {
                        result.warnings.push(format!(
                            "Invalid severity '{s}', expected 'error', 'warning', or 'info'"
                        ));
                    }
                };
            } else {
                result
                    .warnings
                    .push(format!("Severity must be a string, got {}", json_type_name(val)));
            }
            continue;
        }

        // Convert JSON value to TOML value
        if let Some(toml_val) = json_to_toml_value(val) {
            values.insert(key.clone(), toml_val);
        } else if !val.is_null() {
            result
                .warnings
                .push(format!("Could not convert '{key}' value to config format"));
        }
    }

    result.config = Some(crate::config::RuleConfig { severity, values });
    result
}

/// Get a human-readable type name for a JSON value
fn json_type_name(val: &serde_json::Value) -> &'static str {
    match val {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Convert TOML value to JSON value
pub fn toml_value_to_json(toml_val: &toml::Value) -> Option<serde_json::Value> {
    match toml_val {
        toml::Value::String(s) => Some(serde_json::Value::String(s.clone())),
        toml::Value::Integer(i) => Some(serde_json::json!(i)),
        toml::Value::Float(f) => Some(serde_json::json!(f)),
        toml::Value::Boolean(b) => Some(serde_json::Value::Bool(*b)),
        toml::Value::Array(arr) => {
            let json_arr: Vec<_> = arr.iter().filter_map(toml_value_to_json).collect();
            Some(serde_json::Value::Array(json_arr))
        }
        toml::Value::Table(table) => {
            let mut json_obj = serde_json::Map::new();
            for (k, v) in table {
                if let Some(json_v) = toml_value_to_json(v) {
                    json_obj.insert(k.clone(), json_v);
                }
            }
            Some(serde_json::Value::Object(json_obj))
        }
        toml::Value::Datetime(_) => None, // JSON doesn't have a native datetime type
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    // Test configuration struct
    #[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
    #[serde(default)]
    struct TestRuleConfig {
        #[serde(default)]
        enabled: bool,
        #[serde(default)]
        indent: i64,
        #[serde(default)]
        style: String,
        #[serde(default)]
        items: Vec<String>,
    }

    impl RuleConfig for TestRuleConfig {
        const RULE_NAME: &'static str = "TEST001";
    }

    #[test]
    fn test_toml_value_to_json_basic_types() {
        // String
        let toml_str = toml::Value::String("hello".to_string());
        let json_str = toml_value_to_json(&toml_str).unwrap();
        assert_eq!(json_str, serde_json::Value::String("hello".to_string()));

        // Integer
        let toml_int = toml::Value::Integer(42);
        let json_int = toml_value_to_json(&toml_int).unwrap();
        assert_eq!(json_int, serde_json::json!(42));

        // Float
        let toml_float = toml::Value::Float(1.234);
        let json_float = toml_value_to_json(&toml_float).unwrap();
        assert_eq!(json_float, serde_json::json!(1.234));

        // Boolean
        let toml_bool = toml::Value::Boolean(true);
        let json_bool = toml_value_to_json(&toml_bool).unwrap();
        assert_eq!(json_bool, serde_json::Value::Bool(true));
    }

    #[test]
    fn test_toml_value_to_json_complex_types() {
        // Array
        let toml_arr = toml::Value::Array(vec![
            toml::Value::String("a".to_string()),
            toml::Value::String("b".to_string()),
        ]);
        let json_arr = toml_value_to_json(&toml_arr).unwrap();
        assert_eq!(json_arr, serde_json::json!(["a", "b"]));

        // Table
        let mut toml_table = toml::map::Map::new();
        toml_table.insert("key1".to_string(), toml::Value::String("value1".to_string()));
        toml_table.insert("key2".to_string(), toml::Value::Integer(123));
        let toml_tbl = toml::Value::Table(toml_table);
        let json_tbl = toml_value_to_json(&toml_tbl).unwrap();

        let expected = serde_json::json!({
            "key1": "value1",
            "key2": 123
        });
        assert_eq!(json_tbl, expected);
    }

    #[test]
    fn test_toml_value_to_json_datetime() {
        // Datetime should return None
        let toml_dt = toml::Value::Datetime("2023-01-01T00:00:00Z".parse().unwrap());
        assert!(toml_value_to_json(&toml_dt).is_none());
    }

    #[test]
    fn test_json_to_toml_value_basic_types() {
        // Null
        assert!(json_to_toml_value(&serde_json::Value::Null).is_none());

        // Bool
        let json_bool = serde_json::Value::Bool(false);
        let toml_bool = json_to_toml_value(&json_bool).unwrap();
        assert_eq!(toml_bool, toml::Value::Boolean(false));

        // Integer
        let json_int = serde_json::json!(42);
        let toml_int = json_to_toml_value(&json_int).unwrap();
        assert_eq!(toml_int, toml::Value::Integer(42));

        // Float
        let json_float = serde_json::json!(1.234);
        let toml_float = json_to_toml_value(&json_float).unwrap();
        assert_eq!(toml_float, toml::Value::Float(1.234));

        // String
        let json_str = serde_json::Value::String("test".to_string());
        let toml_str = json_to_toml_value(&json_str).unwrap();
        assert_eq!(toml_str, toml::Value::String("test".to_string()));
    }

    #[test]
    fn test_json_to_toml_value_complex_types() {
        // Array
        let json_arr = serde_json::json!(["x", "y", "z"]);
        let toml_arr = json_to_toml_value(&json_arr).unwrap();
        if let toml::Value::Array(arr) = toml_arr {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], toml::Value::String("x".to_string()));
            assert_eq!(arr[1], toml::Value::String("y".to_string()));
            assert_eq!(arr[2], toml::Value::String("z".to_string()));
        } else {
            panic!("Expected array");
        }

        // Object
        let json_obj = serde_json::json!({
            "name": "test",
            "count": 10,
            "active": true
        });
        let toml_obj = json_to_toml_value(&json_obj).unwrap();
        if let toml::Value::Table(table) = toml_obj {
            assert_eq!(table.get("name"), Some(&toml::Value::String("test".to_string())));
            assert_eq!(table.get("count"), Some(&toml::Value::Integer(10)));
            assert_eq!(table.get("active"), Some(&toml::Value::Boolean(true)));
        } else {
            panic!("Expected table");
        }
    }

    #[test]
    fn test_load_rule_config_default() {
        // Create empty config
        let config = crate::config::Config::default();

        // Load config for test rule - should return default
        let rule_config: TestRuleConfig = load_rule_config(&config);
        assert_eq!(rule_config, TestRuleConfig::default());
    }

    #[test]
    fn test_load_rule_config_with_values() {
        // Create config with rule values
        let mut config = crate::config::Config::default();
        let mut rule_values = BTreeMap::new();
        rule_values.insert("enabled".to_string(), toml::Value::Boolean(true));
        rule_values.insert("indent".to_string(), toml::Value::Integer(4));
        rule_values.insert("style".to_string(), toml::Value::String("consistent".to_string()));
        rule_values.insert(
            "items".to_string(),
            toml::Value::Array(vec![
                toml::Value::String("item1".to_string()),
                toml::Value::String("item2".to_string()),
            ]),
        );

        config.rules.insert(
            "TEST001".to_string(),
            crate::config::RuleConfig {
                severity: None,
                values: rule_values,
            },
        );

        // Load config
        let rule_config: TestRuleConfig = load_rule_config(&config);
        assert!(rule_config.enabled);
        assert_eq!(rule_config.indent, 4);
        assert_eq!(rule_config.style, "consistent");
        assert_eq!(rule_config.items, vec!["item1", "item2"]);
    }

    #[test]
    fn test_load_rule_config_partial() {
        // Create config with partial rule values
        let mut config = crate::config::Config::default();
        let mut rule_values = BTreeMap::new();
        rule_values.insert("enabled".to_string(), toml::Value::Boolean(true));
        rule_values.insert("style".to_string(), toml::Value::String("custom".to_string()));

        config.rules.insert(
            "TEST001".to_string(),
            crate::config::RuleConfig {
                severity: None,
                values: rule_values,
            },
        );

        // Load config - missing fields should use defaults from TestRuleConfig::default()
        let rule_config: TestRuleConfig = load_rule_config(&config);
        assert!(rule_config.enabled); // from config
        assert_eq!(rule_config.indent, 0); // default i64
        assert_eq!(rule_config.style, "custom"); // from config
        assert_eq!(rule_config.items, Vec::<String>::new()); // default empty vec
    }

    #[test]
    fn test_conversion_roundtrip() {
        // Test that we can convert TOML -> JSON -> TOML
        let original = toml::Value::Table({
            let mut table = toml::map::Map::new();
            table.insert("string".to_string(), toml::Value::String("test".to_string()));
            table.insert("number".to_string(), toml::Value::Integer(42));
            table.insert("bool".to_string(), toml::Value::Boolean(true));
            table.insert(
                "array".to_string(),
                toml::Value::Array(vec![
                    toml::Value::String("a".to_string()),
                    toml::Value::String("b".to_string()),
                ]),
            );
            table
        });

        let json = toml_value_to_json(&original).unwrap();
        let back_to_toml = json_to_toml_value(&json).unwrap();

        assert_eq!(original, back_to_toml);
    }

    #[test]
    fn test_edge_cases() {
        // Empty array
        let empty_arr = toml::Value::Array(vec![]);
        let json_arr = toml_value_to_json(&empty_arr).unwrap();
        assert_eq!(json_arr, serde_json::json!([]));

        // Empty table
        let empty_table = toml::Value::Table(toml::map::Map::new());
        let json_table = toml_value_to_json(&empty_table).unwrap();
        assert_eq!(json_table, serde_json::json!({}));

        // Nested structures
        let nested = toml::Value::Table({
            let mut outer = toml::map::Map::new();
            outer.insert(
                "inner".to_string(),
                toml::Value::Table({
                    let mut inner = toml::map::Map::new();
                    inner.insert("value".to_string(), toml::Value::Integer(123));
                    inner
                }),
            );
            outer
        });
        let json_nested = toml_value_to_json(&nested).unwrap();
        assert_eq!(
            json_nested,
            serde_json::json!({
                "inner": {
                    "value": 123
                }
            })
        );
    }

    #[test]
    fn test_float_edge_cases() {
        // NaN and infinity are not valid JSON numbers
        let nan = serde_json::Number::from_f64(f64::NAN);
        assert!(nan.is_none());

        let inf = serde_json::Number::from_f64(f64::INFINITY);
        assert!(inf.is_none());

        // Valid float
        let valid_float = toml::Value::Float(1.23);
        let json_float = toml_value_to_json(&valid_float).unwrap();
        assert_eq!(json_float, serde_json::json!(1.23));
    }

    #[test]
    fn test_invalid_config_returns_default() {
        // Create config with unknown field
        let mut config = crate::config::Config::default();
        let mut rule_values = BTreeMap::new();
        rule_values.insert("unknown_field".to_string(), toml::Value::Boolean(true));
        // Use a table value for items, which expects an array
        rule_values.insert("items".to_string(), toml::Value::Table(toml::map::Map::new()));

        config.rules.insert(
            "TEST001".to_string(),
            crate::config::RuleConfig {
                severity: None,
                values: rule_values,
            },
        );

        // Load config - should return default and print warning
        let rule_config: TestRuleConfig = load_rule_config(&config);
        // Should use default values since deserialization failed
        assert_eq!(rule_config, TestRuleConfig::default());
    }

    #[test]
    fn test_invalid_field_type() {
        // Create config with wrong type for field
        let mut config = crate::config::Config::default();
        let mut rule_values = BTreeMap::new();
        // indent should be i64, but we're providing a string
        rule_values.insert("indent".to_string(), toml::Value::String("not_a_number".to_string()));

        config.rules.insert(
            "TEST001".to_string(),
            crate::config::RuleConfig {
                severity: None,
                values: rule_values,
            },
        );

        // Load config - should return default and print warning
        let rule_config: TestRuleConfig = load_rule_config(&config);
        assert_eq!(rule_config, TestRuleConfig::default());
    }

    // ========== Tests for is_rule_name ==========

    #[test]
    fn test_is_rule_name_valid() {
        // Standard rule names
        assert!(is_rule_name("MD001"));
        assert!(is_rule_name("MD060"));
        assert!(is_rule_name("MD123"));
        assert!(is_rule_name("MD999"));

        // Case insensitive
        assert!(is_rule_name("md001"));
        assert!(is_rule_name("Md060"));
        assert!(is_rule_name("mD123"));

        // Longer numbers
        assert!(is_rule_name("MD0001"));
        assert!(is_rule_name("MD12345"));
    }

    #[test]
    fn test_is_rule_name_invalid() {
        // Too short
        assert!(!is_rule_name("MD"));
        assert!(!is_rule_name("MD1"));
        assert!(!is_rule_name("M"));
        assert!(!is_rule_name(""));

        // Non-rule identifiers
        assert!(!is_rule_name("disable"));
        assert!(!is_rule_name("enable"));
        assert!(!is_rule_name("flavor"));
        assert!(!is_rule_name("line-length"));
        assert!(!is_rule_name("global"));

        // Invalid format
        assert!(!is_rule_name("MDA01")); // non-digit after MD
        assert!(!is_rule_name("XD001")); // doesn't start with MD
        assert!(!is_rule_name("MD00A")); // non-digit in number
        assert!(!is_rule_name("1MD001")); // starts with number
        assert!(!is_rule_name("MD-001")); // hyphen in number
    }

    // ========== Tests for json_to_rule_config ==========

    #[test]
    fn test_json_to_rule_config_simple() {
        let json = serde_json::json!({
            "enabled": true,
            "style": "aligned"
        });

        let rule_config = json_to_rule_config(&json).unwrap();

        assert_eq!(rule_config.values.get("enabled"), Some(&toml::Value::Boolean(true)));
        assert_eq!(
            rule_config.values.get("style"),
            Some(&toml::Value::String("aligned".to_string()))
        );
        assert!(rule_config.severity.is_none());
    }

    #[test]
    fn test_json_to_rule_config_with_numbers() {
        let json = serde_json::json!({
            "line-length": 120,
            "max-width": 0,
            "indent": 4
        });

        let rule_config = json_to_rule_config(&json).unwrap();

        assert_eq!(rule_config.values.get("line-length"), Some(&toml::Value::Integer(120)));
        assert_eq!(rule_config.values.get("max-width"), Some(&toml::Value::Integer(0)));
        assert_eq!(rule_config.values.get("indent"), Some(&toml::Value::Integer(4)));
    }

    #[test]
    fn test_json_to_rule_config_with_arrays() {
        let json = serde_json::json!({
            "names": ["JavaScript", "TypeScript", "React"],
            "exclude-patterns": ["*.test.md", "draft-*"]
        });

        let rule_config = json_to_rule_config(&json).unwrap();

        let expected_names = toml::Value::Array(vec![
            toml::Value::String("JavaScript".to_string()),
            toml::Value::String("TypeScript".to_string()),
            toml::Value::String("React".to_string()),
        ]);
        assert_eq!(rule_config.values.get("names"), Some(&expected_names));

        let expected_patterns = toml::Value::Array(vec![
            toml::Value::String("*.test.md".to_string()),
            toml::Value::String("draft-*".to_string()),
        ]);
        assert_eq!(rule_config.values.get("exclude-patterns"), Some(&expected_patterns));
    }

    #[test]
    fn test_json_to_rule_config_with_severity() {
        // Error severity
        let json = serde_json::json!({
            "severity": "error",
            "style": "aligned"
        });
        let rule_config = json_to_rule_config(&json).unwrap();
        assert_eq!(rule_config.severity, Some(crate::rule::Severity::Error));
        assert!(!rule_config.values.contains_key("severity")); // severity should not be in values

        // Warning severity
        let json = serde_json::json!({
            "severity": "warning",
            "enabled": true
        });
        let rule_config = json_to_rule_config(&json).unwrap();
        assert_eq!(rule_config.severity, Some(crate::rule::Severity::Warning));

        // Info severity
        let json = serde_json::json!({
            "severity": "info"
        });
        let rule_config = json_to_rule_config(&json).unwrap();
        assert_eq!(rule_config.severity, Some(crate::rule::Severity::Info));

        // Case insensitive severity
        let json = serde_json::json!({
            "severity": "ERROR"
        });
        let rule_config = json_to_rule_config(&json).unwrap();
        assert_eq!(rule_config.severity, Some(crate::rule::Severity::Error));
    }

    #[test]
    fn test_json_to_rule_config_invalid_severity() {
        // Invalid severity string
        let json = serde_json::json!({
            "severity": "critical",
            "style": "aligned"
        });
        let rule_config = json_to_rule_config(&json).unwrap();
        assert!(rule_config.severity.is_none()); // invalid severity is ignored
        assert_eq!(
            rule_config.values.get("style"),
            Some(&toml::Value::String("aligned".to_string()))
        );

        // Non-string severity
        let json = serde_json::json!({
            "severity": 1,
            "enabled": true
        });
        let rule_config = json_to_rule_config(&json).unwrap();
        assert!(rule_config.severity.is_none()); // non-string severity is ignored
    }

    #[test]
    fn test_json_to_rule_config_non_object() {
        // Non-object values should return None
        assert!(json_to_rule_config(&serde_json::json!(42)).is_none());
        assert!(json_to_rule_config(&serde_json::json!("string")).is_none());
        assert!(json_to_rule_config(&serde_json::json!(true)).is_none());
        assert!(json_to_rule_config(&serde_json::json!([1, 2, 3])).is_none());
        assert!(json_to_rule_config(&serde_json::Value::Null).is_none());
    }

    #[test]
    fn test_json_to_rule_config_empty_object() {
        let json = serde_json::json!({});
        let rule_config = json_to_rule_config(&json).unwrap();
        assert!(rule_config.values.is_empty());
        assert!(rule_config.severity.is_none());
    }

    #[test]
    fn test_json_to_rule_config_nested_objects() {
        // Nested objects should be converted to TOML tables
        let json = serde_json::json!({
            "options": {
                "nested-key": "nested-value",
                "nested-number": 42
            }
        });

        let rule_config = json_to_rule_config(&json).unwrap();

        let options = rule_config.values.get("options").unwrap();
        if let toml::Value::Table(table) = options {
            assert_eq!(
                table.get("nested-key"),
                Some(&toml::Value::String("nested-value".to_string()))
            );
            assert_eq!(table.get("nested-number"), Some(&toml::Value::Integer(42)));
        } else {
            panic!("options should be a table");
        }
    }

    #[test]
    fn test_json_to_rule_config_md060_example() {
        // Real-world MD060 config example
        let json = serde_json::json!({
            "enabled": true,
            "style": "aligned",
            "max-width": 120,
            "column-align": "auto",
            "loose-last-column": false
        });

        let rule_config = json_to_rule_config(&json).unwrap();

        assert_eq!(rule_config.values.get("enabled"), Some(&toml::Value::Boolean(true)));
        assert_eq!(
            rule_config.values.get("style"),
            Some(&toml::Value::String("aligned".to_string()))
        );
        assert_eq!(rule_config.values.get("max-width"), Some(&toml::Value::Integer(120)));
        assert_eq!(
            rule_config.values.get("column-align"),
            Some(&toml::Value::String("auto".to_string()))
        );
        assert_eq!(
            rule_config.values.get("loose-last-column"),
            Some(&toml::Value::Boolean(false))
        );
    }

    #[test]
    fn test_json_to_rule_config_md044_example() {
        // Real-world MD044 config example
        let json = serde_json::json!({
            "names": ["JavaScript", "TypeScript", "GitHub", "macOS"],
            "code-blocks": false,
            "html-elements": false
        });

        let rule_config = json_to_rule_config(&json).unwrap();

        let expected_names = toml::Value::Array(vec![
            toml::Value::String("JavaScript".to_string()),
            toml::Value::String("TypeScript".to_string()),
            toml::Value::String("GitHub".to_string()),
            toml::Value::String("macOS".to_string()),
        ]);
        assert_eq!(rule_config.values.get("names"), Some(&expected_names));
        assert_eq!(
            rule_config.values.get("code-blocks"),
            Some(&toml::Value::Boolean(false))
        );
        assert_eq!(
            rule_config.values.get("html-elements"),
            Some(&toml::Value::Boolean(false))
        );
    }

    // ========== Tests for json_to_rule_config_with_warnings ==========

    #[test]
    fn test_json_to_rule_config_with_warnings_valid() {
        let json = serde_json::json!({
            "severity": "error",
            "enabled": true
        });

        let result = json_to_rule_config_with_warnings(&json);

        assert!(result.config.is_some());
        assert!(
            result.warnings.is_empty(),
            "Expected no warnings, got: {:?}",
            result.warnings
        );
        assert_eq!(result.config.unwrap().severity, Some(crate::rule::Severity::Error));
    }

    #[test]
    fn test_json_to_rule_config_with_warnings_invalid_severity() {
        let json = serde_json::json!({
            "severity": "critical",
            "style": "aligned"
        });

        let result = json_to_rule_config_with_warnings(&json);

        assert!(result.config.is_some());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("Invalid severity 'critical'"));
        // Config should still be created, just without severity
        assert!(result.config.unwrap().severity.is_none());
    }

    #[test]
    fn test_json_to_rule_config_with_warnings_wrong_severity_type() {
        let json = serde_json::json!({
            "severity": 123,
            "enabled": true
        });

        let result = json_to_rule_config_with_warnings(&json);

        assert!(result.config.is_some());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("Severity must be a string"));
    }

    #[test]
    fn test_json_to_rule_config_with_warnings_non_object() {
        let json = serde_json::json!("not an object");

        let result = json_to_rule_config_with_warnings(&json);

        assert!(result.config.is_none());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("Expected object"));
    }

    // ========== Integration tests for Config population ==========

    #[test]
    fn test_rule_config_integration_with_config() {
        // Test that converted rule configs work with the main Config struct
        let mut config = crate::config::Config::default();

        // Simulate what WASM API does: convert JSON to RuleConfig and add to config
        let md060_json = serde_json::json!({
            "enabled": true,
            "style": "aligned",
            "max-width": 120
        });
        let md013_json = serde_json::json!({
            "line-length": 100,
            "code-blocks": false
        });

        if let Some(md060_config) = json_to_rule_config(&md060_json) {
            config.rules.insert("MD060".to_string(), md060_config);
        }
        if let Some(md013_config) = json_to_rule_config(&md013_json) {
            config.rules.insert("MD013".to_string(), md013_config);
        }

        // Verify the configs are in place
        assert!(config.rules.contains_key("MD060"));
        assert!(config.rules.contains_key("MD013"));

        // Verify values can be retrieved
        let md060 = config.rules.get("MD060").unwrap();
        assert_eq!(md060.values.get("enabled"), Some(&toml::Value::Boolean(true)));
        assert_eq!(
            md060.values.get("style"),
            Some(&toml::Value::String("aligned".to_string()))
        );
        assert_eq!(md060.values.get("max-width"), Some(&toml::Value::Integer(120)));
    }

    #[test]
    fn test_rule_config_integration_with_severity() {
        let mut config = crate::config::Config::default();

        let json = serde_json::json!({
            "severity": "error",
            "enabled": true
        });

        if let Some(rule_config) = json_to_rule_config(&json) {
            config.rules.insert("MD041".to_string(), rule_config);
        }

        let md041 = config.rules.get("MD041").unwrap();
        assert_eq!(md041.severity, Some(crate::rule::Severity::Error));
    }

    #[test]
    fn test_rule_config_integration_case_normalization() {
        // Test that rule names are handled correctly (caller should normalize)
        let mut config = crate::config::Config::default();

        let json = serde_json::json!({ "enabled": true });

        // Test various case inputs - caller is responsible for normalization
        for rule_name in ["md060", "MD060", "Md060"] {
            if is_rule_name(rule_name)
                && let Some(rule_config) = json_to_rule_config(&json)
            {
                config.rules.insert(rule_name.to_ascii_uppercase(), rule_config);
            }
        }

        // All should normalize to MD060
        assert!(config.rules.contains_key("MD060"));
        assert_eq!(config.rules.len(), 1); // Only one entry after normalization
    }

    #[test]
    fn test_rule_config_integration_filters_non_rules() {
        // Test that is_rule_name correctly filters non-rule keys
        let keys = ["MD060", "disable", "enable", "flavor", "line-length", "global"];

        let rule_keys: Vec<_> = keys.iter().filter(|k| is_rule_name(k)).collect();

        assert_eq!(rule_keys, vec![&"MD060"]);
    }

    #[test]
    fn test_multiple_rule_configs_with_mixed_validity() {
        // Test handling multiple rules where some have warnings
        let rules = vec![
            ("MD060", serde_json::json!({ "severity": "error", "style": "aligned" })),
            (
                "MD013",
                serde_json::json!({ "severity": "invalid", "line-length": 100 }),
            ),
            ("MD041", serde_json::json!({ "enabled": true })),
        ];

        let mut config = crate::config::Config::default();
        let mut all_warnings = Vec::new();

        for (name, json) in rules {
            let result = json_to_rule_config_with_warnings(&json);
            all_warnings.extend(result.warnings);
            if let Some(rule_config) = result.config {
                config.rules.insert(name.to_string(), rule_config);
            }
        }

        // All rules should be added
        assert_eq!(config.rules.len(), 3);

        // Should have one warning about invalid severity
        assert_eq!(all_warnings.len(), 1);
        assert!(all_warnings[0].contains("Invalid severity"));

        // MD060 should have severity, MD013 should not
        assert_eq!(
            config.rules.get("MD060").unwrap().severity,
            Some(crate::rule::Severity::Error)
        );
        assert!(config.rules.get("MD013").unwrap().severity.is_none());
    }

    // ========== End-to-end integration tests ==========
    // These tests verify the full flow: JSON config -> RuleConfig -> Config -> actual linting

    #[test]
    fn test_end_to_end_md013_line_length_config() {
        // Test that MD013 line-length config actually affects linting behavior
        let content = "# Test\n\nThis is a line that is exactly 50 characters long.\n";

        // Create config with line-length = 40 (should trigger warning)
        let mut config = crate::config::Config::default();
        let json = serde_json::json!({
            "line-length": 40
        });
        if let Some(rule_config) = json_to_rule_config(&json) {
            config.rules.insert("MD013".to_string(), rule_config);
        }

        // Only enable MD013 for this test
        config.global.enable = vec!["MD013".to_string()];

        let rules = crate::rules::all_rules(&config);
        let filtered = crate::rules::filter_rules(&rules, &config.global);

        let result = crate::lint(
            content,
            &filtered,
            false,
            crate::config::MarkdownFlavor::Standard,
            Some(&config),
        );

        let warnings = result.expect("Linting should succeed");

        // Should have MD013 warning because line exceeds 40 chars
        let has_md013 = warnings.iter().any(|w| w.rule_name.as_deref() == Some("MD013"));
        assert!(has_md013, "Should have MD013 warning with line-length=40");
    }

    #[test]
    fn test_end_to_end_md013_line_length_no_warning() {
        // Same content but with higher line-length limit - no warning
        let content = "# Test\n\nThis is a line that is exactly 50 characters long.\n";

        // Create config with line-length = 100 (should NOT trigger warning)
        let mut config = crate::config::Config::default();
        let json = serde_json::json!({
            "line-length": 100
        });
        if let Some(rule_config) = json_to_rule_config(&json) {
            config.rules.insert("MD013".to_string(), rule_config);
        }

        // Only enable MD013 for this test
        config.global.enable = vec!["MD013".to_string()];

        let rules = crate::rules::all_rules(&config);
        let filtered = crate::rules::filter_rules(&rules, &config.global);

        let result = crate::lint(
            content,
            &filtered,
            false,
            crate::config::MarkdownFlavor::Standard,
            Some(&config),
        );

        let warnings = result.expect("Linting should succeed");

        // Should NOT have MD013 warning because line is under 100 chars
        let has_md013 = warnings.iter().any(|w| w.rule_name.as_deref() == Some("MD013"));
        assert!(!has_md013, "Should NOT have MD013 warning with line-length=100");
    }

    #[test]
    fn test_end_to_end_md044_proper_names() {
        // Test that MD044 proper names config actually affects linting
        let content = "# Test\n\nWe use javascript and typescript.\n";

        // Create config with proper names
        let mut config = crate::config::Config::default();
        let json = serde_json::json!({
            "names": ["JavaScript", "TypeScript"],
            "code-blocks": false
        });
        if let Some(rule_config) = json_to_rule_config(&json) {
            config.rules.insert("MD044".to_string(), rule_config);
        }

        // Only enable MD044 for this test
        config.global.enable = vec!["MD044".to_string()];

        let rules = crate::rules::all_rules(&config);
        let filtered = crate::rules::filter_rules(&rules, &config.global);

        let result = crate::lint(
            content,
            &filtered,
            false,
            crate::config::MarkdownFlavor::Standard,
            Some(&config),
        );

        let warnings = result.expect("Linting should succeed");

        // Should have MD044 warnings for improper casing
        let md044_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD044"))
            .collect();

        assert!(
            md044_warnings.len() >= 2,
            "Should have MD044 warnings for 'javascript' and 'typescript', got {}",
            md044_warnings.len()
        );
    }

    #[test]
    fn test_end_to_end_severity_config() {
        // Test that severity config is respected
        let content = "test\n"; // Missing heading, triggers MD041

        let mut config = crate::config::Config::default();
        let json = serde_json::json!({
            "severity": "info"
        });
        if let Some(rule_config) = json_to_rule_config(&json) {
            config.rules.insert("MD041".to_string(), rule_config);
        }

        // Only enable MD041 for this test
        config.global.enable = vec!["MD041".to_string()];

        let rules = crate::rules::all_rules(&config);
        let filtered = crate::rules::filter_rules(&rules, &config.global);

        let result = crate::lint(
            content,
            &filtered,
            false,
            crate::config::MarkdownFlavor::Standard,
            Some(&config),
        );

        let warnings = result.expect("Linting should succeed");

        // Find MD041 warning and verify severity
        let md041 = warnings.iter().find(|w| w.rule_name.as_deref() == Some("MD041"));
        assert!(md041.is_some(), "Should have MD041 warning");
        assert_eq!(
            md041.unwrap().severity,
            crate::rule::Severity::Info,
            "MD041 should have Info severity from config"
        );
    }
}
