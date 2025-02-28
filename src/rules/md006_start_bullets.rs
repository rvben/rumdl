use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug, Default)]
pub struct MD006StartBullets;

impl MD006StartBullets {
    fn is_bullet_list_marker(line: &str) -> bool {
        let trimmed = line.trim_start();
        if let Some(c) = trimmed.chars().next() {
            if c == '*' || c == '-' || c == '+' {
                return trimmed.len() == 1 || trimmed.chars().nth(1).map_or(false, |c| c.is_whitespace());
            }
        }
        false
    }

    fn get_indent_level(line: &str) -> usize {
        line.len() - line.trim_start().len()
    }

    fn is_list_continuation(line: &str) -> bool {
        let indent = Self::get_indent_level(line);
        indent > 0 && !line.trim().is_empty() && !Self::is_bullet_list_marker(line)
    }

    fn is_nested_list(indent: usize, parent_indent: usize) -> bool {
        indent > parent_indent && indent >= 2
    }

    fn get_list_context(line: &str, prev_indent: usize, prev_line_empty: bool) -> (bool, usize) {
        if !Self::is_bullet_list_marker(line) {
            return (false, prev_indent);
        }

        let indent = Self::get_indent_level(line);
        if indent == 0 {
            (true, 0)
        } else if !prev_line_empty && Self::is_nested_list(indent, prev_indent) {
            (true, prev_indent)
        } else {
            (false, 0)
        }
    }
}

impl Rule for MD006StartBullets {
    fn name(&self) -> &'static str {
        "MD006"
    }

    fn description(&self) -> &'static str {
        "Consider starting bulleted lists at the beginning of the line"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut prev_indent = 0;
        let mut prev_line_empty = true;

        for (line_num, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                prev_line_empty = true;
                continue;
            }

            if Self::is_bullet_list_marker(line) {
                let indent = Self::get_indent_level(line);
                let (is_valid, new_indent) = Self::get_list_context(line, prev_indent, prev_line_empty);

                if !is_valid && indent > 0 {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: 1,
                        message: "Top-level lists should not be indented".to_string(),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: 1,
                            replacement: line.trim_start().to_string(),
                        }),
                    });
                }

                prev_indent = new_indent;
            } else if !Self::is_list_continuation(line) {
                prev_indent = 0;
            }
            prev_line_empty = false;
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut prev_indent = 0;
        let mut prev_line_empty = true;

        for line in content.lines() {
            if line.trim().is_empty() {
                prev_line_empty = true;
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if Self::is_bullet_list_marker(line) {
                let indent = Self::get_indent_level(line);
                let (is_valid, new_indent) = Self::get_list_context(line, prev_indent, prev_line_empty);

                if !is_valid && indent > 0 {
                    result.push_str(line.trim_start());
                } else {
                    result.push_str(line);
                }

                prev_indent = new_indent;
            } else {
                if !Self::is_list_continuation(line) {
                    prev_indent = 0;
                }
                result.push_str(line);
            }
            result.push('\n');
            prev_line_empty = false;
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
}
