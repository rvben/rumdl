//! Full output formatter with source line display (ruff-style)

use crate::output::OutputFormatter;
use crate::rule::LintWarning;
use colored::*;

/// Full formatter that shows source lines with carets (ruff-style)
pub struct FullFormatter {
    use_colors: bool,
}

impl Default for FullFormatter {
    fn default() -> Self {
        Self { use_colors: true }
    }
}

impl FullFormatter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn without_colors() -> Self {
        Self { use_colors: false }
    }

    /// Render the source context block: gutter, source line, and caret underline.
    fn render_source_context(&self, output: &mut String, warning: &LintWarning, lines: &[&str]) {
        let line_idx = warning.line.saturating_sub(1);
        if line_idx >= lines.len() {
            return;
        }

        let source_line = lines[line_idx];
        let line_num = warning.line;
        let gutter_width = line_num.to_string().len().max(2);
        let empty_gutter = " ".repeat(gutter_width);

        // Empty gutter line
        if self.use_colors {
            output.push_str(&format!("{empty_gutter} {}\n", "|".blue().bold()));
        } else {
            output.push_str(&format!("{empty_gutter} |\n"));
        }

        // Source line with line number
        if self.use_colors {
            output.push_str(&format!(
                "{:>width$} {} {}\n",
                line_num.to_string().blue().bold(),
                "|".blue().bold(),
                source_line,
                width = gutter_width,
            ));
        } else {
            output.push_str(&format!("{line_num:>gutter_width$} | {source_line}\n"));
        }

        // Caret underline
        let col = warning.column.saturating_sub(1);
        let end_col = if warning.end_column > warning.column {
            warning.end_column.saturating_sub(1)
        } else {
            col + 1
        };
        let caret_len = end_col.saturating_sub(col).max(1);
        let padding = " ".repeat(col);
        let carets = "^".repeat(caret_len);

        if self.use_colors {
            output.push_str(&format!(
                "{empty_gutter} {} {padding}{}\n",
                "|".blue().bold(),
                carets.yellow().bold(),
            ));
        } else {
            output.push_str(&format!("{empty_gutter} | {padding}{carets}\n"));
        }

        // Closing empty gutter line
        if self.use_colors {
            output.push_str(&format!("{empty_gutter} {}\n", "|".blue().bold()));
        } else {
            output.push_str(&format!("{empty_gutter} |\n"));
        }
    }
}

impl OutputFormatter for FullFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        // Without content, fall back to text-style output
        let mut output = String::new();

        for warning in warnings {
            let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");
            let fix_indicator = if warning.fix.is_some() { " [*]" } else { "" };

            let line = format!(
                "{}:{}:{}: [{}] {}{}",
                file_path, warning.line, warning.column, rule_name, warning.message, fix_indicator,
            );

            output.push_str(&line);
            output.push('\n');
        }

        if output.ends_with('\n') {
            output.pop();
        }

        output
    }

    fn format_warnings_with_content(&self, warnings: &[LintWarning], file_path: &str, content: &str) -> String {
        if content.is_empty() {
            return self.format_warnings(warnings, file_path);
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut output = String::new();

        for (i, warning) in warnings.iter().enumerate() {
            if i > 0 {
                output.push('\n');
            }

            let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");
            let fix_indicator = if warning.fix.is_some() { " [*]" } else { "" };

            // Header line: rule name and message
            if self.use_colors {
                output.push_str(&format!(
                    "{} {}{}",
                    rule_name.red().bold(),
                    warning.message,
                    fix_indicator.green()
                ));
            } else {
                output.push_str(&format!("{rule_name} {}{fix_indicator}", warning.message));
            }
            output.push('\n');

            // Location line
            if self.use_colors {
                output.push_str(&format!(
                    " {} {}:{}:{}\n",
                    "-->".blue().bold(),
                    file_path,
                    warning.line,
                    warning.column,
                ));
            } else {
                output.push_str(&format!(" --> {}:{}:{}\n", file_path, warning.line, warning.column,));
            }

            // Source context with gutter, source line, and carets
            self.render_source_context(&mut output, warning, &lines);
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

    fn make_warning(line: usize, column: usize, end_column: usize, rule: &str, message: &str) -> LintWarning {
        LintWarning {
            line,
            column,
            end_line: line,
            end_column,
            rule_name: Some(rule.to_string()),
            message: message.to_string(),
            severity: Severity::Warning,
            fix: None,
        }
    }

    #[test]
    fn test_full_formatter_without_content_falls_back() {
        let formatter = FullFormatter::without_colors();
        let warnings = vec![make_warning(1, 1, 5, "MD001", "Heading increment")];
        let output = formatter.format_warnings(&warnings, "test.md");
        assert!(output.contains("test.md:1:1:"));
        assert!(output.contains("MD001"));
        assert!(output.contains("Heading increment"));
    }

    #[test]
    fn test_full_formatter_with_content() {
        let formatter = FullFormatter::without_colors();
        let content = "# Hello\n\nThis is a test line that is long\n";
        let warnings = vec![make_warning(3, 1, 33, "MD013", "Line length")];
        let output = formatter.format_warnings_with_content(&warnings, "test.md", content);

        assert!(output.contains("MD013 Line length"));
        assert!(output.contains(" --> test.md:3:1"));
        assert!(output.contains("This is a test line that is long"));
        assert!(output.contains("^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^"));
    }

    #[test]
    fn test_full_formatter_with_fix_indicator() {
        let formatter = FullFormatter::without_colors();
        let content = "# Hello\n";
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 8,
            rule_name: Some("MD022".to_string()),
            message: "Headings should be surrounded by blank lines".to_string(),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 0..8,
                replacement: "\n# Hello\n".to_string(),
            }),
        }];
        let output = formatter.format_warnings_with_content(&warnings, "test.md", content);
        assert!(output.contains("[*]"));
    }

    #[test]
    fn test_full_formatter_multiple_warnings() {
        let formatter = FullFormatter::without_colors();
        let content = "# Hello\n\nSecond line\n\nThird line\n";
        let warnings = vec![
            make_warning(1, 1, 8, "MD001", "First issue"),
            make_warning(3, 1, 12, "MD013", "Second issue"),
        ];
        let output = formatter.format_warnings_with_content(&warnings, "test.md", content);

        assert!(output.contains("MD001 First issue"));
        assert!(output.contains("MD013 Second issue"));
        assert!(output.contains("# Hello"));
        assert!(output.contains("Second line"));
    }

    #[test]
    fn test_full_formatter_column_offset() {
        let formatter = FullFormatter::without_colors();
        let content = "Some text with issue here\n";
        let warnings = vec![make_warning(1, 16, 21, "MD001", "Problem here")];
        let output = formatter.format_warnings_with_content(&warnings, "test.md", content);

        // 15 spaces padding (col 16 - 1), then 5 carets (21-16)
        assert!(output.contains("               ^^^^^"));
    }

    #[test]
    fn test_full_formatter_empty_warnings() {
        let formatter = FullFormatter::without_colors();
        let content = "# Hello\n";
        let output = formatter.format_warnings_with_content(&[], "test.md", content);
        assert!(output.is_empty());
    }

    #[test]
    fn test_full_formatter_line_out_of_range() {
        let formatter = FullFormatter::without_colors();
        let content = "Only one line\n";
        let warnings = vec![make_warning(5, 1, 5, "MD001", "Out of range")];
        let output = formatter.format_warnings_with_content(&warnings, "test.md", content);

        // Should still show header and location, just no source context
        assert!(output.contains("MD001 Out of range"));
        assert!(output.contains(" --> test.md:5:1"));
    }

    #[test]
    fn test_full_formatter_no_rule_name() {
        let formatter = FullFormatter::without_colors();
        let content = "# Hello\n";
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            rule_name: None,
            message: "Generic warning".to_string(),
            severity: Severity::Warning,
            fix: None,
        }];
        let output = formatter.format_warnings_with_content(&warnings, "test.md", content);
        assert!(output.contains("unknown Generic warning"));
    }

    #[test]
    fn test_full_formatter_single_char_caret() {
        let formatter = FullFormatter::without_colors();
        let content = "Hello world\n";
        // When end_column == column, should still show at least one caret
        let warnings = vec![make_warning(1, 5, 5, "MD001", "Single char")];
        let output = formatter.format_warnings_with_content(&warnings, "test.md", content);
        assert!(output.contains("    ^"));
    }

    #[test]
    fn test_full_formatter_gutter_width_for_large_line_numbers() {
        let formatter = FullFormatter::without_colors();
        let mut content = String::new();
        for i in 1..=150 {
            content.push_str(&format!("Line {i}\n"));
        }
        let warnings = vec![make_warning(142, 1, 9, "MD001", "At line 142")];
        let output = formatter.format_warnings_with_content(&warnings, "test.md", &content);

        // Gutter should be 3 chars wide for line 142
        assert!(output.contains("142 | Line 142"));
    }

    #[test]
    fn test_full_formatter_empty_content_falls_back() {
        let formatter = FullFormatter::without_colors();
        let warnings = vec![make_warning(1, 1, 5, "MD001", "Test")];
        let output = formatter.format_warnings_with_content(&warnings, "test.md", "");
        // Should fall back to format_warnings (text-style)
        assert!(output.contains("test.md:1:1:"));
    }
}
