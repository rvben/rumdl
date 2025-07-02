//! JUnit XML output format

use crate::output::OutputFormatter;
use crate::rule::LintWarning;

/// JUnit XML formatter for CI systems
pub struct JunitFormatter;

impl Default for JunitFormatter {
    fn default() -> Self {
        Self
    }
}

impl JunitFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for JunitFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        // Format warnings for a single file as a minimal JUnit XML document
        let mut xml = String::new();
        xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        xml.push('\n');

        let escaped_file = xml_escape(file_path);

        xml.push_str(&format!(
            r#"<testsuites name="rumdl" tests="1" failures="{}" errors="0" time="0.000">"#,
            warnings.len()
        ));
        xml.push('\n');

        xml.push_str(&format!(
            r#"  <testsuite name="{}" tests="1" failures="{}" errors="0" time="0.000">"#,
            escaped_file,
            warnings.len()
        ));
        xml.push('\n');

        xml.push_str(&format!(
            r#"    <testcase name="Lint {escaped_file}" classname="rumdl" time="0.000">"#
        ));
        xml.push('\n');

        // Add failures for each warning
        for warning in warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");
            let message = xml_escape(&warning.message);

            xml.push_str(&format!(
                r#"      <failure type="{}" message="{}">{} at line {}, column {}</failure>"#,
                rule_name, message, message, warning.line, warning.column
            ));
            xml.push('\n');
        }

        xml.push_str("    </testcase>\n");
        xml.push_str("  </testsuite>\n");
        xml.push_str("</testsuites>\n");

        xml
    }
}

/// Format all warnings as JUnit XML report
pub fn format_junit_report(all_warnings: &[(String, Vec<LintWarning>)], duration_ms: u64) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');

    // Count total issues
    let total_issues: usize = all_warnings.iter().map(|(_, w)| w.len()).sum();
    let files_with_issues = all_warnings.len();

    // Convert duration to seconds
    let duration_secs = duration_ms as f64 / 1000.0;

    xml.push_str(&format!(
        r#"<testsuites name="rumdl" tests="{files_with_issues}" failures="{total_issues}" errors="0" time="{duration_secs:.3}">"#
    ));
    xml.push('\n');

    // Group warnings by file
    for (file_path, warnings) in all_warnings {
        let escaped_file = xml_escape(file_path);

        xml.push_str(&format!(
            r#"  <testsuite name="{}" tests="1" failures="{}" errors="0" time="0.000">"#,
            escaped_file,
            warnings.len()
        ));
        xml.push('\n');

        // Create a test case for the file
        xml.push_str(&format!(
            r#"    <testcase name="Lint {escaped_file}" classname="rumdl" time="0.000">"#
        ));
        xml.push('\n');

        // Add failures for each warning
        for warning in warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");
            let message = xml_escape(&warning.message);

            xml.push_str(&format!(
                r#"      <failure type="{}" message="{}">{} at line {}, column {}</failure>"#,
                rule_name, message, message, warning.line, warning.column
            ));
            xml.push('\n');
        }

        xml.push_str("    </testcase>\n");
        xml.push_str("  </testsuite>\n");
    }

    xml.push_str("</testsuites>\n");
    xml
}

/// Escape special XML characters
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{Fix, Severity};

    #[test]
    fn test_junit_formatter_default() {
        let _formatter = JunitFormatter;
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_junit_formatter_new() {
        let _formatter = JunitFormatter::new();
        // No fields to test, just ensure it constructs
    }

    #[test]
    fn test_format_warnings_empty() {
        let formatter = JunitFormatter::new();
        let warnings = vec![];
        let output = formatter.format_warnings(&warnings, "test.md");

        assert!(output.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"1\" failures=\"0\" errors=\"0\" time=\"0.000\">"));
        assert!(output.contains("<testsuite name=\"test.md\" tests=\"1\" failures=\"0\" errors=\"0\" time=\"0.000\">"));
        assert!(output.contains("<testcase name=\"Lint test.md\" classname=\"rumdl\" time=\"0.000\">"));
        assert!(output.contains("</testcase>"));
        assert!(output.contains("</testsuite>"));
        assert!(output.contains("</testsuites>"));
    }

    #[test]
    fn test_format_single_warning() {
        let formatter = JunitFormatter::new();
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

        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"1\" failures=\"1\" errors=\"0\" time=\"0.000\">"));
        assert!(
            output.contains("<testsuite name=\"README.md\" tests=\"1\" failures=\"1\" errors=\"0\" time=\"0.000\">")
        );
        assert!(output.contains(
            "<failure type=\"MD001\" message=\"Heading levels should only increment by one level at a time\">"
        ));
        assert!(output.contains("at line 10, column 5</failure>"));
    }

    #[test]
    fn test_format_single_warning_with_fix() {
        let formatter = JunitFormatter::new();
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

        // JUnit format doesn't indicate fixable issues
        assert!(output.contains("<failure type=\"MD001\""));
        assert!(!output.contains("fixable"));
    }

    #[test]
    fn test_format_multiple_warnings() {
        let formatter = JunitFormatter::new();
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

        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"1\" failures=\"2\" errors=\"0\" time=\"0.000\">"));
        assert!(output.contains("<testsuite name=\"test.md\" tests=\"1\" failures=\"2\" errors=\"0\" time=\"0.000\">"));
        assert!(output.contains("<failure type=\"MD001\" message=\"First warning\">"));
        assert!(output.contains("at line 5, column 1</failure>"));
        assert!(output.contains("<failure type=\"MD013\" message=\"Second warning\">"));
        assert!(output.contains("at line 10, column 3</failure>"));
    }

    #[test]
    fn test_format_warning_unknown_rule() {
        let formatter = JunitFormatter::new();
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

        assert!(output.contains("<failure type=\"unknown\" message=\"Unknown rule warning\">"));
    }

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("normal text"), "normal text");
        assert_eq!(xml_escape("text with & ampersand"), "text with &amp; ampersand");
        assert_eq!(xml_escape("text with < and >"), "text with &lt; and &gt;");
        assert_eq!(xml_escape("text with \" quotes"), "text with &quot; quotes");
        assert_eq!(xml_escape("text with ' apostrophe"), "text with &apos; apostrophe");
        assert_eq!(xml_escape("all: < > & \" '"), "all: &lt; &gt; &amp; &quot; &apos;");
    }

    #[test]
    fn test_junit_report_empty() {
        let warnings = vec![];
        let output = format_junit_report(&warnings, 1234);

        assert!(output.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"0\" failures=\"0\" errors=\"0\" time=\"1.234\">"));
        assert!(output.ends_with("</testsuites>\n"));
    }

    #[test]
    fn test_junit_report_single_file() {
        let warnings = vec![(
            "test.md".to_string(),
            vec![LintWarning {
                line: 10,
                column: 5,
                end_line: 10,
                end_column: 15,
                rule_name: Some("MD001"),
                message: "Test warning".to_string(),
                severity: Severity::Warning,
                fix: None,
            }],
        )];

        let output = format_junit_report(&warnings, 500);

        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"1\" failures=\"1\" errors=\"0\" time=\"0.500\">"));
        assert!(output.contains("<testsuite name=\"test.md\" tests=\"1\" failures=\"1\" errors=\"0\" time=\"0.000\">"));
    }

    #[test]
    fn test_junit_report_multiple_files() {
        let warnings = vec![
            (
                "file1.md".to_string(),
                vec![LintWarning {
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 5,
                    rule_name: Some("MD001"),
                    message: "Warning in file 1".to_string(),
                    severity: Severity::Warning,
                    fix: None,
                }],
            ),
            (
                "file2.md".to_string(),
                vec![
                    LintWarning {
                        line: 5,
                        column: 1,
                        end_line: 5,
                        end_column: 10,
                        rule_name: Some("MD013"),
                        message: "Warning 1 in file 2".to_string(),
                        severity: Severity::Warning,
                        fix: None,
                    },
                    LintWarning {
                        line: 10,
                        column: 1,
                        end_line: 10,
                        end_column: 10,
                        rule_name: Some("MD022"),
                        message: "Warning 2 in file 2".to_string(),
                        severity: Severity::Error,
                        fix: None,
                    },
                ],
            ),
        ];

        let output = format_junit_report(&warnings, 2500);

        // Total: 2 files, 3 failures
        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"2\" failures=\"3\" errors=\"0\" time=\"2.500\">"));
        assert!(output.contains("<testsuite name=\"file1.md\" tests=\"1\" failures=\"1\""));
        assert!(output.contains("<testsuite name=\"file2.md\" tests=\"1\" failures=\"2\""));
    }

    #[test]
    fn test_special_characters_in_message() {
        let formatter = JunitFormatter::new();
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: Some("MD001"),
            message: "Warning with < > & \" ' special chars".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];

        let output = formatter.format_warnings(&warnings, "test.md");

        assert!(output.contains("message=\"Warning with &lt; &gt; &amp; &quot; &apos; special chars\""));
        assert!(output.contains(">Warning with &lt; &gt; &amp; &quot; &apos; special chars at line"));
    }

    #[test]
    fn test_special_characters_in_file_path() {
        let formatter = JunitFormatter::new();
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

        let output = formatter.format_warnings(&warnings, "path/with<special>&chars.md");

        assert!(output.contains("<testsuite name=\"path/with&lt;special&gt;&amp;chars.md\""));
        assert!(output.contains("<testcase name=\"Lint path/with&lt;special&gt;&amp;chars.md\""));
    }

    #[test]
    fn test_xml_structure() {
        let formatter = JunitFormatter::new();
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

        let output = formatter.format_warnings(&warnings, "test.md");

        // Verify XML structure is properly nested
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines[0], "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
        assert!(lines[1].starts_with("<testsuites"));
        assert!(lines[2].starts_with("  <testsuite"));
        assert!(lines[3].starts_with("    <testcase"));
        assert!(lines[4].starts_with("      <failure"));
        assert_eq!(lines[5], "    </testcase>");
        assert_eq!(lines[6], "  </testsuite>");
        assert_eq!(lines[7], "</testsuites>");
    }

    #[test]
    fn test_duration_formatting() {
        let warnings = vec![(
            "test.md".to_string(),
            vec![LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("MD001"),
                message: "Test".to_string(),
                severity: Severity::Warning,
                fix: None,
            }],
        )];

        // Test various duration values
        let output1 = format_junit_report(&warnings, 1234);
        assert!(output1.contains("time=\"1.234\""));

        let output2 = format_junit_report(&warnings, 500);
        assert!(output2.contains("time=\"0.500\""));

        let output3 = format_junit_report(&warnings, 12345);
        assert!(output3.contains("time=\"12.345\""));
    }
}
