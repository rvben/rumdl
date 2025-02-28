use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref ATX_NO_SPACE_PATTERN: Regex = Regex::new(r"^(#+)([^#\s])").unwrap();
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(\s*)```").unwrap();
}

#[derive(Debug, Default)]
pub struct MD018NoMissingSpaceAtx;

impl MD018NoMissingSpaceAtx {
    pub fn new() -> Self {
        Self::default()
    }

    fn is_atx_heading_without_space(&self, line: &str) -> bool {
        ATX_NO_SPACE_PATTERN.is_match(line)
    }

    fn fix_atx_heading(&self, line: &str) -> String {
        let captures = ATX_NO_SPACE_PATTERN.captures(line).unwrap();
        let hashes = captures.get(1).unwrap();
        let content = &line[hashes.end()..];
        format!("{} {}", hashes.as_str(), content)
    }
}

impl Rule for MD018NoMissingSpaceAtx {
    fn name(&self) -> &'static str {
        "MD018"
    }

    fn description(&self) -> &'static str {
        "No space after hash on ATX style heading"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut in_code_block = false;

        for (line_num, line) in content.lines().enumerate() {
            if CODE_BLOCK_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                continue;
            }

            if !in_code_block && self.is_atx_heading_without_space(line) {
                let hashes = ATX_NO_SPACE_PATTERN.captures(line).unwrap().get(1).unwrap();
                warnings.push(LintWarning {
                    message: format!(
                        "No space after {} in ATX style heading",
                        "#".repeat(hashes.as_str().len())
                    ),
                    line: line_num + 1,
                    column: hashes.end() + 1,
                    fix: Some(Fix {
                        line: line_num + 1,
                        column: 1,
                        replacement: self.fix_atx_heading(line),
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
            } else if !in_code_block && self.is_atx_heading_without_space(line) {
                result.push_str(&self.fix_atx_heading(line));
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