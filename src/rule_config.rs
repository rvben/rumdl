/// Configuration helper trait and utilities for rules
///
/// This module provides utilities to reduce boilerplate in rule configuration
use toml::Value;

/// Helper macro to implement default_config_section for rules
///
/// Usage:
/// ```ignore
/// fn default_config_section(&self) -> Option<(String, toml::Value)> {
///     impl_default_config!(
///         self.name(),
///         field1: self.field1,
///         field2: self.field2,
///     )
/// }
/// ```
#[macro_export]
macro_rules! impl_default_config {
    ($rule_name:expr, $($field:ident: $value:expr),* $(,)?) => {{
        let mut map = toml::map::Map::new();
        $(
            map.insert(stringify!($field).to_string(), $value);
        )*
        Some(($rule_name.to_string(), toml::Value::Table(map)))
    }};
    ($rule_name:expr $(,)?) => {{
        let map = toml::map::Map::new();
        Some(($rule_name.to_string(), toml::Value::Table(map)))
    }};
}

/// Simpler helpers for creating TOML values
pub fn toml_bool(b: bool) -> Value {
    Value::Boolean(b)
}

pub fn toml_int<T: Into<i64>>(i: T) -> Value {
    Value::Integer(i.into())
}

pub fn toml_string<T: Into<String>>(s: T) -> Value {
    Value::String(s.into())
}

pub fn toml_array<T: Into<String>>(items: Vec<T>) -> Value {
    Value::Array(items.into_iter().map(|s| Value::String(s.into())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toml_bool() {
        assert_eq!(toml_bool(true), Value::Boolean(true));
        assert_eq!(toml_bool(false), Value::Boolean(false));
    }

    #[test]
    fn test_toml_int() {
        assert_eq!(toml_int(42), Value::Integer(42));
        assert_eq!(toml_int(0), Value::Integer(0));
        assert_eq!(toml_int(-10), Value::Integer(-10));

        // Test with different integer types
        assert_eq!(toml_int(42u8), Value::Integer(42));
        assert_eq!(toml_int(42u16), Value::Integer(42));
        assert_eq!(toml_int(42u32), Value::Integer(42));
        assert_eq!(toml_int(42i8), Value::Integer(42));
        assert_eq!(toml_int(42i16), Value::Integer(42));
        assert_eq!(toml_int(42i32), Value::Integer(42));
    }

    #[test]
    fn test_toml_string() {
        assert_eq!(toml_string("hello"), Value::String("hello".to_string()));
        assert_eq!(toml_string(String::from("world")), Value::String("world".to_string()));
        assert_eq!(toml_string(""), Value::String("".to_string()));

        // Test unicode
        assert_eq!(toml_string("你好"), Value::String("你好".to_string()));
        assert_eq!(toml_string("🦀"), Value::String("🦀".to_string()));
    }

    #[test]
    fn test_toml_array() {
        // Test with string literals
        let arr1 = toml_array(vec!["one", "two", "three"]);
        if let Value::Array(values) = arr1 {
            assert_eq!(values.len(), 3);
            assert_eq!(values[0], Value::String("one".to_string()));
            assert_eq!(values[1], Value::String("two".to_string()));
            assert_eq!(values[2], Value::String("three".to_string()));
        } else {
            panic!("Expected array");
        }

        // Test with String objects
        let arr2 = toml_array(vec![String::from("alpha"), String::from("beta")]);
        if let Value::Array(values) = arr2 {
            assert_eq!(values.len(), 2);
            assert_eq!(values[0], Value::String("alpha".to_string()));
            assert_eq!(values[1], Value::String("beta".to_string()));
        } else {
            panic!("Expected array");
        }

        // Test empty array
        let arr3 = toml_array(Vec::<String>::new());
        assert_eq!(arr3, Value::Array(vec![]));
    }

    #[test]
    fn test_impl_default_config_macro() {
        // Test the macro with different field types
        let config = impl_default_config!(
            "MD001",
            enabled: toml_bool(true),
            indent: toml_int(4),
            style: toml_string("consistent"),
        );

        assert!(config.is_some());
        let (rule_name, value) = config.unwrap();
        assert_eq!(rule_name, "MD001");

        if let Value::Table(table) = value {
            assert_eq!(table.get("enabled"), Some(&Value::Boolean(true)));
            assert_eq!(table.get("indent"), Some(&Value::Integer(4)));
            assert_eq!(table.get("style"), Some(&Value::String("consistent".to_string())));
        } else {
            panic!("Expected table");
        }
    }

    #[test]
    fn test_impl_default_config_macro_empty() {
        // Test macro with no fields
        let config = impl_default_config!("MD002");

        assert!(config.is_some());
        let (rule_name, value) = config.unwrap();
        assert_eq!(rule_name, "MD002");

        if let Value::Table(table) = value {
            assert!(table.is_empty());
        } else {
            panic!("Expected table");
        }
    }

    #[test]
    fn test_impl_default_config_macro_complex() {
        // Test with more complex expressions
        let my_array = vec!["item1", "item2"];
        let my_bool = false;
        let my_number = 42 + 58;

        let config = impl_default_config!(
            "MD003",
            items: toml_array(my_array),
            enabled: toml_bool(my_bool),
            count: toml_int(my_number),
        );

        assert!(config.is_some());
        let (rule_name, value) = config.unwrap();
        assert_eq!(rule_name, "MD003");

        if let Value::Table(table) = value {
            assert!(table.contains_key("items"));
            assert_eq!(table.get("enabled"), Some(&Value::Boolean(false)));
            assert_eq!(table.get("count"), Some(&Value::Integer(100)));
        } else {
            panic!("Expected table");
        }
    }

    #[test]
    fn test_macro_field_name_preservation() {
        // Ensure field names are correctly stringified
        let config = impl_default_config!(
            "MD004",
            snake_case_field: toml_string("value1"),
            camelCaseField: toml_string("value2"),
            UPPERCASE_FIELD: toml_string("value3"),
        );

        let (_, value) = config.unwrap();
        if let Value::Table(table) = value {
            assert!(table.contains_key("snake_case_field"));
            assert!(table.contains_key("camelCaseField"));
            assert!(table.contains_key("UPPERCASE_FIELD"));
        } else {
            panic!("Expected table");
        }
    }

    #[test]
    fn test_toml_value_types() {
        // Verify that our helper functions produce the correct TOML value types
        assert!(matches!(toml_bool(true), Value::Boolean(_)));
        assert!(matches!(toml_int(42), Value::Integer(_)));
        assert!(matches!(toml_string("test"), Value::String(_)));
        assert!(matches!(toml_array(vec!["a", "b"]), Value::Array(_)));
    }

    #[test]
    fn test_edge_cases() {
        // Test with maximum/minimum values for i32
        assert_eq!(toml_int(i32::MAX), Value::Integer(i32::MAX as i64));
        assert_eq!(toml_int(i32::MIN), Value::Integer(i32::MIN as i64));

        // Test with special strings
        assert_eq!(toml_string("with\nnewline"), Value::String("with\nnewline".to_string()));
        assert_eq!(toml_string("with\ttab"), Value::String("with\ttab".to_string()));
        assert_eq!(toml_string("with\"quote"), Value::String("with\"quote".to_string()));
    }
}
