use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use regex::Regex;

/// Rule MD042: No empty links
///
/// This rule is triggered when a link has no content (text) or destination (URL).
pub struct MD042NoEmptyLinks;

impl Default for MD042NoEmptyLinks {
    fn default() -> Self {
        Self::new()
    }
}

impl MD042NoEmptyLinks {
    pub fn new() -> Self {
        Self
    }
}

impl Rule for MD042NoEmptyLinks {
    fn name(&self) -> &'static str {
        "MD042"
    }

    fn description(&self) -> &'static str {
        "No empty links"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let empty_link_regex = Regex::new(r"\[([^\]]*)\]\(([^\)]*)\)").unwrap();

        for (line_num, line) in content.lines().enumerate() {
            for cap in empty_link_regex.captures_iter(line) {
                let text = cap.get(1).map_or("", |m| m.as_str());
                let url = cap.get(2).map_or("", |m| m.as_str());

                if text.trim().is_empty() || url.trim().is_empty() {
                    let full_match = cap.get(0).unwrap();
                    warnings.push(LintWarning {
            rule_name: Some(self.name()),
                        message: format!("Empty link found: [{}]({})", text, url),
                        line: line_num + 1,
                        column: full_match.start() + 1,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index
                                .line_col_to_byte_range(line_num + 1, full_match.start() + 1),
                            replacement: String::new(), // Remove empty link
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let empty_link_regex = Regex::new(r"\[([^\]]*)\]\(([^\)]*)\)").unwrap();

        let result = empty_link_regex.replace_all(content, |caps: &regex::Captures| {
            let text = caps.get(1).map_or("", |m| m.as_str());
            let url = caps.get(2).map_or("", |m| m.as_str());

            if text.trim().is_empty() || url.trim().is_empty() {
                String::new()
            } else {
                caps[0].to_string()
            }
        });

        Ok(result.to_string())
    }
}
