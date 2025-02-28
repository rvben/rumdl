use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug, Default)]
pub struct MD006StartLeft;

impl Rule for MD006StartLeft {
    fn name(&self) -> &'static str {
        "MD006"
    }

    fn description(&self) -> &'static str {
        "Consider starting bulleted lists at the beginning of the line"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut in_list = false;

        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('*') || trimmed.starts_with('-') || trimmed.starts_with('+') {
                let indent = line.len() - trimmed.len();
                if !in_list && indent > 0 {
                    warnings.push(LintWarning {
                        message: "List item should start at the beginning of the line".to_string(),
                        line: i + 1,
                        column: 1,
                        fix: Some(Fix {
                            line: i + 1,
                            column: 1,
                            replacement: trimmed.to_string(),
                        }),
                    });
                }
                in_list = true;
            } else if trimmed.is_empty() {
                in_list = false;
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut in_list = false;

        for line in content.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('*') || trimmed.starts_with('-') || trimmed.starts_with('+') {
                if !in_list {
                    result.push_str(trimmed);
                } else {
                    result.push_str(line);
                }
                in_list = true;
            } else {
                result.push_str(line);
                if trimmed.is_empty() {
                    in_list = false;
                }
            }
            result.push('\n');
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 