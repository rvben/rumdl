//! GitHub Actions annotation format

use crate::output::OutputFormatter;
use crate::rule::LintWarning;

/// GitHub Actions formatter
/// Outputs in the format: ::error file=<file>,line=<line>,col=<col>,title=<rule>::<message>
pub struct GitHubFormatter;

impl Default for GitHubFormatter {
    fn default() -> Self {
        Self
    }
}

impl GitHubFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for GitHubFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        let mut output = String::new();

        for warning in warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");

            // GitHub Actions annotation format
            // Use "warning" level since these are linting warnings, not errors
            let line = format!(
                "::warning file={},line={},col={},title={}::{}",
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
