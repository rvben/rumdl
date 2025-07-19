use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

/// Rule MD047: File should end with a single newline
///
/// See [docs/md047.md](../../docs/md047.md) for full documentation, configuration, and examples.

#[derive(Debug, Default, Clone)]
pub struct MD047SingleTrailingNewline;

impl Rule for MD047SingleTrailingNewline {
    fn name(&self) -> &'static str {
        "MD047"
    }

    fn description(&self) -> &'static str {
        "Files should end with a single newline character"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();

        // Empty content is fine
        if content.is_empty() {
            return Ok(warnings);
        }

        // Check if file ends with newline
        let has_trailing_newline = content.ends_with('\n');

        // Check if file has multiple trailing newlines
        let has_multiple_newlines = content.ends_with("\n\n");

        // Only issue warning if there's no newline or more than one
        if !has_trailing_newline || has_multiple_newlines {
            let lines = &ctx.lines;
            let last_line_num = lines.len();
            let last_line_content = lines.last().map(|s| s.content.as_str()).unwrap_or("");

            // Calculate precise character range for the end of file
            let (start_line, start_col, end_line, end_col) = if has_multiple_newlines {
                // For multiple newlines, highlight from the end of the last content line to the end
                let last_content_line = content.trim_end_matches('\n');
                let last_content_line_count = last_content_line.lines().count();
                if last_content_line_count == 0 {
                    (1, 1, 1, 2)
                } else {
                    let line_content = last_content_line.lines().last().unwrap_or("");
                    (
                        last_content_line_count,
                        line_content.len() + 1,
                        last_content_line_count,
                        line_content.len() + 2,
                    )
                }
            } else {
                // For missing newline, highlight the end of the last line
                (
                    last_line_num,
                    last_line_content.len() + 1,
                    last_line_num,
                    last_line_content.len() + 1,
                )
            };

            // Only create LineIndex when we actually need it for the fix
            let line_index = LineIndex::new(content.to_string());

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                message: String::from("File should end with a single newline character"),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: if has_trailing_newline {
                        // For multiple newlines, replace from the position to the end of file
                        let start_range = line_index.line_col_to_byte_range_with_length(start_line, start_col, 0);
                        start_range.start..content.len()
                    } else {
                        // For missing newline, insert at the end of the file
                        let end_pos = content.len();
                        end_pos..end_pos
                    },
                    replacement: if has_trailing_newline {
                        // If there are multiple newlines, fix by ensuring just one
                        let trimmed = content.trim_end();
                        if !trimmed.is_empty() {
                            "\n".to_string()
                        } else {
                            // Handle the case where content is just whitespace and newlines
                            String::new()
                        }
                    } else {
                        // If there's no newline, add one to the last line
                        String::from("\n")
                    },
                }),
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Empty content remains empty
        if content.is_empty() {
            return Ok(String::new());
        }

        // Check current state
        let has_trailing_newline = content.ends_with('\n');
        let has_multiple_newlines = content.ends_with("\n\n");

        // Early return if content is already correct
        if has_trailing_newline && !has_multiple_newlines {
            return Ok(content.to_string());
        }

        // Only allocate when we need to make changes
        if !has_trailing_newline {
            // Content doesn't end with newline, add one
            let mut result = String::with_capacity(content.len() + 1);
            result.push_str(content);
            result.push('\n');
            Ok(result)
        } else {
            // Has multiple newlines, trim them down to just one
            let content_without_trailing_newlines = content.trim_end_matches('\n');
            if content_without_trailing_newlines.is_empty() {
                // Handle the case where content is just newlines
                Ok("\n".to_string())
            } else {
                let mut result = String::with_capacity(content_without_trailing_newlines.len() + 1);
                result.push_str(content_without_trailing_newlines);
                result.push('\n');
                Ok(result)
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD047SingleTrailingNewline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_valid_trailing_newline() {
        let rule = MD047SingleTrailingNewline;
        let content = "Line 1\nLine 2\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_missing_trailing_newline() {
        let rule = MD047SingleTrailingNewline;
        let content = "Line 1\nLine 2";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line 1\nLine 2\n");
    }

    #[test]
    fn test_multiple_trailing_newlines() {
        let rule = MD047SingleTrailingNewline;
        let content = "Line 1\nLine 2\n\n\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line 1\nLine 2\n");
    }

    #[test]
    fn test_blank_file() {
        let rule = MD047SingleTrailingNewline;
        let content = "";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_file_with_only_newlines() {
        let rule = MD047SingleTrailingNewline;
        let content = "\n\n\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "\n");
    }
}
