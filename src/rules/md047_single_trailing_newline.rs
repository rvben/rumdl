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
        let line_index = LineIndex::new(content.to_string());
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
            let lines: Vec<&str> = content.lines().collect();
            let last_line = lines.len();
            let last_column = lines.last().map_or(1, |line| line.len() + 1);

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                message: String::from("File should end with a single newline character"),
                line: last_line,
                column: last_column,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: line_index.line_col_to_byte_range(last_line, last_column),
                    replacement: if has_trailing_newline {
                        // If there are multiple newlines, fix by ensuring just one
                        let trimmed = content.trim_end();
                        if !trimmed.is_empty() {
                            trimmed.to_string() + "\n"
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
            return Ok(content.to_string());
        }

        // If the content doesn't end with a newline, add one
        if !content.ends_with('\n') {
            return Ok(content.to_string() + "\n");
        }

        // Handle multiple trailing newlines
        let mut result = content.to_string();

        // If there are multiple newlines, trim them down to just one
        if content.ends_with("\n\n") {
            // Preserve any whitespace at the end but only have one newline
            let content_without_trailing_newlines = content.trim_end_matches('\n');
            result = content_without_trailing_newlines.to_string() + "\n";
        }

        Ok(result)
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
