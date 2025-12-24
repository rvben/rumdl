//! GitHub Actions annotation format
//!
//! Outputs annotations in GitHub Actions workflow command format for PR annotations.
//! See: <https://docs.github.com/en/actions/reference/workflow-commands-for-github-actions>

use crate::output::OutputFormatter;
use crate::rule::{LintWarning, Severity};

/// GitHub Actions formatter
/// Outputs in the format: `::<level> file=<file>,line=<line>,col=<col>,endLine=<endLine>,endColumn=<endCol>,title=<rule>::<message>`
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

    /// Escape special characters according to GitHub Actions specification
    /// Percent-encodes: %, \r, \n, :, ,
    /// Used for property values (file, title, etc.)
    fn escape_property(value: &str) -> String {
        value
            .replace('%', "%25")
            .replace('\r', "%0D")
            .replace('\n', "%0A")
            .replace(':', "%3A")
            .replace(',', "%2C")
    }

    /// Escape special characters in the message part
    /// Percent-encodes: %, \r, \n
    fn escape_message(value: &str) -> String {
        value.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A")
    }
}

impl OutputFormatter for GitHubFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        let mut output = String::new();

        for warning in warnings {
            let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");

            // Map severity to GitHub Actions annotation level
            let level = match warning.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Info => "notice",
            };

            // Escape special characters in all properties
            let escaped_file = Self::escape_property(file_path);
            let escaped_rule = Self::escape_property(rule_name);
            let escaped_message = Self::escape_message(&warning.message);

            // GitHub Actions annotation format with optional end position
            let line = if warning.end_line != warning.line || warning.end_column != warning.column {
                // Include end position if different from start
                format!(
                    "::{} file={},line={},col={},endLine={},endColumn={},title={}::{}",
                    level,
                    escaped_file,
                    warning.line,
                    warning.column,
                    warning.end_line,
                    warning.end_column,
                    escaped_rule,
                    escaped_message
                )
            } else {
                // Omit end position if same as start
                format!(
                    "::{} file={},line={},col={},title={}::{}",
                    level, escaped_file, warning.line, warning.column, escaped_rule, escaped_message
                )
            };

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
            rule_name: Some("MD001".to_string()),
            message: "Heading levels should only increment by one level at a time".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "README.md");
        assert_eq!(
            output,
            "::warning file=README.md,line=10,col=5,endLine=10,endColumn=15,title=MD001::Heading levels should only increment by one level at a time"
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
        let expected = "::warning file=test.md,line=5,col=1,endLine=5,endColumn=10,title=MD001::First warning\n::error file=test.md,line=10,col=3,endLine=10,endColumn=20,title=MD013::Second warning";
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
            rule_name: Some("MD022".to_string()),
            message: "Headings should be surrounded by blank lines".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 100..110,
                replacement: "\n# Heading\n".to_string(),
            }),
        }];

        let output = formatter.format_warnings(&warnings, "doc.md");
        // GitHub format doesn't show fix indicator but includes end position
        assert_eq!(
            output,
            "::warning file=doc.md,line=15,col=1,endLine=15,endColumn=10,title=MD022::Headings should be surrounded by blank lines"
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
            "::warning file=file.md,line=1,col=1,endLine=1,endColumn=5,title=unknown::Unknown rule warning"
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
            rule_name: Some("MD999".to_string()),
            message: "Edge case warning".to_string(),
            severity: Severity::Error,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "large.md");
        assert_eq!(
            output,
            "::error file=large.md,line=99999,col=12345,endLine=100000,endColumn=12350,title=MD999::Edge case warning"
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
            rule_name: Some("MD001".to_string()),
            message: "Warning with \"quotes\" and 'apostrophes' and \n newline".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "test.md");
        // Newline should be escaped as %0A
        assert_eq!(
            output,
            "::warning file=test.md,line=1,col=1,endLine=1,endColumn=5,title=MD001::Warning with \"quotes\" and 'apostrophes' and %0A newline"
        );
    }

    #[test]
    fn test_percent_encoding() {
        let formatter = GitHubFormatter::new();
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 1,
            rule_name: Some("MD001".to_string()),
            message: "100% complete\r\nNew line".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "test%.md");
        // %, \r, and \n should be encoded
        assert_eq!(
            output,
            "::warning file=test%25.md,line=1,col=1,title=MD001::100%25 complete%0D%0ANew line"
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
            rule_name: Some("MD001".to_string()),
            message: "Test".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "path/with spaces/and-dashes.md");
        assert_eq!(
            output,
            "::warning file=path/with spaces/and-dashes.md,line=1,col=1,endLine=1,endColumn=5,title=MD001::Test"
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
            rule_name: Some("MD010".to_string()),
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
        assert!(output.contains("endLine=42"));
        assert!(output.contains("endColumn=10"));
        assert!(output.contains("title=MD010"));
        assert!(output.ends_with("::Hard tabs"));
    }

    #[test]
    fn test_severity_mapping() {
        let formatter = GitHubFormatter::new();

        // Test that severities are properly mapped
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

        // Warning should use ::warning, Error should use ::error
        assert!(lines[0].starts_with("::warning "));
        assert!(lines[1].starts_with("::error "));
    }

    #[test]
    fn test_commas_in_parameters() {
        let formatter = GitHubFormatter::new();

        // Test that commas in the title and file path are properly escaped
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD,001".to_string()), // Unlikely but test edge case
            message: "Test message, with comma".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "file,with,commas.md");
        // Commas in properties should be escaped as %2C
        assert_eq!(
            output,
            "::warning file=file%2Cwith%2Ccommas.md,line=1,col=1,endLine=1,endColumn=5,title=MD%2C001::Test message, with comma"
        );
    }

    #[test]
    fn test_colons_in_parameters() {
        let formatter = GitHubFormatter::new();

        // Test that colons in file path and rule name are properly escaped
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD:001".to_string()), // Unlikely but test edge case
            message: "Test message: with colon".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "file:with:colons.md");
        // Colons in properties should be escaped as %3A, but not in message
        assert_eq!(
            output,
            "::warning file=file%3Awith%3Acolons.md,line=1,col=1,endLine=1,endColumn=5,title=MD%3A001::Test message: with colon"
        );
    }

    #[test]
    fn test_same_start_end_position() {
        let formatter = GitHubFormatter::new();

        // When start and end are the same, end position should be omitted
        let warnings = vec![LintWarning {
            line: 5,
            column: 10,
            end_line: 5,
            end_column: 10,
            rule_name: Some("MD001".to_string()),
            message: "Single position warning".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "test.md");
        // Should not include endLine and endColumn when they're the same as line and column
        assert_eq!(
            output,
            "::warning file=test.md,line=5,col=10,title=MD001::Single position warning"
        );
    }

    #[test]
    fn test_error_severity() {
        let formatter = GitHubFormatter::new();

        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001".to_string()),
            message: "Error level issue".to_string(),
            severity: Severity::Error,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "test.md");
        assert_eq!(
            output,
            "::error file=test.md,line=1,col=1,endLine=1,endColumn=5,title=MD001::Error level issue"
        );
    }
}
