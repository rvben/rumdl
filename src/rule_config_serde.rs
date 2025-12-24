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

#[cfg(test)]
/// Convert TOML value to JSON value (only used in tests)
fn toml_value_to_json(toml_val: &toml::Value) -> Option<serde_json::Value> {
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
}
