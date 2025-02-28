use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug, Default)]
pub struct MD019MultipleSpaceAtx;

impl Rule for MD019MultipleSpaceAtx {
    fn name(&self) -> &'static str {
        "MD019"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces after hash on ATX style heading should be removed"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let re = Regex::new(r"^(#{1,6})\s{2,}(.+?)(?:\s+#*)?$").unwrap();

        for (line_num, line) in content.lines().enumerate() {
            if let Some(cap) = re.captures(line) {
                let hashes = &cap[1];
                let text = cap[2].trim();
                let start = cap.get(0).unwrap().start();
                warnings.push(LintWarning {
                    line: line_num + 1,
                    column: start + 1,
                    message: "Multiple spaces found after hash on ATX style heading".to_string(),
                    fix: Some(Fix {
                        line: line_num + 1,
                        column: start + 1,
                        replacement: format!("{} {}", hashes, text),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let re = Regex::new(r"^(#{1,6})\s{2,}(.+?)(?:\s+#*)?$").unwrap();

        for line in content.lines() {
            if let Some(cap) = re.captures(line) {
                let hashes = &cap[1];
                let text = cap[2].trim();
                result.push_str(&format!("{} {}\n", hashes, text));
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