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