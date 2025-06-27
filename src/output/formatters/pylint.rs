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
            
            // Convert MD prefix to CMD for pylint convention
            // Pylint uses C for Convention, so CMD = Convention + MD rule
            let pylint_code = if rule_name.starts_with("MD") {
                format!("CMD{}", &rule_name[2..])
            } else {
                format!("C{}", rule_name)
            };

            // Pylint format: file:line:column: [C0000] message
            let line = format!(
                "{}:{}:{}: [{}] {}",
                file_path, warning.line, warning.column, pylint_code, warning.message
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
