use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug)]
pub struct MD035HRStyle {
    style: String,
}

impl Default for MD035HRStyle {
    fn default() -> Self {
        Self {
            style: "---".to_string(),
        }
    }
}

impl MD035HRStyle {
    pub fn new(style: &str) -> Self {
        Self {
            style: style.to_string(),
        }
    }

    fn is_horizontal_rule(&self, line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.len() < 3 {
            return false;
        }

        let first_char = trimmed.chars().next().unwrap();
        if first_char != '-' && first_char != '*' && first_char != '_' {
            return false;
        }

        trimmed.chars().all(|c| c == first_char || c.is_whitespace())
    }
}

impl Rule for MD035HRStyle {
    fn name(&self) -> &'static str {
        "MD035"
    }

    fn description(&self) -> &'static str {
        "Horizontal rule style"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();

        for (i, line) in content.lines().enumerate() {
            if self.is_horizontal_rule(line) && line.trim() != self.style {
                warnings.push(LintWarning {
                    message: format!("Horizontal rule style should be '{}'", self.style),
                    line: i + 1,
                    column: 1,
                    fix: Some(Fix {
                        line: i + 1,
                        column: 1,
                        replacement: self.style.clone(),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            if self.is_horizontal_rule(line) {
                result.push_str(&self.style);
            } else {
                result.push_str(line);
            }
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        if content.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }
} 