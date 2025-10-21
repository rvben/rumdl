//! GitLab Code Quality report format

use crate::output::OutputFormatter;
use crate::rule::LintWarning;
use serde_json::json;

/// GitLab Code Quality formatter
/// Outputs in GitLab's code quality JSON format
pub struct GitLabFormatter;

impl Default for GitLabFormatter {
    fn default() -> Self {
        Self
    }
}

impl GitLabFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for GitLabFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        // Format warnings for a single file as GitLab Code Quality issues
        let issues: Vec<_> = warnings
            .iter()
            .map(|warning| {
                let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");
                let fingerprint = format!("{}-{}-{}-{}", file_path, warning.line, warning.column, rule_name);

                json!({
                    "description": warning.message,
                    "check_name": rule_name,
                    "fingerprint": fingerprint,
                    "severity": "minor",
                    "location": {
                        "path": file_path,
                        "lines": {
                            "begin": warning.line
                        }
                    }
                })
            })
            .collect();

        serde_json::to_string_pretty(&issues).unwrap_or_else(|_| "[]".to_string())
    }
}

/// Format all warnings as GitLab Code Quality report
pub fn format_gitlab_report(all_warnings: &[(String, Vec<LintWarning>)]) -> String {
    let mut issues = Vec::new();

    for (file_path, warnings) in all_warnings {
        for warning in warnings {
            let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");

            // Create a fingerprint for deduplication
            let fingerprint = format!("{}-{}-{}-{}", file_path, warning.line, warning.column, rule_name);

            let issue = json!({
                "description": warning.message,
                "check_name": rule_name,
                "fingerprint": fingerprint,
                "severity": "minor",
                "location": {
                    "path": file_path,
                    "lines": {
                        "begin": warning.line
                    }
                }
            });

            issues.push(issue);
        }
    }

    serde_json::to_string_pretty(&issues).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{Fix, Severity};
    use serde_json::Value;

    #[test]
    fn test_gitlab_formatter_default() {
        let _formatter = GitLabFormatter;
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_gitlab_formatter_new() {
        let _formatter = GitLabFormatter::new();
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_format_warnings_empty() {
        let formatter = GitLabFormatter::new();
        let warnings = vec![];
        let output = formatter.format_warnings(&warnings, "test.md");
        assert_eq!(output, "[]");
    }

    #[test]
    fn test_format_single_warning() {
        let formatter = GitLabFormatter::new();
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
        let issues: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(issues.len(), 1);
        let issue = &issues[0];
        assert_eq!(
            issue["description"],
            "Heading levels should only increment by one level at a time"
        );
        assert_eq!(issue["check_name"], "MD001");
        assert_eq!(issue["fingerprint"], "README.md-10-5-MD001");
        assert_eq!(issue["severity"], "minor");
        assert_eq!(issue["location"]["path"], "README.md");
        assert_eq!(issue["location"]["lines"]["begin"], 10);
    }

    #[test]
    fn test_format_single_warning_with_fix() {
        let formatter = GitLabFormatter::new();
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
        let issues: Vec<Value> = serde_json::from_str(&output).unwrap();

        // GitLab format doesn't indicate fixable issues
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0]["check_name"], "MD001");
    }

    #[test]
    fn test_format_multiple_warnings() {
        let formatter = GitLabFormatter::new();
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
        let issues: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0]["check_name"], "MD001");
        assert_eq!(issues[0]["location"]["lines"]["begin"], 5);
        assert_eq!(issues[1]["check_name"], "MD013");
        assert_eq!(issues[1]["location"]["lines"]["begin"], 10);
    }

    #[test]
    fn test_format_warning_unknown_rule() {
        let formatter = GitLabFormatter::new();
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
        let issues: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(issues[0]["check_name"], "unknown");
        assert_eq!(issues[0]["fingerprint"], "file.md-1-1-unknown");
    }

    #[test]
    fn test_gitlab_report_empty() {
        let warnings = vec![];
        let output = format_gitlab_report(&warnings);
        assert_eq!(output, "[]");
    }

    #[test]
    fn test_gitlab_report_single_file() {
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

        let output = format_gitlab_report(&warnings);
        let issues: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0]["location"]["path"], "test.md");
    }

    #[test]
    fn test_gitlab_report_multiple_files() {
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

        let output = format_gitlab_report(&warnings);
        let issues: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(issues.len(), 3);
        assert_eq!(issues[0]["location"]["path"], "file1.md");
        assert_eq!(issues[1]["location"]["path"], "file2.md");
        assert_eq!(issues[2]["location"]["path"], "file2.md");
    }

    #[test]
    fn test_fingerprint_uniqueness() {
        let formatter = GitLabFormatter::new();

        // Same line/column but different rules should have different fingerprints
        let warnings = vec![
            LintWarning {
                line: 10,
                column: 5,
                end_line: 10,
                end_column: 15,
                rule_name: Some("MD001".to_string()),
                message: "First rule".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 10,
                column: 5,
                end_line: 10,
                end_column: 15,
                rule_name: Some("MD002".to_string()),
                message: "Second rule".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let issues: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_ne!(issues[0]["fingerprint"], issues[1]["fingerprint"]);
        assert_eq!(issues[0]["fingerprint"], "test.md-10-5-MD001");
        assert_eq!(issues[1]["fingerprint"], "test.md-10-5-MD002");
    }

    #[test]
    fn test_severity_always_minor() {
        let formatter = GitLabFormatter::new();

        // Test that all severities are output as "minor" in GitLab format
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
                rule_name: Some("MD002".to_string()),
                message: "Error severity".to_string(),
                severity: Severity::Error,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let issues: Vec<Value> = serde_json::from_str(&output).unwrap();

        // Both should use severity "minor" regardless of actual severity
        assert_eq!(issues[0]["severity"], "minor");
        assert_eq!(issues[1]["severity"], "minor");
    }

    #[test]
    fn test_special_characters_in_message() {
        let formatter = GitLabFormatter::new();
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
        let issues: Vec<Value> = serde_json::from_str(&output).unwrap();

        // JSON should properly handle special characters
        assert_eq!(
            issues[0]["description"],
            "Warning with \"quotes\" and 'apostrophes' and \n newline"
        );
    }

    #[test]
    fn test_special_characters_in_file_path() {
        let formatter = GitLabFormatter::new();
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
        let issues: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(issues[0]["location"]["path"], "path/with spaces/and-dashes.md");
        assert_eq!(issues[0]["fingerprint"], "path/with spaces/and-dashes.md-1-1-MD001");
    }

    #[test]
    fn test_json_pretty_formatting() {
        let formatter = GitLabFormatter::new();
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

        // Check that output is pretty-printed (contains newlines and indentation)
        assert!(output.contains('\n'));
        assert!(output.contains("  "));
    }
}
