
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Default)]
pub struct MD009NoTrailingSpaces;

impl Rule for MD009NoTrailingSpaces {
    fn name(&self) -> &'static str {
        "MD009"
    }

    fn description(&self) -> &'static str {
        "Trailing spaces are not allowed"
    }

    static TRAILING_SPACE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r" +$").unwrap()
    });

    fn fix_trailing_spaces(line: &str) -> String {
        Self::TRAILING_SPACE_RE.replace(line, "").to_string()
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let trailing_spaces = line.len() - Self::fix_trailing_spaces(line).len();
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
            result.push_str(&Self::fix_trailing_spaces(line));
            last_line = line_num;
        }

        Ok(result)
    }
}