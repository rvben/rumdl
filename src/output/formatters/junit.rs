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
        // A single file is one testcase that passes (no warnings) or fails.
        format_junit_report(
            &[(file_path.to_string(), warnings.to_vec())],
            &[file_path.to_string()],
            0,
        )
    }
}

/// Format a JUnit XML report covering every checked file.
///
/// Each checked file is one `<testcase>` ("Lint <file>"): a clean file passes (no
/// `<failure>` child), a file with warnings fails and carries one `<failure>` per
/// warning. `all_files` lists every file that was checked (clean and dirty);
/// `all_warnings` holds only the files that have warnings. Reporting all checked
/// files means a clean run lists its passing files rather than emitting an empty
/// report. Counts stay JUnit-consistent: `tests` is the number of checked files and
/// `failures` the number of files with issues, so `failures <= tests` always holds.
pub fn format_junit_report(
    all_warnings: &[(String, Vec<LintWarning>)],
    all_files: &[String],
    duration_ms: u64,
) -> String {
    use std::collections::HashMap;
    use std::collections::HashSet;

    let warnings_by_file: HashMap<&str, &[LintWarning]> =
        all_warnings.iter().map(|(p, w)| (p.as_str(), w.as_slice())).collect();

    // Report every checked file once, in order; defensively include any warning-
    // bearing file that is not in all_files so a warning is never dropped.
    let mut files: Vec<&str> = Vec::with_capacity(all_files.len());
    let mut seen: HashSet<&str> = HashSet::new();
    for path in all_files {
        if seen.insert(path.as_str()) {
            files.push(path.as_str());
        }
    }
    for (path, _) in all_warnings {
        if seen.insert(path.as_str()) {
            files.push(path.as_str());
        }
    }

    let warnings_for = |file: &str| -> &[LintWarning] { warnings_by_file.get(file).copied().unwrap_or(&[]) };

    let total_tests = files.len();
    let files_with_issues = files.iter().filter(|f| !warnings_for(f).is_empty()).count();
    let duration_secs = duration_ms as f64 / 1000.0;

    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');

    xml.push_str(&format!(
        r#"<testsuites name="rumdl" tests="{total_tests}" failures="{files_with_issues}" errors="0" time="{duration_secs:.3}">"#
    ));
    xml.push('\n');

    for file in files {
        let warnings = warnings_for(file);
        let failed = usize::from(!warnings.is_empty());
        let escaped_file = xml_escape(file);

        xml.push_str(&format!(
            r#"  <testsuite name="{escaped_file}" tests="1" failures="{failed}" errors="0" time="0.000">"#
        ));
        xml.push('\n');

        xml.push_str(&format!(
            r#"    <testcase name="Lint {escaped_file}" classname="rumdl" time="0.000">"#
        ));
        xml.push('\n');

        // A clean file's testcase has no <failure> children, so it passes.
        for warning in warnings {
            let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");
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

    fn warning(line: usize, column: usize, rule: &str, message: &str) -> LintWarning {
        LintWarning {
            line,
            column,
            end_line: line,
            end_column: column,
            rule_name: Some(rule.to_string()),
            message: message.to_string(),
            severity: Severity::Warning,
            fix: None,
        }
    }

    /// Count `<failure ...>` elements in the report.
    fn count_failures(xml: &str) -> usize {
        xml.matches("<failure ").count()
    }

    #[test]
    fn test_junit_formatter_default_and_new() {
        let _ = JunitFormatter;
        let _ = JunitFormatter::new();
    }

    // ---- single-file path (OutputFormatter::format_warnings) -----------------

    #[test]
    fn test_format_warnings_empty_file_passes() {
        // A clean file is a passing testcase: present, no <failure>, failures="0".
        let output = JunitFormatter::new().format_warnings(&[], "test.md");

        assert!(output.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"1\" failures=\"0\" errors=\"0\""));
        assert!(output.contains("<testsuite name=\"test.md\" tests=\"1\" failures=\"0\" errors=\"0\" time=\"0.000\">"));
        assert!(output.contains("<testcase name=\"Lint test.md\" classname=\"rumdl\" time=\"0.000\">"));
        assert_eq!(count_failures(&output), 0);
    }

    #[test]
    fn test_format_single_warning() {
        let warnings = vec![warning(
            10,
            5,
            "MD001",
            "Heading levels should only increment by one level at a time",
        )];
        let output = JunitFormatter::new().format_warnings(&warnings, "README.md");

        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"1\" failures=\"1\" errors=\"0\""));
        assert!(
            output.contains("<testsuite name=\"README.md\" tests=\"1\" failures=\"1\" errors=\"0\" time=\"0.000\">")
        );
        assert!(output.contains(
            "<failure type=\"MD001\" message=\"Heading levels should only increment by one level at a time\">"
        ));
        assert!(output.contains("at line 10, column 5</failure>"));
    }

    #[test]
    fn test_multiple_warnings_in_one_file_are_one_failed_testcase() {
        // One file with two warnings is ONE failed testcase (failures="1"), with two
        // <failure> children - not failures="2" (which would exceed tests="1").
        let warnings = vec![
            warning(5, 1, "MD001", "First warning"),
            warning(10, 3, "MD013", "Second warning"),
        ];
        let output = JunitFormatter::new().format_warnings(&warnings, "test.md");

        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"1\" failures=\"1\" errors=\"0\""));
        assert!(output.contains("<testsuite name=\"test.md\" tests=\"1\" failures=\"1\" errors=\"0\" time=\"0.000\">"));
        assert_eq!(count_failures(&output), 2);
        assert!(output.contains("<failure type=\"MD001\" message=\"First warning\">"));
        assert!(output.contains("<failure type=\"MD013\" message=\"Second warning\">"));
    }

    #[test]
    fn test_format_single_warning_with_fix_has_no_fixable_marker() {
        let mut w = warning(10, 5, "MD001", "Heading");
        w.fix = Some(Fix::new(100..110, "## Heading".to_string()));
        let output = JunitFormatter::new().format_warnings(&[w], "README.md");

        assert!(output.contains("<failure type=\"MD001\""));
        assert!(!output.contains("fixable"));
    }

    #[test]
    fn test_format_warning_unknown_rule() {
        let mut w = warning(1, 1, "MD001", "Unknown rule warning");
        w.rule_name = None;
        let output = JunitFormatter::new().format_warnings(&[w], "file.md");

        assert!(output.contains("<failure type=\"unknown\" message=\"Unknown rule warning\">"));
    }

    // ---- batch report (format_junit_report) ----------------------------------

    #[test]
    fn test_report_no_files_is_empty() {
        let output = format_junit_report(&[], &[], 1234);
        assert!(output.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"0\" failures=\"0\" errors=\"0\" time=\"1.234\">"));
        assert!(output.ends_with("</testsuites>\n"));
    }

    #[test]
    fn test_report_all_clean_files_listed_as_passing() {
        // The core of issue #654: a clean run lists its passing files, not an empty report.
        let all_files = vec!["a.md".to_string(), "b.md".to_string()];
        let output = format_junit_report(&[], &all_files, 500);

        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"2\" failures=\"0\" errors=\"0\" time=\"0.500\">"));
        assert!(output.contains("<testsuite name=\"a.md\" tests=\"1\" failures=\"0\""));
        assert!(output.contains("<testsuite name=\"b.md\" tests=\"1\" failures=\"0\""));
        assert!(output.contains("<testcase name=\"Lint a.md\""));
        assert!(output.contains("<testcase name=\"Lint b.md\""));
        assert_eq!(count_failures(&output), 0);
    }

    #[test]
    fn test_report_mixed_clean_and_dirty() {
        let all_files = vec!["clean.md".to_string(), "dirty.md".to_string(), "clean2.md".to_string()];
        let all_warnings = vec![(
            "dirty.md".to_string(),
            vec![
                warning(1, 2, "MD018", "No space after #"),
                warning(3, 1, "MD012", "Blank lines"),
            ],
        )];
        let output = format_junit_report(&all_warnings, &all_files, 0);

        // 3 files checked, 1 failed file => failures <= tests.
        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"3\" failures=\"1\" errors=\"0\""));
        assert!(output.contains("<testsuite name=\"clean.md\" tests=\"1\" failures=\"0\""));
        assert!(output.contains("<testsuite name=\"clean2.md\" tests=\"1\" failures=\"0\""));
        assert!(output.contains("<testsuite name=\"dirty.md\" tests=\"1\" failures=\"1\""));
        // The dirty file carries both warnings; clean files carry none.
        assert_eq!(count_failures(&output), 2);
    }

    #[test]
    fn test_report_failures_never_exceed_tests() {
        // A file with many warnings is still one failed testcase.
        let all_files = vec!["x.md".to_string()];
        let warnings: Vec<LintWarning> = (1..=5).map(|i| warning(i, 1, "MD013", "long line")).collect();
        let output = format_junit_report(&[("x.md".to_string(), warnings)], &all_files, 0);

        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"1\" failures=\"1\""));
        assert!(output.contains("<testsuite name=\"x.md\" tests=\"1\" failures=\"1\""));
        assert_eq!(count_failures(&output), 5);
    }

    #[test]
    fn test_report_includes_warning_file_absent_from_all_files() {
        // Defensive: a warning-bearing file not present in all_files is still reported.
        let output = format_junit_report(&[("orphan.md".to_string(), vec![warning(1, 1, "MD001", "x")])], &[], 0);
        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"1\" failures=\"1\""));
        assert!(output.contains("<testsuite name=\"orphan.md\""));
    }

    #[test]
    fn test_report_deduplicates_files() {
        // A file appearing in both all_files and all_warnings is rendered once.
        let all_files = vec!["dup.md".to_string()];
        let all_warnings = vec![("dup.md".to_string(), vec![warning(1, 1, "MD001", "x")])];
        let output = format_junit_report(&all_warnings, &all_files, 0);

        assert_eq!(output.matches("<testsuite name=\"dup.md\"").count(), 1);
        assert!(output.contains("<testsuites name=\"rumdl\" tests=\"1\" failures=\"1\""));
    }

    #[test]
    fn test_duration_formatting() {
        let files = vec!["test.md".to_string()];
        let warnings = vec![("test.md".to_string(), vec![warning(1, 1, "MD001", "x")])];
        assert!(format_junit_report(&warnings, &files, 1234).contains("time=\"1.234\""));
        assert!(format_junit_report(&warnings, &files, 500).contains("time=\"0.500\""));
        assert!(format_junit_report(&warnings, &files, 12345).contains("time=\"12.345\""));
    }

    // ---- escaping & structure ------------------------------------------------

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
    fn test_special_characters_in_message() {
        let warnings = vec![warning(1, 1, "MD001", "Warning with < > & \" ' special chars")];
        let output = JunitFormatter::new().format_warnings(&warnings, "test.md");

        assert!(output.contains("message=\"Warning with &lt; &gt; &amp; &quot; &apos; special chars\""));
        assert!(output.contains(">Warning with &lt; &gt; &amp; &quot; &apos; special chars at line"));
    }

    #[test]
    fn test_special_characters_in_file_path() {
        let warnings = vec![warning(1, 1, "MD001", "Test")];
        let output = JunitFormatter::new().format_warnings(&warnings, "path/with<special>&chars.md");

        assert!(output.contains("<testsuite name=\"path/with&lt;special&gt;&amp;chars.md\""));
        assert!(output.contains("<testcase name=\"Lint path/with&lt;special&gt;&amp;chars.md\""));
    }

    #[test]
    fn test_xml_structure_nesting() {
        let warnings = vec![warning(1, 1, "MD001", "Test")];
        let output = JunitFormatter::new().format_warnings(&warnings, "test.md");

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
}
