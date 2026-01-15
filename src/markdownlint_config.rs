//!
//! This module handles parsing and mapping markdownlint config files (JSON/YAML) to rumdl's internal config format.
//! It provides mapping from markdownlint rule keys to rumdl rule keys and provenance tracking for configuration values.

use crate::config::{ConfigSource, SourcedConfig, SourcedValue};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

/// Represents a generic markdownlint config (rule keys to values)
#[derive(Debug, Deserialize)]
pub struct MarkdownlintConfig(pub HashMap<String, serde_yml::Value>);

/// Load a markdownlint config file (JSON or YAML) from the given path
pub fn load_markdownlint_config(path: &str) -> Result<MarkdownlintConfig, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read config file {path}: {e}"))?;

    if path.ends_with(".json") || path.ends_with(".jsonc") {
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {e}"))
    } else if path.ends_with(".yaml") || path.ends_with(".yml") {
        serde_yml::from_str(&content).map_err(|e| format!("Failed to parse YAML: {e}"))
    } else {
        serde_json::from_str(&content)
            .or_else(|_| serde_yml::from_str(&content))
            .map_err(|e| format!("Failed to parse config as JSON or YAML: {e}"))
    }
}

/// Mapping table from markdownlint rule keys/aliases to rumdl rule keys
/// Convert a rule name (which may be an alias like "line-length") to the canonical rule ID (like "MD013").
/// Returns None if the rule name is not recognized.
pub fn markdownlint_to_rumdl_rule_key(key: &str) -> Option<&'static str> {
    // Use the shared alias resolution function from config module
    crate::config::resolve_rule_name_alias(key)
}

fn normalize_toml_table_keys(val: toml::Value) -> toml::Value {
    match val {
        toml::Value::Table(table) => {
            let mut new_table = toml::map::Map::new();
            for (k, v) in table {
                let norm_k = crate::config::normalize_key(&k);
                new_table.insert(norm_k, normalize_toml_table_keys(v));
            }
            toml::Value::Table(new_table)
        }
        toml::Value::Array(arr) => toml::Value::Array(arr.into_iter().map(normalize_toml_table_keys).collect()),
        other => other,
    }
}

/// Map markdownlint-specific option names to rumdl option names for a given rule.
/// This handles incompatibilities between markdownlint and rumdl config schemas.
/// Returns a new table with mapped options, or None if the entire config should be dropped.
fn map_markdownlint_options_to_rumdl(
    rule_key: &str,
    table: toml::map::Map<String, toml::Value>,
) -> Option<toml::map::Map<String, toml::Value>> {
    let mut mapped = toml::map::Map::new();

    match rule_key {
        "MD013" => {
            // MD013 (line-length) has different option names in markdownlint vs rumdl
            for (k, v) in table {
                match k.as_str() {
                    // Markdownlint uses separate line length limits for different content types
                    // rumdl uses boolean flags to enable/disable checking for content types
                    "code-block-line-length" | "code_block_line_length" => {
                        // Ignore: rumdl doesn't support per-content-type line length limits
                        // Instead, users should use code-blocks = false to disable entirely
                        log::warn!(
                            "Ignoring markdownlint option 'code_block_line_length' for MD013. Use 'code-blocks = false' in rumdl to disable line length checking in code blocks."
                        );
                    }
                    "heading-line-length" | "heading_line_length" => {
                        // Ignore: rumdl doesn't support per-content-type line length limits
                        log::warn!(
                            "Ignoring markdownlint option 'heading_line_length' for MD013. Use 'headings = false' in rumdl to disable line length checking in headings."
                        );
                    }
                    "stern" => {
                        // Markdownlint uses "stern", rumdl uses "strict"
                        mapped.insert("strict".to_string(), v);
                    }
                    // Pass through all other options
                    _ => {
                        mapped.insert(k, v);
                    }
                }
            }
            Some(mapped)
        }
        "MD054" => {
            // MD054 (link-image-style) has fundamentally different config models
            // Markdownlint uses style/styles strings, rumdl uses individual boolean flags
            for (k, v) in table {
                match k.as_str() {
                    "style" | "styles" => {
                        // Ignore: rumdl uses individual boolean flags (autolink, inline, full, etc.)
                        // Cannot automatically map string style names to boolean flags
                        log::warn!(
                            "Ignoring markdownlint option '{k}' for MD054. rumdl uses individual boolean flags (autolink, inline, full, collapsed, shortcut, url-inline) instead. Please configure these directly."
                        );
                    }
                    // Pass through all other options (autolink, inline, full, collapsed, shortcut, url-inline)
                    _ => {
                        mapped.insert(k, v);
                    }
                }
            }
            Some(mapped)
        }
        // All other rules: pass through unchanged
        _ => Some(table),
    }
}

/// Map a MarkdownlintConfig to rumdl's internal Config format
impl MarkdownlintConfig {
    /// Map to a SourcedConfig, tracking provenance as Markdownlint for all values.
    pub fn map_to_sourced_rumdl_config(&self, file_path: Option<&str>) -> SourcedConfig {
        let mut sourced_config = SourcedConfig::default();
        let file = file_path.map(|s| s.to_string());
        for (key, value) in &self.0 {
            let mapped = markdownlint_to_rumdl_rule_key(key);
            if let Some(rumdl_key) = mapped {
                let norm_rule_key = rumdl_key.to_ascii_uppercase();
                let toml_value: Option<toml::Value> = serde_yml::from_value::<toml::Value>(value.clone()).ok();
                let toml_value = toml_value.map(normalize_toml_table_keys);
                let rule_config = sourced_config.rules.entry(norm_rule_key.clone()).or_default();
                if let Some(tv) = toml_value {
                    if let toml::Value::Table(mut table) = tv {
                        // Apply markdownlint-to-rumdl option mapping
                        table = match map_markdownlint_options_to_rumdl(&norm_rule_key, table) {
                            Some(mapped) => mapped,
                            None => continue, // Skip this rule entirely if mapping returns None
                        };

                        // Special handling for MD007: Add style = "fixed" for markdownlint compatibility
                        if norm_rule_key == "MD007" && !table.contains_key("style") {
                            table.insert("style".to_string(), toml::Value::String("fixed".to_string()));
                        }

                        for (k, v) in table {
                            let norm_config_key = k; // Already normalized
                            rule_config
                                .values
                                .entry(norm_config_key.clone())
                                .and_modify(|sv| {
                                    sv.value = v.clone();
                                    sv.source = ConfigSource::ProjectConfig;
                                    sv.overrides.push(crate::config::ConfigOverride {
                                        value: v.clone(),
                                        source: ConfigSource::ProjectConfig,
                                        file: file.clone(),
                                        line: None,
                                    });
                                })
                                .or_insert_with(|| SourcedValue {
                                    value: v.clone(),
                                    source: ConfigSource::ProjectConfig,
                                    overrides: vec![crate::config::ConfigOverride {
                                        value: v,
                                        source: ConfigSource::ProjectConfig,
                                        file: file.clone(),
                                        line: None,
                                    }],
                                });
                        }
                    } else {
                        rule_config
                            .values
                            .entry("value".to_string())
                            .and_modify(|sv| {
                                sv.value = tv.clone();
                                sv.source = ConfigSource::ProjectConfig;
                                sv.overrides.push(crate::config::ConfigOverride {
                                    value: tv.clone(),
                                    source: ConfigSource::ProjectConfig,
                                    file: file.clone(),
                                    line: None,
                                });
                            })
                            .or_insert_with(|| SourcedValue {
                                value: tv.clone(),
                                source: ConfigSource::ProjectConfig,
                                overrides: vec![crate::config::ConfigOverride {
                                    value: tv,
                                    source: ConfigSource::ProjectConfig,
                                    file: file.clone(),
                                    line: None,
                                }],
                            });

                        // Special handling for MD007: Add style = "fixed" for markdownlint compatibility
                        if norm_rule_key == "MD007" && !rule_config.values.contains_key("style") {
                            rule_config.values.insert(
                                "style".to_string(),
                                SourcedValue {
                                    value: toml::Value::String("fixed".to_string()),
                                    source: ConfigSource::ProjectConfig,
                                    overrides: vec![crate::config::ConfigOverride {
                                        value: toml::Value::String("fixed".to_string()),
                                        source: ConfigSource::ProjectConfig,
                                        file: file.clone(),
                                        line: None,
                                    }],
                                },
                            );
                        }
                    }
                } else {
                    log::error!(
                        "Could not convert value for rule key {key:?} to rumdl's internal config format. This likely means the configuration value is invalid or not supported for this rule. Please check your markdownlint config."
                    );
                    std::process::exit(1);
                }
            }
        }
        if let Some(_f) = file {
            sourced_config.loaded_files.push(_f);
        }
        sourced_config
    }

    /// Map to a SourcedConfigFragment, for use in config loading.
    pub fn map_to_sourced_rumdl_config_fragment(
        &self,
        file_path: Option<&str>,
    ) -> crate::config::SourcedConfigFragment {
        let mut fragment = crate::config::SourcedConfigFragment::default();
        let file = file_path.map(|s| s.to_string());

        // Accumulate disabled and enabled rules
        let mut disabled_rules = Vec::new();
        let mut enabled_rules = Vec::new();

        for (key, value) in &self.0 {
            let mapped = markdownlint_to_rumdl_rule_key(key);
            if let Some(rumdl_key) = mapped {
                let norm_rule_key = rumdl_key.to_ascii_uppercase();
                // Special handling for boolean values (true/false)
                if value.is_bool() {
                    if !value.as_bool().unwrap_or(false) {
                        // Accumulate disabled rules
                        disabled_rules.push(norm_rule_key.clone());
                    } else {
                        // Accumulate enabled rules
                        enabled_rules.push(norm_rule_key.clone());
                    }
                    continue;
                }
                let toml_value: Option<toml::Value> = serde_yml::from_value::<toml::Value>(value.clone()).ok();
                let toml_value = toml_value.map(normalize_toml_table_keys);
                let rule_config = fragment.rules.entry(norm_rule_key.clone()).or_default();
                if let Some(tv) = toml_value {
                    // Special case: if line-length (MD013) is given a number value directly,
                    // treat it as {"line_length": value}
                    let tv = if norm_rule_key == "MD013" && tv.is_integer() {
                        let mut table = toml::map::Map::new();
                        table.insert("line-length".to_string(), tv);
                        toml::Value::Table(table)
                    } else {
                        tv
                    };

                    if let toml::Value::Table(mut table) = tv {
                        // Apply markdownlint-to-rumdl option mapping
                        table = match map_markdownlint_options_to_rumdl(&norm_rule_key, table) {
                            Some(mapped) => mapped,
                            None => continue, // Skip this rule entirely if mapping returns None
                        };

                        // Special handling for MD007: Add style = "fixed" for markdownlint compatibility
                        if norm_rule_key == "MD007" && !table.contains_key("style") {
                            table.insert("style".to_string(), toml::Value::String("fixed".to_string()));
                        }

                        for (rk, rv) in table {
                            let norm_rk = crate::config::normalize_key(&rk);
                            let sv = rule_config.values.entry(norm_rk.clone()).or_insert_with(|| {
                                crate::config::SourcedValue::new(rv.clone(), crate::config::ConfigSource::ProjectConfig)
                            });
                            sv.push_override(rv, crate::config::ConfigSource::ProjectConfig, file.clone(), None);
                        }
                    } else {
                        rule_config
                            .values
                            .entry("value".to_string())
                            .and_modify(|sv| {
                                sv.value = tv.clone();
                                sv.source = crate::config::ConfigSource::ProjectConfig;
                                sv.overrides.push(crate::config::ConfigOverride {
                                    value: tv.clone(),
                                    source: crate::config::ConfigSource::ProjectConfig,
                                    file: file.clone(),
                                    line: None,
                                });
                            })
                            .or_insert_with(|| crate::config::SourcedValue {
                                value: tv.clone(),
                                source: crate::config::ConfigSource::ProjectConfig,
                                overrides: vec![crate::config::ConfigOverride {
                                    value: tv,
                                    source: crate::config::ConfigSource::ProjectConfig,
                                    file: file.clone(),
                                    line: None,
                                }],
                            });

                        // Special handling for MD007: Add style = "fixed" for markdownlint compatibility
                        if norm_rule_key == "MD007" && !rule_config.values.contains_key("style") {
                            rule_config.values.insert(
                                "style".to_string(),
                                crate::config::SourcedValue {
                                    value: toml::Value::String("fixed".to_string()),
                                    source: crate::config::ConfigSource::ProjectConfig,
                                    overrides: vec![crate::config::ConfigOverride {
                                        value: toml::Value::String("fixed".to_string()),
                                        source: crate::config::ConfigSource::ProjectConfig,
                                        file: file.clone(),
                                        line: None,
                                    }],
                                },
                            );
                        }
                    }
                }
            }
        }

        // Set all disabled rules at once
        if !disabled_rules.is_empty() {
            fragment.global.disable.push_override(
                disabled_rules,
                crate::config::ConfigSource::ProjectConfig,
                file.clone(),
                None,
            );
        }

        // Set all enabled rules at once
        if !enabled_rules.is_empty() {
            fragment.global.enable.push_override(
                enabled_rules,
                crate::config::ConfigSource::ProjectConfig,
                file.clone(),
                None,
            );
        }

        if let Some(_f) = file {
            // SourcedConfigFragment does not have loaded_files, so skip
        }
        fragment
    }
}

// NOTE: 'code-block-style' (MD046) and 'code-fence-style' (MD048) are distinct and must not be merged. See markdownlint docs for details.

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_markdownlint_to_rumdl_rule_key() {
        // Test direct rule names
        assert_eq!(markdownlint_to_rumdl_rule_key("MD001"), Some("MD001"));
        assert_eq!(markdownlint_to_rumdl_rule_key("MD058"), Some("MD058"));

        // Test aliases with hyphens
        assert_eq!(markdownlint_to_rumdl_rule_key("heading-increment"), Some("MD001"));
        assert_eq!(markdownlint_to_rumdl_rule_key("HEADING-INCREMENT"), Some("MD001"));
        assert_eq!(markdownlint_to_rumdl_rule_key("ul-style"), Some("MD004"));
        assert_eq!(markdownlint_to_rumdl_rule_key("no-trailing-spaces"), Some("MD009"));
        assert_eq!(markdownlint_to_rumdl_rule_key("line-length"), Some("MD013"));
        assert_eq!(markdownlint_to_rumdl_rule_key("single-title"), Some("MD025"));
        assert_eq!(markdownlint_to_rumdl_rule_key("single-h1"), Some("MD025"));
        assert_eq!(markdownlint_to_rumdl_rule_key("no-bare-urls"), Some("MD034"));
        assert_eq!(markdownlint_to_rumdl_rule_key("code-block-style"), Some("MD046"));
        assert_eq!(markdownlint_to_rumdl_rule_key("code-fence-style"), Some("MD048"));

        // Test aliases with underscores (should also work)
        assert_eq!(markdownlint_to_rumdl_rule_key("heading_increment"), Some("MD001"));
        assert_eq!(markdownlint_to_rumdl_rule_key("HEADING_INCREMENT"), Some("MD001"));
        assert_eq!(markdownlint_to_rumdl_rule_key("ul_style"), Some("MD004"));
        assert_eq!(markdownlint_to_rumdl_rule_key("no_trailing_spaces"), Some("MD009"));
        assert_eq!(markdownlint_to_rumdl_rule_key("line_length"), Some("MD013"));
        assert_eq!(markdownlint_to_rumdl_rule_key("single_title"), Some("MD025"));
        assert_eq!(markdownlint_to_rumdl_rule_key("single_h1"), Some("MD025"));
        assert_eq!(markdownlint_to_rumdl_rule_key("no_bare_urls"), Some("MD034"));
        assert_eq!(markdownlint_to_rumdl_rule_key("code_block_style"), Some("MD046"));
        assert_eq!(markdownlint_to_rumdl_rule_key("code_fence_style"), Some("MD048"));

        // Test case insensitivity
        assert_eq!(markdownlint_to_rumdl_rule_key("md001"), Some("MD001"));
        assert_eq!(markdownlint_to_rumdl_rule_key("Md001"), Some("MD001"));
        assert_eq!(markdownlint_to_rumdl_rule_key("Line-Length"), Some("MD013"));
        assert_eq!(markdownlint_to_rumdl_rule_key("Line_Length"), Some("MD013"));

        // Test invalid keys
        assert_eq!(markdownlint_to_rumdl_rule_key("MD999"), None);
        assert_eq!(markdownlint_to_rumdl_rule_key("invalid-rule"), None);
        assert_eq!(markdownlint_to_rumdl_rule_key(""), None);
    }

    #[test]
    fn test_normalize_toml_table_keys() {
        use toml::map::Map;

        // Test table normalization
        let mut table = Map::new();
        table.insert("snake_case".to_string(), toml::Value::String("value1".to_string()));
        table.insert("kebab-case".to_string(), toml::Value::String("value2".to_string()));
        table.insert("MD013".to_string(), toml::Value::Integer(100));

        let normalized = normalize_toml_table_keys(toml::Value::Table(table));

        if let toml::Value::Table(norm_table) = normalized {
            assert!(norm_table.contains_key("snake-case"));
            assert!(norm_table.contains_key("kebab-case"));
            assert!(norm_table.contains_key("MD013"));
            assert_eq!(
                norm_table.get("snake-case").unwrap(),
                &toml::Value::String("value1".to_string())
            );
            assert_eq!(
                norm_table.get("kebab-case").unwrap(),
                &toml::Value::String("value2".to_string())
            );
        } else {
            panic!("Expected normalized value to be a table");
        }

        // Test array normalization
        let array = toml::Value::Array(vec![toml::Value::String("test".to_string()), toml::Value::Integer(42)]);
        let normalized_array = normalize_toml_table_keys(array.clone());
        assert_eq!(normalized_array, array);

        // Test simple value passthrough
        let simple = toml::Value::String("simple".to_string());
        assert_eq!(normalize_toml_table_keys(simple.clone()), simple);
    }

    #[test]
    fn test_load_markdownlint_config_json() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(
            temp_file,
            r#"{{
            "MD013": {{ "line_length": 100 }},
            "MD025": true,
            "MD026": false,
            "heading-style": {{ "style": "atx" }}
        }}"#
        )
        .unwrap();

        let config = load_markdownlint_config(temp_file.path().to_str().unwrap()).unwrap();
        assert_eq!(config.0.len(), 4);
        assert!(config.0.contains_key("MD013"));
        assert!(config.0.contains_key("MD025"));
        assert!(config.0.contains_key("MD026"));
        assert!(config.0.contains_key("heading-style"));
    }

    #[test]
    fn test_load_markdownlint_config_yaml() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(
            temp_file,
            r#"MD013:
  line_length: 120
MD025: true
MD026: false
ul-style:
  style: dash"#
        )
        .unwrap();

        let path = temp_file.path().with_extension("yaml");
        std::fs::rename(temp_file.path(), &path).unwrap();

        let config = load_markdownlint_config(path.to_str().unwrap()).unwrap();
        assert_eq!(config.0.len(), 4);
        assert!(config.0.contains_key("MD013"));
        assert!(config.0.contains_key("ul-style"));
    }

    #[test]
    fn test_load_markdownlint_config_invalid() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "invalid json/yaml content {{").unwrap();

        let result = load_markdownlint_config(temp_file.path().to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_markdownlint_config_nonexistent() {
        let result = load_markdownlint_config("/nonexistent/file.json");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to read config file"));
    }

    #[test]
    fn test_map_to_sourced_rumdl_config() {
        let mut config_map = HashMap::new();
        config_map.insert(
            "MD013".to_string(),
            serde_yml::Value::Mapping({
                let mut map = serde_yml::Mapping::new();
                map.insert(
                    serde_yml::Value::String("line_length".to_string()),
                    serde_yml::Value::Number(serde_yml::Number::from(100)),
                );
                map
            }),
        );
        config_map.insert("MD025".to_string(), serde_yml::Value::Bool(true));
        config_map.insert("MD026".to_string(), serde_yml::Value::Bool(false));

        let mdl_config = MarkdownlintConfig(config_map);
        let sourced_config = mdl_config.map_to_sourced_rumdl_config(Some("test.json"));

        // Check MD013 mapping
        assert!(sourced_config.rules.contains_key("MD013"));
        let md013_config = &sourced_config.rules["MD013"];
        assert!(md013_config.values.contains_key("line-length"));
        assert_eq!(md013_config.values["line-length"].value, toml::Value::Integer(100));
        assert_eq!(md013_config.values["line-length"].source, ConfigSource::ProjectConfig);

        // Check that loaded_files is tracked
        assert_eq!(sourced_config.loaded_files.len(), 1);
        assert_eq!(sourced_config.loaded_files[0], "test.json");
    }

    #[test]
    fn test_map_to_sourced_rumdl_config_fragment() {
        let mut config_map = HashMap::new();

        // Test line-length alias for MD013 with numeric value
        config_map.insert(
            "line-length".to_string(),
            serde_yml::Value::Number(serde_yml::Number::from(120)),
        );

        // Test rule disable (false)
        config_map.insert("MD025".to_string(), serde_yml::Value::Bool(false));

        // Test rule enable (true)
        config_map.insert("MD026".to_string(), serde_yml::Value::Bool(true));

        // Test another rule with configuration
        config_map.insert(
            "MD003".to_string(),
            serde_yml::Value::Mapping({
                let mut map = serde_yml::Mapping::new();
                map.insert(
                    serde_yml::Value::String("style".to_string()),
                    serde_yml::Value::String("atx".to_string()),
                );
                map
            }),
        );

        let mdl_config = MarkdownlintConfig(config_map);
        let fragment = mdl_config.map_to_sourced_rumdl_config_fragment(Some("test.yaml"));

        // Check that line-length (MD013) was properly configured
        assert!(fragment.rules.contains_key("MD013"));
        let md013_config = &fragment.rules["MD013"];
        assert!(md013_config.values.contains_key("line-length"));
        assert_eq!(md013_config.values["line-length"].value, toml::Value::Integer(120));

        // Check disabled rule
        assert!(fragment.global.disable.value.contains(&"MD025".to_string()));

        // Check enabled rule
        assert!(fragment.global.enable.value.contains(&"MD026".to_string()));

        // Check rule configuration
        assert!(fragment.rules.contains_key("MD003"));
        let md003_config = &fragment.rules["MD003"];
        assert!(md003_config.values.contains_key("style"));
    }

    #[test]
    fn test_edge_cases() {
        let mut config_map = HashMap::new();

        // Test empty config
        let empty_config = MarkdownlintConfig(HashMap::new());
        let sourced = empty_config.map_to_sourced_rumdl_config(None);
        assert!(sourced.rules.is_empty());

        // Test unknown rule (should be ignored)
        config_map.insert("unknown-rule".to_string(), serde_yml::Value::Bool(true));
        config_map.insert("MD999".to_string(), serde_yml::Value::Bool(true));

        let config = MarkdownlintConfig(config_map);
        let sourced = config.map_to_sourced_rumdl_config(None);
        assert!(sourced.rules.is_empty()); // Unknown rules should be ignored
    }

    #[test]
    fn test_complex_rule_configurations() {
        let mut config_map = HashMap::new();

        // Test MD044 with array configuration
        config_map.insert(
            "MD044".to_string(),
            serde_yml::Value::Mapping({
                let mut map = serde_yml::Mapping::new();
                map.insert(
                    serde_yml::Value::String("names".to_string()),
                    serde_yml::Value::Sequence(vec![
                        serde_yml::Value::String("JavaScript".to_string()),
                        serde_yml::Value::String("GitHub".to_string()),
                    ]),
                );
                map
            }),
        );

        // Test nested configuration
        config_map.insert(
            "MD003".to_string(),
            serde_yml::Value::Mapping({
                let mut map = serde_yml::Mapping::new();
                map.insert(
                    serde_yml::Value::String("style".to_string()),
                    serde_yml::Value::String("atx".to_string()),
                );
                map
            }),
        );

        let mdl_config = MarkdownlintConfig(config_map);
        let sourced = mdl_config.map_to_sourced_rumdl_config(None);

        // Verify MD044 configuration
        assert!(sourced.rules.contains_key("MD044"));
        let md044_config = &sourced.rules["MD044"];
        assert!(md044_config.values.contains_key("names"));

        // Verify MD003 configuration
        assert!(sourced.rules.contains_key("MD003"));
        let md003_config = &sourced.rules["MD003"];
        assert!(md003_config.values.contains_key("style"));
        assert_eq!(
            md003_config.values["style"].value,
            toml::Value::String("atx".to_string())
        );
    }

    #[test]
    fn test_value_types() {
        let mut config_map = HashMap::new();

        // Test different value types
        config_map.insert(
            "MD007".to_string(),
            serde_yml::Value::Number(serde_yml::Number::from(4)),
        ); // Simple number
        config_map.insert(
            "MD009".to_string(),
            serde_yml::Value::Mapping({
                let mut map = serde_yml::Mapping::new();
                map.insert(
                    serde_yml::Value::String("br_spaces".to_string()),
                    serde_yml::Value::Number(serde_yml::Number::from(2)),
                );
                map.insert(
                    serde_yml::Value::String("strict".to_string()),
                    serde_yml::Value::Bool(true),
                );
                map
            }),
        );

        let mdl_config = MarkdownlintConfig(config_map);
        let sourced = mdl_config.map_to_sourced_rumdl_config(None);

        // Check simple number value
        assert!(sourced.rules.contains_key("MD007"));
        assert!(sourced.rules["MD007"].values.contains_key("value"));

        // Check complex configuration
        assert!(sourced.rules.contains_key("MD009"));
        let md009_config = &sourced.rules["MD009"];
        assert!(md009_config.values.contains_key("br-spaces"));
        assert!(md009_config.values.contains_key("strict"));
    }

    #[test]
    fn test_all_rule_aliases() {
        // Test that all documented aliases map correctly
        let aliases = vec![
            ("heading-increment", "MD001"),
            ("heading-style", "MD003"),
            ("ul-style", "MD004"),
            ("list-indent", "MD005"),
            ("ul-indent", "MD007"),
            ("no-trailing-spaces", "MD009"),
            ("no-hard-tabs", "MD010"),
            ("no-reversed-links", "MD011"),
            ("no-multiple-blanks", "MD012"),
            ("line-length", "MD013"),
            ("commands-show-output", "MD014"),
            // MD015-017 don't exist in markdownlint
            ("no-missing-space-atx", "MD018"),
            ("no-multiple-space-atx", "MD019"),
            ("no-missing-space-closed-atx", "MD020"),
            ("no-multiple-space-closed-atx", "MD021"),
            ("blanks-around-headings", "MD022"),
            ("heading-start-left", "MD023"),
            ("no-duplicate-heading", "MD024"),
            ("single-title", "MD025"),
            ("single-h1", "MD025"),
            ("no-trailing-punctuation", "MD026"),
            ("no-multiple-space-blockquote", "MD027"),
            ("no-blanks-blockquote", "MD028"),
            ("ol-prefix", "MD029"),
            ("list-marker-space", "MD030"),
            ("blanks-around-fences", "MD031"),
            ("blanks-around-lists", "MD032"),
            ("no-inline-html", "MD033"),
            ("no-bare-urls", "MD034"),
            ("hr-style", "MD035"),
            ("no-emphasis-as-heading", "MD036"),
            ("no-space-in-emphasis", "MD037"),
            ("no-space-in-code", "MD038"),
            ("no-space-in-links", "MD039"),
            ("fenced-code-language", "MD040"),
            ("first-line-heading", "MD041"),
            ("first-line-h1", "MD041"),
            ("no-empty-links", "MD042"),
            ("required-headings", "MD043"),
            ("proper-names", "MD044"),
            ("no-alt-text", "MD045"),
            ("code-block-style", "MD046"),
            ("single-trailing-newline", "MD047"),
            ("code-fence-style", "MD048"),
            ("emphasis-style", "MD049"),
            ("strong-style", "MD050"),
            ("link-fragments", "MD051"),
            ("reference-links-images", "MD052"),
            ("link-image-reference-definitions", "MD053"),
            ("link-image-style", "MD054"),
            ("table-pipe-style", "MD055"),
            ("table-column-count", "MD056"),
            ("existing-relative-links", "MD057"),
            ("blanks-around-tables", "MD058"),
            ("descriptive-link-text", "MD059"),
            ("table-cell-alignment", "MD060"),
            ("table-format", "MD060"),
            ("forbidden-terms", "MD061"),
            ("nested-code-fence", "MD070"),
            ("blank-line-after-frontmatter", "MD071"),
            ("frontmatter-key-sort", "MD072"),
        ];

        for (alias, expected) in aliases {
            assert_eq!(
                markdownlint_to_rumdl_rule_key(alias),
                Some(expected),
                "Alias {alias} should map to {expected}"
            );
        }
    }
}
