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
                let rule_id = warning.rule_name.unwrap_or("unknown");
                json!({
                    "ruleId": rule_id,
                    "level": "warning",
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
            let rule_id = warning.rule_name.unwrap_or("unknown");

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
                    },
                    "defaultConfiguration": {
                        "level": "warning"
                    }
                })
            });

            let result = json!({
                "ruleId": rule_id,
                "level": "warning",
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
    use crate::rule::{Fix, Severity};
    use serde_json::Value;

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
            rule_name: Some("MD001"),
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
            rule_name: Some("MD001"),
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
                rule_name: Some("MD001"),
                message: "First warning".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 10,
                column: 3,
                end_line: 10,
                end_column: 20,
                rule_name: Some("MD013"),
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
        assert_eq!(results[0]["locations"][0]["physicalLocation"]["region"]["startLine"], 5);
        assert_eq!(results[1]["ruleId"], "MD013");
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
                rule_name: Some("MD001"),
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
                    rule_name: Some("MD001"),
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
                        rule_name: Some("MD013"),
                        message: "Warning 1 in file 2".to_string(),
                        severity: Severity::Warning,
                        fix: None,
                    },
                    LintWarning {
                        line: 10,
                        column: 1,
                        end_line: 10,
                        end_column: 10,
                        rule_name: Some("MD022"),
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
                    rule_name: Some("MD001"),
                    message: "First MD001".to_string(),
                    severity: Severity::Warning,
                    fix: None,
                },
                LintWarning {
                    line: 10,
                    column: 1,
                    end_line: 10,
                    end_column: 5,
                    rule_name: Some("MD001"),
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
    fn test_severity_always_warning() {
        let formatter = SarifFormatter::new();

        // Test that all severities are output as "warning" in SARIF format
        let warnings = vec![
            LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("MD001"),
                message: "Warning severity".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 2,
                column: 1,
                end_line: 2,
                end_column: 5,
                rule_name: Some("MD002"),
                message: "Error severity".to_string(),
                severity: Severity::Error,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let sarif: Value = serde_json::from_str(&output).unwrap();

        let results = sarif["runs"][0]["results"].as_array().unwrap();
        // Both should use level "warning" regardless of severity
        assert_eq!(results[0]["level"], "warning");
        assert_eq!(results[1]["level"], "warning");
    }

    #[test]
    fn test_special_characters_in_message() {
        let formatter = SarifFormatter::new();
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001"),
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
            rule_name: Some("MD001"),
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
}
