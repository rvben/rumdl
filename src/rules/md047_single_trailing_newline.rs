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

        // Check for missing trailing newline
        if !has_trailing_newline {
            let lines = &ctx.lines;
            let last_line_num = lines.len();
            let last_line_content = lines.last().map(|s| s.content.as_str()).unwrap_or("");

            // Calculate precise character range for the end of file
            // For missing newline, highlight the end of the last line
            let (start_line, start_col, end_line, end_col) = (
                last_line_num,
                last_line_content.len() + 1,
                last_line_num,
                last_line_content.len() + 1,
            );

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                message: String::from("File should end with a single newline character"),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                severity: Severity::Warning,
                fix: Some(Fix {
                    // For missing newline, insert at the end of the file
                    range: content.len()..content.len(),
                    // Add newline using the detected line ending style
                    replacement: line_ending.to_string(),
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

        // Check if file already ends with a newline
        let has_trailing_newline = content.ends_with('\n');

        if has_trailing_newline {
            return Ok(content.to_string());
        }

        // Content doesn't end with newline, add one using detected style
        let mut result = String::with_capacity(content.len() + line_ending.len());
        result.push_str(content);
        result.push_str(line_ending);
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
        // Should not trigger when file has trailing newlines
        let rule = MD047SingleTrailingNewline;
        let content = "Line 1\nLine 2\n\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
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
        // Should not trigger when file has CRLF trailing newlines
        let rule = MD047SingleTrailingNewline;
        let content = "Line 1\r\nLine 2\r\n\r\n\r\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
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
        // Should not trigger when file contains only newlines
        let rule = MD047SingleTrailingNewline;
        let content = "\n\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }
}
