/// Rule MD064: No multiple consecutive spaces
///
/// See [docs/md064.md](../../docs/md064.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when multiple consecutive spaces are found in markdown content.
/// Multiple spaces between words serve no purpose and can indicate formatting issues.
///
/// For example:
///
/// ```markdown
/// This is   a sentence with extra spaces.
/// ```
///
/// Should be:
///
/// ```markdown
/// This is a sentence with extra spaces.
/// ```
///
/// This rule does NOT flag:
/// - Spaces inside inline code spans (`` `code   here` ``)
/// - Spaces inside fenced or indented code blocks
/// - Leading whitespace (indentation)
/// - Trailing whitespace (handled by MD009)
/// - Spaces inside HTML comments or HTML blocks
/// - Table rows (alignment padding is intentional)
/// - Front matter content
use crate::filtered_lines::FilteredLinesExt;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::skip_context::is_table_line;
use std::sync::Arc;

/// Regex to find multiple consecutive spaces (2 or more)
use regex::Regex;
use std::sync::LazyLock;

static MULTIPLE_SPACES_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // Match 2 or more consecutive spaces
    Regex::new(r" {2,}").unwrap()
});

#[derive(Debug, Clone, Default)]
pub struct MD064NoMultipleConsecutiveSpaces;

impl MD064NoMultipleConsecutiveSpaces {
    pub fn new() -> Self {
        Self
    }

    /// Check if a byte position is inside an inline code span
    fn is_in_code_span(&self, code_spans: &[crate::lint_context::CodeSpan], byte_pos: usize) -> bool {
        code_spans
            .iter()
            .any(|span| byte_pos >= span.byte_offset && byte_pos < span.byte_end)
    }

    /// Check if a match is trailing whitespace at the end of a line
    /// Trailing spaces are handled by MD009, so MD064 should skip them entirely
    fn is_trailing_whitespace(&self, line: &str, match_end: usize) -> bool {
        // If the match extends to the end of the line, it's trailing whitespace
        let remaining = &line[match_end..];
        remaining.is_empty() || remaining.chars().all(|c| c == '\n' || c == '\r')
    }

    /// Check if the match is part of leading indentation
    fn is_leading_indentation(&self, line: &str, match_start: usize) -> bool {
        // Check if everything before the match is whitespace
        line[..match_start].chars().all(|c| c == ' ' || c == '\t')
    }

    /// Check if the match is immediately after a list marker (handled by MD030)
    fn is_after_list_marker(&self, line: &str, match_start: usize) -> bool {
        let before = line[..match_start].trim_start();

        // Unordered list markers: *, -, +
        if before == "*" || before == "-" || before == "+" {
            return true;
        }

        // Ordered list markers: digits followed by . or )
        // Examples: "1.", "2)", "10.", "123)"
        if before.len() >= 2 {
            let last_char = before.chars().last().unwrap();
            if last_char == '.' || last_char == ')' {
                let prefix = &before[..before.len() - 1];
                if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if the match is immediately after a blockquote marker (handled by MD027)
    /// Patterns: "> ", ">  ", ">>", "> > "
    fn is_after_blockquote_marker(&self, line: &str, match_start: usize) -> bool {
        let before = line[..match_start].trim_start();

        // Check if it's only blockquote markers (> characters, possibly with spaces between)
        if before.is_empty() {
            return false;
        }

        // Pattern: one or more '>' characters, optionally followed by space and more '>'
        let trimmed = before.trim_end();
        if trimmed.chars().all(|c| c == '>') {
            return true;
        }

        // Pattern: "> " at end (nested blockquote with space)
        if trimmed.ends_with('>') {
            let inner = trimmed.trim_end_matches('>').trim();
            if inner.is_empty() || inner.chars().all(|c| c == '>') {
                return true;
            }
        }

        false
    }

    /// Check if the match is inside or after a reference link definition
    /// Pattern: [label]: URL or [label]:  URL
    fn is_reference_link_definition(&self, line: &str, match_start: usize) -> bool {
        let trimmed = line.trim_start();

        // Reference link pattern: [label]: URL
        if trimmed.starts_with('[')
            && let Some(bracket_end) = trimmed.find("]:")
        {
            let colon_pos = trimmed.len() - trimmed.trim_start().len() + bracket_end + 2;
            // Check if the match is right after the ]: marker
            if match_start >= colon_pos - 1 && match_start <= colon_pos + 1 {
                return true;
            }
        }

        false
    }

    /// Check if the match is after a footnote marker
    /// Pattern: [^label]:  text
    fn is_after_footnote_marker(&self, line: &str, match_start: usize) -> bool {
        let trimmed = line.trim_start();

        // Footnote pattern: [^label]: text
        if trimmed.starts_with("[^")
            && let Some(bracket_end) = trimmed.find("]:")
        {
            let leading_spaces = line.len() - trimmed.len();
            let colon_pos = leading_spaces + bracket_end + 2;
            // Check if the match is right after the ]: marker
            if match_start >= colon_pos.saturating_sub(1) && match_start <= colon_pos + 1 {
                return true;
            }
        }

        false
    }

    /// Check if the match is after a definition list marker
    /// Pattern: :   Definition text
    fn is_after_definition_marker(&self, line: &str, match_start: usize) -> bool {
        let before = line[..match_start].trim_start();

        // Definition list marker is just ":"
        before == ":"
    }

    /// Check if the match is inside a task list checkbox
    /// Pattern: - [ ]  or - [x]  (spaces after checkbox)
    fn is_after_task_checkbox(&self, line: &str, match_start: usize) -> bool {
        let before = line[..match_start].trim_start();

        // Task list patterns: *, -, + followed by [ ], [x], or [X]
        // Examples: "- [ ]", "* [x]", "+ [X]"
        if before.len() >= 4 {
            let patterns = [
                "- [ ]", "- [x]", "- [X]", "* [ ]", "* [x]", "* [X]", "+ [ ]", "+ [x]", "+ [X]",
            ];
            for pattern in patterns {
                if before == pattern {
                    return true;
                }
            }
        }

        false
    }

    /// Check if this is a table row without outer pipes (GFM extension)
    /// Pattern: text | text | text (no leading/trailing pipe)
    fn is_table_without_outer_pipes(&self, line: &str) -> bool {
        let trimmed = line.trim();

        // Must contain at least one pipe but not start or end with pipe
        if !trimmed.contains('|') {
            return false;
        }

        // If it starts or ends with |, it's a normal table (handled by is_table_line)
        if trimmed.starts_with('|') || trimmed.ends_with('|') {
            return false;
        }

        // Check if it looks like a table row: has multiple pipe-separated cells
        // Could be data row (word | word) or separator row (--- | ---)
        // Table cells can be empty, so we just check for at least 2 parts
        let parts: Vec<&str> = trimmed.split('|').collect();
        if parts.len() >= 2 {
            // At least first or last cell should have content (not just whitespace)
            // to distinguish from accidental pipes in text
            let first_has_content = !parts.first().unwrap_or(&"").trim().is_empty();
            let last_has_content = !parts.last().unwrap_or(&"").trim().is_empty();
            if first_has_content || last_has_content {
                return true;
            }
        }

        false
    }
}

impl Rule for MD064NoMultipleConsecutiveSpaces {
    fn name(&self) -> &'static str {
        "MD064"
    }

    fn description(&self) -> &'static str {
        "Multiple consecutive spaces"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early return: if no double spaces at all, skip
        if !content.contains("  ") {
            return Ok(vec![]);
        }

        let mut warnings = Vec::new();
        let code_spans: Arc<Vec<crate::lint_context::CodeSpan>> = ctx.code_spans();
        let line_index = &ctx.line_index;

        // Process content lines, automatically skipping front matter, code blocks, HTML
        for line in ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .skip_html_blocks()
            .skip_html_comments()
            .skip_mkdocstrings()
            .skip_esm_blocks()
        {
            // Quick check: skip if line doesn't contain double spaces
            if !line.content.contains("  ") {
                continue;
            }

            // Skip table rows (alignment padding is intentional)
            if is_table_line(line.content) {
                continue;
            }

            // Skip tables without outer pipes (GFM extension)
            if self.is_table_without_outer_pipes(line.content) {
                continue;
            }

            let line_start_byte = line_index.get_line_start_byte(line.line_num).unwrap_or(0);

            // Find all occurrences of multiple consecutive spaces
            for mat in MULTIPLE_SPACES_REGEX.find_iter(line.content) {
                let match_start = mat.start();
                let match_end = mat.end();
                let space_count = match_end - match_start;

                // Skip if this is leading indentation
                if self.is_leading_indentation(line.content, match_start) {
                    continue;
                }

                // Skip trailing whitespace (handled by MD009)
                if self.is_trailing_whitespace(line.content, match_end) {
                    continue;
                }

                // Skip spaces after list markers (handled by MD030)
                if self.is_after_list_marker(line.content, match_start) {
                    continue;
                }

                // Skip spaces after blockquote markers (handled by MD027)
                if self.is_after_blockquote_marker(line.content, match_start) {
                    continue;
                }

                // Skip spaces after footnote markers
                if self.is_after_footnote_marker(line.content, match_start) {
                    continue;
                }

                // Skip spaces after reference link definition markers
                if self.is_reference_link_definition(line.content, match_start) {
                    continue;
                }

                // Skip spaces after definition list markers
                if self.is_after_definition_marker(line.content, match_start) {
                    continue;
                }

                // Skip spaces after task list checkboxes
                if self.is_after_task_checkbox(line.content, match_start) {
                    continue;
                }

                // Calculate absolute byte position
                let abs_byte_start = line_start_byte + match_start;

                // Skip if inside an inline code span
                if self.is_in_code_span(&code_spans, abs_byte_start) {
                    continue;
                }

                // Calculate byte range for the fix
                let abs_byte_end = line_start_byte + match_end;

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: format!("Multiple consecutive spaces ({space_count}) found"),
                    line: line.line_num,
                    column: match_start + 1, // 1-indexed
                    end_line: line.line_num,
                    end_column: match_end + 1, // 1-indexed
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: abs_byte_start..abs_byte_end,
                        replacement: " ".to_string(), // Collapse to single space
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Early return if no double spaces
        if !content.contains("  ") {
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

        // Apply fixes
        let mut result = content.to_string();
        for (range, replacement) in fixes {
            if range.start < result.len() && range.end <= result.len() {
                result.replace_range(range, &replacement);
            }
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.content.contains("  ")
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD064NoMultipleConsecutiveSpaces::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_basic_multiple_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Should flag multiple spaces
        let content = "This is   a sentence with extra spaces.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 8); // Position of first extra space
    }

    #[test]
    fn test_no_issues_single_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Should not flag single spaces
        let content = "This is a normal sentence with single spaces.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_skip_inline_code() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Should not flag spaces inside inline code
        let content = "Use `code   with   spaces` for formatting.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_skip_code_blocks() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Should not flag spaces inside code blocks
        let content = "# Heading\n\n```\ncode   with   spaces\n```\n\nNormal text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_skip_leading_indentation() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Should not flag leading indentation
        let content = "    This is indented text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_skip_trailing_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Should not flag trailing spaces (handled by MD009)
        let content = "Line with trailing spaces   \nNext line.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_skip_all_trailing_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Should not flag any trailing spaces regardless of count
        let content = "Two spaces  \nThree spaces   \nFour spaces    \n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_skip_front_matter() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Should not flag spaces in front matter
        let content = "---\ntitle:   Test   Title\n---\n\nContent here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_skip_html_comments() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Should not flag spaces in HTML comments
        let content = "<!-- comment   with   spaces -->\n\nContent here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_multiple_issues_one_line() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Should flag multiple occurrences on one line
        let content = "This   has   multiple   issues.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3, "Should flag all 3 occurrences");
    }

    #[test]
    fn test_fix_collapses_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        let content = "This is   a sentence   with extra   spaces.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "This is a sentence with extra spaces.");
    }

    #[test]
    fn test_fix_preserves_inline_code() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        let content = "Text   here `code   inside` and   more.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Text here `code   inside` and more.");
    }

    #[test]
    fn test_fix_preserves_trailing_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Trailing spaces should be preserved (handled by MD009)
        let content = "Line with   extra and trailing   \nNext line.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Only the internal "   " gets fixed to " ", trailing spaces are preserved
        assert_eq!(fixed, "Line with extra and trailing   \nNext line.");
    }

    #[test]
    fn test_list_items_with_extra_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        let content = "- Item   one\n- Item   two\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Should flag spaces in list items");
    }

    #[test]
    fn test_blockquote_with_extra_spaces_in_content() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Extra spaces in blockquote CONTENT should be flagged
        let content = "> Quote   with extra   spaces\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Should flag spaces in blockquote content");
    }

    #[test]
    fn test_skip_blockquote_marker_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Extra spaces after blockquote marker are handled by MD027
        let content = ">  Text with extra space after marker\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Three spaces after marker
        let content = ">   Text with three spaces after marker\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Nested blockquotes
        let content = ">>  Nested blockquote\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_mixed_content() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        let content = r#"# Heading

This   has extra spaces.

```
code   here  is  fine
```

- List   item

> Quote   text

Normal paragraph.
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should flag: "This   has" (1), "List   item" (1), "Quote   text" (1)
        assert_eq!(result.len(), 3, "Should flag only content outside code blocks");
    }

    #[test]
    fn test_multibyte_utf8() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Test with multi-byte UTF-8 characters
        let content = "日本語   テスト   文字列";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Should handle multi-byte UTF-8 characters");

        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 2, "Should find 2 occurrences of multiple spaces");
    }

    #[test]
    fn test_table_rows_skipped() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Table rows with alignment padding should be skipped
        let content = "| Header 1 | Header 2 |\n|----------|----------|\n| Cell 1   | Cell 2   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Table rows should be skipped (alignment padding is intentional)
        assert!(result.is_empty());
    }

    #[test]
    fn test_link_text_with_extra_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Link text with extra spaces (should be flagged)
        let content = "[Link   text](https://example.com)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag extra spaces in link text");
    }

    #[test]
    fn test_image_alt_with_extra_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Image alt text with extra spaces (should be flagged)
        let content = "![Alt   text](image.png)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag extra spaces in image alt text");
    }

    #[test]
    fn test_skip_list_marker_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Spaces after list markers are handled by MD030, not MD064
        let content = "*   Item with extra spaces after marker\n-   Another item\n+   Third item\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Ordered list markers
        let content = "1.  Item one\n2.  Item two\n10. Item ten\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Indented list items should also be skipped
        let content = "  *   Indented item\n    1.  Nested numbered item\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_flag_spaces_in_list_content() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Multiple spaces WITHIN list content should still be flagged
        let content = "* Item with   extra spaces in content\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag extra spaces in list content");
    }

    #[test]
    fn test_skip_reference_link_definition_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Reference link definitions may have multiple spaces after the colon
        let content = "[ref]:  https://example.com\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Multiple spaces
        let content = "[reference-link]:   https://example.com \"Title\"\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_skip_footnote_marker_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Footnote definitions may have multiple spaces after the colon
        let content = "[^1]:  Footnote with extra space\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Footnote with longer label
        let content = "[^footnote-label]:   This is the footnote text.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_skip_definition_list_marker_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Definition list markers (PHP Markdown Extra / Pandoc)
        let content = "Term\n:   Definition with extra spaces\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Multiple definitions
        let content = ":    Another definition\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_skip_task_list_checkbox_spaces() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Task list items may have extra spaces after checkbox
        let content = "- [ ]  Task with extra space\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Checked task
        let content = "- [x]  Completed task\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // With asterisk marker
        let content = "* [ ]  Task with asterisk marker\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_skip_table_without_outer_pipes() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // GFM tables without outer pipes should be skipped
        let content = "Col1      | Col2      | Col3\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Separator row
        let content = "--------- | --------- | ---------\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Data row
        let content = "Data1     | Data2     | Data3\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_flag_spaces_in_footnote_content() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Extra spaces WITHIN footnote text content should be flagged
        let content = "[^1]: Footnote with   extra spaces in content.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag extra spaces in footnote content");
    }

    #[test]
    fn test_flag_spaces_in_reference_content() {
        let rule = MD064NoMultipleConsecutiveSpaces::new();

        // Extra spaces in the title of a reference link should be flagged
        let content = "[ref]: https://example.com \"Title   with extra spaces\"\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag extra spaces in reference link title");
    }
}
