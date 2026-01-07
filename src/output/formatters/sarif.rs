//! SARIF 2.1.0 output format

use crate::output::OutputFormatter;
use crate::rule::LintWarning;
use serde_json::json;

/// SARIF (Static Analysis Results Interchange Format) formatter
pub struct SarifFormatter;

impl Default for SarifFormatter {
    fn default() -> Self {
        Self
    }
}

impl SarifFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for SarifFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        // Format warnings for a single file as a minimal SARIF document
        let results: Vec<_> = warnings
            .iter()
            .map(|warning| {
                let rule_id = warning.rule_name.as_deref().unwrap_or("unknown");
                let level = match warning.severity {
                    crate::rule::Severity::Error => "error",
                    crate::rule::Severity::Warning => "warning",
                    crate::rule::Severity::Info => "note",
                };
                json!({
                    "ruleId": rule_id,
                    "level": level,
                    "message": {
                        "text": warning.message
                    },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": {
                                "uri": file_path
                            },
                            "region": {
                                "startLine": warning.line,
                                "startColumn": warning.column
                            }
                        }
                    }]
                })
            })
            .collect();

        let sarif_doc = json!({
            "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "rumdl",
                        "version": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://github.com/rvben/rumdl"
                    }
                },
                "results": results
            }]
        });

        serde_json::to_string_pretty(&sarif_doc).unwrap_or_else(|_| r#"{"version":"2.1.0","runs":[]}"#.to_string())
    }
}

/// Format all warnings as SARIF 2.1.0 report
pub fn format_sarif_report(all_warnings: &[(String, Vec<LintWarning>)]) -> String {
    let mut results = Vec::new();
    let mut rules = std::collections::HashMap::new();

    // Collect all results and build rule index
    for (file_path, warnings) in all_warnings {
        for warning in warnings {
            let rule_id = warning.rule_name.as_deref().unwrap_or("unknown");

            // Add rule to index if not already present
            rules.entry(rule_id).or_insert_with(|| {
                json!({
                    "id": rule_id,
                    "name": rule_id,
                    "shortDescription": {
                        "text": format!("Markdown rule {}", rule_id)
                    },
                    "fullDescription": {
                        "text": format!("Markdown linting rule {}", rule_id)
                    }
                })
            });

            let level = match warning.severity {
                crate::rule::Severity::Error => "error",
                crate::rule::Severity::Warning => "warning",
                crate::rule::Severity::Info => "note",
            };
            let result = json!({
                "ruleId": rule_id,
                "level": level,
                "message": {
                    "text": warning.message
                },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": {
                            "uri": file_path
                        },
                        "region": {
                            "startLine": warning.line,
                            "startColumn": warning.column
                        }
                    }
                }]
            });

            results.push(result);
        }
    }

    // Build the complete SARIF document
    let sarif_doc = json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "rumdl",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/rvben/rumdl",
                    "rules": rules.values().cloned().collect::<Vec<_>>()
                }
            },
            "results": results
        }]
    });

    serde_json::to_string_pretty(&sarif_doc).unwrap_or_else(|_| r#"{"version":"2.1.0","runs":[]}"#.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;
    use crate::rule::{Fix, Rule, Severity};
    use crate::rules::MD032BlanksAroundLists;
    use serde_json::Value;
    use std::path::PathBuf;

    #[test]
    fn test_sarif_formatter_default() {
        let _formatter = SarifFormatter;
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_sarif_formatter_new() {
        let _formatter = SarifFormatter::new();
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_format_warnings_empty() {
        let formatter = SarifFormatter::new();
        let warnings = vec![];
        let output = formatter.format_warnings(&warnings, "test.md");

        let sarif: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(sarif["version"], "2.1.0");
        assert_eq!(
            sarif["$schema"],
            "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json"
        );
        assert_eq!(sarif["runs"][0]["results"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_format_single_warning() {
        let formatter = SarifFormatter::new();
        let warnings = vec![LintWarning {
            line: 10,
            column: 5,
            end_line: 10,
            end_column: 15,
            rule_name: Some("MD001".to_string()),
            message: "Heading levels should only increment by one level at a time".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "README.md");
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);

        let result = &results[0];
        assert_eq!(result["ruleId"], "MD001");
        assert_eq!(result["level"], "warning");
        assert_eq!(
            result["message"]["text"],
            "Heading levels should only increment by one level at a time"
        );
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "README.md"
        );
        assert_eq!(result["locations"][0]["physicalLocation"]["region"]["startLine"], 10);
        assert_eq!(result["locations"][0]["physicalLocation"]["region"]["startColumn"], 5);
    }

    #[test]
    fn test_format_single_warning_with_fix() {
        let formatter = SarifFormatter::new();
        let warnings = vec![LintWarning {
            line: 10,
            column: 5,
            end_line: 10,
            end_column: 15,
            rule_name: Some("MD001".to_string()),
            message: "Heading levels should only increment by one level at a time".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 100..110,
                replacement: "## Heading".to_string(),
            }),
        }];

        let output = formatter.format_warnings(&warnings, "README.md");
        let sarif: Value = serde_json::from_str(&output).unwrap();

        // SARIF format doesn't indicate fixable issues in the basic format
        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["ruleId"], "MD001");
    }

    #[test]
    fn test_format_multiple_warnings() {
        let formatter = SarifFormatter::new();
        let warnings = vec![
            LintWarning {
                line: 5,
                column: 1,
                end_line: 5,
                end_column: 10,
                rule_name: Some("MD001".to_string()),
                message: "First warning".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 10,
                column: 3,
                end_line: 10,
                end_column: 20,
                rule_name: Some("MD013".to_string()),
                message: "Second warning".to_string(),
                severity: Severity::Error,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["ruleId"], "MD001");
        assert_eq!(results[0]["level"], "warning");
        assert_eq!(results[0]["locations"][0]["physicalLocation"]["region"]["startLine"], 5);
        assert_eq!(results[1]["ruleId"], "MD013");
        assert_eq!(results[1]["level"], "error");
        assert_eq!(
            results[1]["locations"][0]["physicalLocation"]["region"]["startLine"],
            10
        );
    }

    #[test]
    fn test_format_warning_unknown_rule() {
        let formatter = SarifFormatter::new();
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: None,
            message: "Unknown rule warning".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "file.md");
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results[0]["ruleId"], "unknown");
    }

    #[test]
    fn test_tool_information() {
        let formatter = SarifFormatter::new();
        let warnings = vec![];
        let output = formatter.format_warnings(&warnings, "test.md");

        let sarif: Value = serde_json::from_str(&output).unwrap();
        let driver = &sarif["runs"][0]["tool"]["driver"];

        assert_eq!(driver["name"], "rumdl");
        assert_eq!(driver["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(driver["informationUri"], "https://github.com/rvben/rumdl");
    }

    #[test]
    fn test_sarif_report_empty() {
        let warnings = vec![];
        let output = format_sarif_report(&warnings);

        let sarif: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(sarif["version"], "2.1.0");
        assert_eq!(sarif["runs"][0]["results"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_sarif_report_single_file() {
        let warnings = vec![(
            "test.md".to_string(),
            vec![LintWarning {
                line: 10,
                column: 5,
                end_line: 10,
                end_column: 15,
                rule_name: Some("MD001".to_string()),
                message: "Test warning".to_string(),
                severity: Severity::Warning,
                fix: None,
            }],
        )];

        let output = format_sarif_report(&warnings);
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "test.md"
        );

        // Check that rule is defined in driver
        let rules = sarif["runs"][0]["tool"]["driver"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["id"], "MD001");
    }

    #[test]
    fn test_sarif_report_multiple_files() {
        let warnings = vec![
            (
                "file1.md".to_string(),
                vec![LintWarning {
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 5,
                    rule_name: Some("MD001".to_string()),
                    message: "Warning in file 1".to_string(),
                    severity: Severity::Warning,
                    fix: None,
                }],
            ),
            (
                "file2.md".to_string(),
                vec![
                    LintWarning {
                        line: 5,
                        column: 1,
                        end_line: 5,
                        end_column: 10,
                        rule_name: Some("MD013".to_string()),
                        message: "Warning 1 in file 2".to_string(),
                        severity: Severity::Warning,
                        fix: None,
                    },
                    LintWarning {
                        line: 10,
                        column: 1,
                        end_line: 10,
                        end_column: 10,
                        rule_name: Some("MD022".to_string()),
                        message: "Warning 2 in file 2".to_string(),
                        severity: Severity::Error,
                        fix: None,
                    },
                ],
            ),
        ];

        let output = format_sarif_report(&warnings);
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);

        // Check severity mapping
        assert_eq!(results[0]["level"], "warning"); // MD001 - Warning
        assert_eq!(results[1]["level"], "warning"); // MD013 - Warning
        assert_eq!(results[2]["level"], "error"); // MD022 - Error

        // Check that all rules are defined
        let rules = sarif["runs"][0]["tool"]["driver"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 3);

        let rule_ids: Vec<&str> = rules.iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert!(rule_ids.contains(&"MD001"));
        assert!(rule_ids.contains(&"MD013"));
        assert!(rule_ids.contains(&"MD022"));
    }

    #[test]
    fn test_rule_deduplication() {
        let warnings = vec![(
            "test.md".to_string(),
            vec![
                LintWarning {
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 5,
                    rule_name: Some("MD001".to_string()),
                    message: "First MD001".to_string(),
                    severity: Severity::Warning,
                    fix: None,
                },
                LintWarning {
                    line: 10,
                    column: 1,
                    end_line: 10,
                    end_column: 5,
                    rule_name: Some("MD001".to_string()),
                    message: "Second MD001".to_string(),
                    severity: Severity::Warning,
                    fix: None,
                },
            ],
        )];

        let output = format_sarif_report(&warnings);
        let sarif: Value = serde_json::from_str(&output).unwrap();

        // Should have 2 results but only 1 rule
        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);

        let rules = sarif["runs"][0]["tool"]["driver"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["id"], "MD001");
    }

    #[test]
    fn test_severity_mapping() {
        let formatter = SarifFormatter::new();

        let warnings = vec![
            LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("MD001".to_string()),
                message: "Warning severity".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 2,
                column: 1,
                end_line: 2,
                end_column: 5,
                rule_name: Some("MD032".to_string()),
                message: "Error severity".to_string(),
                severity: Severity::Error,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results[0]["level"], "warning"); // Warning → "warning"
        assert_eq!(results[1]["level"], "error"); // Error → "error"
    }

    #[test]
    fn test_sarif_report_severity_mapping() {
        let warnings = vec![
            (
                "file1.md".to_string(),
                vec![LintWarning {
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 5,
                    rule_name: Some("MD001".to_string()),
                    message: "Warning".to_string(),
                    severity: Severity::Warning,
                    fix: None,
                }],
            ),
            (
                "file2.md".to_string(),
                vec![LintWarning {
                    line: 5,
                    column: 1,
                    end_line: 5,
                    end_column: 10,
                    rule_name: Some("MD032".to_string()),
                    message: "Error".to_string(),
                    severity: Severity::Error,
                    fix: None,
                }],
            ),
        ];

        let output = format_sarif_report(&warnings);
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["level"], "warning");
        assert_eq!(results[1]["level"], "error");
    }

    #[test]
    fn test_special_characters_in_message() {
        let formatter = SarifFormatter::new();
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001".to_string()),
            message: "Warning with \"quotes\" and 'apostrophes' and \n newline".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "test.md");
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        // JSON should properly handle special characters
        assert_eq!(
            results[0]["message"]["text"],
            "Warning with \"quotes\" and 'apostrophes' and \n newline"
        );
    }

    #[test]
    fn test_special_characters_in_file_path() {
        let formatter = SarifFormatter::new();
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001".to_string()),
            message: "Test".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "path/with spaces/and-dashes.md");
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(
            results[0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "path/with spaces/and-dashes.md"
        );
    }

    #[test]
    fn test_sarif_schema_version() {
        let formatter = SarifFormatter::new();
        let warnings = vec![];
        let output = formatter.format_warnings(&warnings, "test.md");

        let sarif: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            sarif["$schema"],
            "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json"
        );
        assert_eq!(sarif["version"], "2.1.0");
    }

    // ===== Comprehensive tests for full coverage =====

    #[test]
    fn test_md032_integration_produces_warning_level() {
        // Test with actual MD032 rule that produces Warning severity warnings
        let content = "# Heading\n- List item without blank line before";
        let rule = MD032BlanksAroundLists::default();
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, Some(PathBuf::from("test.md")));
        let warnings = rule.check(&ctx).expect("MD032 check should succeed");

        // MD032 should produce at least one warning-level warning
        assert!(!warnings.is_empty(), "MD032 should flag list without blank line");

        let formatter = SarifFormatter::new();
        let output = formatter.format_warnings(&warnings, "test.md");
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        // Verify at least one result has warning level (MD032 uses Severity::Warning)
        assert!(
            results.iter().any(|r| r["level"] == "warning"),
            "MD032 violations should produce 'warning' level in SARIF output"
        );
        // Verify rule ID is MD032
        assert!(
            results.iter().any(|r| r["ruleId"] == "MD032"),
            "Results should include MD032 rule"
        );
    }

    #[test]
    fn test_all_warnings_no_errors() {
        // Edge case: File with only Warning severity (no Error severity)
        let formatter = SarifFormatter::new();
        let warnings = vec![
            LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("MD001".to_string()),
                message: "First warning".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 2,
                column: 1,
                end_line: 2,
                end_column: 5,
                rule_name: Some("MD013".to_string()),
                message: "Second warning".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 3,
                column: 1,
                end_line: 3,
                end_column: 5,
                rule_name: Some("MD041".to_string()),
                message: "Third warning".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        // All should be "warning" level
        assert!(results.iter().all(|r| r["level"] == "warning"));
        // None should be "error" level
        assert!(!results.iter().any(|r| r["level"] == "error"));
    }

    #[test]
    fn test_all_errors_no_warnings() {
        // Edge case: File with only Error severity (no Warning severity)
        let formatter = SarifFormatter::new();
        let warnings = vec![
            LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("MD032".to_string()),
                message: "First error".to_string(),
                severity: Severity::Error,
                fix: None,
            },
            LintWarning {
                line: 2,
                column: 1,
                end_line: 2,
                end_column: 5,
                rule_name: Some("MD032".to_string()),
                message: "Second error".to_string(),
                severity: Severity::Error,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        // All should be "error" level
        assert!(results.iter().all(|r| r["level"] == "error"));
        // None should be "warning" level
        assert!(!results.iter().any(|r| r["level"] == "warning"));
    }

    #[test]
    fn test_mixed_severities_same_file() {
        // Edge case: Same file with both Warning and Error severities interleaved
        let formatter = SarifFormatter::new();
        let warnings = vec![
            LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("MD001".to_string()),
                message: "Warning".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 2,
                column: 1,
                end_line: 2,
                end_column: 5,
                rule_name: Some("MD032".to_string()),
                message: "Error".to_string(),
                severity: Severity::Error,
                fix: None,
            },
            LintWarning {
                line: 3,
                column: 1,
                end_line: 3,
                end_column: 5,
                rule_name: Some("MD013".to_string()),
                message: "Warning".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 4,
                column: 1,
                end_line: 4,
                end_column: 5,
                rule_name: Some("MD032".to_string()),
                message: "Error".to_string(),
                severity: Severity::Error,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 4);

        // Verify exact mapping for each result
        assert_eq!(results[0]["level"], "warning"); // Line 1
        assert_eq!(results[1]["level"], "error"); // Line 2
        assert_eq!(results[2]["level"], "warning"); // Line 3
        assert_eq!(results[3]["level"], "error"); // Line 4

        // Count severities
        let warning_count = results.iter().filter(|r| r["level"] == "warning").count();
        let error_count = results.iter().filter(|r| r["level"] == "error").count();
        assert_eq!(warning_count, 2);
        assert_eq!(error_count, 2);
    }

    #[test]
    fn test_rule_deduplication_preserves_severity() {
        // Test that rule deduplication doesn't lose severity information
        // Same rule (MD032) appears multiple times in same file with Error severity
        let warnings = vec![(
            "test.md".to_string(),
            vec![
                LintWarning {
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 5,
                    rule_name: Some("MD032".to_string()),
                    message: "First MD032 error".to_string(),
                    severity: Severity::Error,
                    fix: None,
                },
                LintWarning {
                    line: 5,
                    column: 1,
                    end_line: 5,
                    end_column: 5,
                    rule_name: Some("MD032".to_string()),
                    message: "Second MD032 error".to_string(),
                    severity: Severity::Error,
                    fix: None,
                },
                LintWarning {
                    line: 10,
                    column: 1,
                    end_line: 10,
                    end_column: 5,
                    rule_name: Some("MD032".to_string()),
                    message: "Third MD032 error".to_string(),
                    severity: Severity::Error,
                    fix: None,
                },
            ],
        )];

        let output = format_sarif_report(&warnings);
        let sarif: Value = serde_json::from_str(&output).unwrap();

        // Should have 3 results, all with error level
        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r["level"] == "error"));
        assert!(results.iter().all(|r| r["ruleId"] == "MD032"));

        // Should have only 1 rule definition (deduplicated)
        let rules = sarif["runs"][0]["tool"]["driver"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["id"], "MD032");
        // Verify defaultConfiguration was removed (should not be present)
        assert!(rules[0].get("defaultConfiguration").is_none());
    }

    #[test]
    fn test_sarif_output_valid_json_schema() {
        // Verify SARIF output is valid JSON and has required top-level fields
        let formatter = SarifFormatter::new();
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001".to_string()),
            message: "Test".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "test.md");

        // Must be valid JSON
        let sarif: Value = serde_json::from_str(&output).expect("SARIF output must be valid JSON");

        // SARIF 2.1.0 required fields at root level
        assert!(sarif.get("version").is_some(), "Must have version field");
        assert!(sarif.get("$schema").is_some(), "Must have $schema field");
        assert!(sarif.get("runs").is_some(), "Must have runs field");

        // Runs must be an array with at least one run
        let runs = sarif["runs"].as_array().expect("runs must be an array");
        assert!(!runs.is_empty(), "Must have at least one run");

        // Each run must have tool and results
        let run = &runs[0];
        assert!(run.get("tool").is_some(), "Run must have tool field");
        assert!(run.get("results").is_some(), "Run must have results field");

        // Tool must have driver
        assert!(run["tool"].get("driver").is_some(), "Tool must have driver field");

        // Results must be an array
        assert!(run["results"].is_array(), "Results must be an array");

        // Each result must have required fields
        let results = run["results"].as_array().unwrap();
        for result in results {
            assert!(result.get("ruleId").is_some(), "Result must have ruleId");
            assert!(result.get("level").is_some(), "Result must have level");
            assert!(result.get("message").is_some(), "Result must have message");
            assert!(result.get("locations").is_some(), "Result must have locations");

            // Level must be a valid SARIF level
            let level = result["level"].as_str().unwrap();
            assert!(
                matches!(level, "warning" | "error" | "note" | "none" | "open"),
                "Level must be valid SARIF level, got: {level}"
            );
        }
    }

    #[test]
    fn test_default_configuration_removed() {
        // Verify that defaultConfiguration is no longer present in rule metadata
        // (it was semantically incorrect since severity is instance-specific)
        let warnings = vec![(
            "test.md".to_string(),
            vec![
                LintWarning {
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 5,
                    rule_name: Some("MD001".to_string()),
                    message: "Warning".to_string(),
                    severity: Severity::Warning,
                    fix: None,
                },
                LintWarning {
                    line: 2,
                    column: 1,
                    end_line: 2,
                    end_column: 5,
                    rule_name: Some("MD032".to_string()),
                    message: "Error".to_string(),
                    severity: Severity::Error,
                    fix: None,
                },
            ],
        )];

        let output = format_sarif_report(&warnings);
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let rules = sarif["runs"][0]["tool"]["driver"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 2);

        // Verify defaultConfiguration is not present in any rule
        for rule in rules {
            assert!(
                rule.get("defaultConfiguration").is_none(),
                "Rule {} should not have defaultConfiguration (it's instance-specific, not rule-specific)",
                rule["id"]
            );
        }
    }

    #[test]
    fn test_unknown_rule_with_error_severity() {
        // Edge case: Unknown rule (None) with Error severity
        let formatter = SarifFormatter::new();
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: None,
            message: "Unknown error".to_string(),
            severity: Severity::Error,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "test.md");
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["ruleId"], "unknown");
        assert_eq!(results[0]["level"], "error"); // Should still map Error → "error"
    }

    #[test]
    fn test_exhaustive_severity_mapping() {
        // Document all Severity enum variants and their SARIF mappings
        // This test will break if new Severity variants are added without updating SARIF mapper
        let formatter = SarifFormatter::new();

        // Test all current Severity variants
        let all_severities = vec![(Severity::Warning, "warning"), (Severity::Error, "error")];

        for (severity, expected_level) in all_severities {
            let warnings = vec![LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("TEST".to_string()),
                message: format!("Test {severity:?}"),
                severity,
                fix: None,
            }];

            let output = formatter.format_warnings(&warnings, "test.md");
            let sarif: Value = serde_json::from_str(&output).unwrap();

            let results = sarif["runs"][0]["results"].as_array().unwrap();
            assert_eq!(
                results[0]["level"], expected_level,
                "Severity::{severity:?} should map to SARIF level '{expected_level}'"
            );
        }
    }
}
