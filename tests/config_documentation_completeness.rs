//! Test to ensure all configuration options are documented
//!
//! This test uses compile-time reflection to validate that every public field
//! in rule config structs has corresponding documentation in docs/mdXXX.md files.
//!
//! Strategy: We manually maintain a list of config structs and their fields,
//! and validate they appear in documentation. This is more robust than parsing.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Config field definition
struct ConfigField {
    rule: &'static str,
    field: &'static str,
    required_in_docs: bool, // Some fields like 'enabled' are implicit
}

/// All config fields that should be documented
/// This list is manually maintained but easy to update
fn get_documented_config_fields() -> Vec<ConfigField> {
    vec![
        // MD004
        ConfigField {
            rule: "MD004",
            field: "style",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD004",
            field: "after-marker",
            required_in_docs: true,
        },
        // MD007
        ConfigField {
            rule: "MD007",
            field: "indent",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD007",
            field: "start-indented",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD007",
            field: "start-indent",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD007",
            field: "style",
            required_in_docs: true,
        },
        // MD009
        ConfigField {
            rule: "MD009",
            field: "br-spaces",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD009",
            field: "list-item-empty-lines",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD009",
            field: "strict",
            required_in_docs: true,
        },
        // MD010
        ConfigField {
            rule: "MD010",
            field: "spaces-per-tab",
            required_in_docs: true,
        },
        // MD012
        ConfigField {
            rule: "MD012",
            field: "maximum",
            required_in_docs: true,
        },
        // MD013
        ConfigField {
            rule: "MD013",
            field: "line-length",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD013",
            field: "code-blocks",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD013",
            field: "tables",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD013",
            field: "headings",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD013",
            field: "paragraphs",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD013",
            field: "strict",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD013",
            field: "reflow",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD013",
            field: "reflow-mode",
            required_in_docs: true,
        },
        // MD014
        ConfigField {
            rule: "MD014",
            field: "show-output",
            required_in_docs: true,
        },
        // MD022
        ConfigField {
            rule: "MD022",
            field: "lines-above",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD022",
            field: "lines-below",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD022",
            field: "allowed-at-start",
            required_in_docs: true,
        },
        // MD024
        ConfigField {
            rule: "MD024",
            field: "siblings-only",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD024",
            field: "allow-different-nesting",
            required_in_docs: true,
        },
        // MD025
        ConfigField {
            rule: "MD025",
            field: "level",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD025",
            field: "allow-document-sections",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD025",
            field: "allow-with-separators",
            required_in_docs: true,
        },
        // MD026
        ConfigField {
            rule: "MD026",
            field: "punctuation",
            required_in_docs: true,
        },
        // MD029
        ConfigField {
            rule: "MD029",
            field: "style",
            required_in_docs: true,
        },
        // MD030
        ConfigField {
            rule: "MD030",
            field: "ul-single",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD030",
            field: "ul-multi",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD030",
            field: "ol-single",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD030",
            field: "ol-multi",
            required_in_docs: true,
        },
        // MD033
        ConfigField {
            rule: "MD033",
            field: "allowed-elements",
            required_in_docs: true,
        },
        // MD035
        ConfigField {
            rule: "MD035",
            field: "style",
            required_in_docs: true,
        },
        // MD036
        ConfigField {
            rule: "MD036",
            field: "punctuation",
            required_in_docs: true,
        },
        // MD044
        ConfigField {
            rule: "MD044",
            field: "names",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD044",
            field: "code-blocks",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD044",
            field: "html-elements",
            required_in_docs: true,
        },
        // MD045
        ConfigField {
            rule: "MD045",
            field: "placeholder-text",
            required_in_docs: true,
        },
        // MD046
        ConfigField {
            rule: "MD046",
            field: "style",
            required_in_docs: true,
        },
        // MD048
        ConfigField {
            rule: "MD048",
            field: "style",
            required_in_docs: true,
        },
        // MD049
        ConfigField {
            rule: "MD049",
            field: "style",
            required_in_docs: true,
        },
        // MD050
        ConfigField {
            rule: "MD050",
            field: "style",
            required_in_docs: true,
        },
        // MD054
        ConfigField {
            rule: "MD054",
            field: "autolink",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD054",
            field: "inline",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD054",
            field: "full",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD054",
            field: "collapsed",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD054",
            field: "shortcut",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD054",
            field: "url-inline",
            required_in_docs: true,
        },
        // MD055
        ConfigField {
            rule: "MD055",
            field: "style",
            required_in_docs: true,
        },
        // MD057
        ConfigField {
            rule: "MD057",
            field: "skip-media-files",
            required_in_docs: true,
        },
        // MD060
        ConfigField {
            rule: "MD060",
            field: "style",
            required_in_docs: true,
        },
        ConfigField {
            rule: "MD060",
            field: "max-width",
            required_in_docs: true,
        },
    ]
}

/// Extract documented config field names from a markdown documentation file
fn get_documented_fields_in_file(doc_path: &Path) -> HashSet<String> {
    let content = fs::read_to_string(doc_path).unwrap_or_default();
    let mut fields = HashSet::new();

    // Look for TOML config blocks
    let mut in_toml_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Start of TOML block
        if trimmed == "```toml" {
            in_toml_block = true;
            continue;
        }

        // End of code block
        if trimmed == "```" {
            in_toml_block = false;
            continue;
        }

        // Extract field names from TOML config
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
                // Remove inline comments
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
    let config_fields = get_documented_config_fields();

    // Group by rule
    let mut rules: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for field in &config_fields {
        if field.required_in_docs {
            rules.entry(field.rule).or_default().push(field.field);
        }
    }

    let mut all_passed = true;
    let mut report = String::from("\n=== Config Documentation Validation ===\n\n");

    for (rule, expected_fields) in &rules {
        let doc_path = Path::new("docs").join(format!("{}.md", rule.to_lowercase()));

        if !doc_path.exists() {
            report.push_str(&format!("❌ {rule}: No documentation file found\n"));
            all_passed = false;
            continue;
        }

        let documented_fields = get_documented_fields_in_file(&doc_path);

        // Find undocumented fields
        let mut undocumented: Vec<String> = expected_fields
            .iter()
            .filter(|&&f| !documented_fields.contains(f))
            .map(|&f| f.to_string())
            .collect();

        undocumented.sort();

        if !undocumented.is_empty() {
            report.push_str(&format!("❌ {rule}: Undocumented config fields:\n"));
            for field in &undocumented {
                report.push_str(&format!("   - {field}\n"));
            }
            let rule_lower = rule.to_lowercase();
            report.push_str(&format!("   File: docs/{rule_lower}.md\n"));
            all_passed = false;
        } else {
            let count = expected_fields.len();
            report.push_str(&format!("✅ {rule}: All {count} config fields documented\n"));
        }
    }

    let count = rules.len();
    report.push_str(&format!("\n=== Summary: {count} rules checked ===\n"));

    // Always print the report for visibility
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
