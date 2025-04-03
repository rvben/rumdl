use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

#[derive(Debug, Default)]
pub struct MD038NoSpaceInCode;

impl MD038NoSpaceInCode {
    fn is_in_code_block(&self, content: &str, line_num: usize) -> bool {
        lazy_static! {
            static ref FENCED_START: Regex =
                Regex::new(r"^(?P<indent>\s*)(?P<fence>```|~~~)").unwrap();
        }

        let mut in_code_block = false;

        let mut current_fence: Option<String> = None;

        for (i, line) in content.lines().enumerate() {
            if i + 1 > line_num {
                break;
            }

            if !in_code_block {
                if let Some(caps) = FENCED_START.captures(line) {
                    in_code_block = true;
                    current_fence = Some(caps.name("fence").unwrap().as_str().to_string());
                }
            } else if let Some(fence) = &current_fence {
                if line.trim_start().starts_with(fence) && line.trim_end().ends_with(fence) {
                    in_code_block = false;
                    current_fence = None;
                }
            }
        }

        in_code_block
    }

    fn check_line(&self, line: &str) -> Vec<(usize, String, String)> {
        let mut issues = Vec::new();

        // Find all code spans and check for spaces

        let mut in_code = false;

        let mut start_pos = 0;

        let chars: Vec<char> = line.chars().collect();

        // Create a mapping from character indices to byte indices

        let char_to_byte_indices: Vec<usize> =
            line.char_indices().map(|(byte_idx, _)| byte_idx).collect();
        // Add the length of the string as the last byte index

        let byte_length = line.len();

        for (i, &c) in chars.iter().enumerate() {
            if c == '`' {
                if !in_code {
                    // Start of code span
                    start_pos = i;
                    in_code = true;
                } else {
                    // End of code span
                    in_code = false;

                    // Skip if this span is part of a longer span (e.g. ``code``)
                    if i > 0 && chars[i - 1] == '`' {
                        continue;
                    }
                    if i < chars.len() - 1 && chars[i + 1] == '`' {
                        continue;
                    }

                    // Get the byte indices for safe slicing
                    let start_byte = char_to_byte_indices[start_pos];
                    let end_byte = if i + 1 < char_to_byte_indices.len() {
                        char_to_byte_indices[i + 1] - 1
                    } else {
                        byte_length - 1
                    };

                    // Check for spaces at start and end (using character indices)
                    let span = &line[start_byte..=end_byte];

                    // Extract content between backticks
                    let content_start_idx = if start_pos + 1 < chars.len() {
                        start_pos + 1
                    } else {
                        start_pos
                    };
                    let content_end_idx = if i > 0 { i - 1 } else { i };

                    // Handle the case where backticks are directly adjacent
                    if content_start_idx <= content_end_idx {
                        let content_start_byte = char_to_byte_indices[content_start_idx];
                        let content_end_byte = if content_end_idx + 1 < char_to_byte_indices.len() {
                            char_to_byte_indices[content_end_idx + 1] - 1
                        } else {
                            byte_length - 1
                        };

                        let content = &line[content_start_byte..=content_end_byte];

                        if content.starts_with(' ') || content.ends_with(' ') {
                            let trimmed = content.trim();
                            if !trimmed.is_empty() {
                                let fixed = format!("`{}`", trimmed);
                                issues.push((
                                    char_to_byte_indices[start_pos] + 1,
                                    span.to_string(),
                                    fixed,
                                ));
                            }
                        }
                    }
                }
            }
        }

        issues
    }
}

impl Rule for MD038NoSpaceInCode {
    fn name(&self) -> &'static str {
        "MD038"
    }

    fn description(&self) -> &'static str {
        "Spaces inside code span elements"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        for (i, line) in content.lines().enumerate() {
            if !self.is_in_code_block(content, i + 1) {
                for (column, _original, fixed) in self.check_line(line) {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: i + 1,
                        column,
                        message: "Spaces inside code span elements should be removed".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range(i + 1, column),
                            replacement: fixed,
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let lines: Vec<&str> = content.lines().collect();

        let mut result = String::new();

        for (i, &line) in lines.iter().enumerate() {
            let mut current_line = line.to_string();

            if !self.is_in_code_block(content, i + 1) {
                // Sort issues by position in reverse order to avoid invalidating positions
                let mut issues = self.check_line(line);
                issues.sort_by(|a, b| b.0.cmp(&a.0));

                for (pos, _original, fixed) in issues {
                    let prefix = &current_line[..pos - 1];
                    let suffix = &current_line[pos - 1 + _original.len()..];
                    current_line = format!("{}{}{}", prefix, fixed, suffix);
                }
            }

            result.push_str(&current_line);
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        // Ensure trailing newline is preserved
        if content.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }
}
