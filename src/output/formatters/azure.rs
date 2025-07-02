//! Azure Pipeline logging format

use crate::output::OutputFormatter;
use crate::rule::LintWarning;

/// Azure Pipeline formatter
/// Outputs in the format: ##vso[task.logissue type=warning;sourcepath=<file>;linenumber=<line>;columnnumber=<col>;code=<rule>]<message>
pub struct AzureFormatter;

impl Default for AzureFormatter {
    fn default() -> Self {
        Self
    }
}

impl AzureFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for AzureFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        let mut output = String::new();

        for warning in warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");

            // Azure Pipeline logging command format
            let line = format!(
                "##vso[task.logissue type=warning;sourcepath={};linenumber={};columnnumber={};code={}]{}",
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
    fn test_azure_formatter_default() {
        let _formatter = AzureFormatter;
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_azure_formatter_new() {
        let _formatter = AzureFormatter::new();
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_format_warnings_empty() {
        let formatter = AzureFormatter::new();
        let warnings = vec![];
        let output = formatter.format_warnings(&warnings, "test.md");
        assert_eq!(output, "");
    }

    #[test]
    fn test_format_single_warning() {
        let formatter = AzureFormatter::new();
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
            "##vso[task.logissue type=warning;sourcepath=README.md;linenumber=10;columnnumber=5;code=MD001]Heading levels should only increment by one level at a time"
        );
    }

    #[test]
    fn test_format_multiple_warnings() {
        let formatter = AzureFormatter::new();
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
        let expected = "##vso[task.logissue type=warning;sourcepath=test.md;linenumber=5;columnnumber=1;code=MD001]First warning\n##vso[task.logissue type=warning;sourcepath=test.md;linenumber=10;columnnumber=3;code=MD013]Second warning";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_format_warning_with_fix() {
        let formatter = AzureFormatter::new();
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
        // Azure format doesn't show fix indicator
        assert_eq!(
            output,
            "##vso[task.logissue type=warning;sourcepath=doc.md;linenumber=15;columnnumber=1;code=MD022]Headings should be surrounded by blank lines"
        );
    }

    #[test]
    fn test_format_warning_unknown_rule() {
        let formatter = AzureFormatter::new();
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
        assert_eq!(
            output,
            "##vso[task.logissue type=warning;sourcepath=file.md;linenumber=1;columnnumber=1;code=unknown]Unknown rule warning"
        );
    }

    #[test]
    fn test_edge_cases() {
        let formatter = AzureFormatter::new();

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
        assert_eq!(
            output,
            "##vso[task.logissue type=warning;sourcepath=large.md;linenumber=99999;columnnumber=12345;code=MD999]Edge case warning"
        );
    }

    #[test]
    fn test_special_characters_in_message() {
        let formatter = AzureFormatter::new();
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
        // Note: Azure DevOps should handle special characters in messages
        assert_eq!(
            output,
            "##vso[task.logissue type=warning;sourcepath=test.md;linenumber=1;columnnumber=1;code=MD001]Warning with \"quotes\" and 'apostrophes' and \n newline"
        );
    }

    #[test]
    fn test_special_characters_in_file_path() {
        let formatter = AzureFormatter::new();
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
        assert_eq!(
            output,
            "##vso[task.logissue type=warning;sourcepath=path/with spaces/and-dashes.md;linenumber=1;columnnumber=1;code=MD001]Test"
        );
    }

    #[test]
    fn test_azure_format_structure() {
        let formatter = AzureFormatter::new();
        let warnings = vec![LintWarning {
            line: 42,
            column: 7,
            end_line: 42,
            end_column: 10,
            rule_name: Some("MD010"),
            message: "Hard tabs".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "test.md");

        // Verify Azure DevOps logging command structure
        assert!(output.starts_with("##vso[task.logissue "));
        assert!(output.contains("type=warning"));
        assert!(output.contains("sourcepath=test.md"));
        assert!(output.contains("linenumber=42"));
        assert!(output.contains("columnnumber=7"));
        assert!(output.contains("code=MD010"));
        assert!(output.ends_with("]Hard tabs"));
    }

    #[test]
    fn test_severity_always_warning() {
        let formatter = AzureFormatter::new();

        // Test that all severities are output as "warning" in Azure format
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

        // Both should use type=warning regardless of severity
        assert!(lines[0].contains("type=warning"));
        assert!(lines[1].contains("type=warning"));
    }

    #[test]
    fn test_semicolons_in_parameters() {
        let formatter = AzureFormatter::new();

        // Test that semicolons in the code don't break the format
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD;001"), // Unlikely but test edge case
            message: "Test message; with semicolon".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "file;with;semicolons.md");
        // The format should still be parseable by Azure DevOps
        assert_eq!(
            output,
            "##vso[task.logissue type=warning;sourcepath=file;with;semicolons.md;linenumber=1;columnnumber=1;code=MD;001]Test message; with semicolon"
        );
    }

    #[test]
    fn test_brackets_in_message() {
        let formatter = AzureFormatter::new();

        // Test that brackets in the message don't break the format
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001"),
            message: "Message with [brackets] and ]unmatched".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "test.md");
        assert_eq!(
            output,
            "##vso[task.logissue type=warning;sourcepath=test.md;linenumber=1;columnnumber=1;code=MD001]Message with [brackets] and ]unmatched"
        );
    }
}
