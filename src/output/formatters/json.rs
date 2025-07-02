//! JSON output formatter

use crate::output::OutputFormatter;
use crate::rule::LintWarning;
use serde_json::{Value, json};

/// JSON formatter for machine-readable output
#[derive(Default)]
pub struct JsonFormatter {
    collect_all: bool,
}

impl JsonFormatter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a formatter that collects all warnings into a single JSON array
    pub fn new_collecting() -> Self {
        Self { collect_all: true }
    }
}

impl OutputFormatter for JsonFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        if self.collect_all {
            // For batch collection mode, just return empty string
            // The actual JSON will be built elsewhere with all files
            return String::new();
        }

        let json_warnings: Vec<Value> = warnings
            .iter()
            .map(|warning| {
                json!({
                    "file": file_path,
                    "line": warning.line,
                    "column": warning.column,
                    "rule": warning.rule_name.unwrap_or("unknown"),
                    "message": warning.message,
                    "severity": "warning",
                    "fixable": warning.fix.is_some(),
                    "fix": warning.fix.as_ref().map(|f| {
                        json!({
                            "range": {
                                "start": f.range.start,
                                "end": f.range.end
                            },
                            "replacement": f.replacement
                        })
                    })
                })
            })
            .collect();

        serde_json::to_string_pretty(&json_warnings).unwrap_or_default()
    }
}

/// Helper to format all warnings from multiple files as a single JSON document
pub fn format_all_warnings_as_json(all_warnings: &[(String, Vec<LintWarning>)]) -> String {
    let mut json_warnings = Vec::new();

    for (file_path, warnings) in all_warnings {
        for warning in warnings {
            json_warnings.push(json!({
                "file": file_path,
                "line": warning.line,
                "column": warning.column,
                "rule": warning.rule_name.unwrap_or("unknown"),
                "message": warning.message,
                "severity": "warning",
                "fixable": warning.fix.is_some(),
                "fix": warning.fix.as_ref().map(|f| {
                    json!({
                        "range": {
                            "start": f.range.start,
                            "end": f.range.end
                        },
                        "replacement": f.replacement
                    })
                })
            }));
        }
    }

    serde_json::to_string_pretty(&json_warnings).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{Fix, Severity};

    #[test]
    fn test_json_formatter_default() {
        let formatter = JsonFormatter::default();
        assert!(!formatter.collect_all);
    }

    #[test]
    fn test_json_formatter_new() {
        let formatter = JsonFormatter::new();
        assert!(!formatter.collect_all);
    }

    #[test]
    fn test_json_formatter_new_collecting() {
        let formatter = JsonFormatter::new_collecting();
        assert!(formatter.collect_all);
    }

    #[test]
    fn test_format_warnings_empty() {
        let formatter = JsonFormatter::new();
        let warnings = vec![];
        let output = formatter.format_warnings(&warnings, "test.md");
        assert_eq!(output, "[]");
    }

    #[test]
    fn test_format_warnings_collecting_mode() {
        let formatter = JsonFormatter::new_collecting();
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001"),
            message: "Test warning".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        // In collecting mode, it returns empty string
        let output = formatter.format_warnings(&warnings, "test.md");
        assert_eq!(output, "");
    }

    #[test]
    fn test_format_single_warning() {
        let formatter = JsonFormatter::new();
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
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["file"], "README.md");
        assert_eq!(parsed[0]["line"], 10);
        assert_eq!(parsed[0]["column"], 5);
        assert_eq!(parsed[0]["rule"], "MD001");
        assert_eq!(
            parsed[0]["message"],
            "Heading levels should only increment by one level at a time"
        );
        assert_eq!(parsed[0]["severity"], "warning");
        assert_eq!(parsed[0]["fixable"], false);
        assert!(parsed[0]["fix"].is_null());
    }

    #[test]
    fn test_format_warning_with_fix() {
        let formatter = JsonFormatter::new();
        let warnings = vec![LintWarning {
            line: 15,
            column: 1,
            end_line: 15,
            end_column: 10,
            rule_name: Some("MD022"),
            message: "Headings should be surrounded by blank lines".to_string(),
            severity: Severity::Error,
            fix: Some(Fix {
                range: 100..110,
                replacement: "\n# Heading\n".to_string(),
            }),
        }];

        let output = formatter.format_warnings(&warnings, "doc.md");
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["file"], "doc.md");
        assert_eq!(parsed[0]["line"], 15);
        assert_eq!(parsed[0]["column"], 1);
        assert_eq!(parsed[0]["rule"], "MD022");
        assert_eq!(parsed[0]["message"], "Headings should be surrounded by blank lines");
        assert_eq!(parsed[0]["severity"], "warning"); // Always "warning" in current impl
        assert_eq!(parsed[0]["fixable"], true);
        assert!(!parsed[0]["fix"].is_null());
        assert_eq!(parsed[0]["fix"]["range"]["start"], 100);
        assert_eq!(parsed[0]["fix"]["range"]["end"], 110);
        assert_eq!(parsed[0]["fix"]["replacement"], "\n# Heading\n");
    }

    #[test]
    fn test_format_multiple_warnings() {
        let formatter = JsonFormatter::new();
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
                fix: Some(Fix {
                    range: 50..60,
                    replacement: "fixed".to_string(),
                }),
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["rule"], "MD001");
        assert_eq!(parsed[0]["message"], "First warning");
        assert_eq!(parsed[0]["fixable"], false);

        assert_eq!(parsed[1]["rule"], "MD013");
        assert_eq!(parsed[1]["message"], "Second warning");
        assert_eq!(parsed[1]["fixable"], true);
    }

    #[test]
    fn test_format_warning_unknown_rule() {
        let formatter = JsonFormatter::new();
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
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed[0]["rule"], "unknown");
    }

    #[test]
    fn test_format_all_warnings_as_json_empty() {
        let all_warnings = vec![];
        let output = format_all_warnings_as_json(&all_warnings);
        assert_eq!(output, "[]");
    }

    #[test]
    fn test_format_all_warnings_as_json_single_file() {
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001"),
            message: "Test warning".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let all_warnings = vec![("test.md".to_string(), warnings)];
        let output = format_all_warnings_as_json(&all_warnings);
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["file"], "test.md");
        assert_eq!(parsed[0]["rule"], "MD001");
    }

    #[test]
    fn test_format_all_warnings_as_json_multiple_files() {
        let warnings1 = vec![
            LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("MD001"),
                message: "Warning 1".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 5,
                column: 1,
                end_line: 5,
                end_column: 10,
                rule_name: Some("MD002"),
                message: "Warning 2".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
        ];

        let warnings2 = vec![LintWarning {
            line: 10,
            column: 1,
            end_line: 10,
            end_column: 20,
            rule_name: Some("MD003"),
            message: "Warning 3".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 100..120,
                replacement: "fixed".to_string(),
            }),
        }];

        let all_warnings = vec![("file1.md".to_string(), warnings1), ("file2.md".to_string(), warnings2)];

        let output = format_all_warnings_as_json(&all_warnings);
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0]["file"], "file1.md");
        assert_eq!(parsed[0]["rule"], "MD001");
        assert_eq!(parsed[1]["file"], "file1.md");
        assert_eq!(parsed[1]["rule"], "MD002");
        assert_eq!(parsed[2]["file"], "file2.md");
        assert_eq!(parsed[2]["rule"], "MD003");
        assert_eq!(parsed[2]["fixable"], true);
    }

    #[test]
    fn test_json_output_is_valid() {
        let formatter = JsonFormatter::new();
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001"),
            message: "Test with \"quotes\" and special chars".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "test.md");

        // Verify it's valid JSON
        let result: Result<Vec<Value>, _> = serde_json::from_str(&output);
        assert!(result.is_ok());

        // Verify pretty printing works
        assert!(output.contains("\n"));
        assert!(output.contains("  "));
    }

    #[test]
    fn test_edge_cases() {
        let formatter = JsonFormatter::new();

        // Test with large values
        let warnings = vec![LintWarning {
            line: 99999,
            column: 12345,
            end_line: 100000,
            end_column: 12350,
            rule_name: Some("MD999"),
            message: "Edge case with\nnewlines\tand tabs".to_string(),
            severity: Severity::Error,
            fix: Some(Fix {
                range: 999999..1000000,
                replacement: "Multi\nline\nreplacement".to_string(),
            }),
        }];

        let output = formatter.format_warnings(&warnings, "large.md");
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed[0]["line"], 99999);
        assert_eq!(parsed[0]["column"], 12345);
        assert_eq!(parsed[0]["fix"]["range"]["start"], 999999);
        assert_eq!(parsed[0]["fix"]["range"]["end"], 1000000);
        assert!(parsed[0]["message"].as_str().unwrap().contains("newlines\tand tabs"));
        assert!(
            parsed[0]["fix"]["replacement"]
                .as_str()
                .unwrap()
                .contains("Multi\nline\nreplacement")
        );
    }
}
