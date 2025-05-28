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
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read config file {}: {}", path, e))?;

    if path.ends_with(".json") || path.ends_with(".jsonc") {
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {}", e))
    } else if path.ends_with(".yaml") || path.ends_with(".yml") {
        serde_yaml::from_str(&content).map_err(|e| format!("Failed to parse YAML: {}", e))
    } else {
        serde_json::from_str(&content)
            .or_else(|_| serde_yaml::from_str(&content))
            .map_err(|e| format!("Failed to parse config as JSON or YAML: {}", e))
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
        toml::Value::Array(arr) => {
            toml::Value::Array(arr.into_iter().map(normalize_toml_table_keys).collect())
        }
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
                let toml_value: Option<toml::Value> =
                    serde_yaml::from_value::<toml::Value>(value.clone()).ok();
                let toml_value = toml_value.map(normalize_toml_table_keys);
                let rule_config = sourced_config
                    .rules
                    .entry(norm_rule_key.clone())
                    .or_default();
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
                    log::error!("Could not convert value for rule key {:?} to rumdl's internal config format. This likely means the configuration value is invalid or not supported for this rule. Please check your markdownlint config.", key);
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
                let toml_value: Option<toml::Value> =
                    serde_yaml::from_value::<toml::Value>(value.clone()).ok();
                let toml_value = toml_value.map(normalize_toml_table_keys);
                let rule_config = fragment.rules.entry(norm_rule_key.clone()).or_default();
                if let Some(tv) = toml_value {
                    if let toml::Value::Table(table) = tv {
                        for (rk, rv) in table {
                            let norm_rk = crate::config::normalize_key(&rk);
                            let sv =
                                rule_config
                                    .values
                                    .entry(norm_rk.clone())
                                    .or_insert_with(|| {
                                        crate::config::SourcedValue::new(
                                            rv.clone(),
                                            crate::config::ConfigSource::Markdownlint,
                                        )
                                    });
                            sv.push_override(
                                rv,
                                crate::config::ConfigSource::Markdownlint,
                                file.clone(),
                                None,
                            );
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
