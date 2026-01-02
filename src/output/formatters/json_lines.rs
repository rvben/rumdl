//! JSON Lines output formatter (one JSON object per line)

use crate::output::OutputFormatter;
use crate::rule::LintWarning;
use serde_json::json;

/// JSON Lines formatter - one JSON object per line
pub struct JsonLinesFormatter;

impl Default for JsonLinesFormatter {
    fn default() -> Self {
        Self
    }
}

impl JsonLinesFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for JsonLinesFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        let mut output = String::new();

        for warning in warnings {
            let json_obj = json!({
                "file": file_path,
                "line": warning.line,
                "column": warning.column,
                "rule": warning.rule_name.as_deref().unwrap_or("unknown"),
                "message": warning.message,
                "severity": warning.severity,
                "fixable": warning.fix.is_some()
            });

            // Compact JSON representation on a single line
            if let Ok(json_str) = serde_json::to_string(&json_obj) {
                output.push_str(&json_str);
                output.push('\n');
            }
        }

        // Remove trailing newline
        if output.ends_with('\n') {
            output.pop();
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{Fix, Severity};
    use serde_json::Value;

    #[test]
    fn test_json_lines_formatter_default() {
        let _formatter = JsonLinesFormatter;
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_json_lines_formatter_new() {
        let _formatter = JsonLinesFormatter::new();
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_format_warnings_empty() {
        let formatter = JsonLinesFormatter::new();
        let warnings = vec![];
        let output = formatter.format_warnings(&warnings, "test.md");
        assert_eq!(output, "");
    }

    #[test]
    fn test_format_single_warning() {
        let formatter = JsonLinesFormatter::new();
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

        // Parse the JSON to verify structure
        let json: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(json["file"], "README.md");
        assert_eq!(json["line"], 10);
        assert_eq!(json["column"], 5);
        assert_eq!(json["rule"], "MD001");
        assert_eq!(
            json["message"],
            "Heading levels should only increment by one level at a time"
        );
        assert_eq!(json["severity"], "warning");
        assert_eq!(json["fixable"], false);
    }

    #[test]
    fn test_format_single_warning_with_fix() {
        let formatter = JsonLinesFormatter::new();
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

        // Parse the JSON to verify structure
        let json: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(json["fixable"], true);
    }

    #[test]
    fn test_format_multiple_warnings() {
        let formatter = JsonLinesFormatter::new();
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
                fix: Some(Fix {
                    range: 50..60,
                    replacement: "fixed".to_string(),
                }),
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let lines: Vec<&str> = output.lines().collect();

        // Should have 2 lines of JSON
        assert_eq!(lines.len(), 2);

        // Parse each line as JSON
        let json1: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(json1["line"], 5);
        assert_eq!(json1["rule"], "MD001");
        assert_eq!(json1["fixable"], false);

        let json2: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(json2["line"], 10);
        assert_eq!(json2["rule"], "MD013");
        assert_eq!(json2["fixable"], true);
    }

    #[test]
    fn test_format_warning_unknown_rule() {
        let formatter = JsonLinesFormatter::new();
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
        let json: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(json["rule"], "unknown");
    }

    #[test]
    fn test_edge_cases() {
        let formatter = JsonLinesFormatter::new();

        // Test large line/column numbers
        let warnings = vec![LintWarning {
            line: 99999,
            column: 12345,
            end_line: 100000,
            end_column: 12350,
            rule_name: Some("MD999".to_string()),
            message: "Edge case warning".to_string(),
            severity: Severity::Error,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "large.md");
        let json: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(json["line"], 99999);
        assert_eq!(json["column"], 12345);
    }

    #[test]
    fn test_special_characters_in_message() {
        let formatter = JsonLinesFormatter::new();
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
        let json: Value = serde_json::from_str(&output).unwrap();

        // JSON should properly escape special characters
        assert_eq!(
            json["message"],
            "Warning with \"quotes\" and 'apostrophes' and \n newline"
        );
    }

    #[test]
    fn test_special_characters_in_file_path() {
        let formatter = JsonLinesFormatter::new();
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
        let json: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(json["file"], "path/with spaces/and-dashes.md");
    }

    #[test]
    fn test_json_lines_format() {
        let formatter = JsonLinesFormatter::new();
        let warnings = vec![
            LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("MD001".to_string()),
                message: "First".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 2,
                column: 1,
                end_line: 2,
                end_column: 5,
                rule_name: Some("MD002".to_string()),
                message: "Second".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 3,
                column: 1,
                end_line: 3,
                end_column: 5,
                rule_name: Some("MD003".to_string()),
                message: "Third".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");

        // Each line should be valid JSON
        for line in output.lines() {
            assert!(serde_json::from_str::<Value>(line).is_ok());
        }

        // Should have 3 lines
        assert_eq!(output.lines().count(), 3);
    }

    #[test]
    fn test_severity_levels() {
        let formatter = JsonLinesFormatter::new();

        // Test that all severity levels are correctly output
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
            LintWarning {
                line: 3,
                column: 1,
                end_line: 3,
                end_column: 5,
                rule_name: Some("MD003".to_string()),
                message: "Info severity".to_string(),
                severity: Severity::Info,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let lines: Vec<&str> = output.lines().collect();

        let json0: Value = serde_json::from_str(lines[0]).unwrap();
        let json1: Value = serde_json::from_str(lines[1]).unwrap();
        let json2: Value = serde_json::from_str(lines[2]).unwrap();

        assert_eq!(json0["severity"], "warning");
        assert_eq!(json1["severity"], "error");
        assert_eq!(json2["severity"], "info");
    }

    #[test]
    fn test_json_field_order() {
        let formatter = JsonLinesFormatter::new();
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

        // Verify that all expected fields are present
        let json: Value = serde_json::from_str(&output).unwrap();
        assert!(json.get("file").is_some());
        assert!(json.get("line").is_some());
        assert!(json.get("column").is_some());
        assert!(json.get("rule").is_some());
        assert!(json.get("message").is_some());
        assert!(json.get("severity").is_some());
        assert!(json.get("fixable").is_some());
    }

    #[test]
    fn test_unicode_in_json() {
        let formatter = JsonLinesFormatter::new();
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001".to_string()),
            message: "Unicode: 你好 émoji 🎉".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "测试.md");
        let json: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(json["message"], "Unicode: 你好 émoji 🎉");
        assert_eq!(json["file"], "测试.md");
    }
}
