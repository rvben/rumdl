use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

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
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        if !lines.is_empty() {
            let has_trailing_newline = content.ends_with('\n');
            let has_multiple_newlines = content.ends_with("\n\n");

            if !has_trailing_newline || has_multiple_newlines {
                let last_line = lines.len();
                let last_column = lines.last().map_or(1, |line| line.len() + 1);

                warnings.push(LintWarning {
                    message: String::from("File should end with a single newline character"),
                    line: last_line + 1,
                    column: last_column,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(last_line + 1, last_column),
                        replacement: if has_trailing_newline {
                            // If there are multiple newlines, trim them and add a single one
                            content.trim_end().to_string() + "\n"
                        } else {
                            // If there's no newline, add one to the last line
                            lines.last().unwrap_or(&"").to_string() + "\n"
                        },
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

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
