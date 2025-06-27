//! JSON output formatter

use crate::output::OutputFormatter;
use crate::rule::LintWarning;
use serde_json::{json, Value};

/// JSON formatter for machine-readable output
pub struct JsonFormatter {
    collect_all: bool,
}

impl JsonFormatter {
    pub fn new() -> Self {
        Self { collect_all: false }
    }

    /// Create a formatter that collects all warnings into a single JSON array
    pub fn new_collecting() -> Self {
        Self { collect_all: true }
    }
}

impl OutputFormatter for JsonFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        if self.collect_all {
            // For batch collection mode, just return empty string
            // The actual JSON will be built elsewhere with all files
            return String::new();
        }

        let json_warnings: Vec<Value> = warnings
            .iter()
            .map(|warning| {
                json!({
                    "file": file_path,
                    "line": warning.line,
                    "column": warning.column,
                    "rule": warning.rule_name.unwrap_or("unknown"),
                    "message": warning.message,
                    "severity": "warning",
                    "fixable": warning.fix.is_some(),
                    "fix": warning.fix.as_ref().map(|f| {
                        json!({
                            "range": {
                                "start": f.range.start,
                                "end": f.range.end
                            },
                            "replacement": f.replacement
                        })
                    })
                })
            })
            .collect();

        serde_json::to_string_pretty(&json_warnings).unwrap_or_default()
    }
}

/// Helper to format all warnings from multiple files as a single JSON document
pub fn format_all_warnings_as_json(all_warnings: &[(String, Vec<LintWarning>)]) -> String {
    let mut json_warnings = Vec::new();

    for (file_path, warnings) in all_warnings {
        for warning in warnings {
            json_warnings.push(json!({
                "file": file_path,
                "line": warning.line,
                "column": warning.column,
                "rule": warning.rule_name.unwrap_or("unknown"),
                "message": warning.message,
                "severity": "warning",
                "fixable": warning.fix.is_some(),
                "fix": warning.fix.as_ref().map(|f| {
                    json!({
                        "range": {
                            "start": f.range.start,
                            "end": f.range.end
                        },
                        "replacement": f.replacement
                    })
                })
            }));
        }
    }

    serde_json::to_string_pretty(&json_warnings).unwrap_or_default()
}
