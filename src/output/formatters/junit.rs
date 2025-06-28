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
    fn format_warnings(&self, _warnings: &[LintWarning], _file_path: &str) -> String {
        // JUnit needs to collect all results and output as a single XML document
        String::new()
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
        r#"<testsuites name="rumdl" tests="{}" failures="{}" errors="0" time="{:.3}">"#,
        files_with_issues, total_issues, duration_secs
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
            r#"    <testcase name="Lint {}" classname="rumdl" time="0.000">"#,
            escaped_file
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
