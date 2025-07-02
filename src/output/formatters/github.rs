//! GitHub Actions annotation format

use crate::output::OutputFormatter;
use crate::rule::LintWarning;

/// GitHub Actions formatter
/// Outputs in the format: ::error file=<file>,line=<line>,col=<col>,title=<rule>::<message>
pub struct GitHubFormatter;

impl Default for GitHubFormatter {
    fn default() -> Self {
        Self
    }
}

impl GitHubFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for GitHubFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        let mut output = String::new();

        for warning in warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");

            // GitHub Actions annotation format
            // Use "warning" level since these are linting warnings, not errors
            let line = format!(
                "::warning file={},line={},col={},title={}::{}",
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
    fn test_github_formatter_default() {
        let _formatter = GitHubFormatter;
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_github_formatter_new() {
        let _formatter = GitHubFormatter::new();
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_format_warnings_empty() {
        let formatter = GitHubFormatter::new();
        let warnings = vec![];
        let output = formatter.format_warnings(&warnings, "test.md");
        assert_eq!(output, "");
    }

    #[test]
    fn test_format_single_warning() {
        let formatter = GitHubFormatter::new();
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
            "::warning file=README.md,line=10,col=5,title=MD001::Heading levels should only increment by one level at a time"
        );
    }

    #[test]
    fn test_format_multiple_warnings() {
        let formatter = GitHubFormatter::new();
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
        let expected = "::warning file=test.md,line=5,col=1,title=MD001::First warning\n::warning file=test.md,line=10,col=3,title=MD013::Second warning";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_format_warning_with_fix() {
        let formatter = GitHubFormatter::new();
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
        // GitHub format doesn't show fix indicator
        assert_eq!(
            output,
            "::warning file=doc.md,line=15,col=1,title=MD022::Headings should be surrounded by blank lines"
        );
    }

    #[test]
    fn test_format_warning_unknown_rule() {
        let formatter = GitHubFormatter::new();
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
            "::warning file=file.md,line=1,col=1,title=unknown::Unknown rule warning"
        );
    }

    #[test]
    fn test_edge_cases() {
        let formatter = GitHubFormatter::new();

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
            "::warning file=large.md,line=99999,col=12345,title=MD999::Edge case warning"
        );
    }

    #[test]
    fn test_special_characters_in_message() {
        let formatter = GitHubFormatter::new();
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
        // Note: GitHub Actions should handle special characters in messages
        assert_eq!(
            output,
            "::warning file=test.md,line=1,col=1,title=MD001::Warning with \"quotes\" and 'apostrophes' and \n newline"
        );
    }

    #[test]
    fn test_special_characters_in_file_path() {
        let formatter = GitHubFormatter::new();
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
            "::warning file=path/with spaces/and-dashes.md,line=1,col=1,title=MD001::Test"
        );
    }

    #[test]
    fn test_github_format_structure() {
        let formatter = GitHubFormatter::new();
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

        // Verify GitHub Actions annotation structure
        assert!(output.starts_with("::warning "));
        assert!(output.contains("file=test.md"));
        assert!(output.contains("line=42"));
        assert!(output.contains("col=7"));
        assert!(output.contains("title=MD010"));
        assert!(output.ends_with("::Hard tabs"));
    }

    #[test]
    fn test_severity_always_warning() {
        let formatter = GitHubFormatter::new();

        // Test that all severities are output as "warning" in GitHub format
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

        // Both should use ::warning regardless of severity
        assert!(lines[0].starts_with("::warning "));
        assert!(lines[1].starts_with("::warning "));
    }

    #[test]
    fn test_commas_in_parameters() {
        let formatter = GitHubFormatter::new();

        // Test that commas in the title don't break the format
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD,001"), // Unlikely but test edge case
            message: "Test message, with comma".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "file,with,commas.md");
        // The format should still be parseable by GitHub Actions
        assert_eq!(
            output,
            "::warning file=file,with,commas.md,line=1,col=1,title=MD,001::Test message, with comma"
        );
    }
}
