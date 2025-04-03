use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

#[derive(Debug, Default)]
pub struct MD039NoSpaceInLinks;

impl MD039NoSpaceInLinks {
    fn is_in_code_block(&self, content: &str, line_num: usize) -> bool {
        let mut in_code_block = false;

        let mut fence_type = None;

        let mut in_inline_code = false;

        for (i, line) in content.lines().enumerate() {
            if i + 1 == line_num {
                // Count backticks in the current line up to this point
                let backticks = line.chars().filter(|&c| c == '`').count();
                in_inline_code = backticks % 2 == 1;
                break;
            }

            let trimmed = line.trim();
            if let Some(fence) = fence_type {
                if trimmed.starts_with(fence) {
                    in_code_block = false;
                    fence_type = None;
                }
            } else if trimmed.starts_with("```") {
                in_code_block = true;
                fence_type = Some("```");
            } else if trimmed.starts_with("~~~") {
                in_code_block = true;
                fence_type = Some("~~~");
            }
        }

        in_code_block || in_inline_code
    }

    fn check_line(&self, line: &str) -> Vec<(usize, String, String)> {
        let mut issues = Vec::new();

        let chars: Vec<char> = line.chars().collect();

        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '[' {
                let text_start_idx = i + 1;
                let mut text_end_idx = None;
                let mut link_start_idx = None;
                let mut link_end_idx = None;
                let mut bracket_depth = 1;
                let mut j = i + 1;

                // Find matching closing bracket
                while j < chars.len() {
                    match chars[j] {
                        '[' => bracket_depth += 1,
                        ']' => {
                            bracket_depth -= 1;
                            if bracket_depth == 0 {
                                text_end_idx = Some(j);
                                // Look for opening parenthesis
                                if j + 1 < chars.len() && chars[j + 1] == '(' {
                                    link_start_idx = Some(j + 2);
                                    // Find closing parenthesis
                                    let mut paren_depth = 1;
                                    let mut k = j + 2;
                                    while k < chars.len() {
                                        match chars[k] {
                                            '(' => paren_depth += 1,
                                            ')' => {
                                                paren_depth -= 1;
                                                if paren_depth == 0 {
                                                    link_end_idx = Some(k);
                                                    break;
                                                }
                                            }
                                            _ => {}
                                        }
                                        k += 1;
                                    }
                                }
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }

                // If we found a complete link pattern
                if let (Some(text_end_idx), Some(link_start_idx), Some(link_end_idx)) =
                    (text_end_idx, link_start_idx, link_end_idx)
                {
                    // Extract text and link using safe char-based operations
                    let text: String = chars[text_start_idx..text_end_idx].iter().collect();
                    let link: String = chars[link_start_idx..link_end_idx].iter().collect();

                    // Check for spaces at start or end of text
                    if text.starts_with(' ') || text.ends_with(' ') {
                        let trimmed_text = text.trim();
                        if !trimmed_text.is_empty() {
                            // Safely reconstruct the original text using char indices
                            let original: String = chars[i..=link_end_idx].iter().collect();
                            let fixed = format!("[{}]({})", trimmed_text, link);

                            // Calculate the byte position for the column
                            // This is the byte offset of the start of the link
                            let byte_position = chars[..i].iter().collect::<String>().len() + 1;

                            issues.push((byte_position, original, fixed));
                        }
                    }

                    i = link_end_idx + 1;
                    continue;
                }
            }
            i += 1;
        }

        issues
    }
}

impl Rule for MD039NoSpaceInLinks {
    fn name(&self) -> &'static str {
        "MD039"
    }

    fn description(&self) -> &'static str {
        "Spaces inside link text"
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
                        message: "Spaces inside link text should be removed".to_string(),
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

        for i in 0..lines.len() {
            let mut line = lines[i].to_string();
            if !self.is_in_code_block(content, i + 1) {
                for (_, original, fixed) in self.check_line(lines[i]) {
                    // Use a safe replacement method
                    line = line.replace(&original, &fixed);
                }
            }
            result.push_str(&line);
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        Ok(result)
    }
}
