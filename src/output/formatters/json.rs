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
                    "rule": warning.rule_name.as_deref().unwrap_or("unknown"),
                    "message": warning.message,
                    "severity": warning.severity,
                    "fixable": warning.fix.is_some(),
                    "fix": warning.fix.as_ref().map(fix_to_json),
                })
            })
            .collect();

        serde_json::to_string_pretty(&json_warnings).unwrap_or_default()
    }
}

fn fix_to_json(fix: &crate::rule::Fix) -> serde_json::Value {
    let mut obj = json!({
        "range": {
            "start": fix.range.start,
            "end": fix.range.end,
        },
        "replacement": fix.replacement,
    });
    if !fix.additional_edits.is_empty() {
        obj["additional_edits"] = serde_json::Value::Array(fix.additional_edits.iter().map(fix_to_json).collect());
    }
    obj
}

/// Format all warnings from multiple files as a single JSON array.
///
/// In fix mode, only remaining (unfixed) warnings are passed in,
/// matching ESLint/Ruff convention of reporting only what's left.
pub fn format_all_warnings_as_json(all_warnings: &[(String, Vec<LintWarning>)]) -> String {
    let mut json_warnings = Vec::new();

    for (file_path, warnings) in all_warnings {
        for warning in warnings {
            json_warnings.push(json!({
                "file": file_path,
                "line": warning.line,
                "column": warning.column,
                "rule": warning.rule_name.as_deref().unwrap_or("unknown"),
                "message": warning.message,
                "severity": warning.severity,
                "fixable": warning.fix.is_some(),
                "fix": warning.fix.as_ref().map(fix_to_json),
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
            rule_name: Some("MD001".to_string()),
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
            rule_name: Some("MD001".to_string()),
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
            rule_name: Some("MD022".to_string()),
            message: "Headings should be surrounded by blank lines".to_string(),
            severity: Severity::Error,
            fix: Some(Fix::new(100..110, "\n# Heading\n".to_string())),
        }];

        let output = formatter.format_warnings(&warnings, "doc.md");
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["file"], "doc.md");
        assert_eq!(parsed[0]["line"], 15);
        assert_eq!(parsed[0]["column"], 1);
        assert_eq!(parsed[0]["rule"], "MD022");
        assert_eq!(parsed[0]["message"], "Headings should be surrounded by blank lines");
        assert_eq!(parsed[0]["severity"], "error");
        assert_eq!(parsed[0]["fixable"], true);
        assert!(!parsed[0]["fix"].is_null());
        assert_eq!(parsed[0]["fix"]["range"]["start"], 100);
        assert_eq!(parsed[0]["fix"]["range"]["end"], 110);
        assert_eq!(parsed[0]["fix"]["replacement"], "\n# Heading\n");
    }

    #[test]
    fn test_format_warning_with_additional_edits() {
        // Models MD054 ref-emit: a fix with one additional_edit. The JSON
        // emitter must surface the secondary edit so external consumers
        // (CI tooling, editors that drive rumdl over JSON, etc.) can apply
        // the full atomic fix rather than only the primary range.
        let formatter = JsonFormatter::new();
        let warnings = vec![LintWarning {
            line: 1,
            column: 5,
            end_line: 1,
            end_column: 32,
            rule_name: Some("MD054".to_string()),
            message: "Inconsistent link style".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix::with_additional_edits(
                4..31,
                "[docs]".to_string(),
                vec![Fix::new(45..45, "\n[docs]: https://example.com\n".to_string())],
            )),
        }];

        let output = formatter.format_warnings(&warnings, "doc.md");
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed[0]["fixable"], true);
        assert_eq!(parsed[0]["fix"]["range"]["start"], 4);
        assert_eq!(parsed[0]["fix"]["range"]["end"], 31);
        assert_eq!(parsed[0]["fix"]["replacement"], "[docs]");

        let extras = parsed[0]["fix"]["additional_edits"]
            .as_array()
            .expect("additional_edits should serialize as an array when non-empty");
        assert_eq!(extras.len(), 1);
        assert_eq!(extras[0]["range"]["start"], 45);
        assert_eq!(extras[0]["range"]["end"], 45);
        assert_eq!(extras[0]["replacement"], "\n[docs]: https://example.com\n");
    }

    #[test]
    fn test_format_warning_omits_empty_additional_edits() {
        // For the common single-edit case, additional_edits must NOT appear in
        // the JSON output (skip_serializing_if = "Vec::is_empty"). Verifying
        // this protects external consumers from churn on every warning.
        let formatter = JsonFormatter::new();
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD009".to_string()),
            message: "Trailing whitespace".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix::new(0..2, " ".to_string())),
        }];

        let output = formatter.format_warnings(&warnings, "doc.md");
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();

        let fix = &parsed[0]["fix"];
        assert!(
            fix.get("additional_edits").is_none(),
            "additional_edits must be omitted when empty, got: {fix}"
        );
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
                fix: Some(Fix::new(50..60, "fixed".to_string())),
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
        let all_warnings: Vec<(String, Vec<LintWarning>)> = vec![];
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
            rule_name: Some("MD001".to_string()),
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
                rule_name: Some("MD001".to_string()),
                message: "Warning 1".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 5,
                column: 1,
                end_line: 5,
                end_column: 10,
                rule_name: Some("MD002".to_string()),
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
            rule_name: Some("MD003".to_string()),
            message: "Warning 3".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix::new(100..120, "fixed".to_string())),
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
            rule_name: Some("MD001".to_string()),
            message: "Test with \"quotes\" and special chars".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "test.md");

        // Verify it's valid JSON
        let result: Result<Vec<Value>, _> = serde_json::from_str(&output);
        assert!(result.is_ok());

        // Verify pretty printing works
        assert!(output.contains('\n'));
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
            rule_name: Some("MD999".to_string()),
            message: "Edge case with\nnewlines\tand tabs".to_string(),
            severity: Severity::Error,
            fix: Some(Fix::new(999999..1000000, "Multi\nline\nreplacement".to_string())),
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

    #[test]
    fn test_severity_levels_in_json() {
        let formatter = JsonFormatter::new();
        let warnings = vec![
            LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("MD001".to_string()),
                message: "Error severity".to_string(),
                severity: Severity::Error,
                fix: None,
            },
            LintWarning {
                line: 2,
                column: 1,
                end_line: 2,
                end_column: 5,
                rule_name: Some("MD002".to_string()),
                message: "Warning severity".to_string(),
                severity: Severity::Warning,
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
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0]["severity"], "error");
        assert_eq!(parsed[1]["severity"], "warning");
        assert_eq!(parsed[2]["severity"], "info");
    }
}
