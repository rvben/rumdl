use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref UNORDERED_LIST_PATTERN: Regex = Regex::new(r"^(\s*)[*+-]\s{2,}").unwrap();
    static ref ORDERED_LIST_PATTERN: Regex = Regex::new(r"^(\s*)\d+\.\s{2,}").unwrap();
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(\s*)```").unwrap();
}

#[derive(Debug)]
pub struct MD016NoMultipleSpaceAfterListMarker {
    pub allow_multiple_spaces: bool,
}

impl Default for MD016NoMultipleSpaceAfterListMarker {
    fn default() -> Self {
        Self { allow_multiple_spaces: false }
    }
}

impl MD016NoMultipleSpaceAfterListMarker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_allow_multiple_spaces(allow_multiple_spaces: bool) -> Self {
        Self { allow_multiple_spaces }
    }

    fn is_unordered_list_with_multiple_spaces(&self, line: &str) -> bool {
        UNORDERED_LIST_PATTERN.is_match(line)
    }

    fn is_ordered_list_with_multiple_spaces(&self, line: &str) -> bool {
        ORDERED_LIST_PATTERN.is_match(line)
    }

    fn fix_unordered_list(&self, line: &str) -> String {
        let captures = UNORDERED_LIST_PATTERN.captures(line).unwrap();
        let indentation = &captures[1];
        let marker = &line[indentation.len()..indentation.len() + 1];
        let content = line[indentation.len() + 1..].trim_start();
        format!("{}{} {}", indentation, marker, content)
    }

    fn fix_ordered_list(&self, line: &str) -> String {
        let captures = ORDERED_LIST_PATTERN.captures(line).unwrap();
        let indentation = &captures[1];
        let marker_end = line[indentation.len()..].find('.').unwrap() + indentation.len() + 1;
        let marker = &line[indentation.len()..marker_end];
        let content = line[marker_end..].trim_start();
        format!("{}{} {}", indentation, marker, content)
    }
}

impl Rule for MD016NoMultipleSpaceAfterListMarker {
    fn name(&self) -> &'static str {
        "MD016"
    }

    fn description(&self) -> &'static str {
        "List markers should not be followed by multiple spaces"
    }

    fn check(&self, content: &str) -> LintResult {
        if self.allow_multiple_spaces {
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
                if self.is_unordered_list_with_multiple_spaces(line) {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: 1,
                        message: "Multiple spaces after unordered list marker".to_string(),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: 1,
                            replacement: self.fix_unordered_list(line),
                        }),
                    });
                } else if self.is_ordered_list_with_multiple_spaces(line) {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: 1,
                        message: "Multiple spaces after ordered list marker".to_string(),
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
        if self.allow_multiple_spaces {
            return Ok(content.to_string());
        }

        let mut result = String::new();
        let mut in_code_block = false;

        for line in content.lines() {
            if CODE_BLOCK_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                result.push_str(line);
            } else if !in_code_block {
                if self.is_unordered_list_with_multiple_spaces(line) {
                    result.push_str(&self.fix_unordered_list(line));
                } else if self.is_ordered_list_with_multiple_spaces(line) {
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