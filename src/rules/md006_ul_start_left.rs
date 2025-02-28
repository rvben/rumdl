use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug, Default)]
pub struct MD006ULStartLeft;

impl MD006ULStartLeft {
    fn is_unordered_list_item(line: &str) -> bool {
        let trimmed = line.trim_start();
        trimmed.starts_with('*') || trimmed.starts_with('-') || trimmed.starts_with('+')
    }

    fn get_indentation(line: &str) -> usize {
        line.len() - line.trim_start().len()
    }
}

impl Rule for MD006ULStartLeft {
    fn name(&self) -> &'static str {
        "MD006"
    }

    fn description(&self) -> &'static str {
        "Consider starting bulleted lists at the beginning of the line"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut in_list = false;

        for (line_num, line) in content.lines().enumerate() {
            if Self::is_unordered_list_item(line) {
                let indent = Self::get_indentation(line);
                if !in_list && indent > 0 {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: 1,
                        message: "First list item should start at the beginning of the line".to_string(),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: 1,
                            replacement: line.trim_start().to_string(),
                        }),
                    });
                }
                in_list = true;
            } else if line.trim().is_empty() {
                in_list = false;
            } else if !line.starts_with(' ') {
                in_list = false;
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut in_list = false;

        for line in content.lines() {
            if Self::is_unordered_list_item(line) {
                if !in_list {
                    result.push_str(line.trim_start());
                    in_list = true;
                } else {
                    result.push_str(line);
                }
            } else {
                result.push_str(line);
                if line.trim().is_empty() {
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