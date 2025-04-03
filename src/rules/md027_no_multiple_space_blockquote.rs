
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug, Default)]
pub struct MD027NoMultipleSpaceBlockquote;

impl MD027NoMultipleSpaceBlockquote {
    fn is_blockquote_line(line: &str) -> bool {
        line.trim_start().starts_with('>')
    }

    fn count_spaces_after_blockquote(line: &str) -> (usize, usize) {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('>') {
            return (0, 0);
        }

        let spaces_before = line.len() - trimmed.len();
        let spaces_after = trimmed[1..]
            .chars()
            .take_while(|c| c.is_whitespace())
            .count();

        (spaces_before, spaces_after)
    }

    fn fix_line(&self, line: &str) -> String {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('>') {
            return line.to_string();
        }

        let spaces_before = line.len() - trimmed.len();
        let content = trimmed[1..].trim_start();
        format!("{}>{}{}",
            " ".repeat(spaces_before),
            if !content.is_empty() { " " } else { "" },
            content
        )
    }
}

impl Rule for MD027NoMultipleSpaceBlockquote {
    fn name(&self) -> &'static str {
        "MD027"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces after blockquote symbol"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            if Self::is_blockquote_line(line) {
                let (_, spaces_after) = Self::count_spaces_after_blockquote(line);
                if spaces_after > 1 || (spaces_after == 1 && line.trim_end() == ">") {
                    warnings.push(LintWarning {
            rule_name: Some(self.name()),
                        message: if line.trim_end() == ">" {
                            "Unnecessary space after empty blockquote symbol".to_string()
                        } else {
                            format!(
                                "Multiple spaces ({}) found after blockquote symbol",
                                spaces_after
                            )
                        },
                        line: line_num + 1,
                        column: line.find('>').unwrap() + 2,
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: 1,
                            replacement: self.fix_line(line),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();

        for line in content.lines() {
            if Self::is_blockquote_line(line) {
                result.push_str(&self.fix_line(line));
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