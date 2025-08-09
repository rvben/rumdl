use crate::utils::range_utils::{LineIndex, calculate_match_range};
use crate::utils::regex_cache::{BOLD_ASTERISK_REGEX, BOLD_UNDERSCORE_REGEX};

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::strong_style::StrongStyle;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Reference definition pattern - matches [ref]: url "title"
    static ref REF_DEF_REGEX: Regex = Regex::new(
        r#"(?m)^[ ]{0,3}\[([^\]]+)\]:\s*([^\s]+)(?:\s+(?:"([^"]*)"|'([^']*)'))?$"#
    ).unwrap();
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

        // Check reference definitions [ref]: url "title" using regex pattern
        for m in REF_DEF_REGEX.find_iter(ctx.content) {
            if m.start() <= byte_pos && byte_pos < m.end() {
                return true;
            }
        }

        false
    }

    /// Check if a byte position is within an HTML tag
    fn is_in_html_tag(&self, ctx: &crate::lint_context::LintContext, byte_pos: usize) -> bool {
        // Check HTML tags
        for html_tag in ctx.html_tags().iter() {
            if html_tag.byte_offset <= byte_pos && byte_pos < html_tag.byte_end {
                return true;
            }
        }
        false
    }

    fn detect_style(&self, ctx: &crate::lint_context::LintContext) -> Option<StrongStyle> {
        let content = ctx.content;

        // Find the first occurrence of either style that's not in a code block, link, HTML tag, or front matter
        let mut first_asterisk = None;
        for m in BOLD_ASTERISK_REGEX.find_iter(content) {
            // Skip matches in front matter
            let (line_num, _) = ctx.offset_to_line_col(m.start());
            let in_front_matter = ctx
                .line_info(line_num)
                .map(|info| info.in_front_matter)
                .unwrap_or(false);

            if !in_front_matter
                && !ctx.is_in_code_block_or_span(m.start())
                && !self.is_in_link(ctx, m.start())
                && !self.is_in_html_tag(ctx, m.start())
            {
                first_asterisk = Some(m);
                break;
            }
        }

        let mut first_underscore = None;
        for m in BOLD_UNDERSCORE_REGEX.find_iter(content) {
            // Skip matches in front matter
            let (line_num, _) = ctx.offset_to_line_col(m.start());
            let in_front_matter = ctx
                .line_info(line_num)
                .map(|info| info.in_front_matter)
                .unwrap_or(false);

            if !in_front_matter
                && !ctx.is_in_code_block_or_span(m.start())
                && !self.is_in_link(ctx, m.start())
                && !self.is_in_html_tag(ctx, m.start())
            {
                first_underscore = Some(m);
                break;
            }
        }

        match (first_asterisk, first_underscore) {
            (Some(a), Some(u)) => {
                // Whichever pattern appears first determines the style
                if a.start() < u.start() {
                    Some(StrongStyle::Asterisk)
                } else {
                    Some(StrongStyle::Underscore)
                }
            }
            (Some(_), None) => Some(StrongStyle::Asterisk),
            (None, Some(_)) => Some(StrongStyle::Underscore),
            (None, None) => None,
        }
    }

    fn is_escaped(&self, text: &str, pos: usize) -> bool {
        if pos == 0 {
            return false;
        }

        let mut backslash_count = 0;
        let mut i = pos;
        while i > 0 {
            i -= 1;
            let c = text.chars().nth(i).unwrap_or(' ');
            if c != '\\' {
                break;
            }
            backslash_count += 1;
        }
        backslash_count % 2 == 1
    }
}

impl Rule for MD050StrongStyle {
    fn name(&self) -> &'static str {
        "MD050"
    }

    fn description(&self) -> &'static str {
        "Strong emphasis style should be consistent"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let target_style = match self.config.style {
            StrongStyle::Consistent => self.detect_style(ctx).unwrap_or(StrongStyle::Asterisk),
            _ => self.config.style,
        };

        let strong_regex = match target_style {
            StrongStyle::Asterisk => &*BOLD_UNDERSCORE_REGEX,
            StrongStyle::Underscore => &*BOLD_ASTERISK_REGEX,
            StrongStyle::Consistent => {
                // This case is handled separately in the calling code
                // but fallback to asterisk style for safety
                &*BOLD_UNDERSCORE_REGEX
            }
        };

        // Track byte position for each line
        let mut byte_pos = 0;

        for (line_num, line) in content.lines().enumerate() {
            // Skip if this line is in front matter
            if let Some(line_info) = ctx.line_info(line_num + 1)
                && line_info.in_front_matter
            {
                byte_pos += line.len() + 1; // +1 for newline
                continue;
            }

            for m in strong_regex.find_iter(line) {
                // Calculate the byte position of this match in the document
                let match_byte_pos = byte_pos + m.start();

                // Skip if this strong text is inside a code block, code span, link, or HTML tag
                if ctx.is_in_code_block_or_span(match_byte_pos)
                    || self.is_in_link(ctx, match_byte_pos)
                    || self.is_in_html_tag(ctx, match_byte_pos)
                {
                    continue;
                }

                if !self.is_escaped(line, m.start()) {
                    let text = &line[m.start() + 2..m.end() - 2];
                    let message = match target_style {
                        StrongStyle::Asterisk => "Strong emphasis should use ** instead of __",
                        StrongStyle::Underscore => "Strong emphasis should use __ instead of **",
                        StrongStyle::Consistent => {
                            // This case is handled separately in the calling code
                            // but fallback to asterisk style for safety
                            "Strong emphasis should use ** instead of __"
                        }
                    };

                    // Calculate precise character range for the entire strong emphasis
                    let (start_line, start_col, end_line, end_col) =
                        calculate_match_range(line_num + 1, line, m.start(), m.len());

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: message.to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range(line_num + 1, m.start() + 1),
                            replacement: match target_style {
                                StrongStyle::Asterisk => format!("**{text}**"),
                                StrongStyle::Underscore => format!("__{text}__"),
                                StrongStyle::Consistent => {
                                    // This case is handled separately in the calling code
                                    // but fallback to asterisk style for safety
                                    format!("**{text}**")
                                }
                            },
                        }),
                    });
                }
            }

            // Update byte position for next line
            byte_pos += line.len() + 1; // +1 for newline
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        let target_style = match self.config.style {
            StrongStyle::Consistent => self.detect_style(ctx).unwrap_or(StrongStyle::Asterisk),
            _ => self.config.style,
        };

        let strong_regex = match target_style {
            StrongStyle::Asterisk => &*BOLD_UNDERSCORE_REGEX,
            StrongStyle::Underscore => &*BOLD_ASTERISK_REGEX,
            StrongStyle::Consistent => {
                // This case is handled separately in the calling code
                // but fallback to asterisk style for safety
                &*BOLD_UNDERSCORE_REGEX
            }
        };

        // Store matches with their positions

        let matches: Vec<(usize, usize)> = strong_regex
            .find_iter(content)
            .filter(|m| {
                // Skip matches in front matter
                let (line_num, _) = ctx.offset_to_line_col(m.start());
                if let Some(line_info) = ctx.line_info(line_num)
                    && line_info.in_front_matter
                {
                    return false;
                }
                !ctx.is_in_code_block_or_span(m.start())
                    && !self.is_in_link(ctx, m.start())
                    && !self.is_in_html_tag(ctx, m.start())
            })
            .filter(|m| !self.is_escaped(content, m.start()))
            .map(|m| (m.start(), m.end()))
            .collect();

        // Process matches in reverse order to maintain correct indices

        let mut result = content.to_string();
        for (start, end) in matches.into_iter().rev() {
            let text = &result[start + 2..end - 2];
            let replacement = match target_style {
                StrongStyle::Asterisk => format!("**{text}**"),
                StrongStyle::Underscore => format!("__{text}__"),
                StrongStyle::Consistent => {
                    // This case is handled separately in the calling code
                    // but fallback to asterisk style for safety
                    format!("**{text}**")
                }
            };
            result.replace_range(start..end, &replacement);
        }

        Ok(result)
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || (!ctx.content.contains("**") && !ctx.content.contains("__"))
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_asterisk_style_with_underscores() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "This is __strong text__ here.";
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_underscore_style_with_asterisks() {
        let rule = MD050StrongStyle::new(StrongStyle::Underscore);
        let content = "This is **strong text** here.";
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
    fn test_consistent_style_first_underscore() {
        let rule = MD050StrongStyle::new(StrongStyle::Consistent);
        let content = "First __strong__ then **also strong**.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // First strong is __, so ** should be flagged
        assert_eq!(result.len(), 1);
        assert!(
            result[0]
                .message
                .contains("Strong emphasis should use __ instead of **")
        );
    }

    #[test]
    fn test_detect_style_asterisk() {
        let rule = MD050StrongStyle::new(StrongStyle::Consistent);
        let ctx = LintContext::new("This has **strong** text.");
        let style = rule.detect_style(&ctx);

        assert_eq!(style, Some(StrongStyle::Asterisk));
    }

    #[test]
    fn test_detect_style_underscore() {
        let rule = MD050StrongStyle::new(StrongStyle::Consistent);
        let ctx = LintContext::new("This has __strong__ text.");
        let style = rule.detect_style(&ctx);

        assert_eq!(style, Some(StrongStyle::Underscore));
    }

    #[test]
    fn test_detect_style_none() {
        let rule = MD050StrongStyle::new(StrongStyle::Consistent);
        let ctx = LintContext::new("No strong text here.");
        let style = rule.detect_style(&ctx);

        assert_eq!(style, None);
    }

    #[test]
    fn test_strong_in_code_block() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "```\n__strong__ in code\n```\n__strong__ outside";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Only the strong outside code block should be flagged
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 4);
    }

    #[test]
    fn test_strong_in_inline_code() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "Text with `__strong__` in code and __strong__ outside.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Only the strong outside inline code should be flagged
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_escaped_strong() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "This is \\__not strong\\__ but __this is__.";
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "This is __strong__ text.");
    }

    #[test]
    fn test_fix_underscores_to_asterisks() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "This is __strong__ text.";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "This is **strong** text.");
    }

    #[test]
    fn test_fix_multiple_strong() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "First __strong__ and second __also strong__.";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "First **strong** and second **also strong**.");
    }

    #[test]
    fn test_fix_preserves_code_blocks() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "```\n__strong__ in code\n```\n__strong__ outside";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "```\n__strong__ in code\n```\n**strong** outside");
    }

    #[test]
    fn test_multiline_content() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "Line 1 with __strong__\nLine 2 with __another__\nLine 3 normal";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 2);
    }

    #[test]
    fn test_nested_emphasis() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "This has __strong with *emphasis* inside__.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
        let content = "";
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
}
