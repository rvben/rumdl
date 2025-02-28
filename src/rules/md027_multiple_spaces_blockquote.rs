use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug, Default)]
pub struct MD027MultipleSpacesBlockquote;

impl Rule for MD027MultipleSpacesBlockquote {
    fn name(&self) -> &'static str {
        "MD027"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces after blockquote symbol should be removed"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let re = Regex::new(r"^(\s*>\s{2,})(.*)$").unwrap();

        for (line_num, line) in content.lines().enumerate() {
            if let Some(cap) = re.captures(line) {
                let start = cap.get(1).unwrap().start();
                let prefix = line[..start].to_string() + ">";
                let text = cap[2].to_string();
                warnings.push(LintWarning {
                    line: line_num + 1,
                    column: start + 1,
                    message: "Multiple spaces found after blockquote symbol".to_string(),
                    fix: Some(Fix {
                        line: line_num + 1,
                        column: start + 1,
                        replacement: format!("{} {}", prefix, text.trim_start()),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let re = Regex::new(r"^(\s*>\s{2,})(.*)$").unwrap();

        for line in content.lines() {
            if let Some(cap) = re.captures(line) {
                let start = cap.get(1).unwrap().start();
                let prefix = line[..start].to_string() + ">";
                let text = cap[2].to_string();
                result.push_str(&format!("{} {}\n", prefix, text.trim_start()));
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 