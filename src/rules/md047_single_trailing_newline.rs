use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

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

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip empty files - they don't need trailing newlines
        ctx.content.is_empty()
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();

        // Empty content is fine
        if content.is_empty() {
            return Ok(warnings);
        }

        // Content has been normalized to LF at I/O boundary
        // Check if file ends with newline
        let has_trailing_newline = content.ends_with('\n');

        // Check for missing trailing newline
        if !has_trailing_newline {
            let lines = &ctx.lines;
            let last_line_num = lines.len();
            let last_line_content = lines.last().map_or("", |s| s.content(content));

            // Calculate precise character range for the end of file
            // For missing newline, highlight the end of the last line
            let (start_line, start_col, end_line, end_col) = (
                last_line_num,
                last_line_content.len() + 1,
                last_line_num,
                last_line_content.len() + 1,
            );

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                message: String::from("File should end with a single newline character"),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                severity: Severity::Warning,
                fix: Some(Fix::new(content.len()..content.len(), "\n".to_string())),
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings).map_err(LintError::InvalidInput)
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_missing_trailing_newline() {
        let rule = MD047SingleTrailingNewline;
        let content = "Line 1\nLine 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_normalized_lf_content() {
        // In production, content is normalized to LF before rules see it
        // This test reflects the actual runtime behavior
        let rule = MD047SingleTrailingNewline;
        let content = "Line 1\nLine 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        let fixed = rule.fix(&ctx).unwrap();
        // Rule always adds LF - I/O boundary converts to CRLF if needed
        assert_eq!(fixed, "Line 1\nLine 2\n");
        assert!(fixed.ends_with('\n'), "Should end with LF");
    }

    #[test]
    fn test_blank_file() {
        let rule = MD047SingleTrailingNewline;
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_file_with_only_newlines() {
        // Should not trigger when file contains only newlines
        let rule = MD047SingleTrailingNewline;
        let content = "\n\n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    /// Roundtrip safety: applying check()'s Fix structs via apply_warning_fixes
    /// must produce the same result as fix(). This guards against check/fix divergence.
    fn assert_check_fix_roundtrip(content: &str) {
        let rule = MD047SingleTrailingNewline;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        let fixed_via_fix = rule.fix(&ctx).unwrap();

        // Apply fixes from check() warnings directly
        let fixed_via_check = if warnings.is_empty() {
            content.to_string()
        } else {
            crate::utils::fix_utils::apply_warning_fixes(content, &warnings).unwrap()
        };

        assert_eq!(
            fixed_via_check, fixed_via_fix,
            "check() Fix structs and fix() must produce identical results for content: {content:?}"
        );
    }

    #[test]
    fn test_roundtrip_missing_newline() {
        assert_check_fix_roundtrip("Line 1\nLine 2");
    }

    #[test]
    fn test_roundtrip_single_trailing_newline() {
        assert_check_fix_roundtrip("Line 1\nLine 2\n");
    }

    #[test]
    fn test_roundtrip_multiple_trailing_newlines() {
        assert_check_fix_roundtrip("Line 1\nLine 2\n\n\n");
    }

    #[test]
    fn test_roundtrip_empty_content() {
        assert_check_fix_roundtrip("");
    }

    #[test]
    fn test_roundtrip_only_newlines() {
        assert_check_fix_roundtrip("\n\n\n");
    }

    #[test]
    fn test_roundtrip_single_line_no_newline() {
        assert_check_fix_roundtrip("Single line");
    }

    #[test]
    fn test_roundtrip_unicode_content() {
        // Multi-byte UTF-8 characters - ensure byte offsets in Fix are correct
        assert_check_fix_roundtrip("Héllo wörld 日本語");
    }

    #[test]
    fn test_roundtrip_inline_disable_on_last_line() {
        // Inline disable should suppress the fix
        let content = "Line 1\nLine 2 <!-- rumdl-disable-line MD047 -->";
        let rule = MD047SingleTrailingNewline;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Inline disable on last line should prevent the fix");
    }
}
