//! Output formatting module for rumdl
//!
//! This module provides different output formats for linting results,
//! similar to how Ruff handles multiple output formats.

use crate::rule::LintWarning;
use std::io::{self, Write};
use std::str::FromStr;

pub mod formatters;

// Re-export formatters
pub use formatters::*;

/// Trait for output formatters
pub trait OutputFormatter {
    /// Format a collection of warnings for output
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String;

    /// Format a summary of results across multiple files
    fn format_summary(&self, _files_processed: usize, _total_warnings: usize, _duration_ms: u64) -> Option<String> {
        // Default: no summary
        None
    }

    /// Whether this formatter should use colors
    fn use_colors(&self) -> bool {
        false
    }
}

/// Available output formats
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    /// Default human-readable format with colors and context
    Text,
    /// Concise format: file:line:col: [RULE] message
    Concise,
    /// Grouped format: violations grouped by file
    Grouped,
    /// JSON format (existing)
    Json,
    /// JSON Lines format (one JSON object per line)
    JsonLines,
    /// GitHub Actions annotation format
    GitHub,
    /// GitLab Code Quality format
    GitLab,
    /// Pylint-compatible format: file:line:column: CODE message
    Pylint,
    /// Azure Pipeline logging format
    Azure,
    /// SARIF 2.1.0 format
    Sarif,
    /// JUnit XML format
    Junit,
}

impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" | "full" => Ok(OutputFormat::Text),
            "concise" => Ok(OutputFormat::Concise),
            "grouped" => Ok(OutputFormat::Grouped),
            "json" => Ok(OutputFormat::Json),
            "json-lines" | "jsonlines" => Ok(OutputFormat::JsonLines),
            "github" => Ok(OutputFormat::GitHub),
            "gitlab" => Ok(OutputFormat::GitLab),
            "pylint" => Ok(OutputFormat::Pylint),
            "azure" => Ok(OutputFormat::Azure),
            "sarif" => Ok(OutputFormat::Sarif),
            "junit" => Ok(OutputFormat::Junit),
            _ => Err(format!("Unknown output format: {s}")),
        }
    }
}

impl OutputFormat {
    /// Create a formatter instance for this format
    pub fn create_formatter(&self) -> Box<dyn OutputFormatter> {
        match self {
            OutputFormat::Text => Box::new(TextFormatter::new()),
            OutputFormat::Concise => Box::new(ConciseFormatter::new()),
            OutputFormat::Grouped => Box::new(GroupedFormatter::new()),
            OutputFormat::Json => Box::new(JsonFormatter::new()),
            OutputFormat::JsonLines => Box::new(JsonLinesFormatter::new()),
            OutputFormat::GitHub => Box::new(GitHubFormatter::new()),
            OutputFormat::GitLab => Box::new(GitLabFormatter::new()),
            OutputFormat::Pylint => Box::new(PylintFormatter::new()),
            OutputFormat::Azure => Box::new(AzureFormatter::new()),
            OutputFormat::Sarif => Box::new(SarifFormatter::new()),
            OutputFormat::Junit => Box::new(JunitFormatter::new()),
        }
    }
}

/// Output writer that handles stdout/stderr routing
pub struct OutputWriter {
    use_stderr: bool,
    _quiet: bool,
    silent: bool,
}

impl OutputWriter {
    pub fn new(use_stderr: bool, quiet: bool, silent: bool) -> Self {
        Self {
            use_stderr,
            _quiet: quiet,
            silent,
        }
    }

    /// Write output to appropriate stream
    pub fn write(&self, content: &str) -> io::Result<()> {
        if self.silent {
            return Ok(());
        }

        if self.use_stderr {
            eprint!("{content}");
            io::stderr().flush()?;
        } else {
            print!("{content}");
            io::stdout().flush()?;
        }
        Ok(())
    }

    /// Write a line to appropriate stream
    pub fn writeln(&self, content: &str) -> io::Result<()> {
        if self.silent {
            return Ok(());
        }

        if self.use_stderr {
            eprintln!("{content}");
        } else {
            println!("{content}");
        }
        Ok(())
    }

    /// Write error/debug output (always to stderr unless silent)
    pub fn write_error(&self, content: &str) -> io::Result<()> {
        if self.silent {
            return Ok(());
        }

        eprintln!("{content}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{Fix, Severity};

    fn create_test_warning(line: usize, message: &str) -> LintWarning {
        LintWarning {
            line,
            column: 5,
            end_line: line,
            end_column: 10,
            rule_name: Some("MD001".to_string()),
            message: message.to_string(),
            severity: Severity::Warning,
            fix: None,
        }
    }

    fn create_test_warning_with_fix(line: usize, message: &str, fix_text: &str) -> LintWarning {
        LintWarning {
            line,
            column: 5,
            end_line: line,
            end_column: 10,
            rule_name: Some("MD001".to_string()),
            message: message.to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 0..5,
                replacement: fix_text.to_string(),
            }),
        }
    }

    #[test]
    fn test_output_format_from_str() {
        // Valid formats
        assert_eq!(OutputFormat::from_str("text").unwrap(), OutputFormat::Text);
        assert_eq!(OutputFormat::from_str("full").unwrap(), OutputFormat::Text);
        assert_eq!(OutputFormat::from_str("concise").unwrap(), OutputFormat::Concise);
        assert_eq!(OutputFormat::from_str("grouped").unwrap(), OutputFormat::Grouped);
        assert_eq!(OutputFormat::from_str("json").unwrap(), OutputFormat::Json);
        assert_eq!(OutputFormat::from_str("json-lines").unwrap(), OutputFormat::JsonLines);
        assert_eq!(OutputFormat::from_str("jsonlines").unwrap(), OutputFormat::JsonLines);
        assert_eq!(OutputFormat::from_str("github").unwrap(), OutputFormat::GitHub);
        assert_eq!(OutputFormat::from_str("gitlab").unwrap(), OutputFormat::GitLab);
        assert_eq!(OutputFormat::from_str("pylint").unwrap(), OutputFormat::Pylint);
        assert_eq!(OutputFormat::from_str("azure").unwrap(), OutputFormat::Azure);
        assert_eq!(OutputFormat::from_str("sarif").unwrap(), OutputFormat::Sarif);
        assert_eq!(OutputFormat::from_str("junit").unwrap(), OutputFormat::Junit);

        // Case insensitive
        assert_eq!(OutputFormat::from_str("TEXT").unwrap(), OutputFormat::Text);
        assert_eq!(OutputFormat::from_str("GitHub").unwrap(), OutputFormat::GitHub);
        assert_eq!(OutputFormat::from_str("JSON-LINES").unwrap(), OutputFormat::JsonLines);

        // Invalid format
        assert!(OutputFormat::from_str("invalid").is_err());
        assert!(OutputFormat::from_str("").is_err());
        assert!(OutputFormat::from_str("xml").is_err());
    }

    #[test]
    fn test_output_format_create_formatter() {
        // Test that each format creates the correct formatter
        let formats = [
            OutputFormat::Text,
            OutputFormat::Concise,
            OutputFormat::Grouped,
            OutputFormat::Json,
            OutputFormat::JsonLines,
            OutputFormat::GitHub,
            OutputFormat::GitLab,
            OutputFormat::Pylint,
            OutputFormat::Azure,
            OutputFormat::Sarif,
            OutputFormat::Junit,
        ];

        for format in &formats {
            let formatter = format.create_formatter();
            // Test that formatter can format warnings
            let warnings = vec![create_test_warning(1, "Test warning")];
            let output = formatter.format_warnings(&warnings, "test.md");
            assert!(!output.is_empty(), "Formatter {format:?} should produce output");
        }
    }

    #[test]
    fn test_output_writer_new() {
        let writer1 = OutputWriter::new(false, false, false);
        assert!(!writer1.use_stderr);
        assert!(!writer1._quiet);
        assert!(!writer1.silent);

        let writer2 = OutputWriter::new(true, true, false);
        assert!(writer2.use_stderr);
        assert!(writer2._quiet);
        assert!(!writer2.silent);

        let writer3 = OutputWriter::new(false, false, true);
        assert!(!writer3.use_stderr);
        assert!(!writer3._quiet);
        assert!(writer3.silent);
    }

    #[test]
    fn test_output_writer_silent_mode() {
        let writer = OutputWriter::new(false, false, true);

        // All write methods should succeed but not produce output when silent
        assert!(writer.write("test").is_ok());
        assert!(writer.writeln("test").is_ok());
        assert!(writer.write_error("test").is_ok());
    }

    #[test]
    fn test_output_writer_write_methods() {
        // Test non-silent mode
        let writer = OutputWriter::new(false, false, false);

        // These should succeed (we can't easily test the actual output)
        assert!(writer.write("test").is_ok());
        assert!(writer.writeln("test line").is_ok());
        assert!(writer.write_error("error message").is_ok());
    }

    #[test]
    fn test_output_writer_stderr_mode() {
        let writer = OutputWriter::new(true, false, false);

        // Should write to stderr instead of stdout
        assert!(writer.write("stderr test").is_ok());
        assert!(writer.writeln("stderr line").is_ok());

        // write_error always goes to stderr
        assert!(writer.write_error("error").is_ok());
    }

    #[test]
    fn test_formatter_trait_default_summary() {
        // Create a simple test formatter
        struct TestFormatter;
        impl OutputFormatter for TestFormatter {
            fn format_warnings(&self, _warnings: &[LintWarning], _file_path: &str) -> String {
                "test".to_string()
            }
        }

        let formatter = TestFormatter;
        assert_eq!(formatter.format_summary(10, 5, 1000), None);
        assert!(!formatter.use_colors());
    }

    #[test]
    fn test_formatter_with_multiple_warnings() {
        let warnings = vec![
            create_test_warning(1, "First warning"),
            create_test_warning(5, "Second warning"),
            create_test_warning_with_fix(10, "Third warning with fix", "fixed content"),
        ];

        // Test with different formatters
        let text_formatter = TextFormatter::new();
        let output = text_formatter.format_warnings(&warnings, "test.md");
        assert!(output.contains("First warning"));
        assert!(output.contains("Second warning"));
        assert!(output.contains("Third warning with fix"));
    }

    #[test]
    fn test_edge_cases() {
        // Empty warnings
        let empty_warnings: Vec<LintWarning> = vec![];
        let formatter = TextFormatter::new();
        let output = formatter.format_warnings(&empty_warnings, "test.md");
        // Most formatters should handle empty warnings gracefully
        assert!(output.is_empty() || output.trim().is_empty());

        // Very long file path
        let long_path = "a/".repeat(100) + "file.md";
        let warnings = vec![create_test_warning(1, "Test")];
        let output = formatter.format_warnings(&warnings, &long_path);
        assert!(!output.is_empty());

        // Unicode in messages
        let unicode_warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 10,
            rule_name: Some("MD001".to_string()),
            message: "Unicode test: 你好 🌟 émphasis".to_string(),
            severity: Severity::Warning,
            fix: None,
        };
        let output = formatter.format_warnings(&[unicode_warning], "test.md");
        assert!(output.contains("Unicode test"));
    }

    #[test]
    fn test_severity_variations() {
        let severities = [Severity::Error, Severity::Warning, Severity::Info];

        for severity in &severities {
            let warning = LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 5,
                rule_name: Some("MD001".to_string()),
                message: format!(
                    "Test {} message",
                    match severity {
                        Severity::Error => "error",
                        Severity::Warning => "warning",
                        Severity::Info => "info",
                    }
                ),
                severity: *severity,
                fix: None,
            };

            let formatter = TextFormatter::new();
            let output = formatter.format_warnings(&[warning], "test.md");
            assert!(!output.is_empty());
        }
    }

    #[test]
    fn test_output_format_equality() {
        assert_eq!(OutputFormat::Text, OutputFormat::Text);
        assert_ne!(OutputFormat::Text, OutputFormat::Json);
        assert_ne!(OutputFormat::Concise, OutputFormat::Grouped);
    }

    #[test]
    fn test_all_formats_handle_no_rule_name() {
        let warning = LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: None, // No rule name
            message: "Generic warning".to_string(),
            severity: Severity::Warning,
            fix: None,
        };

        let formats = [
            OutputFormat::Text,
            OutputFormat::Concise,
            OutputFormat::Grouped,
            OutputFormat::Json,
            OutputFormat::JsonLines,
            OutputFormat::GitHub,
            OutputFormat::GitLab,
            OutputFormat::Pylint,
            OutputFormat::Azure,
            OutputFormat::Sarif,
            OutputFormat::Junit,
        ];

        for format in &formats {
            let formatter = format.create_formatter();
            let output = formatter.format_warnings(std::slice::from_ref(&warning), "test.md");
            assert!(
                !output.is_empty(),
                "Format {format:?} should handle warnings without rule names"
            );
        }
    }
}
