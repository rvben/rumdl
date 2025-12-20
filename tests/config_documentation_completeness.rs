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

        // Look for struct definition
        if trimmed.contains("pub struct MD") && trimmed.contains("Config") {
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

            // Extract field names - look for pub field_name: Type patterns
            if trimmed.starts_with("pub ")
                && trimmed.contains(':')
                && let Some(field_part) = trimmed.strip_prefix("pub ")
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

                        // Check if this file contains a config struct
                        let content = fs::read_to_string(&path).unwrap_or_default();
                        if content.contains(&format!("pub struct {rule_name}Config")) {
                            config_files.entry(rule_name).or_insert_with(Vec::new).push(path);
                        }
                    }
                }
            }
        }
    }

    config_files
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
            continue;
        }

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

    let count = rules.len();
    report.push_str(&format!("\n=== Summary: {count} rules checked ===\n"));

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
