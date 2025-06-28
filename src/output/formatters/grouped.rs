//! Grouped output formatter that groups violations by file

use crate::output::OutputFormatter;
use crate::rule::LintWarning;
use std::collections::HashMap;

/// Grouped formatter: groups violations by file
pub struct GroupedFormatter;

impl Default for GroupedFormatter {
    fn default() -> Self {
        Self
    }
}

impl GroupedFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for GroupedFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        if warnings.is_empty() {
            return String::new();
        }

        let mut output = String::new();

        // Group warnings by their rule name
        let mut grouped: HashMap<&str, Vec<&LintWarning>> = HashMap::new();
        for warning in warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");
            grouped.entry(rule_name).or_default().push(warning);
        }

        // Output file header
        output.push_str(&format!("{}:\n", file_path));

        // Sort rules for consistent output
        let mut rules: Vec<_> = grouped.keys().collect();
        rules.sort();

        for rule_name in rules {
            let rule_warnings = &grouped[rule_name];
            output.push_str(&format!("  {}:\n", rule_name));

            for warning in rule_warnings {
                output.push_str(&format!("    {}:{} {}", warning.line, warning.column, warning.message));
                if warning.fix.is_some() {
                    output.push_str(" (fixable)");
                }
                output.push('\n');
            }
        }

        // Remove trailing newline
        if output.ends_with('\n') {
            output.pop();
        }

        output
    }
}
