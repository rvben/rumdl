use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::code_block_utils::CodeBlockUtils;
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::rules::list_utils::ListUtils;
use lazy_static::lazy_static;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug)]
pub struct MD015NoMissingSpaceAfterListMarker {
    pub require_space: bool,
}

impl Default for MD015NoMissingSpaceAfterListMarker {
    fn default() -> Self {
        Self {
            require_space: true,
        }
    }
}

impl MD015NoMissingSpaceAfterListMarker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_require_space(require_space: bool) -> Self {
        Self { require_space }
    }

    /// Check if a line is a horizontal rule
    fn is_horizontal_rule(line: &str) -> bool {
        let trimmed = line.trim();
        HR_DASH.is_match(trimmed)
            || HR_ASTERISK.is_match(trimmed)
            || HR_UNDERSCORE.is_match(trimmed)
    }

    /// Fix a list item without space for MD015 rule
    fn fix_list_item(line: &str) -> String {
        if let Some(caps) = LIST_ITEM_RE.captures(line) {
            format!("{}{} {}", &caps[1], &caps[2], &caps[3])
        } else {
            line.to_string()
        }
    }
}

lazy_static! {
    static ref HR_DASH: Regex = Regex::new(r"^\-{3,}\s*$").unwrap();
    static ref HR_ASTERISK: Regex = Regex::new(r"^\*{3,}\s*$").unwrap();
    static ref HR_UNDERSCORE: Regex = Regex::new(r"^_{3,}\s*$").unwrap();
}

static LIST_ITEM_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(\s*)((?:[-*+]|\d+[.)]))(\S.*)").unwrap());

impl Rule for MD015NoMissingSpaceAfterListMarker {
    fn name(&self) -> &'static str {
        "MD015"
    }

    fn description(&self) -> &'static str {
        "List markers must be followed by a space"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());

        if !self.require_space {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            // Skip processing if line is in a code block or front matter
            if CodeBlockUtils::is_in_code_block(content, line_num)
                || FrontMatterUtils::is_in_front_matter(content, line_num)
            {
                continue;
            }

            // Skip if this is a horizontal rule
            if Self::is_horizontal_rule(line) {
                continue;
            }

            if ListUtils::is_list_item_without_space(line) {
                warnings.push(LintWarning {
                    severity: Severity::Warning,
                    line: line_num + 1,
                    column: 1,
                    message: if line.trim_start().starts_with(['*', '+', '-']) {
                        "Missing space after unordered list marker".to_string()
                    } else {
                        "Missing space after ordered list marker".to_string()
                    },
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(line_num + 1, 1),
                        replacement: Self::fix_list_item(line),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        if !self.require_space {
            return Ok(content.to_string());
        }

        // Don't modify front matter
        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut in_front_matter = false;
        let mut in_code_block = false;

        for (i, line) in lines.iter().enumerate() {
            // Handle front matter
            if i == 0 && line.trim() == "---" {
                in_front_matter = true;
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if in_front_matter {
                if line.trim() == "---" {
                    in_front_matter = false;
                }
                result.push_str(line);
                result.push('\n');
                continue;
            }

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
                // Skip if this is a horizontal rule
                if Self::is_horizontal_rule(line) {
                    result.push_str(line);
                }
                // Check for list items without space
                else if ListUtils::is_list_item_without_space(line) {
                    result.push_str(&Self::fix_list_item(line));
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
