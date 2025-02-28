use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

/// Rule MD042: No empty links
///
/// This rule is triggered when a link has no content (text) or destination (URL).
pub struct MD042NoEmptyLinks;

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
        let mut warnings = Vec::new();
        let empty_link_regex = Regex::new(r"\[([^\]]*)\]\(([^\)]*)\)").unwrap();

        for (line_num, line) in content.lines().enumerate() {
            for cap in empty_link_regex.captures_iter(line) {
                let text = cap.get(1).map_or("", |m| m.as_str());
                let url = cap.get(2).map_or("", |m| m.as_str());

                if text.trim().is_empty() || url.trim().is_empty() {
                    let full_match = cap.get(0).unwrap();
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: full_match.start() + 1,
                        message: format!("Empty link found: [{}]({})", text, url),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: full_match.start() + 1,
                            replacement: String::new(), // Remove empty link
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
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