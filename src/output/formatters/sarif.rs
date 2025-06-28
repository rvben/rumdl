//! SARIF 2.1.0 output format

use crate::output::OutputFormatter;
use crate::rule::LintWarning;
use serde_json::json;

/// SARIF (Static Analysis Results Interchange Format) formatter
pub struct SarifFormatter;

impl Default for SarifFormatter {
    fn default() -> Self {
        Self
    }
}

impl SarifFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for SarifFormatter {
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String {
        // Format warnings for a single file as a minimal SARIF document
        let results: Vec<_> = warnings.iter().map(|warning| {
            let rule_id = warning.rule_name.unwrap_or("unknown");
            json!({
                "ruleId": rule_id,
                "level": "warning",
                "message": {
                    "text": warning.message
                },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": {
                            "uri": file_path
                        },
                        "region": {
                            "startLine": warning.line,
                            "startColumn": warning.column
                        }
                    }
                }]
            })
        }).collect();
        
        let sarif_doc = json!({
            "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "rumdl",
                        "version": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://github.com/rvben/rumdl"
                    }
                },
                "results": results
            }]
        });
        
        serde_json::to_string_pretty(&sarif_doc).unwrap_or_else(|_| r#"{"version":"2.1.0","runs":[]}"#.to_string())
    }
}

/// Format all warnings as SARIF 2.1.0 report
pub fn format_sarif_report(all_warnings: &[(String, Vec<LintWarning>)]) -> String {
    let mut results = Vec::new();
    let mut rules = std::collections::HashMap::new();

    // Collect all results and build rule index
    for (file_path, warnings) in all_warnings {
        for warning in warnings {
            let rule_id = warning.rule_name.unwrap_or("unknown");

            // Add rule to index if not already present
            rules.entry(rule_id).or_insert_with(|| {
                json!({
                    "id": rule_id,
                    "name": rule_id,
                    "shortDescription": {
                        "text": format!("Markdown rule {}", rule_id)
                    },
                    "fullDescription": {
                        "text": format!("Markdown linting rule {}", rule_id)
                    },
                    "defaultConfiguration": {
                        "level": "warning"
                    }
                })
            });

            let result = json!({
                "ruleId": rule_id,
                "level": "warning",
                "message": {
                    "text": warning.message
                },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": {
                            "uri": file_path
                        },
                        "region": {
                            "startLine": warning.line,
                            "startColumn": warning.column
                        }
                    }
                }]
            });

            results.push(result);
        }
    }

    // Build the complete SARIF document
    let sarif_doc = json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "rumdl",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/rvben/rumdl",
                    "rules": rules.values().cloned().collect::<Vec<_>>()
                }
            },
            "results": results
        }]
    });

    serde_json::to_string_pretty(&sarif_doc).unwrap_or_else(|_| r#"{"version":"2.1.0","runs":[]}"#.to_string())
}
