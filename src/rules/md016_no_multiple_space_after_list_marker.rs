use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::code_block_utils::CodeBlockUtils;
use crate::rules::list_utils::ListUtils;

#[derive(Debug, Default)]
pub struct MD016NoMultipleSpaceAfterListMarker {
    pub allow_multiple_spaces: bool,
}

impl MD016NoMultipleSpaceAfterListMarker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_allow_multiple_spaces(allow_multiple_spaces: bool) -> Self {
        Self {
            allow_multiple_spaces,
        }
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
        let _line_index = LineIndex::new(content.to_string());

        if self.allow_multiple_spaces {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            // Skip processing if line is in a code block
            if CodeBlockUtils::is_in_code_block(content, line_num) {
                continue;
            }

            if ListUtils::is_list_item_with_multiple_spaces(line) {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    severity: Severity::Warning,
                    line: line_num + 1,
                    column: 1,
                    message: if line.trim_start().starts_with(['*', '+', '-']) {
                        "Multiple spaces after unordered list marker".to_string()
                    } else {
                        "Multiple spaces after ordered list marker".to_string()
                    },
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(line_num + 1, 1),
                        replacement: ListUtils::fix_list_item_with_multiple_spaces(line),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        if self.allow_multiple_spaces {
            return Ok(content.to_string());
        }

        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;

        for (i, line) in lines.iter().enumerate() {
            // Track code blocks
            if CodeBlockUtils::is_code_block_delimiter(line) {
                in_code_block = !in_code_block;
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            // Skip processing if line is in a code block
            if in_code_block {
                result.push_str(line);
            } else {
                // Check for list items with multiple spaces
                if ListUtils::is_list_item_with_multiple_spaces(line) {
                    result.push_str(&ListUtils::fix_list_item_with_multiple_spaces(line));
                } else {
                    result.push_str(line);
                }
            }

            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        // Remove trailing newline if original didn't have one

        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
}
