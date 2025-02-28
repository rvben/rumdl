use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref UNORDERED_LIST_PATTERN: Regex = Regex::new(r"^(\s*)[*+-][^\s]").unwrap();
    static ref ORDERED_LIST_PATTERN: Regex = Regex::new(r"^(\s*)\d+\.[^\s]").unwrap();
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(\s*)```").unwrap();
}

#[derive(Debug)]
pub struct MD015NoMissingSpaceAfterListMarker {
    pub require_space: bool,
}

impl Default for MD015NoMissingSpaceAfterListMarker {
    fn default() -> Self {
        Self { require_space: true }
    }
}

impl MD015NoMissingSpaceAfterListMarker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_require_space(require_space: bool) -> Self {
        Self { require_space }
    }

    fn is_unordered_list_without_space(&self, line: &str) -> bool {
        UNORDERED_LIST_PATTERN.is_match(line)
    }

    fn is_ordered_list_without_space(&self, line: &str) -> bool {
        ORDERED_LIST_PATTERN.is_match(line)
    }

    fn fix_unordered_list(&self, line: &str) -> String {
        let captures = UNORDERED_LIST_PATTERN.captures(line).unwrap();
        let indentation = &captures[1];
        let marker = &line[indentation.len()..indentation.len() + 1];
        let content = &line[indentation.len() + 1..];
        format!("{}{} {}", indentation, marker, content)
    }

    fn fix_ordered_list(&self, line: &str) -> String {
        let captures = ORDERED_LIST_PATTERN.captures(line).unwrap();
        let indentation = &captures[1];
        let marker_end = line[indentation.len()..].find('.').unwrap() + indentation.len() + 1;
        let marker = &line[indentation.len()..marker_end];
        let content = &line[marker_end..];
        format!("{}{} {}", indentation, marker, content)
    }
}

impl Rule for MD015NoMissingSpaceAfterListMarker {
    fn name(&self) -> &'static str {
        "MD015"
    }

    fn description(&self) -> &'static str {
        "List markers must be followed by a space"
    }

    fn check(&self, content: &str) -> LintResult {
        if !self.require_space {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let mut in_code_block = false;

        for (line_num, line) in content.lines().enumerate() {
            if CODE_BLOCK_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                continue;
            }

            if !in_code_block {
                if self.is_unordered_list_without_space(line) {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: 1,
                        message: "Missing space after unordered list marker".to_string(),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: 1,
                            replacement: self.fix_unordered_list(line),
                        }),
                    });
                } else if self.is_ordered_list_without_space(line) {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: 1,
                        message: "Missing space after ordered list marker".to_string(),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: 1,
                            replacement: self.fix_ordered_list(line),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if !self.require_space {
            return Ok(content.to_string());
        }

        let mut result = String::new();
        let mut in_code_block = false;

        for line in content.lines() {
            if CODE_BLOCK_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                result.push_str(line);
            } else if !in_code_block {
                if self.is_unordered_list_without_space(line) {
                    result.push_str(&self.fix_unordered_list(line));
                } else if self.is_ordered_list_without_space(line) {
                    result.push_str(&self.fix_ordered_list(line));
                } else {
                    result.push_str(line);
                }
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