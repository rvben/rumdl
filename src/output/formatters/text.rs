//! Default text output formatter with colors and context

use crate::output::OutputFormatter;
use crate::rule::LintWarning;
use colored::*;

/// Default human-readable formatter with colors
pub struct TextFormatter {
    use_colors: bool,
}

impl TextFormatter {
    pub fn new() -> Self {
        Self { use_colors: true }
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
                    format!("[{:5}]", rule_name).yellow().to_string()
                } else {
                    format!("[{:5}]", rule_name)
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
