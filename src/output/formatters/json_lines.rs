//! JSON Lines output formatter (one JSON object per line)

use crate::output::OutputFormatter;
use crate::rule::LintWarning;
use serde_json::json;

/// JSON Lines formatter - one JSON object per line
pub struct JsonLinesFormatter;

impl JsonLinesFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for JsonLinesFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        let mut output = String::new();

        for warning in warnings {
            let json_obj = json!({
                "file": file_path,
                "line": warning.line,
                "column": warning.column,
                "rule": warning.rule_name.unwrap_or("unknown"),
                "message": warning.message,
                "severity": "warning",
                "fixable": warning.fix.is_some()
            });

            // Compact JSON representation on a single line
            if let Ok(json_str) = serde_json::to_string(&json_obj) {
                output.push_str(&json_str);
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
