//! Azure Pipeline logging format

use crate::output::OutputFormatter;
use crate::rule::LintWarning;

/// Azure Pipeline formatter
/// Outputs in the format: ##vso[task.logissue type=warning;sourcepath=<file>;linenumber=<line>;columnnumber=<col>;code=<rule>]<message>
pub struct AzureFormatter;

impl AzureFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for AzureFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        let mut output = String::new();

        for warning in warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");

            // Azure Pipeline logging command format
            let line = format!(
                "##vso[task.logissue type=warning;sourcepath={};linenumber={};columnnumber={};code={}]{}",
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
