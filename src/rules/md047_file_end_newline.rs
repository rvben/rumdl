use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug, Default)]
pub struct MD047FileEndNewline;

impl Rule for MD047FileEndNewline {
    fn name(&self) -> &'static str {
        "MD047"
    }

    fn description(&self) -> &'static str {
        "Files should end with a single newline character"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        if !lines.is_empty() {
            let has_trailing_newline = content.ends_with('\n');
            let has_multiple_newlines = content.ends_with("\n\n");

            if !has_trailing_newline || has_multiple_newlines {
                warnings.push(LintWarning {
                    message: "File should end with a single newline character".to_string(),
                    line: lines.len(),
                    column: lines.last().map_or(1, |line| line.len() + 1),
                    fix: Some(Fix {
                        line: lines.len(),
                        column: lines.last().map_or(1, |line| line.len() + 1),
                        replacement: "\n".to_string(),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = content.to_string();

        // If the content doesn't end with a newline, add one
        if !result.ends_with('\n') {
            result.push('\n');
            return Ok(result);
        }

        // If the content has multiple trailing newlines, remove extras
        while result.ends_with("\n\n") {
            result.pop();
        }

        Ok(result)
    }
} 