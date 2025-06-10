/// Serde-based configuration system for rules
/// 
/// This module provides a modern, type-safe configuration system inspired by Ruff's approach.
/// It eliminates manual TOML construction and provides automatic serialization/deserialization.

use serde::{Serialize};
use serde::de::DeserializeOwned;

/// Trait for rule configurations
pub trait RuleConfig: Serialize + DeserializeOwned + Default + Clone {
    /// The rule name (e.g., "MD009")
    const RULE_NAME: &'static str;
}

/// Helper to load rule configuration from the global config
pub fn load_rule_config<T: RuleConfig>(config: &crate::config::Config) -> T {
    config.rules
        .get(T::RULE_NAME)
        .and_then(|rule_config| {
            // Convert BTreeMap<String, toml::Value> to serde_json::Value
            let json_map: serde_json::Map<String, serde_json::Value> = rule_config.values
                .iter()
                .filter_map(|(k, v)| {
                    toml_value_to_json(v).map(|json_v| (k.clone(), json_v))
                })
                .collect();
            
            let json_value = serde_json::Value::Object(json_map);
            serde_json::from_value(json_value).ok()
        })
        .unwrap_or_default()
}

/// Convert TOML value to JSON value for serde deserialization
fn toml_value_to_json(toml_val: &toml::Value) -> Option<serde_json::Value> {
    match toml_val {
        toml::Value::String(s) => Some(serde_json::Value::String(s.clone())),
        toml::Value::Integer(i) => Some(serde_json::Value::Number((*i).into())),
        toml::Value::Float(f) => serde_json::Number::from_f64(*f).map(serde_json::Value::Number),
        toml::Value::Boolean(b) => Some(serde_json::Value::Bool(*b)),
        toml::Value::Array(arr) => {
            let json_arr: Vec<_> = arr.iter().filter_map(toml_value_to_json).collect();
            Some(serde_json::Value::Array(json_arr))
        },
        toml::Value::Table(table) => {
            let json_map: serde_json::Map<_, _> = table
                .iter()
                .filter_map(|(k, v)| toml_value_to_json(v).map(|json_v| (k.clone(), json_v)))
                .collect();
            Some(serde_json::Value::Object(json_map))
        },
        toml::Value::Datetime(_) => None, // Skip datetime for now
    }
}


/// Convert JSON value to TOML value for default config generation
pub fn json_to_toml_value(json_val: &serde_json::Value) -> Option<toml::Value> {
    match json_val {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(b) => Some(toml::Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(toml::Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Some(toml::Value::Float(f))
            } else {
                None
            }
        },
        serde_json::Value::String(s) => Some(toml::Value::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let toml_arr: Vec<_> = arr.iter().filter_map(json_to_toml_value).collect();
            Some(toml::Value::Array(toml_arr))
        },
        serde_json::Value::Object(obj) => {
            let mut toml_table = toml::map::Map::new();
            for (k, v) in obj {
                if let Some(toml_v) = json_to_toml_value(v) {
                    toml_table.insert(k.clone(), toml_v);
                }
            }
            Some(toml::Value::Table(toml_table))
        },
    }
}