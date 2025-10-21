//! Pylint-compatible output formatter

use crate::output::OutputFormatter;
use crate::rule::LintWarning;

/// Pylint-compatible formatter: file:line:column: CODE message
pub struct PylintFormatter;

impl Default for PylintFormatter {
    fn default() -> Self {
        Self
    }
}

impl PylintFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for PylintFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        let mut output = String::new();

        for warning in warnings {
            let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");

            // Convert MD prefix to CMD for pylint convention
            // Pylint uses C for Convention, so CMD = Convention + MD rule
            let pylint_code = if let Some(stripped) = rule_name.strip_prefix("MD") {
                format!("CMD{stripped}")
            } else {
                format!("C{rule_name}")
            };

            // Pylint format: file:line:column: [C0000] message
            let line = format!(
                "{}:{}:{}: [{}] {}",
                file_path, warning.line, warning.column, pylint_code, warning.message
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
    fn test_pylint_formatter_default() {
        let _formatter = PylintFormatter;
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_pylint_formatter_new() {
        let _formatter = PylintFormatter::new();
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_format_warnings_empty() {
        let formatter = PylintFormatter::new();
        let warnings = vec![];
        let output = formatter.format_warnings(&warnings, "test.md");
        assert_eq!(output, "");
    }

    #[test]
    fn test_format_single_warning() {
        let formatter = PylintFormatter::new();
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
        assert_eq!(
            output,
            "README.md:10:5: [CMD001] Heading levels should only increment by one level at a time"
        );
    }

    #[test]
    fn test_format_multiple_warnings() {
        let formatter = PylintFormatter::new();
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
        let expected = "test.md:5:1: [CMD001] First warning\ntest.md:10:3: [CMD013] Second warning";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_format_warning_with_fix() {
        let formatter = PylintFormatter::new();
        let warnings = vec![LintWarning {
            line: 15,
            column: 1,
            end_line: 15,
            end_column: 10,
            rule_name: Some("MD022".to_string()),
            message: "Headings should be surrounded by blank lines".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 100..110,
                replacement: "\n# Heading\n".to_string(),
            }),
        }];

        let output = formatter.format_warnings(&warnings, "doc.md");
        // Pylint format doesn't show fix indicator
        assert_eq!(
            output,
            "doc.md:15:1: [CMD022] Headings should be surrounded by blank lines"
        );
    }

    #[test]
    fn test_format_warning_unknown_rule() {
        let formatter = PylintFormatter::new();
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
        assert_eq!(output, "file.md:1:1: [Cunknown] Unknown rule warning");
    }

    #[test]
    fn test_format_warning_non_md_rule() {
        let formatter = PylintFormatter::new();
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("CUSTOM001".to_string()),
            message: "Custom rule warning".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "file.md");
        assert_eq!(output, "file.md:1:1: [CCUSTOM001] Custom rule warning");
    }

    #[test]
    fn test_pylint_code_conversion() {
        let formatter = PylintFormatter::new();

        // Test various MD codes
        let test_cases = vec![("MD001", "CMD001"), ("MD010", "CMD010"), ("MD999", "CMD999")];

        for (md_code, expected_pylint) in test_cases {
            let warnings = vec![LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 1,
                rule_name: Some(md_code.to_string()),
                message: "Test".to_string(),
                severity: Severity::Warning,
                fix: None,
            }];

            let output = formatter.format_warnings(&warnings, "test.md");
            assert!(output.contains(&format!("[{expected_pylint}]")));
        }
    }

    #[test]
    fn test_edge_cases() {
        let formatter = PylintFormatter::new();

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
        assert_eq!(output, "large.md:99999:12345: [CMD999] Edge case warning");
    }

    #[test]
    fn test_special_characters_in_message() {
        let formatter = PylintFormatter::new();
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
        assert_eq!(
            output,
            "test.md:1:1: [CMD001] Warning with \"quotes\" and 'apostrophes' and \n newline"
        );
    }

    #[test]
    fn test_special_characters_in_file_path() {
        let formatter = PylintFormatter::new();
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
        assert_eq!(output, "path/with spaces/and-dashes.md:1:1: [CMD001] Test");
    }

    #[test]
    fn test_severity_ignored() {
        let formatter = PylintFormatter::new();

        // Test that severity doesn't affect output (pylint format doesn't show severity)
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
        let lines: Vec<&str> = output.lines().collect();

        // Both should have same format regardless of severity
        assert!(lines[0].starts_with("test.md:1:1: [CMD001]"));
        assert!(lines[1].starts_with("test.md:2:1: [CMD002]"));
    }
}
