use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::BTreeMap;
use toml_edit::DocumentMut;

use super::flavor::{MarkdownFlavor, normalize_key, warn_comma_without_brace_in_pattern};
use super::source_tracking::{ConfigSource, SourcedConfigFragment, SourcedValue};
use super::types::ConfigError;
use super::validation::to_relative_display_path;

/// Parses pyproject.toml content and extracts the [tool.rumdl] section if present.
pub(super) fn parse_pyproject_toml(
    content: &str,
    path: &str,
    source: ConfigSource,
) -> Result<Option<SourcedConfigFragment>, ConfigError> {
    let display_path = to_relative_display_path(path);
    let doc: toml::Value = toml::from_str(content)
        .map_err(|e| ConfigError::ParseError(format!("{display_path}: Failed to parse TOML: {e}")))?;
    let mut fragment = SourcedConfigFragment::default();
    let file = Some(path.to_string());

    // Use the lazily-initialized default registry for alias resolution and schema validation
    let registry = super::registry::default_registry();

    // Parse `extends` from [tool.rumdl] level
    if let Some(rumdl_config) = doc.get("tool").and_then(|t| t.get("rumdl"))
        && let Some(rumdl_table) = rumdl_config.as_table()
        && let Some(extends_val) = rumdl_table.get("extends")
        && let Ok(extends_str) = String::deserialize(extends_val.clone())
    {
        fragment.extends = Some(extends_str);
    }

    // 1. Handle [tool.rumdl] and [tool.rumdl.global] sections
    if let Some(rumdl_config) = doc.get("tool").and_then(|t| t.get("rumdl"))
        && let Some(rumdl_table) = rumdl_config.as_table()
    {
        // Apply global options from the given table through the shared
        // global-key dispatch (`global_keys::apply_global_key`). Keys are
        // normalized first, so snake_case spellings keep working. Rule
        // sections, `extends`, and other non-global keys fall through as
        // Unrecognized and are handled by their own parsers.
        let extract_global_config = |fragment: &mut SourcedConfigFragment, table: &toml::value::Table| {
            use super::global_keys::{ApplyOutcome, apply_global_key};
            for (key, value) in table {
                let norm_key = normalize_key(key);
                match apply_global_key(
                    &mut fragment.global,
                    &norm_key,
                    value,
                    source,
                    file.as_deref(),
                    registry,
                ) {
                    ApplyOutcome::Applied => {
                        // line-length is mirrored into MD013 for backward
                        // compatibility with configs that predate the global key.
                        if norm_key == "line-length" {
                            let rule_entry = fragment.rules.entry(normalize_key("MD013")).or_default();
                            let sv = rule_entry
                                .values
                                .entry(norm_key.clone())
                                .or_insert_with(|| SourcedValue::new(value.clone(), ConfigSource::Default));
                            sv.push_override(value.clone(), source, file.clone());
                        }
                    }
                    ApplyOutcome::TypeMismatch { expected } => {
                        log::warn!("[WARN] Expected {expected} for global key '{norm_key}' in {display_path}");
                    }
                    ApplyOutcome::InvalidValue { message } => {
                        log::warn!("[WARN] {message} in {display_path}");
                    }
                    ApplyOutcome::Unrecognized => {}
                }
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
            let mut per_file_map = BTreeMap::new();
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
                .push_override(per_file_map, source, file.clone());
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
                        "[WARN] Invalid flavor for per-file-flavor pattern '{pattern}' in {display_path}, found {flavor_value:?}. Valid values: standard, mkdocs, mdx, pandoc, quarto, obsidian, kramdown, azure_devops, myst"
                    );
                }
            }
            fragment
                .per_file_flavor
                .push_override(per_file_map, source, file.clone());
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
                "extend-enable",
                "extend_enable",
                "extend-disable",
                "extend_disable",
                "extends",
            ]
            .contains(&norm_rule_key.as_str());

            // Special handling for line-length: could be global config OR rule section
            let is_line_length_global =
                (norm_rule_key == "line-length" || norm_rule_key == "line_length") && !value.is_table();

            if is_global_key || is_line_length_global {
                continue;
            }

            // Handle [tool.rumdl.rules.MDxxx] — accepted as an alias for [tool.rumdl.MDxxx].
            if norm_rule_key == "rules" {
                if let Some(inner_table) = value.as_table() {
                    for (inner_key, inner_value) in inner_table {
                        if let Some(resolved_inner_rule) = registry.resolve_rule_name(inner_key) {
                            if let Some(inner_rule_table) = inner_value.as_table() {
                                apply_rule_table_toml(
                                    &resolved_inner_rule,
                                    inner_rule_table,
                                    &mut fragment,
                                    source,
                                    &file,
                                    &display_path,
                                );
                            }
                        } else {
                            // Store without the "rules." prefix so validation's edit-distance
                            // suggestion logic sees the bare rule name, not "rules.MD999".
                            fragment.unknown_keys.push((
                                format!("[tool.rumdl.{inner_key}]"),
                                String::new(),
                                Some(path.to_string()),
                            ));
                        }
                    }
                }
                continue;
            }

            // Try to resolve as a rule name (handles both canonical names and aliases)
            if let Some(resolved_rule_name) = registry.resolve_rule_name(key)
                && value.is_table()
                && let Some(rule_config_table) = value.as_table()
            {
                apply_rule_table_toml(
                    &resolved_rule_name,
                    rule_config_table,
                    &mut fragment,
                    source,
                    &file,
                    &display_path,
                );
            } else if registry.resolve_rule_name(key).is_none() {
                // Key is not a global/special key and not a recognized rule name
                // Track unknown keys under [tool.rumdl] for validation
                fragment
                    .unknown_keys
                    .push(("[tool.rumdl]".to_string(), key.clone(), Some(path.to_string())));
            }
        }
    }

    // 2. Handle [tool.rumdl.MDxxx] and [tool.rumdl.rules.MDxxx] nested under [tool]
    if let Some(tool_table) = doc.get("tool").and_then(|t| t.as_table()) {
        for (key, value) in tool_table {
            // Accept both "rumdl.MDxxx" and "rumdl.rules.MDxxx" prefixes
            let rule_name = key.strip_prefix("rumdl.rules.").or_else(|| key.strip_prefix("rumdl."));
            if let Some(rule_name) = rule_name {
                if let Some(resolved_rule_name) = registry.resolve_rule_name(rule_name) {
                    if let Some(rule_table) = value.as_table() {
                        apply_rule_table_toml(
                            &resolved_rule_name,
                            rule_table,
                            &mut fragment,
                            source,
                            &file,
                            &display_path,
                        );
                    }
                } else if rule_name.to_ascii_uppercase().starts_with("MD") || rule_name.chars().any(char::is_alphabetic)
                {
                    fragment.unknown_keys.push((
                        format!("[tool.rumdl.{rule_name}]"),
                        String::new(),
                        Some(path.to_string()),
                    ));
                }
            }
        }
    }

    // 3. Handle [tool.rumdl.MDxxx] and [tool.rumdl.rules.MDxxx] as top-level dotted keys
    if let Some(doc_table) = doc.as_table() {
        for (key, value) in doc_table {
            // Accept both "tool.rumdl.MDxxx" and "tool.rumdl.rules.MDxxx" prefixes
            let rule_name = key
                .strip_prefix("tool.rumdl.rules.")
                .or_else(|| key.strip_prefix("tool.rumdl."));
            if let Some(rule_name) = rule_name {
                if let Some(resolved_rule_name) = registry.resolve_rule_name(rule_name) {
                    if let Some(rule_table) = value.as_table() {
                        apply_rule_table_toml(
                            &resolved_rule_name,
                            rule_table,
                            &mut fragment,
                            source,
                            &file,
                            &display_path,
                        );
                    }
                } else if rule_name.to_ascii_uppercase().starts_with("MD") || rule_name.chars().any(char::is_alphabetic)
                {
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
    let has_any = fragment.extends.is_some()
        || !fragment.global.enable.value.is_empty()
        || !fragment.global.disable.value.is_empty()
        || !fragment.global.extend_enable.value.is_empty()
        || !fragment.global.extend_disable.value.is_empty()
        || !fragment.global.include.value.is_empty()
        || !fragment.global.exclude.value.is_empty()
        || !fragment.global.fixable.value.is_empty()
        || !fragment.global.unfixable.value.is_empty()
        || fragment.global.output_format.is_some()
        || fragment.global.cache_dir.is_some()
        || fragment.global.cache.source != ConfigSource::Default
        || fragment.global.flavor.source != ConfigSource::Default
        || fragment.global.respect_gitignore.source != ConfigSource::Default
        || fragment.global.force_exclude.source != ConfigSource::Default
        || !fragment.per_file_ignores.value.is_empty()
        || !fragment.per_file_flavor.value.is_empty()
        || !fragment.rules.is_empty();
    if has_any { Ok(Some(fragment)) } else { Ok(None) }
}

/// Applies a rule configuration table (in standard `toml` format) into the fragment.
/// Used for rule sections parsed from pyproject.toml, including `[tool.rumdl.MDxxx]`
/// and `[tool.rumdl.rules.MDxxx]` forms.
fn apply_rule_table_toml(
    norm_rule_name: &str,
    rule_config_table: &toml::value::Table,
    fragment: &mut SourcedConfigFragment,
    source: ConfigSource,
    file: &Option<String>,
    display_path: &str,
) {
    let rule_entry = fragment.rules.entry(norm_rule_name.to_string()).or_default();
    for (rk, rv) in rule_config_table {
        let norm_rk = normalize_key(rk);

        if norm_rk == "severity" {
            if let Ok(severity) = crate::rule::Severity::deserialize(rv.clone()) {
                if let Some(ref mut sv) = rule_entry.severity {
                    sv.push_override(severity, source, file.clone());
                } else {
                    rule_entry.severity = Some(SourcedValue::new(severity, source));
                }
            } else if let Some(severity_str) = rv.as_str() {
                log::warn!(
                    "[WARN] Invalid severity '{severity_str}' for rule {norm_rule_name} in {display_path}. Valid values: error, warning"
                );
            }
            continue;
        }

        let toml_val = rv.clone();
        let sv = rule_entry
            .values
            .entry(norm_rk.clone())
            .or_insert_with(|| SourcedValue::new(toml_val.clone(), ConfigSource::Default));
        sv.push_override(toml_val, source, file.clone());
    }
}

pub(super) use super::global_keys::is_global_value_key;

/// Parse a single global config key-value pair and store it in the fragment.
/// Used by both the `[global]` section handler and the top-level key handler.
/// Delegates the per-key typing and setting to `global_keys::apply_global_key`,
/// keeping only the toml_edit bridging and warning phrasing here.
fn parse_global_key(
    norm_key: &str,
    value_item: &toml_edit::Item,
    fragment: &mut SourcedConfigFragment,
    source: ConfigSource,
    file: &Option<String>,
    display_path: &str,
    registry: &super::registry::RuleRegistry,
) -> bool {
    use super::global_keys::{ApplyOutcome, apply_global_key, toml_edit_value_to_toml};

    if !super::global_keys::is_global_value_key(norm_key) {
        return false;
    }

    let Some(edit_value) = value_item.as_value() else {
        log::warn!(
            "[WARN] Expected a value for global key '{}' in {}, found {}",
            norm_key,
            display_path,
            value_item.type_name()
        );
        return true;
    };

    let value = toml_edit_value_to_toml(edit_value);
    match apply_global_key(
        &mut fragment.global,
        norm_key,
        &value,
        source,
        file.as_deref(),
        registry,
    ) {
        ApplyOutcome::Applied => {}
        ApplyOutcome::TypeMismatch { expected } => {
            log::warn!(
                "[WARN] Expected {} for global key '{}' in {}, found {}",
                expected,
                norm_key,
                display_path,
                value_item.type_name()
            );
        }
        ApplyOutcome::InvalidValue { message } => {
            log::warn!("[WARN] {message} in {display_path}");
        }
        ApplyOutcome::Unrecognized => unreachable!("guarded by is_global_value_key above"),
    }
    true
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

    // Parse top-level `extends` key (not inside any section)
    if let Some(extends_item) = doc.get("extends")
        && let Some(extends_val) = extends_item.as_value()
        && let Some(extends_str) = extends_val.as_str()
    {
        fragment.extends = Some(extends_str.to_string());
    }

    // Use the lazily-initialized default registry for alias resolution and schema validation
    let registry = super::registry::default_registry();

    // Handle top-level global keys (ruff-style shorthand).
    // These are parsed BEFORE [global] so that explicit [global] section values
    // take precedence via push_override.
    for (key, item) in doc.iter() {
        if item.is_value() {
            let norm_key = normalize_key(key);
            if is_global_value_key(&norm_key) {
                let handled = parse_global_key(&norm_key, item, &mut fragment, source, &file, &display_path, registry);
                debug_assert!(
                    handled,
                    "Key '{norm_key}' is in GLOBAL_VALUE_KEYS but not handled by parse_global_key"
                );
            }
        }
    }

    // Handle [global] section (overrides top-level shorthand keys above)
    if let Some(global_item) = doc.get("global")
        && let Some(global_table) = global_item.as_table()
    {
        for (key, value_item) in global_table {
            let norm_key = normalize_key(key);
            if !parse_global_key(
                &norm_key,
                value_item,
                &mut fragment,
                source,
                &file,
                &display_path,
                registry,
            ) {
                // Track unknown global keys for validation
                fragment
                    .unknown_keys
                    .push(("[global]".to_string(), key.to_string(), Some(path.to_string())));
                log::warn!("[WARN] Unknown key in [global] section of {display_path}: {key}");
            }
        }
    }

    // Handle [per-file-ignores] section
    if let Some(per_file_item) = doc.get("per-file-ignores")
        && let Some(per_file_table) = per_file_item.as_table()
    {
        let mut per_file_map = BTreeMap::new();
        for (pattern, value_item) in per_file_table {
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
            .push_override(per_file_map, source, file.clone());
    }

    // Handle [per-file-flavor] section
    if let Some(per_file_item) = doc.get("per-file-flavor")
        && let Some(per_file_table) = per_file_item.as_table()
    {
        let mut per_file_map = IndexMap::new();
        for (pattern, value_item) in per_file_table {
            if let Some(toml_edit::Value::String(formatted_string)) = value_item.as_value() {
                let flavor_str = formatted_string.value();
                match MarkdownFlavor::deserialize(toml::Value::String(flavor_str.clone())) {
                    Ok(flavor) => {
                        per_file_map.insert(pattern.to_string(), flavor);
                    }
                    Err(_) => {
                        log::warn!(
                            "[WARN] Invalid flavor '{flavor_str}' for pattern '{pattern}' in {display_path}. Valid values: standard, mkdocs, mdx, pandoc, quarto, obsidian, kramdown, azure_devops, myst"
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
            .push_override(per_file_map, source, file.clone());
    }

    // Handle [code-block-tools] section
    if let Some(cbt_item) = doc.get("code-block-tools")
        && let Some(cbt_table) = cbt_item.as_table()
    {
        // Convert the table to a proper TOML document for deserialization
        // We need to create a new document with just this section, properly formatted
        let mut cbt_doc = toml_edit::DocumentMut::new();
        for (key, value) in cbt_table {
            cbt_doc[key] = value.clone();
        }
        let cbt_toml_str = cbt_doc.to_string();
        match toml::from_str::<crate::code_block_tools::CodeBlockToolsConfig>(&cbt_toml_str) {
            Ok(cbt_config) => {
                fragment
                    .code_block_tools
                    .push_override(cbt_config, source, file.clone());
            }
            Err(e) => {
                log::warn!("[WARN] Failed to parse [code-block-tools] section in {display_path}: {e}");
            }
        }
    }

    // Rule-specific: all other top-level tables
    for (key, item) in doc.iter() {
        // Skip known special sections and top-level value keys (already handled above)
        if key == "global"
            || key == "per-file-ignores"
            || key == "per-file-flavor"
            || key == "code-block-tools"
            || key == "extends"
        {
            continue;
        }

        // Skip top-level value keys that were already parsed as global config
        if item.is_value() {
            let norm_key = normalize_key(key);
            if is_global_value_key(&norm_key) {
                continue;
            }
        }

        // Handle [rules.MDxxx] wrapper — accepted as an alias for flat [MDxxx] sections.
        // This lets users port markdownlint configs that group rules under a [rules] heading.
        if key == "rules" {
            if let Some(rules_tbl) = item.as_table() {
                for (inner_key, inner_item) in rules_tbl {
                    if let Some(norm_inner_rule) = registry.resolve_rule_name(inner_key) {
                        if let Some(inner_tbl) = inner_item.as_table() {
                            apply_rule_table_toml_edit(
                                &norm_inner_rule,
                                inner_tbl,
                                &mut fragment,
                                source,
                                &file,
                                &display_path,
                            );
                        } else {
                            let type_name = inner_item.type_name();
                            log::warn!(
                                "[WARN] Expected table for rule '{inner_key}' in [rules] section of {display_path}, found {type_name}; ignoring"
                            );
                        }
                    } else {
                        // Store without the "rules." prefix so validation's edit-distance
                        // suggestion logic sees the bare rule name (e.g. "MD999"), not "rules.MD999".
                        fragment
                            .unknown_keys
                            .push((format!("[{inner_key}]"), String::new(), Some(path.to_string())));
                    }
                }
            }
            continue;
        }

        // Resolve rule name (handles both canonical names like "MD004" and aliases like "ul-style")
        let Some(norm_rule_name) = registry.resolve_rule_name(key) else {
            // Unknown rule - always track it for validation and suggestions
            fragment
                .unknown_keys
                .push((format!("[{key}]"), String::new(), Some(path.to_string())));
            continue;
        };

        if let Some(tbl) = item.as_table() {
            apply_rule_table_toml_edit(&norm_rule_name, tbl, &mut fragment, source, &file, &display_path);
        } else if item.is_value() {
            log::warn!(
                "[WARN] Ignoring top-level value key in {display_path}: '{key}'. Expected a table like [{key}]."
            );
        }
    }

    Ok(fragment)
}

/// Applies a rule configuration table (in toml_edit format) into the fragment.
/// Used for both `[MDxxx]` and `[rules.MDxxx]` top-level table forms in rumdl.toml.
fn apply_rule_table_toml_edit(
    norm_rule_name: &str,
    tbl: &toml_edit::Table,
    fragment: &mut SourcedConfigFragment,
    source: ConfigSource,
    file: &Option<String>,
    display_path: &str,
) {
    let rule_entry = fragment.rules.entry(norm_rule_name.to_string()).or_default();
    for (rk, rv_item) in tbl {
        let norm_rk = normalize_key(rk);

        // Special handling for severity - store in rule_entry.severity
        if norm_rk == "severity" {
            if let Some(toml_edit::Value::String(formatted_string)) = rv_item.as_value() {
                let severity_str = formatted_string.value();
                match crate::rule::Severity::deserialize(toml::Value::String(severity_str.clone())) {
                    Ok(severity) => {
                        if let Some(ref mut sv) = rule_entry.severity {
                            sv.push_override(severity, source, file.clone());
                        } else {
                            rule_entry.severity = Some(SourcedValue::new(severity, source));
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
                for item in formatted_array {
                    match item {
                        toml_edit::Value::String(formatted) => {
                            values.push(toml::Value::String(formatted.value().clone()))
                        }
                        toml_edit::Value::Integer(formatted) => values.push(toml::Value::Integer(*formatted.value())),
                        toml_edit::Value::Float(formatted) => values.push(toml::Value::Float(*formatted.value())),
                        toml_edit::Value::Boolean(formatted) => values.push(toml::Value::Boolean(*formatted.value())),
                        toml_edit::Value::Datetime(formatted) => values.push(toml::Value::Datetime(*formatted.value())),
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
            sv.push_override(toml_val, source, file.clone());
        }
    }
}

/// Loads and converts a markdownlint config file (.json or .yaml) into a SourcedConfigFragment.
pub(super) fn load_from_markdownlint(path: &str) -> Result<SourcedConfigFragment, ConfigError> {
    let display_path = to_relative_display_path(path);
    // Use the unified loader from markdownlint_config.rs
    let ml_config = crate::markdownlint_config::load_markdownlint_config(path)
        .map_err(|e| ConfigError::ParseError(format!("{display_path}: {e}")))?;
    Ok(ml_config.map_to_sourced_rumdl_config_fragment(Some(path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(content: &str) -> Option<SourcedConfigFragment> {
        // Match the production call path: pyproject.toml is loaded with
        // ConfigSource::PyprojectToml (see source_from_filename).
        parse_pyproject_toml(content, "pyproject.toml", ConfigSource::PyprojectToml).unwrap()
    }

    #[test]
    fn pyproject_with_only_flavor_is_kept() {
        let fragment = parse("[tool.rumdl]\nflavor = \"mkdocs\"\n")
            .expect("[tool.rumdl] with only `flavor` must not be discarded");
        assert_eq!(fragment.global.flavor.value, MarkdownFlavor::MkDocs);
        assert_ne!(fragment.global.flavor.source, ConfigSource::Default);
    }

    #[test]
    fn pyproject_with_only_respect_gitignore_is_kept() {
        let fragment = parse("[tool.rumdl]\nrespect-gitignore = false\n")
            .expect("[tool.rumdl] with only `respect-gitignore` must not be discarded");
        assert!(!fragment.global.respect_gitignore.value);
    }

    #[test]
    fn pyproject_with_only_force_exclude_is_kept() {
        let fragment = parse("[tool.rumdl]\nforce-exclude = true\n")
            .expect("[tool.rumdl] with only `force-exclude` must not be discarded");
        assert!(fragment.global.force_exclude.value);
    }

    #[test]
    fn pyproject_with_only_cache_true_is_kept() {
        // cache defaults to true, so an explicit `cache = true` must be
        // detected via its source, not by a value heuristic.
        let fragment =
            parse("[tool.rumdl]\ncache = true\n").expect("[tool.rumdl] with only `cache = true` must not be discarded");
        assert!(fragment.global.cache.value);
        assert_ne!(fragment.global.cache.source, ConfigSource::Default);
    }

    #[test]
    fn pyproject_with_no_rumdl_section_is_none() {
        assert!(parse("[tool.black]\nline-length = 88\n").is_none());
    }
}
