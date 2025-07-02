//! Grouped output formatter that groups violations by file

use crate::output::OutputFormatter;
use crate::rule::LintWarning;
use std::collections::HashMap;

/// Grouped formatter: groups violations by file
pub struct GroupedFormatter;

impl Default for GroupedFormatter {
    fn default() -> Self {
        Self
    }
}

impl GroupedFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for GroupedFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        if warnings.is_empty() {
            return String::new();
        }

        let mut output = String::new();

        // Group warnings by their rule name
        let mut grouped: HashMap<&str, Vec<&LintWarning>> = HashMap::new();
        for warning in warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");
            grouped.entry(rule_name).or_default().push(warning);
        }

        // Output file header
        output.push_str(&format!("{file_path}:\n"));

        // Sort rules for consistent output
        let mut rules: Vec<_> = grouped.keys().collect();
        rules.sort();

        for rule_name in rules {
            let rule_warnings = &grouped[rule_name];
            output.push_str(&format!("  {rule_name}:\n"));

            for warning in rule_warnings {
                output.push_str(&format!("    {}:{} {}", warning.line, warning.column, warning.message));
                if warning.fix.is_some() {
                    output.push_str(" (fixable)");
                }
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

    #[test]
    fn test_grouped_formatter_default() {
        let _formatter = GroupedFormatter;
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_grouped_formatter_new() {
        let _formatter = GroupedFormatter::new();
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_format_warnings_empty() {
        let formatter = GroupedFormatter::new();
        let warnings = vec![];
        let output = formatter.format_warnings(&warnings, "test.md");
        assert_eq!(output, "");
    }

    #[test]
    fn test_format_single_warning() {
        let formatter = GroupedFormatter::new();
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
        let expected = "README.md:\n  MD001:\n    10:5 Heading levels should only increment by one level at a time";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_format_single_warning_with_fix() {
        let formatter = GroupedFormatter::new();
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
        let expected =
            "README.md:\n  MD001:\n    10:5 Heading levels should only increment by one level at a time (fixable)";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_format_multiple_warnings_same_rule() {
        let formatter = GroupedFormatter::new();
        let warnings = vec![
            LintWarning {
                line: 5,
                column: 1,
                end_line: 5,
                end_column: 10,
                rule_name: Some("MD001"),
                message: "First violation".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 10,
                column: 3,
                end_line: 10,
                end_column: 20,
                rule_name: Some("MD001"),
                message: "Second violation".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let expected = "test.md:\n  MD001:\n    5:1 First violation\n    10:3 Second violation";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_format_multiple_warnings_different_rules() {
        let formatter = GroupedFormatter::new();
        let warnings = vec![
            LintWarning {
                line: 5,
                column: 1,
                end_line: 5,
                end_column: 10,
                rule_name: Some("MD001"),
                message: "Heading increment".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 10,
                column: 3,
                end_line: 10,
                end_column: 20,
                rule_name: Some("MD013"),
                message: "Line too long".to_string(),
                severity: Severity::Error,
                fix: Some(Fix {
                    range: 50..60,
                    replacement: "fixed".to_string(),
                }),
            },
            LintWarning {
                line: 15,
                column: 1,
                end_line: 15,
                end_column: 5,
                rule_name: Some("MD001"),
                message: "Another heading issue".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let expected = "test.md:\n  MD001:\n    5:1 Heading increment\n    15:1 Another heading issue\n  MD013:\n    10:3 Line too long (fixable)";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_format_warning_unknown_rule() {
        let formatter = GroupedFormatter::new();
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
        let expected = "file.md:\n  unknown:\n    1:1 Unknown rule warning";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_rule_sorting() {
        let formatter = GroupedFormatter::new();
        let warnings = vec![
            LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("MD010"),
                message: "Hard tabs".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 2,
                column: 1,
                end_line: 2,
                end_column: 5,
                rule_name: Some("MD001"),
                message: "Heading".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 3,
                column: 1,
                end_line: 3,
                end_column: 5,
                rule_name: Some("MD005"),
                message: "List indent".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let lines: Vec<&str> = output.lines().collect();

        // Verify rules are sorted alphabetically
        assert_eq!(lines[1], "  MD001:");
        assert_eq!(lines[3], "  MD005:");
        assert_eq!(lines[5], "  MD010:");
    }

    #[test]
    fn test_edge_cases() {
        let formatter = GroupedFormatter::new();

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
        let expected = "large.md:\n  MD999:\n    99999:12345 Edge case warning";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_special_characters_in_message() {
        let formatter = GroupedFormatter::new();
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
        let expected = "test.md:\n  MD001:\n    1:1 Warning with \"quotes\" and 'apostrophes' and \n newline";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_special_characters_in_file_path() {
        let formatter = GroupedFormatter::new();
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
        let expected = "path/with spaces/and-dashes.md:\n  MD001:\n    1:1 Test";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_mixed_fixable_unfixable() {
        let formatter = GroupedFormatter::new();
        let warnings = vec![
            LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("MD001"),
                message: "Not fixable".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
            LintWarning {
                line: 2,
                column: 1,
                end_line: 2,
                end_column: 5,
                rule_name: Some("MD001"),
                message: "Fixable".to_string(),
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: 10..20,
                    replacement: "fix".to_string(),
                }),
            },
            LintWarning {
                line: 3,
                column: 1,
                end_line: 3,
                end_column: 5,
                rule_name: Some("MD001"),
                message: "Also not fixable".to_string(),
                severity: Severity::Warning,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let expected = "test.md:\n  MD001:\n    1:1 Not fixable\n    2:1 Fixable (fixable)\n    3:1 Also not fixable";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_severity_not_shown() {
        let formatter = GroupedFormatter::new();

        // Test that severity doesn't affect output
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
                rule_name: Some("MD001"),
                message: "Error severity".to_string(),
                severity: Severity::Error,
                fix: None,
            },
        ];

        let output = formatter.format_warnings(&warnings, "test.md");
        let expected = "test.md:\n  MD001:\n    1:1 Warning severity\n    2:1 Error severity";
        assert_eq!(output, expected);
    }
}
