/// Rule MD037: No spaces around emphasis markers
///
/// See [docs/md037.md](../../docs/md037.md) for full documentation, configuration, and examples.
use crate::filtered_lines::FilteredLinesExt;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::emphasis_utils::{
    EmphasisSpan, find_emphasis_markers, find_emphasis_spans, has_doc_patterns, replace_inline_code,
    replace_inline_math,
};
use crate::utils::kramdown_utils::has_span_ial;
use crate::utils::regex_cache::UNORDERED_LIST_MARKER_REGEX;
use crate::utils::skip_context::{
    is_in_html_comment, is_in_jsx_expression, is_in_math_context, is_in_mdx_comment, is_in_mkdocs_markup,
    is_in_table_cell,
};

/// Check if an emphasis span has spacing issues that should be flagged
#[inline]
fn has_spacing_issues(span: &EmphasisSpan) -> bool {
    span.has_leading_space || span.has_trailing_space
}

/// Truncate long text for display in warning messages
/// Shows first ~30 and last ~30 chars with ellipsis in middle for readability
#[inline]
fn truncate_for_display(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }

    let prefix_len = max_len / 2 - 2; // -2 for "..."
    let suffix_len = max_len / 2 - 2;

    // Use floor_char_boundary to safely find UTF-8 character boundaries
    let prefix_end = text.floor_char_boundary(prefix_len.min(text.len()));
    let suffix_start = text.floor_char_boundary(text.len().saturating_sub(suffix_len));

    format!("{}...{}", &text[..prefix_end], &text[suffix_start..])
}

/// Rule MD037: Spaces inside emphasis markers
#[derive(Clone)]
pub struct MD037NoSpaceInEmphasis;

impl Default for MD037NoSpaceInEmphasis {
    fn default() -> Self {
        Self
    }
}

impl MD037NoSpaceInEmphasis {
    /// Check if a byte position is within a link (inline links, reference links, or reference definitions)
    fn is_in_link(&self, ctx: &crate::lint_context::LintContext, byte_pos: usize) -> bool {
        // Check inline and reference links
        for link in &ctx.links {
            if link.byte_offset <= byte_pos && byte_pos < link.byte_end {
                return true;
            }
        }

        // Check images (which use similar syntax)
        for image in &ctx.images {
            if image.byte_offset <= byte_pos && byte_pos < image.byte_end {
                return true;
            }
        }

        // Check reference definitions [ref]: url "title" using pre-computed data (O(1) vs O(n))
        ctx.is_in_reference_def(byte_pos)
    }
}

impl Rule for MD037NoSpaceInEmphasis {
    fn name(&self) -> &'static str {
        "MD037"
    }

    fn description(&self) -> &'static str {
        "Spaces inside emphasis markers"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _timer = crate::profiling::ScopedTimer::new("MD037_check");

        // Early return: if no emphasis markers at all, skip processing
        if !content.contains('*') && !content.contains('_') {
            return Ok(vec![]);
        }

        // Create LineIndex for correct byte position calculations across all line ending types
        let line_index = &ctx.line_index;

        let mut warnings = Vec::new();

        // Process content lines, automatically skipping front matter, code blocks, and math blocks
        // Math blocks contain LaTeX syntax where _ and * have special meaning
        for line in ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .skip_math_blocks()
        {
            // Skip if the line doesn't contain any emphasis markers
            if !line.content.contains('*') && !line.content.contains('_') {
                continue;
            }

            // Check for emphasis issues on the original line
            self.check_line_for_emphasis_issues_fast(line.content, line.line_num, &mut warnings);
        }

        // Filter out warnings for emphasis markers that are inside links, HTML comments, math, or MkDocs markup
        let mut filtered_warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1;
            let line_start_pos = line_index.get_line_start_byte(line_num).unwrap_or(0);

            // Find warnings for this line
            for warning in &warnings {
                if warning.line == line_num {
                    // Calculate byte position of the warning
                    let byte_pos = line_start_pos + (warning.column - 1);
                    // Calculate position within the line (0-indexed)
                    let line_pos = warning.column - 1;

                    // Skip if inside links, HTML comments, math contexts, tables, code spans, MDX constructs, or MkDocs markup
                    // Note: is_in_code_span uses pulldown-cmark and correctly handles multi-line spans
                    if !self.is_in_link(ctx, byte_pos)
                        && !is_in_html_comment(content, byte_pos)
                        && !is_in_math_context(ctx, byte_pos)
                        && !is_in_table_cell(ctx, line_num, warning.column)
                        && !ctx.is_in_code_span(line_num, warning.column)
                        && !is_in_jsx_expression(ctx, byte_pos)
                        && !is_in_mdx_comment(ctx, byte_pos)
                        && !is_in_mkdocs_markup(line, line_pos, ctx.flavor)
                    {
                        let mut adjusted_warning = warning.clone();
                        if let Some(fix) = &mut adjusted_warning.fix {
                            // Convert line-relative range to absolute range
                            let abs_start = line_start_pos + fix.range.start;
                            let abs_end = line_start_pos + fix.range.end;
                            fix.range = abs_start..abs_end;
                        }
                        filtered_warnings.push(adjusted_warning);
                    }
                }
            }
        }

        Ok(filtered_warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _timer = crate::profiling::ScopedTimer::new("MD037_fix");

        // Fast path: if no emphasis markers, return unchanged
        if !content.contains('*') && !content.contains('_') {
            return Ok(content.to_string());
        }

        // First check for issues and get all warnings with fixes
        let warnings = self.check(ctx)?;

        // If no warnings, return original content
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        // Apply fixes
        let mut result = content.to_string();
        let mut offset: isize = 0;

        // Sort warnings by position to apply fixes in the correct order
        let mut sorted_warnings: Vec<_> = warnings.iter().filter(|w| w.fix.is_some()).collect();
        sorted_warnings.sort_by_key(|w| (w.line, w.column));

        for warning in sorted_warnings {
            if let Some(fix) = &warning.fix {
                // Apply fix with offset adjustment
                let actual_start = (fix.range.start as isize + offset) as usize;
                let actual_end = (fix.range.end as isize + offset) as usize;

                // Make sure we're not out of bounds
                if actual_start < result.len() && actual_end <= result.len() {
                    // Replace the text
                    result.replace_range(actual_start..actual_end, &fix.replacement);
                    // Update offset for future replacements
                    offset += fix.replacement.len() as isize - (fix.range.end - fix.range.start) as isize;
                }
            }
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Emphasis
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.likely_has_emphasis()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD037NoSpaceInEmphasis)
    }
}

impl MD037NoSpaceInEmphasis {
    /// Optimized line checking for emphasis spacing issues
    #[inline]
    fn check_line_for_emphasis_issues_fast(&self, line: &str, line_num: usize, warnings: &mut Vec<LintWarning>) {
        // Quick documentation pattern checks
        if has_doc_patterns(line) {
            return;
        }

        // Optimized list detection with fast path
        // When a list marker is detected, ALWAYS check only the content after the marker,
        // never the full line. This prevents the list marker (* + -) from being mistaken
        // for emphasis markers.
        if (line.starts_with(' ') || line.starts_with('*') || line.starts_with('+') || line.starts_with('-'))
            && UNORDERED_LIST_MARKER_REGEX.is_match(line)
        {
            if let Some(caps) = UNORDERED_LIST_MARKER_REGEX.captures(line)
                && let Some(full_match) = caps.get(0)
            {
                let list_marker_end = full_match.end();
                if list_marker_end < line.len() {
                    let remaining_content = &line[list_marker_end..];

                    // Always check just the remaining content (after the list marker).
                    // The list marker itself is never emphasis.
                    self.check_line_content_for_emphasis_fast(remaining_content, line_num, list_marker_end, warnings);
                }
            }
            return;
        }

        // Check the entire line
        self.check_line_content_for_emphasis_fast(line, line_num, 0, warnings);
    }

    /// Optimized line content checking for emphasis issues
    fn check_line_content_for_emphasis_fast(
        &self,
        content: &str,
        line_num: usize,
        offset: usize,
        warnings: &mut Vec<LintWarning>,
    ) {
        // Replace inline code and inline math to avoid false positives
        // with emphasis markers inside backticks or dollar signs
        let processed_content = replace_inline_code(content);
        let processed_content = replace_inline_math(&processed_content);

        // Find all emphasis markers using optimized parsing
        let markers = find_emphasis_markers(&processed_content);
        if markers.is_empty() {
            return;
        }

        // Find valid emphasis spans
        let spans = find_emphasis_spans(&processed_content, markers);

        // Check each span for spacing issues
        for span in spans {
            if has_spacing_issues(&span) {
                // Calculate the full span including markers
                let full_start = span.opening.start_pos;
                let full_end = span.closing.end_pos();
                let full_text = &content[full_start..full_end];

                // Skip if this emphasis has a Kramdown span IAL immediately after it
                // (no space between emphasis and IAL)
                if full_end < content.len() {
                    let remaining = &content[full_end..];
                    // Check if IAL starts immediately after the emphasis (no whitespace)
                    if remaining.starts_with('{') && has_span_ial(remaining.split_whitespace().next().unwrap_or("")) {
                        continue;
                    }
                }

                // Create the marker string efficiently
                let marker_char = span.opening.as_char();
                let marker_str = if span.opening.count == 1 {
                    marker_char.to_string()
                } else {
                    format!("{marker_char}{marker_char}")
                };

                // Create the fixed version by trimming spaces from content
                let trimmed_content = span.content.trim();
                let fixed_text = format!("{marker_str}{trimmed_content}{marker_str}");

                // Truncate long emphasis spans for readable warning messages
                let display_text = truncate_for_display(full_text, 60);

                let warning = LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: format!("Spaces inside emphasis markers: {display_text:?}"),
                    line: line_num,
                    column: offset + full_start + 1, // +1 because columns are 1-indexed
                    end_line: line_num,
                    end_column: offset + full_end + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: (offset + full_start)..(offset + full_end),
                        replacement: fixed_text,
                    }),
                };

                warnings.push(warning);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_emphasis_marker_parsing() {
        let markers = find_emphasis_markers("This has *single* and **double** emphasis");
        assert_eq!(markers.len(), 4); // *, *, **, **

        let markers = find_emphasis_markers("*start* and *end*");
        assert_eq!(markers.len(), 4); // *, *, *, *
    }

    #[test]
    fn test_emphasis_span_detection() {
        let markers = find_emphasis_markers("This has *valid* emphasis");
        let spans = find_emphasis_spans("This has *valid* emphasis", markers);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "valid");
        assert!(!spans[0].has_leading_space);
        assert!(!spans[0].has_trailing_space);

        let markers = find_emphasis_markers("This has * invalid * emphasis");
        let spans = find_emphasis_spans("This has * invalid * emphasis", markers);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, " invalid ");
        assert!(spans[0].has_leading_space);
        assert!(spans[0].has_trailing_space);
    }

    #[test]
    fn test_with_document_structure() {
        let rule = MD037NoSpaceInEmphasis;

        // Test with no spaces inside emphasis - should pass
        let content = "This is *correct* emphasis and **strong emphasis**";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "No warnings expected for correct emphasis");

        // Test with actual spaces inside emphasis - use content that should warn
        let content = "This is * text with spaces * and more content";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty(), "Expected warnings for spaces in emphasis");

        // Test with code blocks - emphasis in code should be ignored
        let content = "This is *correct* emphasis\n```\n* incorrect * in code block\n```\nOutside block with * spaces in emphasis *";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            !result.is_empty(),
            "Expected warnings for spaces in emphasis outside code block"
        );
    }

    #[test]
    fn test_emphasis_in_links_not_flagged() {
        let rule = MD037NoSpaceInEmphasis;
        let content = r#"Check this [* spaced asterisk *](https://example.com/*test*) link.

This has * real spaced emphasis * that should be flagged."#;
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Test passed - emphasis inside links are filtered out correctly

        // Only the real emphasis outside links should be flagged
        assert_eq!(
            result.len(),
            1,
            "Expected exactly 1 warning, but got: {:?}",
            result.len()
        );
        assert!(result[0].message.contains("Spaces inside emphasis markers"));
        // Should flag "* real spaced emphasis *" but not emphasis patterns inside links
        assert!(result[0].line == 3); // Line with "* real spaced emphasis *"
    }

    #[test]
    fn test_emphasis_in_links_vs_outside_links() {
        let rule = MD037NoSpaceInEmphasis;
        let content = r#"Check [* spaced *](https://example.com/*test*) and inline * real spaced * text.

[* link *]: https://example.com/*path*"#;
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only the actual emphasis outside links should be flagged
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Spaces inside emphasis markers"));
        // Should be the "* real spaced *" text on line 1
        assert!(result[0].line == 1);
    }

    #[test]
    fn test_issue_49_asterisk_in_inline_code() {
        // Test for issue #49 - Asterisk within backticks identified as for emphasis
        let rule = MD037NoSpaceInEmphasis;

        // Test case from issue #49
        let content = "The `__mul__` method is needed for left-hand multiplication (`vector * 3`) and `__rmul__` is needed for right-hand multiplication (`3 * vector`).";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag asterisks inside inline code as emphasis (issue #49). Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_28_inline_code_in_emphasis() {
        // Test for issue #28 - MD037 should not flag inline code inside emphasis as spaces
        let rule = MD037NoSpaceInEmphasis;

        // Test case 1: inline code with single backticks inside bold emphasis
        let content = "Though, we often call this an **inline `if`** because it looks sort of like an `if`-`else` statement all in *one line* of code.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag inline code inside emphasis as spaces (issue #28). Got: {result:?}"
        );

        // Test case 2: multiple inline code snippets inside emphasis
        let content2 = "The **`foo` and `bar`** methods are important.";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert!(
            result2.is_empty(),
            "Should not flag multiple inline code snippets inside emphasis. Got: {result2:?}"
        );

        // Test case 3: inline code with underscores for emphasis
        let content3 = "This is __inline `code`__ with underscores.";
        let ctx3 = LintContext::new(content3, crate::config::MarkdownFlavor::Standard, None);
        let result3 = rule.check(&ctx3).unwrap();
        assert!(
            result3.is_empty(),
            "Should not flag inline code with underscore emphasis. Got: {result3:?}"
        );

        // Test case 4: single asterisk emphasis with inline code
        let content4 = "This is *inline `test`* with single asterisks.";
        let ctx4 = LintContext::new(content4, crate::config::MarkdownFlavor::Standard, None);
        let result4 = rule.check(&ctx4).unwrap();
        assert!(
            result4.is_empty(),
            "Should not flag inline code with single asterisk emphasis. Got: {result4:?}"
        );

        // Test case 5: actual spaces that should be flagged
        let content5 = "This has * real spaces * that should be flagged.";
        let ctx5 = LintContext::new(content5, crate::config::MarkdownFlavor::Standard, None);
        let result5 = rule.check(&ctx5).unwrap();
        assert!(!result5.is_empty(), "Should still flag actual spaces in emphasis");
        assert!(result5[0].message.contains("Spaces inside emphasis markers"));
    }

    #[test]
    fn test_multibyte_utf8_no_panic() {
        // Regression test: ensure multi-byte UTF-8 characters don't cause panics
        // in the truncate_for_display function when handling long emphasis spans.
        // These test cases include various scripts that could trigger boundary issues.
        let rule = MD037NoSpaceInEmphasis;

        // Greek text with emphasis
        let greek = "Αυτό είναι ένα * τεστ με ελληνικά * και πολύ μεγάλο κείμενο που θα πρέπει να περικοπεί σωστά.";
        let ctx = LintContext::new(greek, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Greek text should not panic");

        // Chinese text with emphasis
        let chinese = "这是一个 * 测试文本 * 包含中文字符，需要正确处理多字节边界。";
        let ctx = LintContext::new(chinese, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Chinese text should not panic");

        // Cyrillic/Russian text with emphasis
        let cyrillic = "Это * тест с кириллицей * и очень длинным текстом для проверки обрезки.";
        let ctx = LintContext::new(cyrillic, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Cyrillic text should not panic");

        // Mixed multi-byte characters in a long emphasis span that triggers truncation
        let mixed =
            "日本語と * 中文と한국어が混在する非常に長いテキストでtruncate_for_displayの境界処理をテスト * します。";
        let ctx = LintContext::new(mixed, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Mixed CJK text should not panic");

        // Arabic text (right-to-left) with emphasis
        let arabic = "هذا * اختبار بالعربية * مع نص طويل جداً لاختبار معالجة حدود الأحرف.";
        let ctx = LintContext::new(arabic, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Arabic text should not panic");

        // Emoji with emphasis
        let emoji = "This has * 🎉 party 🎊 celebration 🥳 emojis * that use multi-byte sequences.";
        let ctx = LintContext::new(emoji, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Emoji text should not panic");
    }

    #[test]
    fn test_template_shortcode_syntax_not_flagged() {
        // Test for FastAPI/MkDocs style template syntax {* ... *}
        // These should NOT be flagged as emphasis with spaces
        let rule = MD037NoSpaceInEmphasis;

        // FastAPI style code inclusion
        let content = "{* ../../docs_src/cookie_param_models/tutorial001.py hl[9:12,16] *}";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Template shortcode syntax should not be flagged. Got: {result:?}"
        );

        // Another FastAPI example
        let content = "{* ../../docs_src/conditional_openapi/tutorial001.py hl[6,11] *}";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Template shortcode syntax should not be flagged. Got: {result:?}"
        );

        // Multiple shortcodes on different lines
        let content = "# Header\n\n{* file1.py *}\n\nSome text.\n\n{* file2.py hl[1-5] *}";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multiple template shortcodes should not be flagged. Got: {result:?}"
        );

        // But actual emphasis with spaces should still be flagged
        let content = "This has * real spaced emphasis * here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty(), "Real spaced emphasis should still be flagged");
    }

    #[test]
    fn test_multiline_code_span_not_flagged() {
        // Test for multi-line code spans - asterisks inside should not be flagged
        // This tests the case where a code span starts on one line and ends on another
        let rule = MD037NoSpaceInEmphasis;

        // Code span spanning multiple lines with asterisks inside
        let content = "# Test\n\naffects the structure. `1 + 0 + 0` is parsed as `(1 + 0) +\n0` while `1 + 0 * 0` is parsed as `1 + (0 * 0)`. Since the pattern";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag asterisks inside multi-line code spans. Got: {result:?}"
        );

        // Another multi-line code span case
        let content2 = "Text with `code that\nspans * multiple * lines` here.";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert!(
            result2.is_empty(),
            "Should not flag asterisks inside multi-line code spans. Got: {result2:?}"
        );
    }

    #[test]
    fn test_mkdocs_icon_shortcode_not_flagged() {
        // Test that MkDocs icon shortcodes with asterisks inside are not flagged
        let rule = MD037NoSpaceInEmphasis;

        // Icon shortcode syntax like :material-star: should not trigger MD037
        // because it's valid MkDocs Material syntax
        let content = "Click :material-check: to confirm and :fontawesome-solid-star: for favorites.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag MkDocs icon shortcodes. Got: {result:?}"
        );

        // Actual emphasis with spaces should still be flagged even in MkDocs mode
        let content2 = "This has * real spaced emphasis * but also :material-check: icon.";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::MkDocs, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert!(
            !result2.is_empty(),
            "Should still flag real spaced emphasis in MkDocs mode"
        );
    }

    #[test]
    fn test_mkdocs_pymdown_markup_not_flagged() {
        // Test that PyMdown extension markup is not flagged as emphasis issues
        let rule = MD037NoSpaceInEmphasis;

        // Keys notation (++ctrl+alt+delete++)
        let content = "Press ++ctrl+c++ to copy.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag PyMdown Keys notation. Got: {result:?}"
        );

        // Mark notation (==highlighted==)
        let content2 = "This is ==highlighted text== for emphasis.";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::MkDocs, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert!(
            result2.is_empty(),
            "Should not flag PyMdown Mark notation. Got: {result2:?}"
        );

        // Insert notation (^^inserted^^)
        let content3 = "This is ^^inserted text^^ here.";
        let ctx3 = LintContext::new(content3, crate::config::MarkdownFlavor::MkDocs, None);
        let result3 = rule.check(&ctx3).unwrap();
        assert!(
            result3.is_empty(),
            "Should not flag PyMdown Insert notation. Got: {result3:?}"
        );

        // Mixed content with real emphasis issue and PyMdown markup
        let content4 = "Press ++ctrl++ then * spaced emphasis * here.";
        let ctx4 = LintContext::new(content4, crate::config::MarkdownFlavor::MkDocs, None);
        let result4 = rule.check(&ctx4).unwrap();
        assert!(
            !result4.is_empty(),
            "Should still flag real spaced emphasis alongside PyMdown markup"
        );
    }
}
