use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

/// Rule MD038: No space inside code span markers
///
/// See [docs/md038.md](../../docs/md038.md) for full documentation, configuration, and examples.
///
/// MD038: Spaces inside code span elements
///
/// This rule is triggered when there are spaces inside code span elements.
///
/// For example:
///
/// ``` markdown
/// ` some text`
/// `some text `
/// ` some text `
/// ```
///
/// To fix this issue, remove the leading and trailing spaces within the code span markers:
///
/// ``` markdown
/// `some text`
/// ```
///
/// Note: Code spans containing backticks (e.g., `` `backticks` inside ``) are not flagged
/// to avoid breaking nested backtick structures used to display backticks in documentation.
#[derive(Debug, Clone, Default)]
pub struct MD038NoSpaceInCode {
    pub enabled: bool,
}

impl MD038NoSpaceInCode {
    pub fn new() -> Self {
        Self { enabled: true }
    }

    /// Check if a code span is likely part of a nested backtick structure
    fn is_likely_nested_backticks(&self, ctx: &crate::lint_context::LintContext, span_index: usize) -> bool {
        // If there are multiple code spans on the same line, and there's text
        // between them that contains "code" or other indicators, it's likely nested
        let code_spans = ctx.code_spans();
        let current_span = &code_spans[span_index];
        let current_line = current_span.line;

        // Look for other code spans on the same line
        let same_line_spans: Vec<_> = code_spans
            .iter()
            .enumerate()
            .filter(|(i, s)| s.line == current_line && *i != span_index)
            .collect();

        if same_line_spans.is_empty() {
            return false;
        }

        // Check if there's content between spans that might indicate nesting
        // Get the line content
        let line_idx = current_line - 1; // Convert to 0-based
        if line_idx >= ctx.lines.len() {
            return false;
        }

        let line_content = &ctx.lines[line_idx].content(ctx.content);

        // For each pair of adjacent code spans, check what's between them
        for (_, other_span) in &same_line_spans {
            let start = current_span.end_col.min(other_span.end_col);
            let end = current_span.start_col.max(other_span.start_col);

            if start < end && end <= line_content.len() {
                // Use .get() to safely handle multi-byte UTF-8 characters
                if let Some(between) = line_content.get(start..end) {
                    // If there's text containing "code" or similar patterns between spans,
                    // it's likely they're showing nested backticks
                    if between.contains("code") || between.contains("backtick") {
                        return true;
                    }
                }
            }
        }

        false
    }
}

impl Rule for MD038NoSpaceInCode {
    fn name(&self) -> &'static str {
        "MD038"
    }

    fn description(&self) -> &'static str {
        "Spaces inside code span elements"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Other
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        if !self.enabled {
            return Ok(vec![]);
        }

        let mut warnings = Vec::new();

        // Use centralized code spans from LintContext
        let code_spans = ctx.code_spans();
        for (i, code_span) in code_spans.iter().enumerate() {
            let code_content = &code_span.content;

            // Skip empty code spans
            if code_content.is_empty() {
                continue;
            }

            // Early check: if no leading/trailing whitespace, skip
            let has_leading_space = code_content.chars().next().is_some_and(|c| c.is_whitespace());
            let has_trailing_space = code_content.chars().last().is_some_and(|c| c.is_whitespace());

            if !has_leading_space && !has_trailing_space {
                continue;
            }

            let trimmed = code_content.trim();

            // Check if there are leading or trailing spaces
            if code_content != trimmed {
                // CommonMark behavior: if there is exactly ONE space at start AND ONE at end,
                // and the content after trimming is non-empty, those spaces are stripped.
                // We should NOT flag this case since the spaces are intentionally stripped.
                // See: https://spec.commonmark.org/0.31.2/#code-spans
                //
                // Examples:
                // ` text ` → "text" (spaces stripped, NOT flagged)
                // `  text ` → " text" (extra leading space remains, FLAGGED)
                // ` text  ` → "text " (extra trailing space remains, FLAGGED)
                // ` text` → " text" (no trailing space to balance, FLAGGED)
                // `text ` → "text " (no leading space to balance, FLAGGED)
                if has_leading_space && has_trailing_space && !trimmed.is_empty() {
                    let leading_spaces = code_content.len() - code_content.trim_start().len();
                    let trailing_spaces = code_content.len() - code_content.trim_end().len();

                    // Exactly one space on each side - CommonMark strips them
                    if leading_spaces == 1 && trailing_spaces == 1 {
                        continue;
                    }
                }
                // Check if the content itself contains backticks - if so, skip to avoid
                // breaking nested backtick structures
                if trimmed.contains('`') {
                    continue;
                }

                // Skip inline R code in Quarto/RMarkdown: `r expression`
                // This is a legitimate pattern where space is required after 'r'
                if ctx.flavor == crate::config::MarkdownFlavor::Quarto
                    && trimmed.starts_with('r')
                    && trimmed.len() > 1
                    && trimmed.chars().nth(1).is_some_and(|c| c.is_whitespace())
                {
                    continue;
                }

                // Check if this might be part of a nested backtick structure
                // by looking for other code spans nearby that might indicate nesting
                if self.is_likely_nested_backticks(ctx, i) {
                    continue;
                }

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: code_span.line,
                    column: code_span.start_col + 1, // Convert to 1-indexed
                    end_line: code_span.line,
                    end_column: code_span.end_col, // Don't add 1 to match test expectation
                    message: "Spaces inside code span elements".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: code_span.byte_offset..code_span.byte_end,
                        replacement: format!(
                            "{}{}{}",
                            "`".repeat(code_span.backtick_count),
                            trimmed,
                            "`".repeat(code_span.backtick_count)
                        ),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        if !self.enabled {
            return Ok(content.to_string());
        }

        // Early return if no backticks in content
        if !content.contains('`') {
            return Ok(content.to_string());
        }

        // Get warnings to identify what needs to be fixed
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        // Collect all fixes and sort by position (reverse order to avoid position shifts)
        let mut fixes: Vec<(std::ops::Range<usize>, String)> = warnings
            .into_iter()
            .filter_map(|w| w.fix.map(|f| (f.range, f.replacement)))
            .collect();

        fixes.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));

        // Apply fixes - only allocate string when we have fixes to apply
        let mut result = content.to_string();
        for (range, replacement) in fixes {
            result.replace_range(range, &replacement);
        }

        Ok(result)
    }

    /// Check if content is likely to have code spans
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        !ctx.likely_has_code()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD038NoSpaceInCode { enabled: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_md038_readme_false_positives() {
        // These are the exact cases from README.md that are incorrectly flagged
        let rule = MD038NoSpaceInCode::new();
        let valid_cases = vec![
            "3. `pyproject.toml` (must contain `[tool.rumdl]` section)",
            "#### Effective Configuration (`rumdl config`)",
            "- Blue: `.rumdl.toml`",
            "### Defaults Only (`rumdl config --defaults`)",
        ];

        for case in valid_cases {
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Should not flag code spans without leading/trailing spaces: '{}'. Got {} warnings",
                case,
                result.len()
            );
        }
    }

    #[test]
    fn test_md038_valid() {
        let rule = MD038NoSpaceInCode::new();
        let valid_cases = vec![
            "This is `code` in a sentence.",
            "This is a `longer code span` in a sentence.",
            "This is `code with internal spaces` which is fine.",
            "Code span at `end of line`",
            "`Start of line` code span",
            "Multiple `code spans` in `one line` are fine",
            "Code span with `symbols: !@#$%^&*()`",
            "Empty code span `` is technically valid",
        ];
        for case in valid_cases {
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(result.is_empty(), "Valid case should not have warnings: {case}");
        }
    }

    #[test]
    fn test_md038_invalid() {
        let rule = MD038NoSpaceInCode::new();
        // Flag cases that violate CommonMark:
        // - Space only at start (no matching end space)
        // - Space only at end (no matching start space)
        // - Multiple spaces at start or end (extra space will remain after CommonMark stripping)
        let invalid_cases = vec![
            // Unbalanced: only leading space
            "This is ` code` with leading space.",
            // Unbalanced: only trailing space
            "This is `code ` with trailing space.",
            // Multiple leading spaces (one will remain after CommonMark strips one)
            "This is `  code ` with double leading space.",
            // Multiple trailing spaces (one will remain after CommonMark strips one)
            "This is ` code  ` with double trailing space.",
            // Multiple spaces both sides
            "This is `  code  ` with double spaces both sides.",
        ];
        for case in invalid_cases {
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(!result.is_empty(), "Invalid case should have warnings: {case}");
        }
    }

    #[test]
    fn test_md038_valid_commonmark_stripping() {
        let rule = MD038NoSpaceInCode::new();
        // These cases have exactly ONE space at start AND ONE at end.
        // CommonMark strips both, so these should NOT be flagged.
        // See: https://spec.commonmark.org/0.31.2/#code-spans
        let valid_cases = vec![
            "Type ` y ` to confirm.",
            "Use ` git commit -m \"message\" ` to commit.",
            "The variable ` $HOME ` contains home path.",
            "The pattern ` *.txt ` matches text files.",
            "This is ` random word ` with unnecessary spaces.",
            "Text with ` plain text ` is valid.",
            "Code with ` just code ` here.",
            "Multiple ` word ` spans with ` text ` in one line.",
            "This is ` code ` with both leading and trailing single space.",
            "Use ` - ` as separator.",
        ];
        for case in valid_cases {
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Single space on each side should not be flagged (CommonMark strips them): {case}"
            );
        }
    }

    #[test]
    fn test_md038_fix() {
        let rule = MD038NoSpaceInCode::new();
        // Only cases that violate CommonMark should be fixed
        let test_cases = vec![
            // Unbalanced: only leading space - should be fixed
            (
                "This is ` code` with leading space.",
                "This is `code` with leading space.",
            ),
            // Unbalanced: only trailing space - should be fixed
            (
                "This is `code ` with trailing space.",
                "This is `code` with trailing space.",
            ),
            // Single space on both sides - NOT fixed (valid per CommonMark)
            (
                "This is ` code ` with both spaces.",
                "This is ` code ` with both spaces.", // unchanged
            ),
            // Double leading space - should be fixed
            (
                "This is `  code ` with double leading space.",
                "This is `code` with double leading space.",
            ),
            // Mixed: one valid (single space both), one invalid (trailing only)
            (
                "Multiple ` code ` and `spans ` to fix.",
                "Multiple ` code ` and `spans` to fix.", // only spans is fixed
            ),
        ];
        for (input, expected) in test_cases {
            let ctx = crate::lint_context::LintContext::new(input, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.fix(&ctx).unwrap();
            assert_eq!(result, expected, "Fix did not produce expected output for: {input}");
        }
    }

    #[test]
    fn test_check_invalid_leading_space() {
        let rule = MD038NoSpaceInCode::new();
        let input = "This has a ` leading space` in code";
        let ctx = crate::lint_context::LintContext::new(input, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(result[0].fix.is_some());
    }

    #[test]
    fn test_code_span_parsing_nested_backticks() {
        let content = "Code with ` nested `code` example ` should preserve backticks";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        println!("Content: {content}");
        println!("Code spans found:");
        let code_spans = ctx.code_spans();
        for (i, span) in code_spans.iter().enumerate() {
            println!(
                "  Span {}: line={}, col={}-{}, backticks={}, content='{}'",
                i, span.line, span.start_col, span.end_col, span.backtick_count, span.content
            );
        }

        // This test reveals the issue - we're getting multiple separate code spans instead of one
        assert_eq!(code_spans.len(), 2, "Should parse as 2 code spans");
    }

    #[test]
    fn test_nested_backtick_detection() {
        let rule = MD038NoSpaceInCode::new();

        // Test that code spans with backticks are skipped
        let content = "Code with `` `backticks` inside `` should not be flagged";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Code spans with backticks should be skipped");
    }

    #[test]
    fn test_quarto_inline_r_code() {
        // Test that Quarto-specific R code exception works
        let rule = MD038NoSpaceInCode::new();

        // Test inline R code - should NOT trigger warning in Quarto flavor
        // The key pattern is "r " followed by code
        let content = r#"The result is `r nchar("test")` which equals 4."#;

        // Quarto flavor should allow R code
        let ctx_quarto = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let result_quarto = rule.check(&ctx_quarto).unwrap();
        assert!(
            result_quarto.is_empty(),
            "Quarto inline R code should not trigger warnings. Got {} warnings",
            result_quarto.len()
        );

        // Test that invalid code spans (not matching CommonMark stripping) still get flagged in Quarto
        // Use only trailing space - this violates CommonMark (no balanced stripping)
        let content_other = "This has `plain text ` with trailing space.";
        let ctx_other =
            crate::lint_context::LintContext::new(content_other, crate::config::MarkdownFlavor::Quarto, None);
        let result_other = rule.check(&ctx_other).unwrap();
        assert_eq!(
            result_other.len(),
            1,
            "Quarto should still flag non-R code spans with improper spaces"
        );
    }

    #[test]
    fn test_multibyte_utf8_no_panic() {
        // Regression test: ensure multi-byte UTF-8 characters don't cause panics
        // when checking for nested backticks between code spans.
        // These are real examples from the-art-of-command-line translations.
        let rule = MD038NoSpaceInCode::new();

        // Greek text with code spans
        let greek = "- Χρήσιμα εργαλεία της γραμμής εντολών είναι τα `ping`,` ipconfig`, `traceroute` και `netstat`.";
        let ctx = crate::lint_context::LintContext::new(greek, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Greek text should not panic");

        // Chinese text with code spans
        let chinese = "- 當你需要對文字檔案做集合交、並、差運算時，`sort`/`uniq` 很有幫助。";
        let ctx = crate::lint_context::LintContext::new(chinese, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Chinese text should not panic");

        // Cyrillic/Ukrainian text with code spans
        let cyrillic = "- Основи роботи з файлами: `ls` і `ls -l`, `less`, `head`,` tail` і `tail -f`.";
        let ctx = crate::lint_context::LintContext::new(cyrillic, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Cyrillic text should not panic");

        // Mixed multi-byte with multiple code spans on same line
        let mixed = "使用 `git` 命令和 `npm` 工具来管理项目，可以用 `docker` 容器化。";
        let ctx = crate::lint_context::LintContext::new(mixed, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx);
        assert!(
            result.is_ok(),
            "Mixed Chinese text with multiple code spans should not panic"
        );
    }
}
