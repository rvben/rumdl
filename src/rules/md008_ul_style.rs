use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug)]
pub struct MD008ULStyle {
    style: char,
}

impl Default for MD008ULStyle {
    fn default() -> Self {
        Self { style: '*' }
    }
}

impl MD008ULStyle {
    pub fn new(style: char) -> Self {
        Self { style }
    }

    fn get_list_marker(line: &str) -> Option<char> {
        let trimmed = line.trim_start();
        if trimmed.starts_with(['*', '+', '-']) {
            Some(trimmed.chars().next().unwrap())
        } else {
            None
        }
    }
}

impl Rule for MD008ULStyle {
    fn name(&self) -> &'static str {
        "MD008"
    }

    fn description(&self) -> &'static str {
        "Unordered list style"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            if let Some(marker) = Self::get_list_marker(line) {
                if marker != self.style {
                    warnings.push(LintWarning {
                        message: format!(
                            "Unordered list item marker '{}' should be '{}'",
                            marker, self.style
                        ),
                        line: line_num + 1,
                        column: line.find(marker).unwrap() + 1,
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: line.find(marker).unwrap() + 1,
                            replacement: line.replacen(marker, &self.style.to_string(), 1),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut last_line = 0;

        for (line_num, line) in content.lines().enumerate() {
            if line_num > last_line {
                result.push('\n');
            }

            if let Some(marker) = Self::get_list_marker(line) {
                if marker != self.style {
                    result.push_str(&line.replacen(marker, &self.style.to_string(), 1));
                } else {
                    result.push_str(line);
                }
            } else {
                result.push_str(line);
            }

            last_line = line_num;
        }

        Ok(result)
    }
} 