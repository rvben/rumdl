use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug)]
pub struct MD012MultipleBlankLines {
    pub maximum: usize,
}

impl Default for MD012MultipleBlankLines {
    fn default() -> Self {
        Self { maximum: 1 }
    }
}

impl MD012MultipleBlankLines {
    pub fn new(maximum: usize) -> Self {
        Self { maximum }
    }
}

impl Rule for MD012MultipleBlankLines {
    fn name(&self) -> &'static str {
        "MD012"
    }

    fn description(&self) -> &'static str {
        "Multiple consecutive blank lines should be reduced"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut blank_count = 0;
        let mut start_line = 0;

        for (line_num, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                if blank_count == 0 {
                    start_line = line_num;
                }
                blank_count += 1;
            } else {
                if blank_count > self.maximum {
                    warnings.push(LintWarning {
                        line: start_line + 1,
                        column: 1,
                        message: format!("Multiple blank lines found ({}), maximum allowed is {}", blank_count, self.maximum),
                        fix: Some(Fix {
                            line: start_line + 1,
                            column: 1,
                            replacement: "\n".repeat(self.maximum),
                        }),
                    });
                }
                blank_count = 0;
            }
        }

        // Check for trailing blank lines
        if blank_count > self.maximum {
            warnings.push(LintWarning {
                line: start_line + 1,
                column: 1,
                message: format!("Multiple blank lines found ({}), maximum allowed is {}", blank_count, self.maximum),
                fix: Some(Fix {
                    line: start_line + 1,
                    column: 1,
                    replacement: "\n".repeat(self.maximum),
                }),
            });
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut blank_count = 0;
        let mut buffer = String::new();

        for line in content.lines() {
            if line.trim().is_empty() {
                blank_count += 1;
                buffer.push('\n');
            } else {
                if blank_count > 0 {
                    result.push_str(&"\n".repeat(blank_count.min(self.maximum)));
                    buffer.clear();
                }
                result.push_str(line);
                result.push('\n');
                blank_count = 0;
            }
        }

        // Handle trailing blank lines
        if blank_count > 0 {
            result.push_str(&"\n".repeat(blank_count.min(self.maximum)));
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 