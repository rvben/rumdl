use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref CLOSED_ATX_MULTIPLE_SPACE_PATTERN: Regex = Regex::new(r"^(\s*)(#+)(\s+)(.*?)(\s+)(#+)\s*$").unwrap();
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(\s*)```").unwrap();
}

#[derive(Debug, Default)]
pub struct MD021NoMultipleSpaceClosedAtx;

impl MD021NoMultipleSpaceClosedAtx {
    pub fn new() -> Self {
        Self::default()
    }

    fn is_closed_atx_heading_with_multiple_spaces(&self, line: &str) -> bool {
        if let Some(captures) = CLOSED_ATX_MULTIPLE_SPACE_PATTERN.captures(line) {
            let start_spaces = captures.get(3).unwrap().as_str().len();
            let end_spaces = captures.get(5).unwrap().as_str().len();
            start_spaces > 1 || end_spaces > 1
        } else {
            false
        }
    }

    fn fix_closed_atx_heading(&self, line: &str) -> String {
        if let Some(captures) = CLOSED_ATX_MULTIPLE_SPACE_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[4];
            let closing_hashes = &captures[6];
            format!("{}{} {} {}", indentation, opening_hashes, content.trim(), closing_hashes)
        } else {
            line.to_string()
        }
    }

    fn count_spaces(&self, line: &str) -> (usize, usize) {
        if let Some(captures) = CLOSED_ATX_MULTIPLE_SPACE_PATTERN.captures(line) {
            let start_spaces = captures.get(3).unwrap().as_str().len();
            let end_spaces = captures.get(5).unwrap().as_str().len();
            (start_spaces, end_spaces)
        } else {
            (0, 0)
        }
    }
}

impl Rule for MD021NoMultipleSpaceClosedAtx {
    fn name(&self) -> &'static str {
        "MD021"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces inside hashes on closed ATX style heading"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut in_code_block = false;

        for (line_num, line) in content.lines().enumerate() {
            if CODE_BLOCK_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                continue;
            }

            if !in_code_block && self.is_closed_atx_heading_with_multiple_spaces(line) {
                let captures = CLOSED_ATX_MULTIPLE_SPACE_PATTERN.captures(line).unwrap();
                let indentation = captures.get(1).unwrap();
                let opening_hashes = captures.get(2).unwrap();
                let (start_spaces, end_spaces) = self.count_spaces(line);
                let message = if start_spaces > 1 && end_spaces > 1 {
                    format!(
                        "Multiple spaces ({} at start, {} at end) inside hashes on closed ATX style heading with {} hashes",
                        start_spaces,
                        end_spaces,
                        opening_hashes.as_str().len()
                    )
                } else if start_spaces > 1 {
                    format!(
                        "Multiple spaces ({}) after opening hashes on closed ATX style heading with {} hashes",
                        start_spaces,
                        opening_hashes.as_str().len()
                    )
                } else {
                    format!(
                        "Multiple spaces ({}) before closing hashes on closed ATX style heading with {} hashes",
                        end_spaces,
                        opening_hashes.as_str().len()
                    )
                };
                warnings.push(LintWarning {
                    message,
                    line: line_num + 1,
                    column: indentation.end() + 1,
                    fix: Some(Fix {
                        line: line_num + 1,
                        column: 1,
                        replacement: self.fix_closed_atx_heading(line),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut in_code_block = false;

        for line in content.lines() {
            if CODE_BLOCK_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                result.push_str(line);
            } else if !in_code_block && self.is_closed_atx_heading_with_multiple_spaces(line) {
                result.push_str(&self.fix_closed_atx_heading(line));
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }

        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 