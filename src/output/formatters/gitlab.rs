//! GitLab Code Quality report format

use crate::output::OutputFormatter;
use crate::rule::LintWarning;
use serde_json::json;

/// GitLab Code Quality formatter
/// Outputs in GitLab's code quality JSON format
pub struct GitLabFormatter;

impl Default for GitLabFormatter {
    fn default() -> Self {
        Self
    }
}

impl GitLabFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for GitLabFormatter {
    fn format_warnings(&self, _warnings: &[LintWarning], _file_path: &str) -> String {
        // GitLab format needs to collect all issues and output as a single JSON array
        // For now, return empty string and handle collection separately
        String::new()
    }
}

/// Format all warnings as GitLab Code Quality report
pub fn format_gitlab_report(all_warnings: &[(String, Vec<LintWarning>)]) -> String {
    let mut issues = Vec::new();

    for (file_path, warnings) in all_warnings {
        for warning in warnings {
            let rule_name = warning.rule_name.unwrap_or("unknown");

            // Create a fingerprint for deduplication
            let fingerprint = format!("{}-{}-{}-{}", file_path, warning.line, warning.column, rule_name);

            let issue = json!({
                "description": warning.message,
                "check_name": rule_name,
                "fingerprint": fingerprint,
                "severity": "minor",
                "location": {
                    "path": file_path,
                    "lines": {
                        "begin": warning.line
                    }
                }
            });

            issues.push(issue);
        }
    }

    serde_json::to_string_pretty(&issues).unwrap_or_else(|_| "[]".to_string())
}
