//! Concise output formatter for easy parsing by editors

use crate::output::OutputFormatter;
use crate::rule::LintWarning;

/// Concise formatter: file:line:col: [RULE] message
pub struct ConciseFormatter;

impl Default for ConciseFormatter {
    fn default() -> Self {
        Self
    }
}

impl ConciseFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for ConciseFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        let mut output = String::new();

        for warning in warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");

            // Simple format without colors: file:line:col: [RULE] message
            let line = format!(
                "{}:{}:{}: [{}] {}",
                file_path, warning.line, warning.column, rule_name, warning.message
            );

            output.push_str(&line);
            output.push('\n');
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

    #[test]
    fn test_concise_formatter_default() {
        let _formatter = ConciseFormatter;
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_concise_formatter_new() {
        let _formatter = ConciseFormatter::new();
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_format_warnings_empty() {
        let formatter = ConciseFormatter::new();
        let warnings = vec![];
        let output = formatter.format_warnings(&warnings, "test.md");
        assert_eq!(output, "");
    }

    #[test]
    fn test_format_single_warning() {
        let formatter = ConciseFormatter::new();
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
        assert_eq!(
            output,
            "README.md:10:5: [MD001] Heading levels should only increment by one level at a time"
        );
    }

    #[test]
    fn test_format_warning_with_fix() {
        let formatter = ConciseFormatter::new();
        let warnings = vec![LintWarning {
            line: 15,
            column: 1,
            end_line: 15,
            end_column: 10,
            rule_name: Some("MD022"),
            message: "Headings should be surrounded by blank lines".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 100..110,
                replacement: "\n# Heading\n".to_string(),
            }),
        }];

        let output = formatter.format_warnings(&warnings, "doc.md");
        // Concise format doesn't show fix indicator
        assert_eq!(
            output,
            "doc.md:15:1: [MD022] Headings should be surrounded by blank lines"
        );
    }

    #[test]
    fn test_format_multiple_warnings() {
        let formatter = ConciseFormatter::new();
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
        let expected = "test.md:5:1: [MD001] First warning\ntest.md:10:3: [MD013] Second warning";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_format_warning_unknown_rule() {
        let formatter = ConciseFormatter::new();
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
        assert_eq!(output, "file.md:1:1: [unknown] Unknown rule warning");
    }

    #[test]
    fn test_edge_cases() {
        let formatter = ConciseFormatter::new();

        // Test large line/column numbers
        let warnings = vec![LintWarning {
            line: 99999,
            column: 12345,
            end_line: 100000,
            end_column: 12350,
            rule_name: Some("MD999"),
            message: "Edge case warning".to_string(),
            severity: Severity::Error,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "large.md");
        assert_eq!(output, "large.md:99999:12345: [MD999] Edge case warning");
    }

    #[test]
    fn test_special_characters_in_message() {
        let formatter = ConciseFormatter::new();
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
        assert_eq!(
            output,
            "test.md:1:1: [MD001] Warning with \"quotes\" and 'apostrophes' and \n newline"
        );
    }

    #[test]
    fn test_special_characters_in_file_path() {
        let formatter = ConciseFormatter::new();
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
        assert_eq!(output, "path/with spaces/and-dashes.md:1:1: [MD001] Test");
    }

    #[test]
    fn test_concise_format_consistency() {
        let formatter = ConciseFormatter::new();

        // Test that the format is consistent with the expected pattern
        let warnings = vec![
            LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("MD001"),
                message: "Test 1".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 2,
                column: 2,
                end_line: 2,
                end_column: 6,
                rule_name: Some("MD002"),
                message: "Test 2".to_string(),
                severity: Severity::Error,
                fix: Some(Fix {
                    range: 10..20,
                    replacement: "fix".to_string(),
                }),
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(lines.len(), 2);

        // Each line should follow the pattern: file:line:col: [RULE] message
        for line in lines {
            assert!(line.contains(":"));
            assert!(line.contains(" [MD"));
            assert!(line.contains("] "));
        }
    }

    #[test]
    fn test_severity_ignored() {
        let formatter = ConciseFormatter::new();

        // Test that severity doesn't affect output (concise format doesn't show severity)
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
        let lines: Vec<&str> = output.lines().collect();

        // Both should have same format regardless of severity
        assert!(lines[0].starts_with("test.md:1:1: [MD001]"));
        assert!(lines[1].starts_with("test.md:2:1: [MD002]"));
    }
}
