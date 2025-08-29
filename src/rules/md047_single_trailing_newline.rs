use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

/// Detect the line ending style used in the content
fn detect_line_ending(content: &str) -> &'static str {
    // Check for CRLF first (more specific than LF)
    if content.contains("\r\n") {
        "\r\n"
    } else if content.contains('\n') {
        "\n"
    } else {
        // Default to LF for empty or single-line files
        "\n"
    }
}

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

        // Detect the line ending style used in the document
        let line_ending = detect_line_ending(content);

        // Check if file ends with newline (supporting both LF and CRLF)
        let has_trailing_newline = content.ends_with('\n');

        // Check if file has multiple trailing newlines (supporting both styles)
        let has_multiple_newlines = content.ends_with(&format!("{line_ending}{line_ending}"));

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
                            line_ending.to_string()
                        } else {
                            // Handle the case where content is just whitespace and newlines
                            String::new()
                        }
                    } else {
                        // If there's no newline, add one using the detected line ending style
                        line_ending.to_string()
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

        // Detect the line ending style used in the document
        let line_ending = detect_line_ending(content);

        // Check current state
        let has_trailing_newline = content.ends_with('\n');
        let has_multiple_newlines = content.ends_with(&format!("{line_ending}{line_ending}"));

        // Early return if content is already correct
        if has_trailing_newline && !has_multiple_newlines {
            return Ok(content.to_string());
        }

        // Only allocate when we need to make changes
        if !has_trailing_newline {
            // Content doesn't end with newline, add one using detected style
            let mut result = String::with_capacity(content.len() + line_ending.len());
            result.push_str(content);
            result.push_str(line_ending);
            Ok(result)
        } else {
            // Has multiple newlines, trim them down to just one
            // Need to handle both LF and CRLF when trimming
            let content_without_trailing_newlines = if line_ending == "\r\n" {
                content.trim_end_matches("\r\n")
            } else {
                content.trim_end_matches('\n')
            };

            if content_without_trailing_newlines.is_empty() {
                // Handle the case where content is just newlines
                Ok(line_ending.to_string())
            } else {
                let mut result = String::with_capacity(content_without_trailing_newlines.len() + line_ending.len());
                result.push_str(content_without_trailing_newlines);
                result.push_str(line_ending);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_missing_trailing_newline() {
        let rule = MD047SingleTrailingNewline;
        let content = "Line 1\nLine 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line 1\nLine 2\n");
    }

    #[test]
    fn test_multiple_trailing_newlines() {
        let rule = MD047SingleTrailingNewline;
        let content = "Line 1\nLine 2\n\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line 1\nLine 2\n");
    }

    #[test]
    fn test_crlf_line_ending_preservation() {
        let rule = MD047SingleTrailingNewline;
        // Content with CRLF line endings but missing final newline
        let content = "Line 1\r\nLine 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        let fixed = rule.fix(&ctx).unwrap();
        // Should preserve CRLF style
        assert_eq!(fixed, "Line 1\r\nLine 2\r\n");
        assert!(fixed.ends_with("\r\n"), "Should end with CRLF");
    }

    #[test]
    fn test_crlf_multiple_newlines() {
        let rule = MD047SingleTrailingNewline;
        // Content with CRLF line endings and multiple trailing newlines
        let content = "Line 1\r\nLine 2\r\n\r\n\r\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        let fixed = rule.fix(&ctx).unwrap();
        // Should preserve CRLF style and reduce to single trailing newline
        assert_eq!(fixed, "Line 1\r\nLine 2\r\n");
    }

    #[test]
    fn test_detect_line_ending() {
        assert_eq!(detect_line_ending("Line 1\nLine 2"), "\n");
        assert_eq!(detect_line_ending("Line 1\r\nLine 2"), "\r\n");
        assert_eq!(detect_line_ending("Single line"), "\n");
        assert_eq!(detect_line_ending(""), "\n");

        // Mixed line endings should detect CRLF (first match wins)
        assert_eq!(detect_line_ending("Line 1\r\nLine 2\nLine 3"), "\r\n");
    }

    #[test]
    fn test_blank_file() {
        let rule = MD047SingleTrailingNewline;
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_file_with_only_newlines() {
        let rule = MD047SingleTrailingNewline;
        let content = "\n\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "\n");
    }
}
