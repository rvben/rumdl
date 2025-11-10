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

        let line_content = &ctx.lines[line_idx].content;

        // For each pair of adjacent code spans, check what's between them
        for (_, other_span) in &same_line_spans {
            let start = current_span.end_col.min(other_span.end_col);
            let end = current_span.start_col.max(other_span.start_col);

            if start < end && end <= line_content.len() {
                let between = &line_content[start..end];
                // If there's text containing "code" or similar patterns between spans,
                // it's likely they're showing nested backticks
                if between.contains("code") || between.contains("backtick") {
                    return true;
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
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard);
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
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard);
            let result = rule.check(&ctx).unwrap();
            assert!(result.is_empty(), "Valid case should not have warnings: {case}");
        }
    }

    #[test]
    fn test_md038_invalid() {
        let rule = MD038NoSpaceInCode::new();
        // All spaces should be flagged (matching markdownlint behavior)
        let invalid_cases = vec![
            "Type ` y ` to confirm.",
            "Use ` git commit -m \"message\" ` to commit.",
            "The variable ` $HOME ` contains home path.",
            "The pattern ` *.txt ` matches text files.",
            "This is ` random word ` with unnecessary spaces.",
            "Text with ` plain text ` should be flagged.",
            "Code with ` just code ` here.",
            "Multiple ` word ` spans with ` text ` in one line.",
            "This is ` code` with leading space.",
            "This is `code ` with trailing space.",
            "This is ` code ` with both leading and trailing space.",
        ];
        for case in invalid_cases {
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard);
            let result = rule.check(&ctx).unwrap();
            assert!(!result.is_empty(), "Invalid case should have warnings: {case}");
        }
    }

    #[test]
    fn test_md038_fix() {
        let rule = MD038NoSpaceInCode::new();
        let test_cases = vec![
            (
                "This is ` code` with leading space.",
                "This is `code` with leading space.",
            ),
            (
                "This is `code ` with trailing space.",
                "This is `code` with trailing space.",
            ),
            ("This is ` code ` with both spaces.", "This is `code` with both spaces."),
            (
                "Multiple ` code ` and `spans ` to fix.",
                "Multiple `code` and `spans` to fix.",
            ),
        ];
        for (input, expected) in test_cases {
            let ctx = crate::lint_context::LintContext::new(input, crate::config::MarkdownFlavor::Standard);
            let result = rule.fix(&ctx).unwrap();
            assert_eq!(result, expected, "Fix did not produce expected output for: {input}");
        }
    }

    #[test]
    fn test_check_invalid_leading_space() {
        let rule = MD038NoSpaceInCode::new();
        let input = "This has a ` leading space` in code";
        let ctx = crate::lint_context::LintContext::new(input, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(result[0].fix.is_some());
    }

    #[test]
    fn test_code_span_parsing_nested_backticks() {
        let content = "Code with ` nested `code` example ` should preserve backticks";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);

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
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
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
        let ctx_quarto = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Quarto);
        let result_quarto = rule.check(&ctx_quarto).unwrap();
        assert!(
            result_quarto.is_empty(),
            "Quarto inline R code should not trigger warnings. Got {} warnings",
            result_quarto.len()
        );

        // Test that other code with spaces still gets flagged in Quarto
        let content_other = "This has ` plain text ` with spaces.";
        let ctx_other = crate::lint_context::LintContext::new(content_other, crate::config::MarkdownFlavor::Quarto);
        let result_other = rule.check(&ctx_other).unwrap();
        assert_eq!(
            result_other.len(),
            1,
            "Quarto should still flag non-R code spans with improper spaces"
        );
    }
}
