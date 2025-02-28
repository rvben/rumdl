use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref CLOSED_ATX_NO_SPACE_PATTERN: Regex = Regex::new(r"^(\s*)(#+)([^#\s].*?)([^#\s])(#+)\s*$").unwrap();
    static ref CLOSED_ATX_NO_SPACE_START_PATTERN: Regex = Regex::new(r"^(\s*)(#+)([^#\s].*?)\s(#+)\s*$").unwrap();
    static ref CLOSED_ATX_NO_SPACE_END_PATTERN: Regex = Regex::new(r"^(\s*)(#+)\s(.*?)([^#\s])(#+)\s*$").unwrap();
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(\s*)```").unwrap();
}

#[derive(Debug, Default)]
pub struct MD020NoMissingSpaceClosedAtx;

impl MD020NoMissingSpaceClosedAtx {
    pub fn new() -> Self {
        Self::default()
    }

    fn is_closed_atx_heading_without_space(&self, line: &str) -> bool {
        CLOSED_ATX_NO_SPACE_PATTERN.is_match(line) ||
        CLOSED_ATX_NO_SPACE_START_PATTERN.is_match(line) ||
        CLOSED_ATX_NO_SPACE_END_PATTERN.is_match(line)
    }

    fn fix_closed_atx_heading(&self, line: &str) -> String {
        if let Some(captures) = CLOSED_ATX_NO_SPACE_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let last_char = &captures[4];
            let closing_hashes = &captures[5];
            format!("{}{} {}{} {}", indentation, opening_hashes, content, last_char, closing_hashes)
        } else if let Some(captures) = CLOSED_ATX_NO_SPACE_START_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let closing_hashes = &captures[4];
            format!("{}{} {} {}", indentation, opening_hashes, content, closing_hashes)
        } else if let Some(captures) = CLOSED_ATX_NO_SPACE_END_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let last_char = &captures[4];
            let closing_hashes = &captures[5];
            format!("{}{} {}{} {}", indentation, opening_hashes, content, last_char, closing_hashes)
        } else {
            line.to_string()
        }
    }
}

impl Rule for MD020NoMissingSpaceClosedAtx {
    fn name(&self) -> &'static str {
        "MD020"
    }

    fn description(&self) -> &'static str {
        "No space inside hashes on closed ATX style heading"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut in_code_block = false;

        for (line_num, line) in content.lines().enumerate() {
            if CODE_BLOCK_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                continue;
            }

            if !in_code_block && self.is_closed_atx_heading_without_space(line) {
                let captures = if let Some(c) = CLOSED_ATX_NO_SPACE_PATTERN.captures(line) {
                    c
                } else if let Some(c) = CLOSED_ATX_NO_SPACE_START_PATTERN.captures(line) {
                    c
                } else {
                    CLOSED_ATX_NO_SPACE_END_PATTERN.captures(line).unwrap()
                };
                let indentation = captures.get(1).unwrap();
                let opening_hashes = captures.get(2).unwrap();
                warnings.push(LintWarning {
                    message: format!(
                        "Missing space inside hashes on closed ATX style heading with {} hashes",
                        opening_hashes.as_str().len()
                    ),
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
            } else if !in_code_block && self.is_closed_atx_heading_without_space(line) {
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