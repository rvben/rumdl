//! Concise output formatter for easy parsing by editors

use crate::output::OutputFormatter;
use crate::rule::LintWarning;

/// Concise formatter: file:line:col: [RULE] message
pub struct ConciseFormatter;

impl Default for ConciseFormatter {
    fn default() -> Self {
        Self
    }
}

impl ConciseFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for ConciseFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        let mut output = String::new();

        for warning in warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");

            // Simple format without colors: file:line:col: [RULE] message
            let line = format!(
                "{}:{}:{}: [{}] {}",
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
