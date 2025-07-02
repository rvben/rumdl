//! Default text output formatter with colors and context

use crate::output::OutputFormatter;
use crate::rule::LintWarning;
use colored::*;

/// Default human-readable formatter with colors
pub struct TextFormatter {
    use_colors: bool,
}

impl Default for TextFormatter {
    fn default() -> Self {
        Self { use_colors: true }
    }
}

impl TextFormatter {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TextFormatter {
    pub fn without_colors() -> Self {
        Self { use_colors: false }
    }
}

impl OutputFormatter for TextFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        let mut output = String::new();

        for warning in warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");

            // Add fix indicator if this warning has a fix
            let fix_indicator = if warning.fix.is_some() { " [*]" } else { "" };

            // Format: file:line:column: [rule] message [*]
            let line = format!(
                "{}:{}:{}: {} {}{}",
                if self.use_colors {
                    file_path.blue().underline().to_string()
                } else {
                    file_path.to_string()
                },
                if self.use_colors {
                    warning.line.to_string().cyan().to_string()
                } else {
                    warning.line.to_string()
                },
                if self.use_colors {
                    warning.column.to_string().cyan().to_string()
                } else {
                    warning.column.to_string()
                },
                if self.use_colors {
                    format!("[{rule_name:5}]").yellow().to_string()
                } else {
                    format!("[{rule_name:5}]")
                },
                warning.message,
                if self.use_colors {
                    fix_indicator.green().to_string()
                } else {
                    fix_indicator.to_string()
                }
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

    fn use_colors(&self) -> bool {
        self.use_colors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{Fix, Severity};

    #[test]
    fn test_text_formatter_default() {
        let formatter = TextFormatter::default();
        assert!(formatter.use_colors());
    }

    #[test]
    fn test_text_formatter_new() {
        let formatter = TextFormatter::new();
        assert!(formatter.use_colors());
    }

    #[test]
    fn test_text_formatter_without_colors() {
        let formatter = TextFormatter::without_colors();
        assert!(!formatter.use_colors());
    }

    #[test]
    fn test_format_warnings_empty() {
        let formatter = TextFormatter::without_colors();
        let warnings = vec![];
        let output = formatter.format_warnings(&warnings, "test.md");
        assert_eq!(output, "");
    }

    #[test]
    fn test_format_single_warning_no_colors() {
        let formatter = TextFormatter::without_colors();
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
    fn test_format_warning_with_fix_no_colors() {
        let formatter = TextFormatter::without_colors();
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
        assert_eq!(
            output,
            "doc.md:15:1: [MD022] Headings should be surrounded by blank lines [*]"
        );
    }

    #[test]
    fn test_format_multiple_warnings_no_colors() {
        let formatter = TextFormatter::without_colors();
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
        let expected = "test.md:5:1: [MD001] First warning\ntest.md:10:3: [MD013] Second warning [*]";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_format_warning_unknown_rule() {
        let formatter = TextFormatter::without_colors();
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
    fn test_format_warnings_with_colors() {
        // The colored crate might disable colors in test environments
        // So we'll test the structure rather than actual ANSI codes
        let formatter = TextFormatter::new(); // default has colors
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001"),
            message: "Test warning".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 0..5,
                replacement: "fixed".to_string(),
            }),
        }];

        let output = formatter.format_warnings(&warnings, "test.md");

        // Verify the formatter is set to use colors
        assert!(formatter.use_colors());

        // Check that core content is present (colors might be disabled in tests)
        assert!(output.contains("test.md")); // file path is still there
        assert!(output.contains("MD001")); // rule name is still there
        assert!(output.contains("Test warning")); // message is still there
        assert!(output.contains("[*]")); // fix indicator is still there

        // Note: We don't check the exact format with colors because ANSI codes
        // make exact string matching unreliable. The individual component checks above
        // are sufficient to verify the output is correct.
    }

    #[test]
    fn test_rule_name_padding() {
        let formatter = TextFormatter::without_colors();

        // Test short rule name gets padded
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD1"),
            message: "Test".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "test.md");
        assert!(output.contains("[MD1  ]")); // Should be padded to 5 chars
    }

    #[test]
    fn test_edge_cases() {
        let formatter = TextFormatter::without_colors();

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
        let formatter = TextFormatter::without_colors();
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
        assert!(output.contains("Warning with \"quotes\" and 'apostrophes' and \n newline"));
    }

    #[test]
    fn test_special_characters_in_file_path() {
        let formatter = TextFormatter::without_colors();
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
        assert!(output.starts_with("path/with spaces/and-dashes.md:1:1:"));
    }

    #[test]
    fn test_use_colors_trait_method() {
        let formatter_with_colors = TextFormatter::new();
        assert!(formatter_with_colors.use_colors());

        let formatter_without_colors = TextFormatter::without_colors();
        assert!(!formatter_without_colors.use_colors());
    }
}
