//!
//! This module handles parsing and mapping markdownlint config files (JSON/YAML) to rumdl's internal config format.
//! It provides mapping from markdownlint rule keys to rumdl rule keys and provenance tracking for configuration values.

use crate::config::{ConfigSource, SourcedConfig, SourcedValue};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

/// Represents a generic markdownlint config (rule keys to values)
#[derive(Debug, Deserialize)]
pub struct MarkdownlintConfig(pub HashMap<String, serde_yaml::Value>);

/// Load a markdownlint config file (JSON or YAML) from the given path
pub fn load_markdownlint_config(path: &str) -> Result<MarkdownlintConfig, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read config file {path}: {e}"))?;

    if path.ends_with(".json") || path.ends_with(".jsonc") {
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {e}"))
    } else if path.ends_with(".yaml") || path.ends_with(".yml") {
        serde_yaml::from_str(&content).map_err(|e| format!("Failed to parse YAML: {e}"))
    } else {
        serde_json::from_str(&content)
            .or_else(|_| serde_yaml::from_str(&content))
            .map_err(|e| format!("Failed to parse config as JSON or YAML: {e}"))
    }
}

/// Mapping table from markdownlint rule keys/aliases to rumdl rule keys
fn markdownlint_to_rumdl_rule_key(key: &str) -> Option<&'static str> {
    match key.to_ascii_uppercase().as_str() {
        "MD001" | "HEADING-INCREMENT" => Some("MD001"),
        "MD002" | "FIRST-HEADING-H1" => Some("MD002"),
        "MD003" | "HEADING-STYLE" => Some("MD003"),
        "MD004" | "UL-STYLE" => Some("MD004"),
        "MD005" | "LIST-INDENT" => Some("MD005"),
        "MD006" | "UL-START-LEFT" => Some("MD006"),
        "MD007" | "UL-INDENT" => Some("MD007"),
        "MD008" => Some("MD008"),
        "MD009" | "NO-TRAILING-SPACES" => Some("MD009"),
        "MD010" | "NO-HARD-TABS" => Some("MD010"),
        "MD011" | "NO-REVERSED-LINKS" => Some("MD011"),
        "MD012" | "NO-MULTIPLE-BLANKS" => Some("MD012"),
        "MD013" | "LINE-LENGTH" => Some("MD013"),
        "MD014" | "COMMANDS-SHOW-OUTPUT" => Some("MD014"),
        "MD015" | "NO-MISSING-SPACE-AFTER-LIST-MARKER" => Some("MD015"),
        "MD018" | "NO-MISSING-SPACE-ATX" => Some("MD018"),
        "MD019" | "NO-MULTIPLE-SPACE-ATX" => Some("MD019"),
        "MD020" | "NO-MISSING-SPACE-CLOSED-ATX" => Some("MD020"),
        "MD021" | "NO-MULTIPLE-SPACE-CLOSED-ATX" => Some("MD021"),
        "MD022" | "BLANKS-AROUND-HEADINGS" => Some("MD022"),
        "MD023" | "HEADING-START-LEFT" => Some("MD023"),
        "MD024" | "NO-DUPLICATE-HEADING" => Some("MD024"),
        "MD025" | "SINGLE-TITLE" | "SINGLE-H1" => Some("MD025"),
        "MD026" | "NO-TRAILING-PUNCTUATION" => Some("MD026"),
        "MD027" | "NO-MULTIPLE-SPACE-BLOCKQUOTE" => Some("MD027"),
        "MD028" | "NO-BLANKS-BLOCKQUOTE" => Some("MD028"),
        "MD029" | "OL-PREFIX" => Some("MD029"),
        "MD030" | "LIST-MARKER-SPACE" => Some("MD030"),
        "MD031" | "BLANKS-AROUND-FENCES" => Some("MD031"),
        "MD032" | "BLANKS-AROUND-LISTS" => Some("MD032"),
        "MD033" | "NO-INLINE-HTML" => Some("MD033"),
        "MD034" | "NO-BARE-URLS" => Some("MD034"),
        "MD035" | "HR-STYLE" => Some("MD035"),
        "MD036" | "NO-EMPHASIS-AS-HEADING" => Some("MD036"),
        "MD037" | "NO-SPACE-IN-EMPHASIS" => Some("MD037"),
        "MD038" | "NO-SPACE-IN-CODE" => Some("MD038"),
        "MD039" | "NO-SPACE-IN-LINKS" => Some("MD039"),
        "MD040" | "FENCED-CODE-LANGUAGE" => Some("MD040"),
        "MD041" | "FIRST-LINE-HEADING" | "FIRST-LINE-H1" => Some("MD041"),
        "MD042" | "NO-EMPTY-LINKS" => Some("MD042"),
        "MD043" | "REQUIRED-HEADINGS" => Some("MD043"),
        "MD044" | "PROPER-NAMES" => Some("MD044"),
        "MD045" | "NO-ALT-TEXT" => Some("MD045"),
        "MD046" | "CODE-BLOCK-STYLE" => Some("MD046"),
        "MD047" | "SINGLE-TRAILING-NEWLINE" => Some("MD047"),
        "MD048" | "CODE-FENCE-STYLE" => Some("MD048"),
        "MD049" | "EMPHASIS-STYLE" => Some("MD049"),
        "MD050" | "STRONG-STYLE" => Some("MD050"),
        "MD051" | "LINK-FRAGMENTS" => Some("MD051"),
        "MD052" | "REFERENCE-LINKS-IMAGES" => Some("MD052"),
        "MD053" | "LINK-IMAGE-REFERENCE-DEFINITIONS" => Some("MD053"),
        "MD054" | "LINK-IMAGE-STYLE" => Some("MD054"),
        "MD055" | "TABLE-PIPE-STYLE" => Some("MD055"),
        "MD056" | "TABLE-COLUMN-COUNT" => Some("MD056"),
        "MD057" | "EXISTING-RELATIVE-LINKS" => Some("MD057"),
        "MD058" | "BLANKS-AROUND-TABLES" => Some("MD058"),
        _ => None,
    }
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
                let toml_value: Option<toml::Value> = serde_yaml::from_value::<toml::Value>(value.clone()).ok();
                let toml_value = toml_value.map(normalize_toml_table_keys);
                let rule_config = sourced_config.rules.entry(norm_rule_key.clone()).or_default();
                if let Some(tv) = toml_value {
                    if let toml::Value::Table(table) = tv {
                        for (k, v) in table {
                            let norm_config_key = k; // Already normalized
                            rule_config
                                .values
                                .entry(norm_config_key.clone())
                                .and_modify(|sv| {
                                    sv.value = v.clone();
                                    sv.source = ConfigSource::Markdownlint;
                                    sv.overrides.push(crate::config::ConfigOverride {
                                        value: v.clone(),
                                        source: ConfigSource::Markdownlint,
                                        file: file.clone(),
                                        line: None,
                                    });
                                })
                                .or_insert_with(|| SourcedValue {
                                    value: v.clone(),
                                    source: ConfigSource::Markdownlint,
                                    overrides: vec![crate::config::ConfigOverride {
                                        value: v,
                                        source: ConfigSource::Markdownlint,
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
                                sv.source = ConfigSource::Markdownlint;
                                sv.overrides.push(crate::config::ConfigOverride {
                                    value: tv.clone(),
                                    source: ConfigSource::Markdownlint,
                                    file: file.clone(),
                                    line: None,
                                });
                            })
                            .or_insert_with(|| SourcedValue {
                                value: tv.clone(),
                                source: ConfigSource::Markdownlint,
                                overrides: vec![crate::config::ConfigOverride {
                                    value: tv,
                                    source: ConfigSource::Markdownlint,
                                    file: file.clone(),
                                    line: None,
                                }],
                            });
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
        for (key, value) in &self.0 {
            // Special handling for line-length as a global setting
            if key.eq_ignore_ascii_case("line-length") || key.eq_ignore_ascii_case("line_length") {
                if let Some(line_length) = value.as_u64() {
                    fragment.global.line_length.push_override(
                        line_length,
                        crate::config::ConfigSource::Markdownlint,
                        file.clone(),
                        None,
                    );
                }
                continue;
            }

            let mapped = markdownlint_to_rumdl_rule_key(key);
            if let Some(rumdl_key) = mapped {
                let norm_rule_key = rumdl_key.to_ascii_uppercase();
                // Special handling for boolean values (true/false)
                if value.is_bool() {
                    if !value.as_bool().unwrap_or(false) {
                        // Add to global.disable
                        fragment.global.disable.push_override(
                            vec![norm_rule_key.clone()],
                            crate::config::ConfigSource::Markdownlint,
                            file.clone(),
                            None,
                        );
                    } else {
                        // Add to global.enable (if true)
                        fragment.global.enable.push_override(
                            vec![norm_rule_key.clone()],
                            crate::config::ConfigSource::Markdownlint,
                            file.clone(),
                            None,
                        );
                    }
                    continue;
                }
                let toml_value: Option<toml::Value> = serde_yaml::from_value::<toml::Value>(value.clone()).ok();
                let toml_value = toml_value.map(normalize_toml_table_keys);
                let rule_config = fragment.rules.entry(norm_rule_key.clone()).or_default();
                if let Some(tv) = toml_value {
                    if let toml::Value::Table(table) = tv {
                        for (rk, rv) in table {
                            let norm_rk = crate::config::normalize_key(&rk);
                            let sv = rule_config.values.entry(norm_rk.clone()).or_insert_with(|| {
                                crate::config::SourcedValue::new(rv.clone(), crate::config::ConfigSource::Markdownlint)
                            });
                            sv.push_override(rv, crate::config::ConfigSource::Markdownlint, file.clone(), None);
                        }
                    } else {
                        rule_config
                            .values
                            .entry("value".to_string())
                            .and_modify(|sv| {
                                sv.value = tv.clone();
                                sv.source = crate::config::ConfigSource::Markdownlint;
                                sv.overrides.push(crate::config::ConfigOverride {
                                    value: tv.clone(),
                                    source: crate::config::ConfigSource::Markdownlint,
                                    file: file.clone(),
                                    line: None,
                                });
                            })
                            .or_insert_with(|| crate::config::SourcedValue {
                                value: tv.clone(),
                                source: crate::config::ConfigSource::Markdownlint,
                                overrides: vec![crate::config::ConfigOverride {
                                    value: tv,
                                    source: crate::config::ConfigSource::Markdownlint,
                                    file: file.clone(),
                                    line: None,
                                }],
                            });
                    }
                }
            }
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

        // Test aliases
        assert_eq!(markdownlint_to_rumdl_rule_key("heading-increment"), Some("MD001"));
        assert_eq!(markdownlint_to_rumdl_rule_key("HEADING-INCREMENT"), Some("MD001"));
        assert_eq!(markdownlint_to_rumdl_rule_key("first-heading-h1"), Some("MD002"));
        assert_eq!(markdownlint_to_rumdl_rule_key("ul-style"), Some("MD004"));
        assert_eq!(markdownlint_to_rumdl_rule_key("no-trailing-spaces"), Some("MD009"));
        assert_eq!(markdownlint_to_rumdl_rule_key("line-length"), Some("MD013"));
        assert_eq!(markdownlint_to_rumdl_rule_key("single-title"), Some("MD025"));
        assert_eq!(markdownlint_to_rumdl_rule_key("single-h1"), Some("MD025"));
        assert_eq!(markdownlint_to_rumdl_rule_key("no-bare-urls"), Some("MD034"));
        assert_eq!(markdownlint_to_rumdl_rule_key("code-block-style"), Some("MD046"));
        assert_eq!(markdownlint_to_rumdl_rule_key("code-fence-style"), Some("MD048"));

        // Test case insensitivity
        assert_eq!(markdownlint_to_rumdl_rule_key("md001"), Some("MD001"));
        assert_eq!(markdownlint_to_rumdl_rule_key("Md001"), Some("MD001"));
        assert_eq!(markdownlint_to_rumdl_rule_key("Line-Length"), Some("MD013"));

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
            serde_yaml::Value::Mapping({
                let mut map = serde_yaml::Mapping::new();
                map.insert(
                    serde_yaml::Value::String("line_length".to_string()),
                    serde_yaml::Value::Number(serde_yaml::Number::from(100)),
                );
                map
            }),
        );
        config_map.insert("MD025".to_string(), serde_yaml::Value::Bool(true));
        config_map.insert("MD026".to_string(), serde_yaml::Value::Bool(false));

        let mdl_config = MarkdownlintConfig(config_map);
        let sourced_config = mdl_config.map_to_sourced_rumdl_config(Some("test.json"));

        // Check MD013 mapping
        assert!(sourced_config.rules.contains_key("MD013"));
        let md013_config = &sourced_config.rules["MD013"];
        assert!(md013_config.values.contains_key("line-length"));
        assert_eq!(md013_config.values["line-length"].value, toml::Value::Integer(100));
        assert_eq!(md013_config.values["line-length"].source, ConfigSource::Markdownlint);

        // Check that loaded_files is tracked
        assert_eq!(sourced_config.loaded_files.len(), 1);
        assert_eq!(sourced_config.loaded_files[0], "test.json");
    }

    #[test]
    fn test_map_to_sourced_rumdl_config_fragment() {
        let mut config_map = HashMap::new();

        // Test global line-length setting
        config_map.insert(
            "line-length".to_string(),
            serde_yaml::Value::Number(serde_yaml::Number::from(120)),
        );

        // Test rule disable (false)
        config_map.insert("MD025".to_string(), serde_yaml::Value::Bool(false));

        // Test rule enable (true)
        config_map.insert("MD026".to_string(), serde_yaml::Value::Bool(true));

        // Test rule with configuration
        config_map.insert(
            "MD013".to_string(),
            serde_yaml::Value::Mapping({
                let mut map = serde_yaml::Mapping::new();
                map.insert(
                    serde_yaml::Value::String("line_length".to_string()),
                    serde_yaml::Value::Number(serde_yaml::Number::from(100)),
                );
                map
            }),
        );

        let mdl_config = MarkdownlintConfig(config_map);
        let fragment = mdl_config.map_to_sourced_rumdl_config_fragment(Some("test.yaml"));

        // Check global line-length
        assert_eq!(fragment.global.line_length.value, 120);
        assert_eq!(fragment.global.line_length.source, ConfigSource::Markdownlint);

        // Check disabled rule
        assert!(fragment.global.disable.value.contains(&"MD025".to_string()));

        // Check enabled rule
        assert!(fragment.global.enable.value.contains(&"MD026".to_string()));

        // Check rule configuration
        assert!(fragment.rules.contains_key("MD013"));
        let md013_config = &fragment.rules["MD013"];
        assert!(md013_config.values.contains_key("line-length"));
    }

    #[test]
    fn test_edge_cases() {
        let mut config_map = HashMap::new();

        // Test empty config
        let empty_config = MarkdownlintConfig(HashMap::new());
        let sourced = empty_config.map_to_sourced_rumdl_config(None);
        assert!(sourced.rules.is_empty());

        // Test unknown rule (should be ignored)
        config_map.insert("unknown-rule".to_string(), serde_yaml::Value::Bool(true));
        config_map.insert("MD999".to_string(), serde_yaml::Value::Bool(true));

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
            serde_yaml::Value::Mapping({
                let mut map = serde_yaml::Mapping::new();
                map.insert(
                    serde_yaml::Value::String("names".to_string()),
                    serde_yaml::Value::Sequence(vec![
                        serde_yaml::Value::String("JavaScript".to_string()),
                        serde_yaml::Value::String("GitHub".to_string()),
                    ]),
                );
                map
            }),
        );

        // Test nested configuration
        config_map.insert(
            "MD003".to_string(),
            serde_yaml::Value::Mapping({
                let mut map = serde_yaml::Mapping::new();
                map.insert(
                    serde_yaml::Value::String("style".to_string()),
                    serde_yaml::Value::String("atx".to_string()),
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
            serde_yaml::Value::Number(serde_yaml::Number::from(4)),
        ); // Simple number
        config_map.insert(
            "MD009".to_string(),
            serde_yaml::Value::Mapping({
                let mut map = serde_yaml::Mapping::new();
                map.insert(
                    serde_yaml::Value::String("br_spaces".to_string()),
                    serde_yaml::Value::Number(serde_yaml::Number::from(2)),
                );
                map.insert(
                    serde_yaml::Value::String("strict".to_string()),
                    serde_yaml::Value::Bool(true),
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
            ("first-heading-h1", "MD002"),
            ("heading-style", "MD003"),
            ("ul-style", "MD004"),
            ("list-indent", "MD005"),
            ("ul-start-left", "MD006"),
            ("ul-indent", "MD007"),
            ("no-trailing-spaces", "MD009"),
            ("no-hard-tabs", "MD010"),
            ("no-reversed-links", "MD011"),
            ("no-multiple-blanks", "MD012"),
            ("line-length", "MD013"),
            ("commands-show-output", "MD014"),
            ("no-missing-space-after-list-marker", "MD015"),
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
