use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::mkdocs_extensions::is_inline_hilite_content;

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

    /// Check if a code span is part of Hugo template syntax (e.g., {{raw `...`}})
    ///
    /// Hugo static site generator uses backticks as part of template delimiters,
    /// not markdown code spans. This function detects common Hugo shortcode patterns:
    /// - {{raw `...`}} - Raw HTML shortcode
    /// - {{< `...` >}} - Partial shortcode
    /// - {{% `...` %}} - Shortcode with percent delimiters
    /// - {{ `...` }} - Generic shortcode
    ///
    /// The detection is conservative to avoid false positives:
    /// - Requires opening {{ pattern before the backtick
    /// - Requires closing }} after the code span
    /// - Handles multi-line templates correctly
    ///
    /// Returns true if the code span is part of Hugo template syntax and should be skipped.
    fn is_hugo_template_syntax(
        &self,
        ctx: &crate::lint_context::LintContext,
        code_span: &crate::lint_context::CodeSpan,
    ) -> bool {
        let start_line_idx = code_span.line.saturating_sub(1);
        if start_line_idx >= ctx.lines.len() {
            return false;
        }

        let start_line_content = ctx.lines[start_line_idx].content(ctx.content);

        // start_col is 0-indexed character position
        let span_start_col = code_span.start_col;

        // Check if there's Hugo template syntax before the code span on the same line
        // Pattern: {{raw ` or {{< ` or similar Hugo template patterns
        // The code span starts at the backtick, so we need to check what's before it
        // span_start_col is the position of the backtick (0-indexed character position)
        // Minimum pattern is "{{ `" which has 3 characters before the backtick
        if span_start_col >= 3 {
            // Look backwards for Hugo template patterns
            // Get the content up to (but not including) the backtick
            let before_span: String = start_line_content.chars().take(span_start_col).collect();

            // Check for Hugo template patterns: {{raw `, {{< `, {{% `, etc.
            // The backtick is at span_start_col, so we check if the content before it
            // ends with the Hugo pattern (without the backtick), and verify the next char is a backtick
            let char_at_span_start = start_line_content.chars().nth(span_start_col).unwrap_or(' ');

            // Match Hugo shortcode patterns:
            // - {{raw ` - Raw HTML shortcode
            // - {{< ` - Partial shortcode (may have parameters before backtick)
            // - {{% ` - Shortcode with percent delimiters
            // - {{ ` - Generic shortcode
            // Also handle cases with parameters: {{< highlight go ` or {{< code ` etc.
            // We check if the pattern starts with {{ and contains the shortcode type before the backtick
            let is_hugo_start =
                // Exact match: {{raw `
                (before_span.ends_with("{{raw ") && char_at_span_start == '`')
                // Partial shortcode: {{< ` or {{< name ` or {{< name param ` etc.
                || (before_span.starts_with("{{<") && before_span.ends_with(' ') && char_at_span_start == '`')
                // Percent shortcode: {{% `
                || (before_span.ends_with("{{% ") && char_at_span_start == '`')
                // Generic shortcode: {{ `
                || (before_span.ends_with("{{ ") && char_at_span_start == '`');

            if is_hugo_start {
                // Check if there's a closing }} after the code span
                // First check the end line of the code span
                let end_line_idx = code_span.end_line.saturating_sub(1);
                if end_line_idx < ctx.lines.len() {
                    let end_line_content = ctx.lines[end_line_idx].content(ctx.content);
                    let end_line_char_count = end_line_content.chars().count();
                    let span_end_col = code_span.end_col.min(end_line_char_count);

                    // Check for closing }} on the same line as the end of the code span
                    if span_end_col < end_line_char_count {
                        let after_span: String = end_line_content.chars().skip(span_end_col).collect();
                        if after_span.trim_start().starts_with("}}") {
                            return true;
                        }
                    }

                    // Also check the next line for closing }}
                    let next_line_idx = code_span.end_line;
                    if next_line_idx < ctx.lines.len() {
                        let next_line = ctx.lines[next_line_idx].content(ctx.content);
                        if next_line.trim_start().starts_with("}}") {
                            return true;
                        }
                    }
                }
            }
        }

        false
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

                // Skip InlineHilite syntax in MkDocs: `#!python code`
                // The space after the language specifier is legitimate
                if ctx.flavor == crate::config::MarkdownFlavor::MkDocs && is_inline_hilite_content(trimmed) {
                    continue;
                }

                // Check if this is part of Hugo template syntax (e.g., {{raw `...`}})
                // Hugo uses backticks as part of template delimiters, not markdown code spans
                if self.is_hugo_template_syntax(ctx, code_span) {
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

    /// Comprehensive tests for Hugo template syntax detection
    ///
    /// These tests ensure MD038 correctly handles Hugo template syntax patterns
    /// without false positives, while maintaining correct detection of actual
    /// code span spacing issues.
    #[test]
    fn test_hugo_template_syntax_comprehensive() {
        let rule = MD038NoSpaceInCode::new();

        // ===== VALID HUGO TEMPLATE SYNTAX (Should NOT trigger warnings) =====

        // Basic Hugo shortcode patterns
        let valid_hugo_cases = vec![
            // Raw HTML shortcode
            (
                "{{raw `\n\tgo list -f '{{.DefaultGODEBUG}}' my/main/package\n`}}",
                "Multi-line raw shortcode",
            ),
            (
                "Some text {{raw ` code `}} more text",
                "Inline raw shortcode with spaces",
            ),
            ("{{raw `code`}}", "Raw shortcode without spaces"),
            // Partial shortcode
            ("{{< ` code ` >}}", "Partial shortcode with spaces"),
            ("{{< `code` >}}", "Partial shortcode without spaces"),
            // Shortcode with percent
            ("{{% ` code ` %}}", "Percent shortcode with spaces"),
            ("{{% `code` %}}", "Percent shortcode without spaces"),
            // Generic shortcode
            ("{{ ` code ` }}", "Generic shortcode with spaces"),
            ("{{ `code` }}", "Generic shortcode without spaces"),
            // Shortcodes with parameters (common Hugo pattern)
            ("{{< highlight go `code` >}}", "Shortcode with highlight parameter"),
            ("{{< code `go list` >}}", "Shortcode with code parameter"),
            // Multi-line Hugo templates
            ("{{raw `\n\tcommand here\n\tmore code\n`}}", "Multi-line raw template"),
            ("{{< highlight `\ncode here\n` >}}", "Multi-line highlight template"),
            // Hugo templates with nested Go template syntax
            (
                "{{raw `\n\t{{.Variable}}\n\t{{range .Items}}\n`}}",
                "Nested Go template syntax",
            ),
            // Edge case: Hugo template at start of line
            ("{{raw `code`}}", "Hugo template at line start"),
            // Edge case: Hugo template at end of line
            ("Text {{raw `code`}}", "Hugo template at end of line"),
            // Edge case: Multiple Hugo templates
            ("{{raw `code1`}} and {{raw `code2`}}", "Multiple Hugo templates"),
        ];

        for (case, description) in valid_hugo_cases {
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Hugo template syntax should not trigger MD038 warnings: {description} - {case}"
            );
        }

        // ===== FALSE POSITIVE PREVENTION (Non-Hugo asymmetric spaces should be flagged) =====

        // These have asymmetric spaces (leading-only or trailing-only) and should be flagged
        // Per CommonMark spec: symmetric single-space pairs are stripped and NOT flagged
        let should_be_flagged = vec![
            ("This is ` code` with leading space.", "Leading space only"),
            ("This is `code ` with trailing space.", "Trailing space only"),
            ("Text `  code ` here", "Extra leading space (asymmetric)"),
            ("Text ` code  ` here", "Extra trailing space (asymmetric)"),
            ("Text `  code` here", "Double leading, no trailing"),
            ("Text `code  ` here", "No leading, double trailing"),
        ];

        for (case, description) in should_be_flagged {
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                !result.is_empty(),
                "Should flag asymmetric space code spans: {description} - {case}"
            );
        }

        // ===== COMMONMARK SYMMETRIC SPACE BEHAVIOR (Should NOT be flagged) =====

        // Per CommonMark 0.31.2: When a code span has exactly one space at start AND end,
        // those spaces are stripped from the output. This is intentional, not an error.
        // These cases should NOT trigger MD038.
        let symmetric_single_space = vec![
            ("Text ` code ` here", "Symmetric single space - CommonMark strips"),
            ("{raw ` code `}", "Looks like Hugo but missing opening {{"),
            ("raw ` code `}}", "Missing opening {{ - but symmetric spaces"),
        ];

        for (case, description) in symmetric_single_space {
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "CommonMark symmetric spaces should NOT be flagged: {description} - {case}"
            );
        }

        // ===== EDGE CASES: Unicode and Special Characters =====

        let unicode_cases = vec![
            ("{{raw `\n\t你好世界\n`}}", "Unicode in Hugo template"),
            ("{{raw `\n\t🎉 emoji\n`}}", "Emoji in Hugo template"),
            ("{{raw `\n\tcode with \"quotes\"\n`}}", "Quotes in Hugo template"),
            (
                "{{raw `\n\tcode with 'single quotes'\n`}}",
                "Single quotes in Hugo template",
            ),
        ];

        for (case, description) in unicode_cases {
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Hugo templates with special characters should not trigger warnings: {description} - {case}"
            );
        }

        // ===== BOUNDARY CONDITIONS =====

        // Minimum valid Hugo pattern
        assert!(
            rule.check(&crate::lint_context::LintContext::new(
                "{{ ` ` }}",
                crate::config::MarkdownFlavor::Standard,
                None
            ))
            .unwrap()
            .is_empty(),
            "Minimum Hugo pattern should be valid"
        );

        // Hugo template with only whitespace
        assert!(
            rule.check(&crate::lint_context::LintContext::new(
                "{{raw `\n\t\n`}}",
                crate::config::MarkdownFlavor::Standard,
                None
            ))
            .unwrap()
            .is_empty(),
            "Hugo template with only whitespace should be valid"
        );
    }

    /// Test interaction with other markdown elements
    #[test]
    fn test_hugo_template_with_other_markdown() {
        let rule = MD038NoSpaceInCode::new();

        // Hugo template inside a list
        let content = r#"1. First item
2. Second item with {{raw `code`}} template
3. Third item"#;
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Hugo template in list should not trigger warnings");

        // Hugo template in blockquote
        let content = r#"> Quote with {{raw `code`}} template"#;
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Hugo template in blockquote should not trigger warnings"
        );

        // Hugo template near regular code span (should flag the regular one)
        let content = r#"{{raw `code`}} and ` bad code` here"#;
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag regular code span but not Hugo template");
    }

    /// Performance test: Many Hugo templates
    #[test]
    fn test_hugo_template_performance() {
        let rule = MD038NoSpaceInCode::new();

        // Create content with many Hugo templates
        let mut content = String::new();
        for i in 0..100 {
            content.push_str(&format!("{{{{raw `code{i}\n`}}}}\n"));
        }

        let ctx = crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let start = std::time::Instant::now();
        let result = rule.check(&ctx).unwrap();
        let duration = start.elapsed();

        assert!(result.is_empty(), "Many Hugo templates should not trigger warnings");
        assert!(
            duration.as_millis() < 1000,
            "Performance test: Should process 100 Hugo templates in <1s, took {duration:?}"
        );
    }

    #[test]
    fn test_mkdocs_inline_hilite_not_flagged() {
        // InlineHilite syntax: `#!language code` should NOT be flagged
        // The space after the language specifier is legitimate
        let rule = MD038NoSpaceInCode::new();

        let valid_cases = vec![
            "`#!python print('hello')`",
            "`#!js alert('hi')`",
            "`#!c++ cout << x;`",
            "Use `#!python import os` to import modules",
            "`#!bash echo $HOME`",
        ];

        for case in valid_cases {
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::MkDocs, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "InlineHilite syntax should not be flagged in MkDocs: {case}"
            );
        }

        // Test that InlineHilite IS flagged in Standard flavor (not MkDocs-aware)
        let content = "`#!python print('hello')`";
        let ctx_standard =
            crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result_standard = rule.check(&ctx_standard).unwrap();
        // In standard flavor, the content " print('hello')" has no special meaning
        // But since "#!python print('hello')" doesn't have leading/trailing spaces, it's valid!
        assert!(
            result_standard.is_empty(),
            "InlineHilite with no extra spaces should not be flagged even in Standard flavor"
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
