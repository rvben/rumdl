use crate::utils::range_utils::calculate_match_range;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::strong_style::StrongStyle;
use crate::utils::code_block_utils::StrongSpanDetail;
use crate::utils::skip_context::{is_in_jsx_expression, is_in_math_context, is_in_mdx_comment, is_in_mkdocs_markup};

/// Check if a byte position within a line is inside a backtick-delimited code span.
/// This is a line-level fallback for cases where pulldown-cmark's code span detection
/// misses spans due to table parsing interference (e.g., pipes inside code spans
/// in table rows cause pulldown-cmark to misidentify cell boundaries).
fn is_in_inline_code_on_line(line: &str, byte_pos: usize) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'`' {
            let open_start = i;
            let mut backtick_count = 0;
            while i < bytes.len() && bytes[i] == b'`' {
                backtick_count += 1;
                i += 1;
            }

            // Search for matching closing backticks
            let mut j = i;
            while j < bytes.len() {
                if bytes[j] == b'`' {
                    let mut close_count = 0;
                    while j < bytes.len() && bytes[j] == b'`' {
                        close_count += 1;
                        j += 1;
                    }
                    if close_count == backtick_count {
                        // Found matching pair: code span covers open_start..j
                        if byte_pos >= open_start && byte_pos < j {
                            return true;
                        }
                        i = j;
                        break;
                    }
                } else {
                    j += 1;
                }
            }

            if j >= bytes.len() {
                // No matching close found, remaining text is not a code span
                break;
            }
        } else {
            i += 1;
        }
    }

    false
}

/// Convert a StrongSpanDetail to a StrongStyle
fn span_style(span: &StrongSpanDetail) -> StrongStyle {
    if span.is_asterisk {
        StrongStyle::Asterisk
    } else {
        StrongStyle::Underscore
    }
}

mod md050_config;
use md050_config::MD050Config;

/// Rule MD050: Strong style
///
/// See [docs/md050.md](../../docs/md050.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when strong markers (** or __) are used in an inconsistent way.
#[derive(Debug, Default, Clone)]
pub struct MD050StrongStyle {
    config: MD050Config,
}

impl MD050StrongStyle {
    pub fn new(style: StrongStyle) -> Self {
        Self {
            config: MD050Config { style },
        }
    }

    pub fn from_config_struct(config: MD050Config) -> Self {
        Self { config }
    }

    /// Check if a byte position is within a link (inline links, reference links, or reference definitions).
    /// Delegates to LintContext::is_in_link which uses O(log n) binary search.
    fn is_in_link(ctx: &crate::lint_context::LintContext, byte_pos: usize) -> bool {
        ctx.is_in_link(byte_pos)
    }

    /// Check if a byte position is within an HTML tag. O(log n) via binary search.
    fn is_in_html_tag(html_tags: &[crate::lint_context::HtmlTag], byte_pos: usize) -> bool {
        let idx = html_tags.partition_point(|tag| tag.byte_offset <= byte_pos);
        idx > 0 && byte_pos < html_tags[idx - 1].byte_end
    }

    /// Check if a byte position is within HTML code tags (<code>...</code>).
    /// Uses pre-computed code ranges for O(log n) lookup via binary search.
    fn is_in_html_code_content(code_ranges: &[(usize, usize)], byte_pos: usize) -> bool {
        let idx = code_ranges.partition_point(|&(start, _)| start <= byte_pos);
        idx > 0 && byte_pos < code_ranges[idx - 1].1
    }

    /// Pre-compute ranges covered by <code>...</code> HTML tags.
    /// Returns sorted Vec of (start, end) byte ranges.
    fn compute_html_code_ranges(html_tags: &[crate::lint_context::HtmlTag]) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        let mut open_code_end: Option<usize> = None;

        for tag in html_tags {
            if tag.tag_name == "code" {
                if tag.is_self_closing {
                    continue;
                } else if !tag.is_closing {
                    open_code_end = Some(tag.byte_end);
                } else if tag.is_closing {
                    if let Some(start) = open_code_end {
                        ranges.push((start, tag.byte_offset));
                    }
                    open_code_end = None;
                }
            }
        }
        // Handle unclosed <code> tag
        if let Some(start) = open_code_end {
            ranges.push((start, usize::MAX));
        }
        ranges
    }

    /// Check if a strong emphasis span should be skipped based on context
    fn should_skip_span(
        &self,
        ctx: &crate::lint_context::LintContext,
        html_tags: &[crate::lint_context::HtmlTag],
        html_code_ranges: &[(usize, usize)],
        span_start: usize,
    ) -> bool {
        let lines = ctx.raw_lines();
        let (line_num, col) = ctx.offset_to_line_col(span_start);

        // Skip matches in front matter or mkdocstrings blocks
        if ctx
            .line_info(line_num)
            .is_some_and(|info| info.in_front_matter || info.in_mkdocstrings)
        {
            return true;
        }

        // Check MkDocs markup
        let in_mkdocs_markup = lines
            .get(line_num.saturating_sub(1))
            .is_some_and(|line| is_in_mkdocs_markup(line, col.saturating_sub(1), ctx.flavor));

        // Line-level inline code fallback for cases pulldown-cmark misses
        let in_inline_code = lines
            .get(line_num.saturating_sub(1))
            .is_some_and(|line| is_in_inline_code_on_line(line, col.saturating_sub(1)));

        ctx.is_in_code_block_or_span(span_start)
            || in_inline_code
            || Self::is_in_link(ctx, span_start)
            || Self::is_in_html_tag(html_tags, span_start)
            || Self::is_in_html_code_content(html_code_ranges, span_start)
            || in_mkdocs_markup
            || is_in_math_context(ctx, span_start)
            || is_in_jsx_expression(ctx, span_start)
            || is_in_mdx_comment(ctx, span_start)
    }

    #[cfg(test)]
    fn detect_style(&self, ctx: &crate::lint_context::LintContext) -> Option<StrongStyle> {
        let html_tags = ctx.html_tags();
        let html_code_ranges = Self::compute_html_code_ranges(&html_tags);
        self.detect_style_from_spans(ctx, &html_tags, &html_code_ranges, &ctx.strong_spans)
    }

    fn detect_style_from_spans(
        &self,
        ctx: &crate::lint_context::LintContext,
        html_tags: &[crate::lint_context::HtmlTag],
        html_code_ranges: &[(usize, usize)],
        spans: &[StrongSpanDetail],
    ) -> Option<StrongStyle> {
        let mut asterisk_count = 0;
        let mut underscore_count = 0;

        for span in spans {
            if self.should_skip_span(ctx, html_tags, html_code_ranges, span.start) {
                continue;
            }

            match span_style(span) {
                StrongStyle::Asterisk => asterisk_count += 1,
                StrongStyle::Underscore => underscore_count += 1,
                StrongStyle::Consistent => {}
            }
        }

        match (asterisk_count, underscore_count) {
            (0, 0) => None,
            (_, 0) => Some(StrongStyle::Asterisk),
            (0, _) => Some(StrongStyle::Underscore),
            // In case of a tie, prefer asterisk (matches CommonMark recommendation)
            (a, u) => {
                if a >= u {
                    Some(StrongStyle::Asterisk)
                } else {
                    Some(StrongStyle::Underscore)
                }
            }
        }
    }
}

impl Rule for MD050StrongStyle {
    fn name(&self) -> &'static str {
        "MD050"
    }

    fn description(&self) -> &'static str {
        "Strong emphasis style should be consistent"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Emphasis
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let line_index = &ctx.line_index;
        let lines = ctx.raw_lines();

        let mut warnings = Vec::new();

        let spans = &ctx.strong_spans;
        let html_tags = ctx.html_tags();
        let html_code_ranges = Self::compute_html_code_ranges(&html_tags);

        let target_style = match self.config.style {
            StrongStyle::Consistent => self
                .detect_style_from_spans(ctx, &html_tags, &html_code_ranges, spans)
                .unwrap_or(StrongStyle::Asterisk),
            _ => self.config.style,
        };

        for span in spans {
            // Only flag spans that use the wrong style
            if span_style(span) == target_style {
                continue;
            }

            // Skip too-short spans
            if span.end - span.start < 4 {
                continue;
            }

            // Only check skip context for wrong-style spans (the minority)
            if self.should_skip_span(ctx, &html_tags, &html_code_ranges, span.start) {
                continue;
            }

            let (line_num, _col) = ctx.offset_to_line_col(span.start);
            let line_start = line_index.get_line_start_byte(line_num).unwrap_or(0);
            let line_content = lines.get(line_num - 1).unwrap_or(&"");
            let match_start_in_line = span.start - line_start;
            let match_len = span.end - span.start;

            let inner_text = &content[span.start + 2..span.end - 2];

            // NOTE: Intentional deviation from markdownlint behavior.
            // markdownlint reports two warnings per emphasis (one for opening marker,
            // one for closing marker). We report one warning per emphasis block because:
            // 1. The markers are semantically one unit - you can't fix one without the other
            // 2. Cleaner output - "10 issues" vs "20 issues" for 10 bold words
            // 3. The fix is atomic - replacing the entire emphasis at once
            let message = match target_style {
                StrongStyle::Asterisk => "Strong emphasis should use ** instead of __",
                StrongStyle::Underscore => "Strong emphasis should use __ instead of **",
                StrongStyle::Consistent => "Strong emphasis should use ** instead of __",
            };

            let (start_line, start_col, end_line, end_col) =
                calculate_match_range(line_num, line_content, match_start_in_line, match_len);

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: message.to_string(),
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: span.start..span.end,
                    replacement: match target_style {
                        StrongStyle::Asterisk => format!("**{inner_text}**"),
                        StrongStyle::Underscore => format!("__{inner_text}__"),
                        StrongStyle::Consistent => format!("**{inner_text}**"),
                    },
                }),
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
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(crate::rule::LintError::InvalidInput)
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Strong uses double markers, but likely_has_emphasis checks for count > 1
        ctx.content.is_empty() || !ctx.likely_has_emphasis()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let json_value = serde_json::to_value(&self.config).ok()?;
        Some((
            self.name().to_string(),
            crate::rule_config_serde::json_to_toml_value(&json_value)?,
        ))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD050Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_asterisk_style_with_asterisks() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "This is **strong text** here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_asterisk_style_with_underscores() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "This is __strong text__ here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(
            result[0]
                .message
                .contains("Strong emphasis should use ** instead of __")
        );
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 9);
    }

    #[test]
    fn test_underscore_style_with_underscores() {
        let rule = MD050StrongStyle::new(StrongStyle::Underscore);
        let content = "This is __strong text__ here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_underscore_style_with_asterisks() {
        let rule = MD050StrongStyle::new(StrongStyle::Underscore);
        let content = "This is **strong text** here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(
            result[0]
                .message
                .contains("Strong emphasis should use __ instead of **")
        );
    }

    #[test]
    fn test_consistent_style_first_asterisk() {
        let rule = MD050StrongStyle::new(StrongStyle::Consistent);
        let content = "First **strong** then __also strong__.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // First strong is **, so __ should be flagged
        assert_eq!(result.len(), 1);
        assert!(
            result[0]
                .message
                .contains("Strong emphasis should use ** instead of __")
        );
    }

    #[test]
    fn test_consistent_style_tie_prefers_asterisk() {
        let rule = MD050StrongStyle::new(StrongStyle::Consistent);
        let content = "First __strong__ then **also strong**.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Equal counts (1 vs 1), so prefer asterisks per CommonMark recommendation
        // The __ should be flagged to change to **
        assert_eq!(result.len(), 1);
        assert!(
            result[0]
                .message
                .contains("Strong emphasis should use ** instead of __")
        );
    }

    #[test]
    fn test_detect_style_asterisk() {
        let rule = MD050StrongStyle::new(StrongStyle::Consistent);
        let ctx = LintContext::new(
            "This has **strong** text.",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let style = rule.detect_style(&ctx);

        assert_eq!(style, Some(StrongStyle::Asterisk));
    }

    #[test]
    fn test_detect_style_underscore() {
        let rule = MD050StrongStyle::new(StrongStyle::Consistent);
        let ctx = LintContext::new(
            "This has __strong__ text.",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        let style = rule.detect_style(&ctx);

        assert_eq!(style, Some(StrongStyle::Underscore));
    }

    #[test]
    fn test_detect_style_none() {
        let rule = MD050StrongStyle::new(StrongStyle::Consistent);
        let ctx = LintContext::new("No strong text here.", crate::config::MarkdownFlavor::Standard, None);
        let style = rule.detect_style(&ctx);

        assert_eq!(style, None);
    }

    #[test]
    fn test_strong_in_code_block() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "```\n__strong__ in code\n```\n__strong__ outside";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only the strong outside code block should be flagged
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 4);
    }

    #[test]
    fn test_strong_in_inline_code() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "Text with `__strong__` in code and __strong__ outside.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only the strong outside inline code should be flagged
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_escaped_strong() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "This is \\__not strong\\__ but __this is__.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only the unescaped strong should be flagged
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 30);
    }

    #[test]
    fn test_fix_asterisks_to_underscores() {
        let rule = MD050StrongStyle::new(StrongStyle::Underscore);
        let content = "This is **strong** text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "This is __strong__ text.");
    }

    #[test]
    fn test_fix_underscores_to_asterisks() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "This is __strong__ text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "This is **strong** text.");
    }

    #[test]
    fn test_fix_multiple_strong() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "First __strong__ and second __also strong__.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "First **strong** and second **also strong**.");
    }

    #[test]
    fn test_fix_preserves_code_blocks() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "```\n__strong__ in code\n```\n__strong__ outside";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "```\n__strong__ in code\n```\n**strong** outside");
    }

    #[test]
    fn test_multiline_content() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "Line 1 with __strong__\nLine 2 with __another__\nLine 3 normal";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 2);
    }

    #[test]
    fn test_nested_emphasis() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "This has __strong with *emphasis* inside__.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_default_config() {
        let rule = MD050StrongStyle::new(StrongStyle::Consistent);
        let (name, _config) = rule.default_config_section().unwrap();
        assert_eq!(name, "MD050");
    }

    #[test]
    fn test_strong_in_links_not_flagged() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = r#"Instead of assigning to `self.value`, we're relying on the [`__dict__`][__dict__] in our object to hold that value instead.

Hint:

- [An article on something](https://blog.yuo.be/2018/08/16/__init_subclass__-a-simpler-way-to-implement-class-registries-in-python/ "Some details on using `__init_subclass__`")


[__dict__]: https://www.pythonmorsels.com/where-are-attributes-stored/"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // None of the __ patterns in links should be flagged
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_strong_in_links_vs_outside_links() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = r#"We're doing this because generator functions return a generator object which [is an iterator][generators are iterators] and **we need `__iter__` to return an [iterator][]**.

Instead of assigning to `self.value`, we're relying on the [`__dict__`][__dict__] in our object to hold that value instead.

This is __real strong text__ that should be flagged.

[__dict__]: https://www.pythonmorsels.com/where-are-attributes-stored/"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only the real strong text should be flagged, not the __ in links
        assert_eq!(result.len(), 1);
        assert!(
            result[0]
                .message
                .contains("Strong emphasis should use ** instead of __")
        );
        // The flagged text should be "real strong text"
        assert!(result[0].line > 4); // Should be on the line with "real strong text"
    }

    #[test]
    fn test_front_matter_not_flagged() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "---\ntitle: What's __init__.py?\nother: __value__\n---\n\nThis __should be flagged__.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only the strong text outside front matter should be flagged
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 6);
        assert!(
            result[0]
                .message
                .contains("Strong emphasis should use ** instead of __")
        );
    }

    #[test]
    fn test_html_tags_not_flagged() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = r#"# Test

This has HTML with underscores:

<iframe src="https://example.com/__init__/__repr__"> </iframe>

This __should be flagged__ as inconsistent."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only the strong text outside HTML tags should be flagged
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 7);
        assert!(
            result[0]
                .message
                .contains("Strong emphasis should use ** instead of __")
        );
    }

    #[test]
    fn test_mkdocs_keys_notation_not_flagged() {
        // Keys notation uses ++ which shouldn't be flagged as strong emphasis
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "Press ++ctrl+alt+del++ to restart.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Keys notation should not be flagged as strong emphasis
        assert!(
            result.is_empty(),
            "Keys notation should not be flagged as strong emphasis. Got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_caret_notation_not_flagged() {
        // Insert notation (^^text^^) should not be flagged as strong emphasis
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "This is ^^inserted^^ text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Insert notation should not be flagged as strong emphasis. Got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_mark_notation_not_flagged() {
        // Mark notation (==highlight==) should not be flagged
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "This is ==highlighted== text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Mark notation should not be flagged as strong emphasis. Got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_mixed_content_with_real_strong() {
        // Mixed content: MkDocs markup + real strong emphasis that should be flagged
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "Press ++ctrl++ and __underscore strong__ here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Only the real underscore strong should be flagged (not Keys notation)
        assert_eq!(result.len(), 1, "Expected 1 warning, got: {result:?}");
        assert!(
            result[0]
                .message
                .contains("Strong emphasis should use ** instead of __")
        );
    }

    #[test]
    fn test_mkdocs_icon_shortcode_not_flagged() {
        // Icon shortcodes like :material-star: should not affect strong detection
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "Click :material-check: and __this should be flagged__.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // The underscore strong should still be flagged
        assert_eq!(result.len(), 1);
        assert!(
            result[0]
                .message
                .contains("Strong emphasis should use ** instead of __")
        );
    }

    #[test]
    fn test_math_block_not_flagged() {
        // Math blocks contain _ and * characters that are not emphasis
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = r#"# Math Section

$$
E = mc^2
x_1 + x_2 = y
a**b = c
$$

This __should be flagged__ outside math.
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let result = rule.check(&ctx).unwrap();

        // Only the strong outside math block should be flagged
        assert_eq!(result.len(), 1, "Expected 1 warning, got: {result:?}");
        assert!(result[0].line > 7, "Warning should be on line after math block");
    }

    #[test]
    fn test_math_block_with_underscores_not_flagged() {
        // LaTeX subscripts use underscores that shouldn't be flagged
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = r#"$$
x_1 + x_2 + x__3 = y
\alpha__\beta
$$
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let result = rule.check(&ctx).unwrap();

        // Nothing should be flagged - all content is in math block
        assert!(
            result.is_empty(),
            "Math block content should not be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_math_block_with_asterisks_not_flagged() {
        // LaTeX multiplication uses asterisks that shouldn't be flagged
        let rule = MD050StrongStyle::new(StrongStyle::Underscore);
        let content = r#"$$
a**b = c
2 ** 3 = 8
x***y
$$
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let result = rule.check(&ctx).unwrap();

        // Nothing should be flagged - all content is in math block
        assert!(
            result.is_empty(),
            "Math block content should not be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_math_block_fix_preserves_content() {
        // Fix should not modify content inside math blocks
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = r#"$$
x__y = z
$$

This __word__ should change.
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Math block content should be unchanged
        assert!(fixed.contains("x__y = z"), "Math block content should be preserved");
        // Strong outside should be fixed
        assert!(fixed.contains("**word**"), "Strong outside math should be fixed");
    }

    #[test]
    fn test_inline_math_simple() {
        // Simple inline math without underscore patterns that could be confused with strong
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "The formula $E = mc^2$ is famous and __this__ is strong.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let result = rule.check(&ctx).unwrap();

        // __this__ should be flagged (it's outside the inline math)
        assert_eq!(
            result.len(),
            1,
            "Expected 1 warning for strong outside math. Got: {result:?}"
        );
    }

    #[test]
    fn test_multiple_math_blocks_and_strong() {
        // Test with multiple math blocks and strong emphasis between them
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = r#"# Document

$$
a = b
$$

This __should be flagged__ text.

$$
c = d
$$
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let result = rule.check(&ctx).unwrap();

        // Only the strong between math blocks should be flagged
        assert_eq!(result.len(), 1, "Expected 1 warning. Got: {result:?}");
        assert!(result[0].message.contains("**"));
    }

    #[test]
    fn test_html_tag_skip_consistency_between_check_and_fix() {
        // Verify that check() and fix() share the same HTML tag boundary logic,
        // so double underscores inside HTML attributes are skipped consistently.
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);

        let content = r#"<a href="__test__">link</a>

This __should be flagged__ text."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let check_result = rule.check(&ctx).unwrap();
        let fix_result = rule.fix(&ctx).unwrap();

        // Only the __should be flagged__ outside the HTML tag should be flagged
        assert_eq!(
            check_result.len(),
            1,
            "check() should flag exactly one emphasis outside HTML tags"
        );
        assert!(check_result[0].message.contains("**"));

        // fix() should only transform the same emphasis that check() flagged
        assert!(
            fix_result.contains("**should be flagged**"),
            "fix() should convert the flagged emphasis"
        );
        assert!(
            fix_result.contains("__test__"),
            "fix() should not modify emphasis inside HTML tags"
        );
    }

    #[test]
    fn test_detect_style_ignores_emphasis_in_inline_code_on_table_lines() {
        // In Consistent mode, detect_style() should not count emphasis markers
        // inside inline code spans on table cell lines, matching check() and fix().
        let rule = MD050StrongStyle::new(StrongStyle::Consistent);

        // The only real emphasis is **real** (asterisks). The __code__ inside
        // backtick code spans should be ignored by detect_style().
        let content = "| `__code__` | **real** |\n| --- | --- |\n| data | data |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let style = rule.detect_style(&ctx);
        // Should detect asterisk as the dominant style (underscore inside code is skipped)
        assert_eq!(style, Some(StrongStyle::Asterisk));
    }

    #[test]
    fn test_five_underscores_not_flagged() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "This is a series of underscores: _____";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "_____ should not be flagged as strong emphasis. Got: {result:?}"
        );
    }

    #[test]
    fn test_five_asterisks_not_flagged() {
        let rule = MD050StrongStyle::new(StrongStyle::Underscore);
        let content = "This is a series of asterisks: *****";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "***** should not be flagged as strong emphasis. Got: {result:?}"
        );
    }

    #[test]
    fn test_five_underscores_with_frontmatter_not_flagged() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "---\ntitle: Level 1 heading\n---\n\nThis is a series of underscores: _____\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "_____ should not be flagged. Got: {result:?}");
    }

    #[test]
    fn test_four_underscores_not_flagged() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "This is: ____";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "____ should not be flagged. Got: {result:?}");
    }

    #[test]
    fn test_four_asterisks_not_flagged() {
        let rule = MD050StrongStyle::new(StrongStyle::Underscore);
        let content = "This is: ****";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "**** should not be flagged. Got: {result:?}");
    }

    #[test]
    fn test_detect_style_ignores_underscore_sequences() {
        let rule = MD050StrongStyle::new(StrongStyle::Consistent);
        let content = "This is: _____ and also **real bold**";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let style = rule.detect_style(&ctx);
        assert_eq!(style, Some(StrongStyle::Asterisk));
    }

    #[test]
    fn test_fix_does_not_modify_underscore_sequences() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "Some _____ sequence and __real bold__ text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("_____"), "_____ should be preserved");
        assert!(fixed.contains("**real bold**"), "Real bold should be converted");
    }

    #[test]
    fn test_six_or_more_consecutive_markers_not_flagged() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        for count in [6, 7, 8, 10] {
            let underscores = "_".repeat(count);
            let asterisks = "*".repeat(count);
            let content_u = format!("Text with {underscores} here");
            let content_a = format!("Text with {asterisks} here");

            let ctx_u = LintContext::new(&content_u, crate::config::MarkdownFlavor::Standard, None);
            let ctx_a = LintContext::new(&content_a, crate::config::MarkdownFlavor::Standard, None);

            let result_u = rule.check(&ctx_u).unwrap();
            let result_a = rule.check(&ctx_a).unwrap();

            assert!(
                result_u.is_empty(),
                "{count} underscores should not be flagged. Got: {result_u:?}"
            );
            assert!(
                result_a.is_empty(),
                "{count} asterisks should not be flagged. Got: {result_a:?}"
            );
        }
    }

    #[test]
    fn test_mkdocstrings_block_not_flagged() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "# Example\n\nWe have here some **bold text**.\n\n::: my_module.MyClass\n    options:\n      members:\n        - __init__\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "__init__ inside mkdocstrings block should not be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocstrings_block_fix_preserves_content() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "# Example\n\nWe have here some **bold text**.\n\n::: my_module.MyClass\n    options:\n      members:\n        - __init__\n        - __repr__\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(
            fixed.contains("__init__"),
            "__init__ in mkdocstrings block should be preserved"
        );
        assert!(
            fixed.contains("__repr__"),
            "__repr__ in mkdocstrings block should be preserved"
        );
        assert!(fixed.contains("**bold text**"), "Real bold text should be unchanged");
    }

    #[test]
    fn test_mkdocstrings_block_with_strong_outside() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "::: my_module.MyClass\n    options:\n      members:\n        - __init__\n\nThis __should be flagged__ outside.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            1,
            "Only strong outside mkdocstrings should be flagged. Got: {result:?}"
        );
        assert_eq!(result[0].line, 6);
    }

    #[test]
    fn test_thematic_break_not_flagged() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "Before\n\n*****\n\nAfter";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Thematic break (*****) should not be flagged. Got: {result:?}"
        );

        let content2 = "Before\n\n_____\n\nAfter";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert!(
            result2.is_empty(),
            "Thematic break (_____) should not be flagged. Got: {result2:?}"
        );
    }
}
