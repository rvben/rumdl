//! Pylint-compatible output formatter

use crate::output::OutputFormatter;
use crate::rule::LintWarning;

/// Pylint-compatible formatter: file:line:column: CODE message
pub struct PylintFormatter;

impl PylintFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for PylintFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        let mut output = String::new();

        for warning in warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");

            // Pylint format: file:line:column: [C0000] message
            // We use C (convention) for all markdown linting rules
            let line = format!(
                "{}:{}:{}: [C{}] {}",
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
