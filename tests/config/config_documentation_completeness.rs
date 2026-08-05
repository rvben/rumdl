//! Test to ensure all configuration options are documented
//!
//! This test dynamically extracts config fields by parsing the source code
//! of config structs, then validates they appear in documentation.
//!
//! This approach is robust because:
//! - No manual field list to maintain
//! - Automatically catches new fields
//! - Catches removed fields
//! - Uses actual config struct definitions as source of truth

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// Strip the leading visibility from a struct field declaration, returning the rest.
///
/// Returns `None` for a line that is not a `pub`/`pub(...)` field. Serde reads a field
/// whatever its visibility, so `pub(super) x: bool` is as user-settable as `pub x: bool`.
fn strip_field_visibility(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("pub")?;
    let rest = match rest.strip_prefix('(') {
        Some(restricted) => restricted.split_once(')')?.1,
        None => rest,
    };
    // A space must separate the visibility from the field name, so `pub_field: T`
    // (a field that merely starts with "pub") is not mistaken for one.
    rest.strip_prefix(' ').filter(|r| r.contains(':'))
}

/// Extract config field names from a config struct source file
fn extract_fields_from_config_file(file_path: &Path) -> HashSet<String> {
    let content = fs::read_to_string(file_path).unwrap_or_default();
    let mut fields = HashSet::new();
    let mut in_struct = false;
    let mut brace_depth = 0;
    let mut pending_rename: Option<String> = None;
    let mut pending_skip = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Look for struct definition. The visibility prefix is not matched: a rule
        // in a private module declares its config `pub(super)`, and requiring `pub`
        // extracted zero fields from it, which reads as "nothing to document".
        if trimmed.contains("struct MD") && trimmed.contains("Config") && trimmed.contains('{') {
            in_struct = true;
            if trimmed.contains('{') {
                brace_depth = 1;
            }
            continue;
        }

        if in_struct {
            // Track braces
            brace_depth += trimmed.matches('{').count();
            brace_depth -= trimmed.matches('}').count();

            if brace_depth == 0 {
                in_struct = false;
                pending_rename = None;
                pending_skip = false;
                continue;
            }

            // Check for #[serde(skip)] - marks internal fields not for user config
            if trimmed.contains("#[serde") && trimmed.contains("skip") {
                pending_skip = true;
                continue;
            }

            // Check for #[serde(rename = "...")] attributes
            if trimmed.contains("#[serde") && trimmed.contains("rename") {
                // Extract the rename value - this will be used for the next field
                if let Some(start) = trimmed.find("rename = \"")
                    && let Some(end) = trimmed[start + 10..].find('"')
                {
                    let renamed = &trimmed[start + 10..start + 10 + end];
                    pending_rename = Some(renamed.to_string());
                }
                continue;
            }

            // Extract field names - look for pub field_name: Type patterns. The
            // visibility may be restricted (`pub(super) ignore: Vec<String>`), which
            // says nothing about whether a user can set the key.
            if let Some(field_part) = strip_field_visibility(trimmed)
                && let Some(colon_pos) = field_part.find(':')
            {
                // Skip internal fields marked with #[serde(skip)]
                if pending_skip {
                    pending_skip = false;
                    pending_rename = None;
                    continue;
                }

                // If we have a pending rename, use that instead of the field name
                if let Some(renamed) = pending_rename.take() {
                    fields.insert(renamed);
                } else {
                    // No rename, use the field name converted to kebab-case
                    let field_name = field_part[..colon_pos].trim();
                    let kebab_name = field_name.replace('_', "-");
                    fields.insert(kebab_name);
                }
            }
        }
    }

    fields
}

/// Find all config files for rules
fn find_all_config_files() -> HashMap<String, Vec<std::path::PathBuf>> {
    let mut config_files = HashMap::new();

    // Check both patterns:
    // 1. src/rules/mdXXX_name/mdXXX_config.rs
    // 2. src/rules/mdXXX_name.rs (inline config)

    let rules_dir = Path::new("src/rules");
    if let Ok(entries) = fs::read_dir(rules_dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                // Check for mdXXX_config.rs inside the directory
                let dir_name = path.file_name().unwrap().to_str().unwrap();
                if let Some(rule_prefix) = dir_name.strip_prefix("md")
                    && let Some(num_end) = rule_prefix.find('_')
                {
                    let rule_num = &rule_prefix[..num_end];
                    // Parse as number to format correctly (MD001, not MD0001)
                    if let Ok(num) = rule_num.parse::<u32>() {
                        let rule_name = format!("MD{num:03}");

                        // Look for config file - try md###_config.rs format
                        let config_file = path.join(format!("md{rule_num}_config.rs"));
                        if config_file.exists() {
                            config_files.entry(rule_name).or_insert_with(Vec::new).push(config_file);
                        }
                    }
                }
            } else if path.is_file() {
                // Check for mdXXX_name.rs with inline config
                if let Some(filename) = path.file_name().and_then(|n| n.to_str())
                    && filename.starts_with("md")
                    && filename.ends_with(".rs")
                    && let Some(rule_prefix) = filename.strip_prefix("md").and_then(|s| s.strip_suffix(".rs"))
                    && let Some(num_end) = rule_prefix.find('_')
                {
                    let rule_num = &rule_prefix[..num_end];
                    // Parse as number to format correctly (MD001, not MD0001)
                    if let Ok(num) = rule_num.parse::<u32>() {
                        let rule_name = format!("MD{num:03}");

                        // Check if this file contains a config struct. Matched without
                        // a visibility prefix: a rule in a private module declares it
                        // `pub(super)`, and keying on `pub` dropped those rules from
                        // the sweep while it still reported a passing count.
                        let content = fs::read_to_string(&path).unwrap_or_default();
                        if content.contains(&format!("struct {rule_name}Config")) {
                            config_files.entry(rule_name).or_insert_with(Vec::new).push(path);
                        }
                    }
                }
            }
        }
    }

    config_files
}

/// The config keys a rule declares at runtime, as the validator sees them.
///
/// Used as an oracle for the source scan above: the two are derived independently, so
/// a rule with keys here and none there means the scan has gone blind, not that the
/// rule has nothing to document.
fn declared_config_keys(rule_name: &str) -> Vec<String> {
    rumdl_lib::config::default_registry()
        .config_keys_for(rule_name)
        .map(|keys| {
            keys.into_iter()
                // Accepted for every rule and belonging to no rule's own settings, so
                // they are documented once centrally rather than per rule.
                .filter(|key| !matches!(key.as_str(), "enabled" | "severity"))
                .collect()
        })
        .unwrap_or_default()
}

/// Extract documented field names from a markdown documentation file
fn get_documented_fields_in_file(doc_path: &Path) -> HashSet<String> {
    let content = fs::read_to_string(doc_path).unwrap_or_default();
    let mut fields = HashSet::new();
    let mut in_toml_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "```toml" {
            in_toml_block = true;
            continue;
        }

        if trimmed == "```" {
            in_toml_block = false;
            continue;
        }

        if in_toml_block {
            // Skip section headers like [MD007]
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                continue;
            }

            // Skip comments
            if trimmed.starts_with('#') {
                continue;
            }

            // Look for field = value patterns
            if let Some(equals_pos) = trimmed.find('=') {
                let field = trimmed[..equals_pos].trim();
                let field = if let Some(comment_pos) = field.find('#') {
                    field[..comment_pos].trim()
                } else {
                    field
                };

                if !field.is_empty() {
                    fields.insert(field.to_string());
                }
            }
        }
    }

    fields
}

#[test]
fn test_all_config_fields_are_documented() {
    let config_files = find_all_config_files();

    let mut all_passed = true;
    let mut report = String::from("\n=== Config Documentation Validation ===\n\n");
    let mut checked = 0usize;
    let mut not_scanned: Vec<String> = Vec::new();

    let mut rules: Vec<_> = config_files.keys().cloned().collect();
    rules.sort();

    for rule_name in &rules {
        let files = &config_files[rule_name];
        let doc_path = Path::new("docs").join(format!("{}.md", rule_name.to_lowercase()));

        if !doc_path.exists() {
            report.push_str(&format!("⚠️  {rule_name}: No documentation file found\n"));
            continue;
        }

        // Extract fields from all config files for this rule
        let mut config_fields = HashSet::new();
        for file in files {
            let fields = extract_fields_from_config_file(file);
            config_fields.extend(fields);
        }

        if config_fields.is_empty() {
            // A rule whose source scan yields nothing is NOT a checked rule. Say so,
            // and cross-check it against the keys the rule declares at runtime: an
            // empty scan of a rule that has settings means this parser stopped
            // matching the source, which would otherwise pass as full coverage.
            let declared = declared_config_keys(rule_name);
            assert!(
                declared.is_empty(),
                "{rule_name}: no config fields were parsed out of {files:?}, but the rule \
                 declares {declared:?}. The source scan has stopped matching this rule, \
                 so its documentation is no longer checked."
            );
            not_scanned.push(rule_name.clone());
            continue;
        }
        checked += 1;

        let documented_fields = get_documented_fields_in_file(&doc_path);

        // Find undocumented fields
        let mut undocumented: Vec<String> = config_fields
            .iter()
            .filter(|f| !documented_fields.contains(*f))
            .cloned()
            .collect();

        undocumented.sort();

        if !undocumented.is_empty() {
            report.push_str(&format!("❌ {rule_name}: Undocumented config fields:\n"));
            for field in &undocumented {
                report.push_str(&format!("   - {field}\n"));
            }
            let rule_lower = rule_name.to_lowercase();
            report.push_str(&format!("   File: docs/{rule_lower}.md\n"));
            all_passed = false;
        } else {
            let count = config_fields.len();
            report.push_str(&format!("✅ {rule_name}: All {count} config fields documented\n"));
        }
    }

    let found = rules.len();
    report.push_str(&format!(
        "\n=== Summary: {checked} rules checked, {found} config structs found ===\n"
    ));
    if !not_scanned.is_empty() {
        report.push_str(&format!("Rules with no user-settable fields: {not_scanned:?}\n"));
    }

    println!("{report}");

    if !all_passed {
        panic!(
            "\n\n❌ Some config fields are not documented!\n\
            Please add documentation for the fields listed above.\n\
            Documentation files are in docs/mdXXX.md\n\
            \n\
            To fix: Add TOML examples showing the missing config fields.\n"
        );
    }
}
