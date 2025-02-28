use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug, Default)]
pub struct MD009NoTrailingSpaces;

impl Rule for MD009NoTrailingSpaces {
    fn name(&self) -> &'static str {
        "MD009"
    }

    fn description(&self) -> &'static str {
        "Trailing spaces are not allowed"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let trailing_spaces = line.len() - line.trim_end().len();
            if trailing_spaces > 0 {
                warnings.push(LintWarning {
                    message: format!("Found {} trailing space(s)", trailing_spaces),
                    line: line_num + 1,
                    column: line.len() - trailing_spaces + 1,
                    fix: Some(Fix {
                        line: line_num + 1,
                        column: line.len() - trailing_spaces + 1,
                        replacement: String::new(),
                    }),
                });
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
            result.push_str(line.trim_end());
            last_line = line_num;
        }

        Ok(result)
    }
} 