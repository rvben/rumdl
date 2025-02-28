use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug, Default)]
pub struct MD032BlanksAroundLists;

impl MD032BlanksAroundLists {
    fn is_list_item(line: &str) -> bool {
        let list_re = Regex::new(r"^(\s*)([-*+]|\d+\.)\s").unwrap();
        list_re.is_match(line)
    }

    fn is_empty_line(line: &str) -> bool {
        line.trim().is_empty()
    }
}

impl Rule for MD032BlanksAroundLists {
    fn name(&self) -> &'static str {
        "MD032"
    }

    fn description(&self) -> &'static str {
        "Lists should be surrounded by blank lines"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            if Self::is_list_item(line) {
                // Check if this is the start of a list
                if i > 0 && !Self::is_list_item(lines[i - 1]) && !Self::is_empty_line(lines[i - 1]) {
                    warnings.push(LintWarning {
                        message: "List should be preceded by a blank line".to_string(),
                        line: i + 1,
                        column: 1,
                        fix: Some(Fix {
                            line: i + 1,
                            column: 1,
                            replacement: format!("\n{}", line),
                        }),
                    });
                }

                // Check if this is the end of a list
                if i < lines.len() - 1 && !Self::is_list_item(lines[i + 1]) && !Self::is_empty_line(lines[i + 1]) {
                    warnings.push(LintWarning {
                        message: "List should be followed by a blank line".to_string(),
                        line: i + 2,
                        column: 1,
                        fix: Some(Fix {
                            line: i + 2,
                            column: 1,
                            replacement: format!("\n{}", lines[i + 1]),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            if Self::is_list_item(lines[i]) {
                // Add blank line before list if needed
                if i > 0 && !Self::is_list_item(lines[i - 1]) && !Self::is_empty_line(lines[i - 1]) {
                    result.push('\n');
                }

                // Add the list item
                result.push_str(lines[i]);
                result.push('\n');

                // Add blank line after list if needed
                if i < lines.len() - 1 && !Self::is_list_item(lines[i + 1]) && !Self::is_empty_line(lines[i + 1]) {
                    result.push('\n');
                }
            } else {
                result.push_str(lines[i]);
                result.push('\n');
            }
            i += 1;
        }

        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 