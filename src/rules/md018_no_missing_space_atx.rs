use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::heading_utils::HeadingUtils;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref ATX_NO_SPACE_PATTERN: Regex = Regex::new(r"^(#+)([^#\s])").unwrap();
}

#[derive(Debug, Default)]
pub struct MD018NoMissingSpaceAtx;

impl MD018NoMissingSpaceAtx {
    pub fn new() -> Self {
        Self
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
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            // Skip lines in code blocks
            if HeadingUtils::is_in_code_block(content, line_num) {
                continue;
            }

            if self.is_atx_heading_without_space(line) {
                let hashes = ATX_NO_SPACE_PATTERN.captures(line).unwrap().get(1).unwrap();
                warnings.push(LintWarning {
                    message: format!(
                        "No space after {} in ATX style heading",
                        "#".repeat(hashes.as_str().len())
                    ),
                    line: line_num + 1,
                    column: hashes.end() + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(line_num + 1, 1),
                        replacement: self.fix_atx_heading(line),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let mut result = String::new();

        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            // Skip lines in code blocks
            if HeadingUtils::is_in_code_block(content, i) {
                result.push_str(line);
            } else if self.is_atx_heading_without_space(line) {
                result.push_str(&self.fix_atx_heading(line));
            } else {
                result.push_str(line);
            }

            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        // Preserve trailing newline if original had it
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }
}
