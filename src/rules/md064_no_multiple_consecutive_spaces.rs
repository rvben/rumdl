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
use crate::rule_config_serde::RuleConfig;
use crate::utils::sentence_utils::is_after_sentence_ending;
use crate::utils::skip_context::is_table_line;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Regex to find multiple consecutive spaces (2 or more)
use regex::Regex;
use std::sync::LazyLock;

static MULTIPLE_SPACES_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // Match 2 or more consecutive spaces
    Regex::new(r" {2,}").unwrap()
});

/// Configuration for MD064 (No multiple consecutive spaces)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD064Config {
    /// Allow exactly two spaces after sentence-ending punctuation (default: false)
    ///
    /// When enabled, allows exactly 2 spaces after sentence-ending punctuation
    /// (`.`, `!`, `?`) while still flagging multiple spaces elsewhere. This
    /// supports the traditional typewriter convention of two spaces after sentences.
    ///
    /// Sentence-ending punctuation includes:
    /// - Period: `.`
    /// - Exclamation mark: `!`
    /// - Question mark: `?`
    ///
    /// Also recognizes closing punctuation after sentence endings:
    /// - Quotes: `."`, `!"`, `?"`, `.'`, `!'`, `?'`
    /// - Parentheses: `.)`, `!)`, `?)`
    /// - Brackets: `.]`, `!]`, `?]`
    /// - Ellipsis: `...`
    ///
    /// Example with `allow-sentence-double-space = true`:
    /// ```markdown
    /// First sentence.  Second sentence.    <- OK (2 spaces after period)
    /// Multiple   spaces here.              <- Flagged (3 spaces, not after sentence)
    /// Word  word in middle.                <- Flagged (2 spaces, not after sentence)
    /// ```
    #[serde(
        default = "default_allow_sentence_double_space",
        alias = "allow_sentence_double_space"
    )]
    pub allow_sentence_double_space: bool,
}

fn default_allow_sentence_double_space() -> bool {
    false
}

impl Default for MD064Config {
    fn default() -> Self {
        Self {
            allow_sentence_double_space: default_allow_sentence_double_space(),
        }
    }
}

impl RuleConfig for MD064Config {
    const RULE_NAME: &'static str = "MD064";
}

#[derive(Debug, Clone)]
pub struct MD064NoMultipleConsecutiveSpaces {
    config: MD064Config,
}

impl Default for MD064NoMultipleConsecutiveSpaces {
    fn default() -> Self {
        Self::new()
    }
}

impl MD064NoMultipleConsecutiveSpaces {
    pub fn new() -> Self {
        Self {
            config: MD064Config::default(),
        }
    }

    pub fn from_config_struct(config: MD064Config) -> Self {
        Self { config }
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

    /// Check if the space count looks like a tab replacement (multiple of 4)
    /// Tab replacements (4, 8, 12, etc. spaces) are intentional and should not be collapsed.
    /// This prevents MD064 from undoing MD010's tab-to-spaces conversion.
    fn is_tab_replacement_pattern(&self, space_count: usize) -> bool {
        space_count >= 4 && space_count.is_multiple_of(4)
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

                // Skip tab replacement patterns (4, 8, 12, etc. spaces)
                // This prevents MD064 from undoing MD010's tab-to-spaces conversion
                if self.is_tab_replacement_pattern(space_count) {
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

                // Allow exactly 2 spaces after sentence-ending punctuation if configured
                // This supports the traditional typewriter convention of two spaces after sentences
                if self.config.allow_sentence_double_space
                    && space_count == 2
                    && is_after_sentence_ending(line.content, match_start)
                {
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

                // Determine the replacement: if allow_sentence_double_space is enabled
                // and this is after a sentence ending, collapse to 2 spaces, otherwise to 1
                let replacement =
                    if self.config.allow_sentence_double_space && is_after_sentence_ending(line.content, match_start) {
                        "  ".to_string() // Collapse to two spaces after sentence
                    } else {
                        " ".to_string() // Collapse to single space
                    };

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
                        replacement,
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

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD064Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD064Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD064Config>(config);
        Box::new(MD064NoMultipleConsecutiveSpaces::from_config_struct(rule_config))
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

    // === allow-sentence-double-space tests ===

    #[test]
    fn test_sentence_double_space_disabled_by_default() {
        // Default config should flag double spaces after sentences
        let rule = MD064NoMultipleConsecutiveSpaces::new();
        let content = "First sentence.  Second sentence.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Default should flag 2 spaces after period");
    }

    #[test]
    fn test_sentence_double_space_enabled_allows_period() {
        // With allow_sentence_double_space, 2 spaces after period should be OK
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "First sentence.  Second sentence.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after period");
    }

    #[test]
    fn test_sentence_double_space_enabled_allows_exclamation() {
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "Wow!  That was great.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after exclamation");
    }

    #[test]
    fn test_sentence_double_space_enabled_allows_question() {
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "Is this OK?  Yes it is.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after question mark");
    }

    #[test]
    fn test_sentence_double_space_flags_mid_sentence() {
        // Even with allow_sentence_double_space, mid-sentence double spaces should be flagged
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "Word  word in the middle.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag 2 spaces mid-sentence");
    }

    #[test]
    fn test_sentence_double_space_flags_triple_after_period() {
        // 3+ spaces after sentence should still be flagged
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "First sentence.   Three spaces here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag 3 spaces even after period");
    }

    #[test]
    fn test_sentence_double_space_with_closing_quote() {
        // "Quoted sentence."  Next sentence.
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = r#"He said "Hello."  Then he left."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after .\" ");

        // With single quote
        let content = "She said 'Goodbye.'  And she was gone.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after .' ");
    }

    #[test]
    fn test_sentence_double_space_with_curly_quotes() {
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        // Curly double quote: U+201C (") and U+201D (")
        // Build string with actual Unicode characters
        let content = format!(
            "He said {}Hello.{}  Then left.",
            '\u{201C}', // "
            '\u{201D}'  // "
        );
        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after curly double quote");

        // Curly single quote: U+2018 (') and U+2019 (')
        let content = format!(
            "She said {}Hi.{}  And left.",
            '\u{2018}', // '
            '\u{2019}'  // '
        );
        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after curly single quote");
    }

    #[test]
    fn test_sentence_double_space_with_closing_paren() {
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "(See reference.)  The next point is.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after .) ");
    }

    #[test]
    fn test_sentence_double_space_with_closing_bracket() {
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "[Citation needed.]  More text here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after .] ");
    }

    #[test]
    fn test_sentence_double_space_with_ellipsis() {
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "He paused...  Then continued.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after ellipsis");
    }

    #[test]
    fn test_sentence_double_space_complex_ending() {
        // Multiple closing punctuation: .")
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = r#"(He said "Yes.")  Then they agreed."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after .\") ");
    }

    #[test]
    fn test_sentence_double_space_mixed_content() {
        // Mix of sentence endings and mid-sentence spaces
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "Good sentence.  Bad  mid-sentence.  Another good one!  OK?  Yes.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should only flag mid-sentence double space");
        assert!(
            result[0].column > 15 && result[0].column < 25,
            "Should flag the 'Bad  mid' double space"
        );
    }

    #[test]
    fn test_sentence_double_space_fix_collapses_to_two() {
        // Fix should collapse 3+ spaces to 2 after sentence, 1 elsewhere
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "Sentence.   Three spaces here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "Sentence.  Three spaces here.",
            "Should collapse to 2 spaces after sentence"
        );
    }

    #[test]
    fn test_sentence_double_space_fix_collapses_mid_sentence_to_one() {
        // Fix should collapse mid-sentence spaces to 1
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "Word  word here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Word word here.", "Should collapse to 1 space mid-sentence");
    }

    #[test]
    fn test_sentence_double_space_config_kebab_case() {
        let toml_str = r#"
            allow-sentence-double-space = true
        "#;
        let config: MD064Config = toml::from_str(toml_str).unwrap();
        assert!(config.allow_sentence_double_space);
    }

    #[test]
    fn test_sentence_double_space_config_snake_case() {
        let toml_str = r#"
            allow_sentence_double_space = true
        "#;
        let config: MD064Config = toml::from_str(toml_str).unwrap();
        assert!(config.allow_sentence_double_space);
    }

    #[test]
    fn test_sentence_double_space_at_line_start() {
        // Period at very start shouldn't cause issues
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        // This is an edge case - spaces at start are leading indentation
        let content = ".  Text after period at start.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        // This should not panic
        let _result = rule.check(&ctx).unwrap();
    }

    #[test]
    fn test_sentence_double_space_guillemets() {
        // French-style quotes (guillemets)
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "Il a dit «Oui.»  Puis il est parti.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after .» (guillemet)");
    }

    #[test]
    fn test_sentence_double_space_multiple_sentences() {
        // Multiple consecutive sentences with double spacing
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "First.  Second.  Third.  Fourth.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow all sentence-ending double spaces");
    }

    #[test]
    fn test_sentence_double_space_abbreviation_detection() {
        // Known abbreviations should NOT be treated as sentence endings
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        // "Dr.  Smith" - Dr. is a known abbreviation, should be flagged
        let content = "Dr.  Smith arrived.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag Dr. as abbreviation, not sentence ending");

        // "Prof.  Williams" - Prof. is a known abbreviation
        let content = "Prof.  Williams teaches.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag Prof. as abbreviation");

        // "e.g.  this" - e.g. is a known abbreviation
        let content = "Use e.g.  this example.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag e.g. as abbreviation");

        // Unknown abbreviation-like words are treated as potential sentence endings
        // "Inc.  Next" - Inc. is NOT in our abbreviation list
        let content = "Acme Inc.  Next company.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Inc. not in abbreviation list, treated as sentence end"
        );
    }

    #[test]
    fn test_sentence_double_space_default_config_has_correct_defaults() {
        let config = MD064Config::default();
        assert!(
            !config.allow_sentence_double_space,
            "Default allow_sentence_double_space should be false"
        );
    }

    #[test]
    fn test_sentence_double_space_from_config_integration() {
        use crate::config::Config;
        use std::collections::BTreeMap;

        let mut config = Config::default();
        let mut values = BTreeMap::new();
        values.insert("allow-sentence-double-space".to_string(), toml::Value::Boolean(true));
        config.rules.insert(
            "MD064".to_string(),
            crate::config::RuleConfig { severity: None, values },
        );

        let rule = MD064NoMultipleConsecutiveSpaces::from_config(&config);

        // Verify the rule uses the loaded config
        let content = "Sentence.  Two spaces OK.  But three   is not.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should only flag the triple spaces");
    }

    #[test]
    fn test_sentence_double_space_after_inline_code() {
        // Issue #345: Sentence ending with inline code should allow double space
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        // Basic case from issue report
        let content = "Hello from `backticks`.  How's it going?";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should allow 2 spaces after inline code ending with period"
        );

        // Multiple inline code spans
        let content = "Use `foo` and `bar`.  Next sentence.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after code at end of sentence");

        // With exclamation mark
        let content = "The `code` worked!  Celebrate.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after code with exclamation");

        // With question mark
        let content = "Is `null` falsy?  Yes.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after code with question mark");

        // Inline code mid-sentence (not at end) - double space SHOULD be flagged
        let content = "The `code`  is here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag 2 spaces after code mid-sentence");
    }

    #[test]
    fn test_sentence_double_space_code_with_closing_punctuation() {
        // Inline code followed by period in parentheses
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        // Code in parentheses
        let content = "(see `example`).  Next sentence.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after code in parentheses");

        // Code in quotes
        let content = "He said \"use `code`\".  Then left.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after code in quotes");
    }

    #[test]
    fn test_sentence_double_space_after_emphasis() {
        // Sentence ending with emphasis should allow double space
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        // Asterisk emphasis
        let content = "The word is *important*.  Next sentence.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after emphasis");

        // Underscore emphasis
        let content = "The word is _important_.  Next sentence.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after underscore emphasis");

        // Bold (asterisk)
        let content = "The word is **critical**.  Next sentence.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after bold");

        // Bold (underscore)
        let content = "The word is __critical__.  Next sentence.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after underscore bold");
    }

    #[test]
    fn test_sentence_double_space_after_strikethrough() {
        // Sentence ending with strikethrough should allow double space
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        let content = "This is ~~wrong~~.  Next sentence.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after strikethrough");

        // With exclamation
        let content = "That was ~~bad~~!  Learn from it.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should allow 2 spaces after strikethrough with exclamation"
        );
    }

    #[test]
    fn test_sentence_double_space_after_extended_markdown() {
        // Extended markdown syntax (highlight, superscript)
        let config = MD064Config {
            allow_sentence_double_space: true,
        };
        let rule = MD064NoMultipleConsecutiveSpaces::from_config_struct(config);

        // Highlight syntax
        let content = "This is ==highlighted==.  Next sentence.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after highlight");

        // Superscript
        let content = "E equals mc^2^.  Einstein said.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should allow 2 spaces after superscript");
    }
}
