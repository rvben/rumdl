use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug, Default)]
pub struct MD021MultipleSpaceClosedAtx;

impl Rule for MD021MultipleSpaceClosedAtx {
    fn name(&self) -> &'static str {
        "MD021"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces inside hashes on closed ATX style heading should be removed"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let re = Regex::new(r"^(#{1,6})\s*(.+?)\s*(#+)\s*$").unwrap();

        for (line_num, line) in content.lines().enumerate() {
            if let Some(cap) = re.captures(line) {
                let opening_hashes = &cap[1];
                let text = cap[2].trim();
                let closing_hashes = &cap[3];
                let start = cap.get(0).unwrap().start();

                if text.trim().is_empty() || opening_hashes.len() != closing_hashes.len() {
                    continue;
                }

                if line != format!("{} {} {}", opening_hashes, text, closing_hashes) {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: start + 1,
                        message: "Multiple spaces found inside hashes on closed ATX style heading".to_string(),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: start + 1,
                            replacement: format!("{} {} {}", opening_hashes, text, closing_hashes),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let re = Regex::new(r"^(#{1,6})\s*(.+?)\s*(#+)\s*$").unwrap();

        for line in content.lines() {
            if let Some(cap) = re.captures(line) {
                let opening_hashes = &cap[1];
                let text = cap[2].trim();
                let closing_hashes = &cap[3];

                if text.trim().is_empty() || opening_hashes.len() != closing_hashes.len() {
                    result.push_str(line);
                } else {
                    result.push_str(&format!("{} {} {}", opening_hashes, text, closing_hashes));
                }
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 