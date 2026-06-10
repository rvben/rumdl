//! The single dispatch table for global configuration keys.
//!
//! Three contexts accept global keys: `rumdl.toml`/`.rumdl.toml` (parsed
//! with `toml_edit`), `pyproject.toml` (parsed with `toml`), and inline
//! `--config key=value` overrides. They previously each carried their own
//! per-key match with its own type checks, and drifted. The key list, the
//! expected types, and the setters now live here once: adding a global key
//! means adding it to [`GLOBAL_VALUE_KEYS`] and one arm in
//! [`apply_global_key`]; every context picks it up.
//!
//! Callers keep their own key discovery (which table to scan, alias
//! spellings) and their own diagnostics phrasing, driven by the returned
//! [`ApplyOutcome`].

use std::str::FromStr;

use super::flavor::{MarkdownFlavor, normalize_key};
use super::registry::RuleRegistry;
use super::source_tracking::{ConfigSource, SourcedGlobalConfig, SourcedValue};
use crate::types::LineLength;

/// Global configuration keys that hold plain values (normalized kebab-case).
pub const GLOBAL_VALUE_KEYS: &[&str] = &[
    "enable",
    "disable",
    "include",
    "exclude",
    "extend-enable",
    "extend-disable",
    "respect-gitignore",
    "force-exclude",
    "line-length",
    "output-format",
    "cache-dir",
    "cache",
    "fixable",
    "unfixable",
    "flavor",
];

/// Whether a (normalized) key names a global value setting.
pub fn is_global_value_key(key: &str) -> bool {
    GLOBAL_VALUE_KEYS.contains(&key)
}

/// Result of applying a candidate global key.
#[derive(Debug)]
pub enum ApplyOutcome {
    /// Key recognized and value stored.
    Applied,
    /// Key recognized but the value has the wrong TOML type; nothing stored.
    TypeMismatch { expected: &'static str },
    /// Key recognized, type correct, but the value is invalid (e.g. an
    /// unknown flavor name); nothing stored.
    InvalidValue { message: String },
    /// Not a global value key.
    Unrecognized,
}

/// Apply one global key to the global config section.
///
/// `norm_key` must already be normalized (see [`normalize_key`]); rule-list
/// values resolve rule-name aliases through `registry`. `origin` is the
/// config file supplying the value (`None` for CLI input) and feeds the
/// provenance shown by `rumdl config`.
pub fn apply_global_key(
    global: &mut SourcedGlobalConfig,
    norm_key: &str,
    value: &toml::Value,
    source: ConfigSource,
    origin: Option<&str>,
    registry: &RuleRegistry,
) -> ApplyOutcome {
    let origin = origin.map(std::string::ToString::to_string);

    let resolve_rule_list = |arr: &[toml::Value]| -> Vec<String> {
        arr.iter()
            .filter_map(|v| v.as_str())
            .map(|s| registry.resolve_rule_name(s).unwrap_or_else(|| normalize_key(s)))
            .collect()
    };
    let to_strings =
        |arr: &[toml::Value]| -> Vec<String> { arr.iter().filter_map(|v| v.as_str()).map(str::to_string).collect() };

    match norm_key {
        "enable" | "disable" | "extend-enable" | "extend-disable" | "fixable" | "unfixable" => {
            let toml::Value::Array(arr) = value else {
                return ApplyOutcome::TypeMismatch { expected: "array" };
            };
            let values = resolve_rule_list(arr);
            match norm_key {
                "enable" => global.enable.push_override(values, source, origin),
                "disable" => global.disable.push_override(values, source, origin),
                "extend-enable" => global.extend_enable.push_override(values, source, origin),
                "extend-disable" => global.extend_disable.push_override(values, source, origin),
                "fixable" => global.fixable.push_override(values, source, origin),
                "unfixable" => global.unfixable.push_override(values, source, origin),
                _ => unreachable!("outer match limits the keys"),
            }
            ApplyOutcome::Applied
        }
        "include" | "exclude" => {
            let toml::Value::Array(arr) = value else {
                return ApplyOutcome::TypeMismatch { expected: "array" };
            };
            let values = to_strings(arr);
            match norm_key {
                "include" => global.include.push_override(values, source, origin),
                "exclude" => global.exclude.push_override(values, source, origin),
                _ => unreachable!("outer match limits the keys"),
            }
            ApplyOutcome::Applied
        }
        "respect-gitignore" | "force-exclude" | "cache" => {
            let Some(b) = value.as_bool() else {
                return ApplyOutcome::TypeMismatch { expected: "boolean" };
            };
            match norm_key {
                "respect-gitignore" => global.respect_gitignore.push_override(b, source, origin),
                "force-exclude" => global.force_exclude.push_override(b, source, origin),
                "cache" => global.cache.push_override(b, source, origin),
                _ => unreachable!("outer match limits the keys"),
            }
            ApplyOutcome::Applied
        }
        "line-length" => {
            let Some(n) = value.as_integer() else {
                return ApplyOutcome::TypeMismatch { expected: "integer" };
            };
            // Negative lengths are nonsense; clamp instead of wrapping.
            global
                .line_length
                .push_override(LineLength::new(n.max(0) as usize), source, origin);
            ApplyOutcome::Applied
        }
        "output-format" | "cache-dir" => {
            let Some(s) = value.as_str() else {
                return ApplyOutcome::TypeMismatch { expected: "string" };
            };
            let slot = match norm_key {
                "output-format" => &mut global.output_format,
                "cache-dir" => &mut global.cache_dir,
                _ => unreachable!("outer match limits the keys"),
            };
            if let Some(sv) = slot.as_mut() {
                sv.push_override(s.to_string(), source, origin);
            } else {
                let mut sv = SourcedValue::new(s.to_string(), source);
                sv.origin = origin;
                *slot = Some(sv);
            }
            ApplyOutcome::Applied
        }
        "flavor" => {
            let Some(s) = value.as_str() else {
                return ApplyOutcome::TypeMismatch { expected: "string" };
            };
            match MarkdownFlavor::from_str(s) {
                Ok(flavor) => {
                    global.flavor.push_override(flavor, source, origin);
                    ApplyOutcome::Applied
                }
                Err(_) => ApplyOutcome::InvalidValue {
                    message: format!("unknown markdown flavor '{s}'"),
                },
            }
        }
        _ => ApplyOutcome::Unrecognized,
    }
}

/// Convert a `toml_edit` value to a plain `toml::Value` so the `rumdl.toml`
/// parser can feed [`apply_global_key`]. Inline tables become tables;
/// datetimes are stringified (no global key is datetime-typed, so this only
/// affects the mismatch diagnostics).
pub(super) fn toml_edit_value_to_toml(value: &toml_edit::Value) -> toml::Value {
    match value {
        toml_edit::Value::String(s) => toml::Value::String(s.value().clone()),
        toml_edit::Value::Integer(i) => toml::Value::Integer(*i.value()),
        toml_edit::Value::Float(f) => toml::Value::Float(*f.value()),
        toml_edit::Value::Boolean(b) => toml::Value::Boolean(*b.value()),
        toml_edit::Value::Datetime(d) => toml::Value::String(d.value().to_string()),
        toml_edit::Value::Array(arr) => toml::Value::Array(arr.iter().map(toml_edit_value_to_toml).collect()),
        toml_edit::Value::InlineTable(t) => toml::Value::Table(
            t.iter()
                .map(|(k, v)| (k.to_string(), toml_edit_value_to_toml(v)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::registry::default_registry;

    fn apply(key: &str, value: &toml::Value) -> (SourcedGlobalConfig, ApplyOutcome) {
        let mut global = SourcedGlobalConfig::default();
        let outcome = apply_global_key(
            &mut global,
            key,
            value,
            ConfigSource::ProjectConfig,
            Some("test.toml"),
            default_registry(),
        );
        (global, outcome)
    }

    #[test]
    fn every_global_key_is_recognized() {
        // The key list and the dispatch must stay in lockstep: every listed
        // key must produce Applied or TypeMismatch, never Unrecognized.
        for key in GLOBAL_VALUE_KEYS {
            let (_, outcome) = apply(key, &toml::Value::Datetime("1979-05-27".parse().unwrap()));
            assert!(
                !matches!(outcome, ApplyOutcome::Unrecognized),
                "key '{key}' is listed but not dispatched"
            );
        }
        let (_, outcome) = apply("not-a-key", &toml::Value::Boolean(true));
        assert!(matches!(outcome, ApplyOutcome::Unrecognized));
    }

    #[test]
    fn applies_values_with_origin() {
        let (global, outcome) = apply("line-length", &toml::Value::Integer(120));
        assert!(matches!(outcome, ApplyOutcome::Applied));
        assert_eq!(global.line_length.value.get(), 120);
        assert_eq!(global.line_length.origin.as_deref(), Some("test.toml"));

        let (global, outcome) = apply(
            "enable",
            &toml::Value::Array(vec![toml::Value::String("ul-style".to_string())]),
        );
        assert!(matches!(outcome, ApplyOutcome::Applied));
        assert_eq!(global.enable.value, vec!["MD004".to_string()], "aliases resolve");
    }

    #[test]
    fn rejects_wrong_types_without_storing() {
        let (global, outcome) = apply("line-length", &toml::Value::String("wide".to_string()));
        assert!(matches!(outcome, ApplyOutcome::TypeMismatch { expected: "integer" }));
        assert_eq!(global.line_length.source, ConfigSource::Default);
    }

    #[test]
    fn negative_line_length_clamps_to_zero() {
        let (global, outcome) = apply("line-length", &toml::Value::Integer(-5));
        assert!(matches!(outcome, ApplyOutcome::Applied));
        assert_eq!(global.line_length.value.get(), 0);
    }

    #[test]
    fn unknown_flavor_is_invalid_not_stored() {
        let (global, outcome) = apply("flavor", &toml::Value::String("nonexistent".to_string()));
        assert!(matches!(outcome, ApplyOutcome::InvalidValue { .. }));
        assert_eq!(global.flavor.source, ConfigSource::Default);
    }
}
