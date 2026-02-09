use crate::types::LineLength;
use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use toml_edit::DocumentMut;

use super::flavor::{MarkdownFlavor, normalize_key, warn_comma_without_brace_in_pattern};
use super::source_tracking::{ConfigSource, SourcedConfigFragment, SourcedValue};
use super::types::ConfigError;
use super::validation::to_relative_display_path;

/// Parses pyproject.toml content and extracts the [tool.rumdl] section if present.
pub(super) fn parse_pyproject_toml(content: &str, path: &str) -> Result<Option<SourcedConfigFragment>, ConfigError> {
    let display_path = to_relative_display_path(path);
    let doc: toml::Value = toml::from_str(content)
        .map_err(|e| ConfigError::ParseError(format!("{display_path}: Failed to parse TOML: {e}")))?;
    let mut fragment = SourcedConfigFragment::default();
    let source = ConfigSource::PyprojectToml;
    let file = Some(path.to_string());

    // Use the lazily-initialized default registry for alias resolution and schema validation
    let registry = super::registry::default_registry();

    // 1. Handle [tool.rumdl] and [tool.rumdl.global] sections
    if let Some(rumdl_config) = doc.get("tool").and_then(|t| t.get("rumdl"))
        && let Some(rumdl_table) = rumdl_config.as_table()
    {
        // Helper function to extract global config from a table
        let extract_global_config = |fragment: &mut SourcedConfigFragment, table: &toml::value::Table| {
            // Extract global options from the given table
            if let Some(enable) = table.get("enable")
                && let Ok(values) = Vec::<String>::deserialize(enable.clone())
            {
                // Resolve rule name aliases (e.g., "ul-style" -> "MD004")
                let normalized_values: Vec<String> = values
                    .into_iter()
                    .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
                    .collect();
                // Only push if non-empty: an empty enable list in rumdl config means
                // "no filter" (all rules enabled), not "disable all rules".
                // The empty-but-explicit case is only valid for markdownlint's `default: false`.
                if !normalized_values.is_empty() {
                    fragment
                        .global
                        .enable
                        .push_override(normalized_values, source, file.clone(), None);
                }
            }

            if let Some(disable) = table.get("disable")
                && let Ok(values) = Vec::<String>::deserialize(disable.clone())
            {
                // Resolve rule name aliases
                let normalized_values: Vec<String> = values
                    .into_iter()
                    .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
                    .collect();
                fragment
                    .global
                    .disable
                    .push_override(normalized_values, source, file.clone(), None);
            }

            if let Some(include) = table.get("include")
                && let Ok(values) = Vec::<String>::deserialize(include.clone())
            {
                fragment
                    .global
                    .include
                    .push_override(values, source, file.clone(), None);
            }

            if let Some(exclude) = table.get("exclude")
                && let Ok(values) = Vec::<String>::deserialize(exclude.clone())
            {
                fragment
                    .global
                    .exclude
                    .push_override(values, source, file.clone(), None);
            }

            if let Some(respect_gitignore) = table
                .get("respect-gitignore")
                .or_else(|| table.get("respect_gitignore"))
                && let Ok(value) = bool::deserialize(respect_gitignore.clone())
            {
                fragment
                    .global
                    .respect_gitignore
                    .push_override(value, source, file.clone(), None);
            }

            if let Some(force_exclude) = table.get("force-exclude").or_else(|| table.get("force_exclude"))
                && let Ok(value) = bool::deserialize(force_exclude.clone())
            {
                fragment
                    .global
                    .force_exclude
                    .push_override(value, source, file.clone(), None);
            }

            if let Some(output_format) = table.get("output-format").or_else(|| table.get("output_format"))
                && let Ok(value) = String::deserialize(output_format.clone())
            {
                if fragment.global.output_format.is_none() {
                    fragment.global.output_format = Some(SourcedValue::new(value.clone(), source));
                } else {
                    fragment
                        .global
                        .output_format
                        .as_mut()
                        .unwrap()
                        .push_override(value, source, file.clone(), None);
                }
            }

            if let Some(fixable) = table.get("fixable")
                && let Ok(values) = Vec::<String>::deserialize(fixable.clone())
            {
                let normalized_values = values
                    .into_iter()
                    .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
                    .collect();
                fragment
                    .global
                    .fixable
                    .push_override(normalized_values, source, file.clone(), None);
            }

            if let Some(unfixable) = table.get("unfixable")
                && let Ok(values) = Vec::<String>::deserialize(unfixable.clone())
            {
                let normalized_values = values
                    .into_iter()
                    .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
                    .collect();
                fragment
                    .global
                    .unfixable
                    .push_override(normalized_values, source, file.clone(), None);
            }

            if let Some(flavor) = table.get("flavor")
                && let Ok(value) = MarkdownFlavor::deserialize(flavor.clone())
            {
                fragment.global.flavor.push_override(value, source, file.clone(), None);
            }

            // Handle line-length special case - this should set the global line_length
            if let Some(line_length) = table.get("line-length").or_else(|| table.get("line_length"))
                && let Ok(value) = u64::deserialize(line_length.clone())
            {
                fragment
                    .global
                    .line_length
                    .push_override(LineLength::new(value as usize), source, file.clone(), None);

                // Also add to MD013 rule config for backward compatibility
                let norm_md013_key = normalize_key("MD013");
                let rule_entry = fragment.rules.entry(norm_md013_key).or_default();
                let norm_line_length_key = normalize_key("line-length");
                let sv = rule_entry
                    .values
                    .entry(norm_line_length_key)
                    .or_insert_with(|| SourcedValue::new(line_length.clone(), ConfigSource::Default));
                sv.push_override(line_length.clone(), source, file.clone(), None);
            }

            if let Some(cache_dir) = table.get("cache-dir").or_else(|| table.get("cache_dir"))
                && let Ok(value) = String::deserialize(cache_dir.clone())
            {
                if fragment.global.cache_dir.is_none() {
                    fragment.global.cache_dir = Some(SourcedValue::new(value.clone(), source));
                } else {
                    fragment
                        .global
                        .cache_dir
                        .as_mut()
                        .unwrap()
                        .push_override(value, source, file.clone(), None);
                }
            }

            if let Some(cache) = table.get("cache")
                && let Ok(value) = bool::deserialize(cache.clone())
            {
                fragment.global.cache.push_override(value, source, file.clone(), None);
            }
        };

        // First, check for [tool.rumdl.global] section
        if let Some(global_table) = rumdl_table.get("global").and_then(|g| g.as_table()) {
            extract_global_config(&mut fragment, global_table);
        }

        // Also extract global options from [tool.rumdl] directly (for flat structure)
        extract_global_config(&mut fragment, rumdl_table);

        // --- Extract per-file-ignores configurations ---
        // Check both hyphenated and underscored versions for compatibility
        let per_file_ignores_key = rumdl_table
            .get("per-file-ignores")
            .or_else(|| rumdl_table.get("per_file_ignores"));

        if let Some(per_file_ignores_value) = per_file_ignores_key
            && let Some(per_file_table) = per_file_ignores_value.as_table()
        {
            let mut per_file_map = HashMap::new();
            for (pattern, rules_value) in per_file_table {
                warn_comma_without_brace_in_pattern(pattern, &display_path);
                if let Ok(rules) = Vec::<String>::deserialize(rules_value.clone()) {
                    let normalized_rules = rules
                        .into_iter()
                        .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
                        .collect();
                    per_file_map.insert(pattern.clone(), normalized_rules);
                } else {
                    log::warn!(
                        "[WARN] Expected array for per-file-ignores pattern '{pattern}' in {display_path}, found {rules_value:?}"
                    );
                }
            }
            fragment
                .per_file_ignores
                .push_override(per_file_map, source, file.clone(), None);
        }

        // --- Extract per-file-flavor configurations ---
        // Check both hyphenated and underscored versions for compatibility
        let per_file_flavor_key = rumdl_table
            .get("per-file-flavor")
            .or_else(|| rumdl_table.get("per_file_flavor"));

        if let Some(per_file_flavor_value) = per_file_flavor_key
            && let Some(per_file_table) = per_file_flavor_value.as_table()
        {
            let mut per_file_map = IndexMap::new();
            for (pattern, flavor_value) in per_file_table {
                if let Ok(flavor) = MarkdownFlavor::deserialize(flavor_value.clone()) {
                    per_file_map.insert(pattern.clone(), flavor);
                } else {
                    log::warn!(
                        "[WARN] Invalid flavor for per-file-flavor pattern '{pattern}' in {display_path}, found {flavor_value:?}. Valid values: standard, mkdocs, mdx, quarto"
                    );
                }
            }
            fragment
                .per_file_flavor
                .push_override(per_file_map, source, file.clone(), None);
        }

        // --- Extract rule-specific configurations ---
        for (key, value) in rumdl_table {
            let norm_rule_key = normalize_key(key);

            // Skip keys already handled as global or special cases
            // Note: Only skip these if they're NOT tables (rule sections are tables)
            let is_global_key = [
                "enable",
                "disable",
                "include",
                "exclude",
                "respect_gitignore",
                "respect-gitignore",
                "force_exclude",
                "force-exclude",
                "output_format",
                "output-format",
                "fixable",
                "unfixable",
                "per-file-ignores",
                "per_file_ignores",
                "per-file-flavor",
                "per_file_flavor",
                "global",
                "flavor",
                "cache_dir",
                "cache-dir",
                "cache",
            ]
            .contains(&norm_rule_key.as_str());

            // Special handling for line-length: could be global config OR rule section
            let is_line_length_global =
                (norm_rule_key == "line-length" || norm_rule_key == "line_length") && !value.is_table();

            if is_global_key || is_line_length_global {
                continue;
            }

            // Try to resolve as a rule name (handles both canonical names and aliases)
            if let Some(resolved_rule_name) = registry.resolve_rule_name(key)
                && value.is_table()
                && let Some(rule_config_table) = value.as_table()
            {
                let rule_entry = fragment.rules.entry(resolved_rule_name.clone()).or_default();
                for (rk, rv) in rule_config_table {
                    let norm_rk = normalize_key(rk);

                    // Special handling for severity - store in rule_entry.severity
                    if norm_rk == "severity" {
                        if let Ok(severity) = crate::rule::Severity::deserialize(rv.clone()) {
                            if rule_entry.severity.is_none() {
                                rule_entry.severity = Some(SourcedValue::new(severity, source));
                            } else {
                                rule_entry.severity.as_mut().unwrap().push_override(
                                    severity,
                                    source,
                                    file.clone(),
                                    None,
                                );
                            }
                        }
                        continue; // Skip regular value processing for severity
                    }

                    let toml_val = rv.clone();

                    let sv = rule_entry
                        .values
                        .entry(norm_rk.clone())
                        .or_insert_with(|| SourcedValue::new(toml_val.clone(), ConfigSource::Default));
                    sv.push_override(toml_val, source, file.clone(), None);
                }
            } else if registry.resolve_rule_name(key).is_none() {
                // Key is not a global/special key and not a recognized rule name
                // Track unknown keys under [tool.rumdl] for validation
                fragment
                    .unknown_keys
                    .push(("[tool.rumdl]".to_string(), key.to_string(), Some(path.to_string())));
            }
        }
    }

    // 2. Handle [tool.rumdl.MDxxx] sections as rule-specific config (nested under [tool])
    if let Some(tool_table) = doc.get("tool").and_then(|t| t.as_table()) {
        for (key, value) in tool_table.iter() {
            if let Some(rule_name) = key.strip_prefix("rumdl.") {
                // Try to resolve as a rule name (handles both canonical names and aliases)
                if let Some(resolved_rule_name) = registry.resolve_rule_name(rule_name) {
                    if let Some(rule_table) = value.as_table() {
                        let rule_entry = fragment.rules.entry(resolved_rule_name.clone()).or_default();
                        for (rk, rv) in rule_table {
                            let norm_rk = normalize_key(rk);

                            // Special handling for severity - store in rule_entry.severity
                            if norm_rk == "severity" {
                                if let Ok(severity) = crate::rule::Severity::deserialize(rv.clone()) {
                                    if rule_entry.severity.is_none() {
                                        rule_entry.severity = Some(SourcedValue::new(severity, source));
                                    } else {
                                        rule_entry.severity.as_mut().unwrap().push_override(
                                            severity,
                                            source,
                                            file.clone(),
                                            None,
                                        );
                                    }
                                }
                                continue; // Skip regular value processing for severity
                            }

                            let toml_val = rv.clone();
                            let sv = rule_entry
                                .values
                                .entry(norm_rk.clone())
                                .or_insert_with(|| SourcedValue::new(toml_val.clone(), source));
                            sv.push_override(toml_val, source, file.clone(), None);
                        }
                    }
                } else if rule_name.to_ascii_uppercase().starts_with("MD")
                    || rule_name.chars().any(|c| c.is_alphabetic())
                {
                    // Track unknown rule sections like [tool.rumdl.MD999] or [tool.rumdl.unknown-rule]
                    fragment.unknown_keys.push((
                        format!("[tool.rumdl.{rule_name}]"),
                        String::new(),
                        Some(path.to_string()),
                    ));
                }
            }
        }
    }

    // 3. Handle [tool.rumdl.MDxxx] sections as top-level keys (e.g., [tool.rumdl.MD007] or [tool.rumdl.line-length])
    if let Some(doc_table) = doc.as_table() {
        for (key, value) in doc_table.iter() {
            if let Some(rule_name) = key.strip_prefix("tool.rumdl.") {
                // Try to resolve as a rule name (handles both canonical names and aliases)
                if let Some(resolved_rule_name) = registry.resolve_rule_name(rule_name) {
                    if let Some(rule_table) = value.as_table() {
                        let rule_entry = fragment.rules.entry(resolved_rule_name.clone()).or_default();
                        for (rk, rv) in rule_table {
                            let norm_rk = normalize_key(rk);

                            // Special handling for severity - store in rule_entry.severity
                            if norm_rk == "severity" {
                                if let Ok(severity) = crate::rule::Severity::deserialize(rv.clone()) {
                                    if rule_entry.severity.is_none() {
                                        rule_entry.severity = Some(SourcedValue::new(severity, source));
                                    } else {
                                        rule_entry.severity.as_mut().unwrap().push_override(
                                            severity,
                                            source,
                                            file.clone(),
                                            None,
                                        );
                                    }
                                }
                                continue; // Skip regular value processing for severity
                            }

                            let toml_val = rv.clone();
                            let sv = rule_entry
                                .values
                                .entry(norm_rk.clone())
                                .or_insert_with(|| SourcedValue::new(toml_val.clone(), source));
                            sv.push_override(toml_val, source, file.clone(), None);
                        }
                    }
                } else if rule_name.to_ascii_uppercase().starts_with("MD")
                    || rule_name.chars().any(|c| c.is_alphabetic())
                {
                    // Track unknown rule sections like [tool.rumdl.MD999] or [tool.rumdl.unknown-rule]
                    fragment.unknown_keys.push((
                        format!("[tool.rumdl.{rule_name}]"),
                        String::new(),
                        Some(path.to_string()),
                    ));
                }
            }
        }
    }

    // Only return Some(fragment) if any config was found
    let has_any = !fragment.global.enable.value.is_empty()
        || !fragment.global.disable.value.is_empty()
        || !fragment.global.include.value.is_empty()
        || !fragment.global.exclude.value.is_empty()
        || !fragment.global.fixable.value.is_empty()
        || !fragment.global.unfixable.value.is_empty()
        || fragment.global.output_format.is_some()
        || fragment.global.cache_dir.is_some()
        || !fragment.global.cache.value
        || !fragment.per_file_ignores.value.is_empty()
        || !fragment.per_file_flavor.value.is_empty()
        || !fragment.rules.is_empty();
    if has_any { Ok(Some(fragment)) } else { Ok(None) }
}

/// Parses rumdl.toml / .rumdl.toml content.
pub(super) fn parse_rumdl_toml(
    content: &str,
    path: &str,
    source: ConfigSource,
) -> Result<SourcedConfigFragment, ConfigError> {
    let display_path = to_relative_display_path(path);
    let doc = content
        .parse::<DocumentMut>()
        .map_err(|e| ConfigError::ParseError(format!("{display_path}: Failed to parse TOML: {e}")))?;
    let mut fragment = SourcedConfigFragment::default();
    // source parameter provided by caller
    let file = Some(path.to_string());

    // Use the lazily-initialized default registry for alias resolution and schema validation
    let registry = super::registry::default_registry();

    // Handle [global] section
    if let Some(global_item) = doc.get("global")
        && let Some(global_table) = global_item.as_table()
    {
        for (key, value_item) in global_table.iter() {
            let norm_key = normalize_key(key);
            match norm_key.as_str() {
                "enable" | "disable" | "include" | "exclude" => {
                    if let Some(toml_edit::Value::Array(formatted_array)) = value_item.as_value() {
                        // Corrected: Iterate directly over the Formatted<Array>
                        let values: Vec<String> = formatted_array
                                .iter()
                                .filter_map(|item| item.as_str()) // Extract strings
                                .map(|s| s.to_string())
                                .collect();

                        // Resolve rule name aliases for enable/disable (e.g., "ul-style" -> "MD004")
                        let final_values = if norm_key == "enable" || norm_key == "disable" {
                            values
                                .into_iter()
                                .map(|s| registry.resolve_rule_name(&s).unwrap_or_else(|| normalize_key(&s)))
                                .collect()
                        } else {
                            values
                        };

                        match norm_key.as_str() {
                            "enable" => {
                                // Only push if non-empty: an empty enable list in rumdl config
                                // means "no filter", not "disable all rules".
                                if !final_values.is_empty() {
                                    fragment
                                        .global
                                        .enable
                                        .push_override(final_values, source, file.clone(), None);
                                }
                            }
                            "disable" => {
                                fragment
                                    .global
                                    .disable
                                    .push_override(final_values, source, file.clone(), None)
                            }
                            "include" => {
                                fragment
                                    .global
                                    .include
                                    .push_override(final_values, source, file.clone(), None)
                            }
                            "exclude" => {
                                fragment
                                    .global
                                    .exclude
                                    .push_override(final_values, source, file.clone(), None)
                            }
                            _ => unreachable!("Outer match guarantees only enable/disable/include/exclude"),
                        }
                    } else {
                        log::warn!(
                            "[WARN] Expected array for global key '{}' in {}, found {}",
                            key,
                            display_path,
                            value_item.type_name()
                        );
                    }
                }
                "respect_gitignore" | "respect-gitignore" => {
                    // Handle both cases
                    if let Some(toml_edit::Value::Boolean(formatted_bool)) = value_item.as_value() {
                        let val = *formatted_bool.value();
                        fragment
                            .global
                            .respect_gitignore
                            .push_override(val, source, file.clone(), None);
                    } else {
                        log::warn!(
                            "[WARN] Expected boolean for global key '{}' in {}, found {}",
                            key,
                            display_path,
                            value_item.type_name()
                        );
                    }
                }
                "force_exclude" | "force-exclude" => {
                    // Handle both cases
                    if let Some(toml_edit::Value::Boolean(formatted_bool)) = value_item.as_value() {
                        let val = *formatted_bool.value();
                        fragment
                            .global
                            .force_exclude
                            .push_override(val, source, file.clone(), None);
                    } else {
                        log::warn!(
                            "[WARN] Expected boolean for global key '{}' in {}, found {}",
                            key,
                            display_path,
                            value_item.type_name()
                        );
                    }
                }
                "line_length" | "line-length" => {
                    // Handle both cases
                    if let Some(toml_edit::Value::Integer(formatted_int)) = value_item.as_value() {
                        let val = LineLength::new(*formatted_int.value() as usize);
                        fragment
                            .global
                            .line_length
                            .push_override(val, source, file.clone(), None);
                    } else {
                        log::warn!(
                            "[WARN] Expected integer for global key '{}' in {}, found {}",
                            key,
                            display_path,
                            value_item.type_name()
                        );
                    }
                }
                "output_format" | "output-format" => {
                    // Handle both cases
                    if let Some(toml_edit::Value::String(formatted_string)) = value_item.as_value() {
                        let val = formatted_string.value().clone();
                        if fragment.global.output_format.is_none() {
                            fragment.global.output_format = Some(SourcedValue::new(val.clone(), source));
                        } else {
                            fragment.global.output_format.as_mut().unwrap().push_override(
                                val,
                                source,
                                file.clone(),
                                None,
                            );
                        }
                    } else {
                        log::warn!(
                            "[WARN] Expected string for global key '{}' in {}, found {}",
                            key,
                            display_path,
                            value_item.type_name()
                        );
                    }
                }
                "cache_dir" | "cache-dir" => {
                    // Handle both cases
                    if let Some(toml_edit::Value::String(formatted_string)) = value_item.as_value() {
                        let val = formatted_string.value().clone();
                        if fragment.global.cache_dir.is_none() {
                            fragment.global.cache_dir = Some(SourcedValue::new(val.clone(), source));
                        } else {
                            fragment
                                .global
                                .cache_dir
                                .as_mut()
                                .unwrap()
                                .push_override(val, source, file.clone(), None);
                        }
                    } else {
                        log::warn!(
                            "[WARN] Expected string for global key '{}' in {}, found {}",
                            key,
                            display_path,
                            value_item.type_name()
                        );
                    }
                }
                "cache" => {
                    if let Some(toml_edit::Value::Boolean(b)) = value_item.as_value() {
                        let val = *b.value();
                        fragment.global.cache.push_override(val, source, file.clone(), None);
                    } else {
                        log::warn!(
                            "[WARN] Expected boolean for global key '{}' in {}, found {}",
                            key,
                            display_path,
                            value_item.type_name()
                        );
                    }
                }
                "fixable" => {
                    if let Some(toml_edit::Value::Array(formatted_array)) = value_item.as_value() {
                        let values: Vec<String> = formatted_array
                            .iter()
                            .filter_map(|item| item.as_str())
                            .map(normalize_key)
                            .collect();
                        fragment
                            .global
                            .fixable
                            .push_override(values, source, file.clone(), None);
                    } else {
                        log::warn!(
                            "[WARN] Expected array for global key '{}' in {}, found {}",
                            key,
                            display_path,
                            value_item.type_name()
                        );
                    }
                }
                "unfixable" => {
                    if let Some(toml_edit::Value::Array(formatted_array)) = value_item.as_value() {
                        let values: Vec<String> = formatted_array
                            .iter()
                            .filter_map(|item| item.as_str())
                            .map(|s| registry.resolve_rule_name(s).unwrap_or_else(|| normalize_key(s)))
                            .collect();
                        fragment
                            .global
                            .unfixable
                            .push_override(values, source, file.clone(), None);
                    } else {
                        log::warn!(
                            "[WARN] Expected array for global key '{}' in {}, found {}",
                            key,
                            display_path,
                            value_item.type_name()
                        );
                    }
                }
                "flavor" => {
                    if let Some(toml_edit::Value::String(formatted_string)) = value_item.as_value() {
                        let val = formatted_string.value();
                        if let Ok(flavor) = MarkdownFlavor::from_str(val) {
                            fragment.global.flavor.push_override(flavor, source, file.clone(), None);
                        } else {
                            log::warn!("[WARN] Unknown markdown flavor '{val}' in {display_path}");
                        }
                    } else {
                        log::warn!(
                            "[WARN] Expected string for global key '{}' in {}, found {}",
                            key,
                            display_path,
                            value_item.type_name()
                        );
                    }
                }
                _ => {
                    // Track unknown global keys for validation
                    fragment
                        .unknown_keys
                        .push(("[global]".to_string(), key.to_string(), Some(path.to_string())));
                    log::warn!("[WARN] Unknown key in [global] section of {display_path}: {key}");
                }
            }
        }
    }

    // Handle [per-file-ignores] section
    if let Some(per_file_item) = doc.get("per-file-ignores")
        && let Some(per_file_table) = per_file_item.as_table()
    {
        let mut per_file_map = HashMap::new();
        for (pattern, value_item) in per_file_table.iter() {
            warn_comma_without_brace_in_pattern(pattern, &display_path);
            if let Some(toml_edit::Value::Array(formatted_array)) = value_item.as_value() {
                let rules: Vec<String> = formatted_array
                    .iter()
                    .filter_map(|item| item.as_str())
                    .map(|s| registry.resolve_rule_name(s).unwrap_or_else(|| normalize_key(s)))
                    .collect();
                per_file_map.insert(pattern.to_string(), rules);
            } else {
                let type_name = value_item.type_name();
                log::warn!(
                    "[WARN] Expected array for per-file-ignores pattern '{pattern}' in {display_path}, found {type_name}"
                );
            }
        }
        fragment
            .per_file_ignores
            .push_override(per_file_map, source, file.clone(), None);
    }

    // Handle [per-file-flavor] section
    if let Some(per_file_item) = doc.get("per-file-flavor")
        && let Some(per_file_table) = per_file_item.as_table()
    {
        let mut per_file_map = IndexMap::new();
        for (pattern, value_item) in per_file_table.iter() {
            if let Some(toml_edit::Value::String(formatted_string)) = value_item.as_value() {
                let flavor_str = formatted_string.value();
                match MarkdownFlavor::deserialize(toml::Value::String(flavor_str.to_string())) {
                    Ok(flavor) => {
                        per_file_map.insert(pattern.to_string(), flavor);
                    }
                    Err(_) => {
                        log::warn!(
                            "[WARN] Invalid flavor '{flavor_str}' for pattern '{pattern}' in {display_path}. Valid values: standard, mkdocs, mdx, quarto"
                        );
                    }
                }
            } else {
                let type_name = value_item.type_name();
                log::warn!(
                    "[WARN] Expected string for per-file-flavor pattern '{pattern}' in {display_path}, found {type_name}"
                );
            }
        }
        fragment
            .per_file_flavor
            .push_override(per_file_map, source, file.clone(), None);
    }

    // Handle [code-block-tools] section
    if let Some(cbt_item) = doc.get("code-block-tools")
        && let Some(cbt_table) = cbt_item.as_table()
    {
        // Convert the table to a proper TOML document for deserialization
        // We need to create a new document with just this section, properly formatted
        let mut cbt_doc = toml_edit::DocumentMut::new();
        for (key, value) in cbt_table.iter() {
            cbt_doc[key] = value.clone();
        }
        let cbt_toml_str = cbt_doc.to_string();
        match toml::from_str::<crate::code_block_tools::CodeBlockToolsConfig>(&cbt_toml_str) {
            Ok(cbt_config) => {
                fragment
                    .code_block_tools
                    .push_override(cbt_config, source, file.clone(), None);
            }
            Err(e) => {
                log::warn!("[WARN] Failed to parse [code-block-tools] section in {display_path}: {e}");
            }
        }
    }

    // Rule-specific: all other top-level tables
    for (key, item) in doc.iter() {
        // Skip known special sections
        if key == "global" || key == "per-file-ignores" || key == "per-file-flavor" || key == "code-block-tools" {
            continue;
        }

        // Resolve rule name (handles both canonical names like "MD004" and aliases like "ul-style")
        let norm_rule_name = if let Some(resolved) = registry.resolve_rule_name(key) {
            resolved
        } else {
            // Unknown rule - always track it for validation and suggestions
            fragment
                .unknown_keys
                .push((format!("[{key}]"), String::new(), Some(path.to_string())));
            continue;
        };

        if let Some(tbl) = item.as_table() {
            let rule_entry = fragment.rules.entry(norm_rule_name.clone()).or_default();
            for (rk, rv_item) in tbl.iter() {
                let norm_rk = normalize_key(rk);

                // Special handling for severity - store in rule_entry.severity
                if norm_rk == "severity" {
                    if let Some(toml_edit::Value::String(formatted_string)) = rv_item.as_value() {
                        let severity_str = formatted_string.value();
                        match crate::rule::Severity::deserialize(toml::Value::String(severity_str.to_string())) {
                            Ok(severity) => {
                                if rule_entry.severity.is_none() {
                                    rule_entry.severity = Some(SourcedValue::new(severity, source));
                                } else {
                                    rule_entry.severity.as_mut().unwrap().push_override(
                                        severity,
                                        source,
                                        file.clone(),
                                        None,
                                    );
                                }
                            }
                            Err(_) => {
                                log::warn!(
                                    "[WARN] Invalid severity '{severity_str}' for rule {norm_rule_name} in {display_path}. Valid values: error, warning"
                                );
                            }
                        }
                    }
                    continue; // Skip regular value processing for severity
                }

                let maybe_toml_val: Option<toml::Value> = match rv_item.as_value() {
                    Some(toml_edit::Value::String(formatted)) => Some(toml::Value::String(formatted.value().clone())),
                    Some(toml_edit::Value::Integer(formatted)) => Some(toml::Value::Integer(*formatted.value())),
                    Some(toml_edit::Value::Float(formatted)) => Some(toml::Value::Float(*formatted.value())),
                    Some(toml_edit::Value::Boolean(formatted)) => Some(toml::Value::Boolean(*formatted.value())),
                    Some(toml_edit::Value::Datetime(formatted)) => Some(toml::Value::Datetime(*formatted.value())),
                    Some(toml_edit::Value::Array(formatted_array)) => {
                        // Convert toml_edit Array to toml::Value::Array
                        let mut values = Vec::new();
                        for item in formatted_array.iter() {
                            match item {
                                toml_edit::Value::String(formatted) => {
                                    values.push(toml::Value::String(formatted.value().clone()))
                                }
                                toml_edit::Value::Integer(formatted) => {
                                    values.push(toml::Value::Integer(*formatted.value()))
                                }
                                toml_edit::Value::Float(formatted) => {
                                    values.push(toml::Value::Float(*formatted.value()))
                                }
                                toml_edit::Value::Boolean(formatted) => {
                                    values.push(toml::Value::Boolean(*formatted.value()))
                                }
                                toml_edit::Value::Datetime(formatted) => {
                                    values.push(toml::Value::Datetime(*formatted.value()))
                                }
                                _ => {
                                    log::warn!(
                                        "[WARN] Skipping unsupported array element type in key '{norm_rule_name}.{norm_rk}' in {display_path}"
                                    );
                                }
                            }
                        }
                        Some(toml::Value::Array(values))
                    }
                    Some(toml_edit::Value::InlineTable(_)) => {
                        log::warn!(
                            "[WARN] Skipping inline table value for key '{norm_rule_name}.{norm_rk}' in {display_path}. Table conversion not yet fully implemented in parser."
                        );
                        None
                    }
                    None => {
                        log::warn!(
                            "[WARN] Skipping non-value item for key '{norm_rule_name}.{norm_rk}' in {display_path}. Expected simple value."
                        );
                        None
                    }
                };
                if let Some(toml_val) = maybe_toml_val {
                    let sv = rule_entry
                        .values
                        .entry(norm_rk.clone())
                        .or_insert_with(|| SourcedValue::new(toml_val.clone(), ConfigSource::Default));
                    sv.push_override(toml_val, source, file.clone(), None);
                }
            }
        } else if item.is_value() {
            log::warn!(
                "[WARN] Ignoring top-level value key in {display_path}: '{key}'. Expected a table like [{key}]."
            );
        }
    }

    Ok(fragment)
}

/// Loads and converts a markdownlint config file (.json or .yaml) into a SourcedConfigFragment.
pub(super) fn load_from_markdownlint(path: &str) -> Result<SourcedConfigFragment, ConfigError> {
    let display_path = to_relative_display_path(path);
    // Use the unified loader from markdownlint_config.rs
    let ml_config = crate::markdownlint_config::load_markdownlint_config(path)
        .map_err(|e| ConfigError::ParseError(format!("{display_path}: {e}")))?;
    Ok(ml_config.map_to_sourced_rumdl_config_fragment(Some(path)))
}
