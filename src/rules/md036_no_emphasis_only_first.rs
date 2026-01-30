//!
//! Rule MD036: No emphasis used as a heading
//!
//! See [docs/md036.md](../../docs/md036.md) for full documentation, configuration, and examples.

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::calculate_emphasis_range;
use regex::Regex;
use std::sync::LazyLock;
use toml;

mod md036_config;
pub use md036_config::HeadingStyle;
pub use md036_config::MD036Config;

// Optimize regex patterns with compilation once at startup
// Note: The content between emphasis markers should not contain other emphasis markers
// to avoid matching nested emphasis like _**text**_ or **_text_**
static RE_ASTERISK_SINGLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\*([^*_\n]+)\*\s*$").unwrap());
static RE_UNDERSCORE_SINGLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*_([^*_\n]+)_\s*$").unwrap());
static RE_ASTERISK_DOUBLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\*\*([^*_\n]+)\*\*\s*$").unwrap());
static RE_UNDERSCORE_DOUBLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*__([^*_\n]+)__\s*$").unwrap());
static LIST_MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(?:[*+-]|\d+\.)\s+").unwrap());
static BLOCKQUOTE_MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*>").unwrap());
static HEADING_MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#+\s").unwrap());
static HEADING_WITH_EMPHASIS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(#+\s+).*(?:\*\*|\*|__|_)").unwrap());
// Pattern to match common Table of Contents labels that should not be converted to headings
static TOC_LABEL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:\*\*|\*|__|_)(?:Table of Contents|Contents|TOC|Index)(?:\*\*|\*|__|_)\s*$").unwrap()
});

/// Rule MD036: Emphasis used instead of a heading
#[derive(Clone, Default)]
pub struct MD036NoEmphasisAsHeading {
    config: MD036Config,
}

impl MD036NoEmphasisAsHeading {
    pub fn new(punctuation: String) -> Self {
        Self {
            config: MD036Config {
                punctuation,
                fix: false,
                heading_style: HeadingStyle::default(),
                heading_level: crate::types::HeadingLevel::new(2).unwrap(),
            },
        }
    }

    pub fn new_with_fix(punctuation: String, fix: bool, heading_style: HeadingStyle, heading_level: u8) -> Self {
        // Validate heading level, defaulting to 2 if invalid
        let validated_level = crate::types::HeadingLevel::new(heading_level)
            .unwrap_or_else(|_| crate::types::HeadingLevel::new(2).unwrap());
        Self {
            config: MD036Config {
                punctuation,
                fix,
                heading_style,
                heading_level: validated_level,
            },
        }
    }

    pub fn from_config_struct(config: MD036Config) -> Self {
        Self { config }
    }

    /// Generate the ATX heading prefix for the configured heading level
    fn atx_prefix(&self) -> String {
        // HeadingLevel is already validated to 1-6, no clamping needed
        let level = self.config.heading_level.get();
        format!("{} ", "#".repeat(level as usize))
    }

    fn ends_with_punctuation(&self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return false;
        }
        // Check if the last character is in the punctuation set
        trimmed
            .chars()
            .last()
            .is_some_and(|ch| self.config.punctuation.contains(ch))
    }

    fn contains_link_or_code(&self, text: &str) -> bool {
        // Check for inline code: `code`
        // This is simple but effective since we're checking text that's already
        // been identified as emphasized content
        if text.contains('`') {
            return true;
        }

        // Check for markdown links: [text](url) or [text][ref]
        // We need both [ and ] for it to be a potential link
        // and either ( ) for inline links or ][ for reference links
        if text.contains('[') && text.contains(']') {
            // Check for inline link pattern [...](...)
            if text.contains("](") {
                return true;
            }
            // Check for reference link pattern [...][...] or [...][]
            if text.contains("][") || text.ends_with(']') {
                return true;
            }
        }

        false
    }

    fn is_entire_line_emphasized(
        &self,
        line: &str,
        ctx: &crate::lint_context::LintContext,
        line_num: usize,
    ) -> Option<(usize, String, usize, usize)> {
        let original_line = line;
        let line = line.trim();

        // Fast path for empty lines and lines that don't contain emphasis markers
        if line.is_empty() || (!line.contains('*') && !line.contains('_')) {
            return None;
        }

        // Skip if line is already a heading (but not a heading with emphasis)
        if HEADING_MARKER.is_match(line) && !HEADING_WITH_EMPHASIS.is_match(line) {
            return None;
        }

        // Skip if line is a Table of Contents label (common legitimate use of bold text)
        if TOC_LABEL_PATTERN.is_match(line) {
            return None;
        }

        // Skip if line is in a list, blockquote, code block, or HTML comment
        if LIST_MARKER.is_match(line)
            || BLOCKQUOTE_MARKER.is_match(line)
            || ctx
                .line_info(line_num + 1)
                .is_some_and(|info| info.in_code_block || info.in_html_comment)
        {
            return None;
        }

        // Helper closure to check common conditions for all emphasis patterns
        let check_emphasis = |text: &str, level: usize, pattern: String| -> Option<(usize, String, usize, usize)> {
            // Check if text ends with punctuation - if so, don't flag it
            if !self.config.punctuation.is_empty() && self.ends_with_punctuation(text) {
                return None;
            }
            // Skip if text contains links or inline code (matches markdownlint behavior)
            // In markdownlint, these would be multiple tokens and thus not flagged
            if self.contains_link_or_code(text) {
                return None;
            }
            // Find position in original line by looking for the emphasis pattern
            let start_pos = original_line.find(&pattern).unwrap_or(0);
            let end_pos = start_pos + pattern.len();
            Some((level, text.to_string(), start_pos, end_pos))
        };

        // Check for *emphasis* pattern (entire line)
        if let Some(caps) = RE_ASTERISK_SINGLE.captures(line) {
            let text = caps.get(1).unwrap().as_str();
            let pattern = format!("*{text}*");
            return check_emphasis(text, 1, pattern);
        }

        // Check for _emphasis_ pattern (entire line)
        if let Some(caps) = RE_UNDERSCORE_SINGLE.captures(line) {
            let text = caps.get(1).unwrap().as_str();
            let pattern = format!("_{text}_");
            return check_emphasis(text, 1, pattern);
        }

        // Check for **strong** pattern (entire line)
        if let Some(caps) = RE_ASTERISK_DOUBLE.captures(line) {
            let text = caps.get(1).unwrap().as_str();
            let pattern = format!("**{text}**");
            return check_emphasis(text, 2, pattern);
        }

        // Check for __strong__ pattern (entire line)
        if let Some(caps) = RE_UNDERSCORE_DOUBLE.captures(line) {
            let text = caps.get(1).unwrap().as_str();
            let pattern = format!("__{text}__");
            return check_emphasis(text, 2, pattern);
        }

        None
    }
}

impl Rule for MD036NoEmphasisAsHeading {
    fn name(&self) -> &'static str {
        "MD036"
    }

    fn description(&self) -> &'static str {
        "Emphasis should not be used instead of a heading"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        // Fast path for empty content or content without emphasis markers
        if content.is_empty() || (!content.contains('*') && !content.contains('_')) {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();

        for (i, line) in content.lines().enumerate() {
            // Skip obvious non-matches quickly
            if line.trim().is_empty() || (!line.contains('*') && !line.contains('_')) {
                continue;
            }

            if let Some((_level, text, start_pos, end_pos)) = self.is_entire_line_emphasized(line, ctx, i) {
                let (start_line, start_col, end_line, end_col) =
                    calculate_emphasis_range(i + 1, line, start_pos, end_pos);

                // Only include fix if auto-fix is enabled in config
                let fix = if self.config.fix {
                    let prefix = self.atx_prefix();
                    // Get the byte range for the full line content
                    let range = ctx.line_index.line_content_range(i + 1);
                    // Preserve leading whitespace by not including it in the replacement
                    let leading_ws: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                    Some(Fix {
                        range,
                        replacement: format!("{leading_ws}{prefix}{text}"),
                    })
                } else {
                    None
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!("Emphasis used instead of a heading: '{text}'"),
                    severity: Severity::Warning,
                    fix,
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Auto-fix is opt-in: only apply if explicitly enabled in config
        // When disabled, check() returns warnings without fixes, so this is a no-op
        if !self.config.fix {
            return Ok(ctx.content.to_string());
        }

        // Get warnings with their inline fixes
        let warnings = self.check(ctx)?;

        // If no warnings with fixes, return original content
        if warnings.is_empty() || !warnings.iter().any(|w| w.fix.is_some()) {
            return Ok(ctx.content.to_string());
        }

        // Collect all fixes and sort by range start (descending) to apply from end to beginning
        let mut fixes: Vec<_> = warnings
            .iter()
            .filter_map(|w| w.fix.as_ref().map(|f| (f.range.start, f.range.end, &f.replacement)))
            .collect();
        fixes.sort_by(|a, b| b.0.cmp(&a.0));

        // Apply fixes from end to beginning to preserve byte offsets
        let mut result = ctx.content.to_string();
        for (start, end, replacement) in fixes {
            if start < result.len() && end <= result.len() && start <= end {
                result.replace_range(start..end, replacement);
            }
        }

        Ok(result)
    }

    /// Check if this rule should be skipped for performance
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or has no emphasis markers
        ctx.content.is_empty() || !ctx.likely_has_emphasis()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "punctuation".to_string(),
            toml::Value::String(self.config.punctuation.clone()),
        );
        map.insert("fix".to_string(), toml::Value::Boolean(self.config.fix));
        map.insert("heading-style".to_string(), toml::Value::String("atx".to_string()));
        map.insert(
            "heading-level".to_string(),
            toml::Value::Integer(i64::from(self.config.heading_level.get())),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let punctuation = crate::config::get_rule_config_value::<String>(config, "MD036", "punctuation")
            .unwrap_or_else(|| ".,;:!?".to_string());

        let fix = crate::config::get_rule_config_value::<bool>(config, "MD036", "fix").unwrap_or(false);

        // heading_style currently only supports "atx"
        let heading_style = HeadingStyle::Atx;

        // HeadingLevel validation is handled by new_with_fix, which defaults to 2 if invalid
        let heading_level = crate::config::get_rule_config_value::<u8>(config, "MD036", "heading-level")
            .or_else(|| crate::config::get_rule_config_value::<u8>(config, "MD036", "heading_level"))
            .unwrap_or(2);

        Box::new(MD036NoEmphasisAsHeading::new_with_fix(
            punctuation,
            fix,
            heading_style,
            heading_level,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_single_asterisk_emphasis() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "*This is emphasized*\n\nRegular text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(
            result[0]
                .message
                .contains("Emphasis used instead of a heading: 'This is emphasized'")
        );
    }

    #[test]
    fn test_single_underscore_emphasis() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "_This is emphasized_\n\nRegular text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(
            result[0]
                .message
                .contains("Emphasis used instead of a heading: 'This is emphasized'")
        );
    }

    #[test]
    fn test_double_asterisk_strong() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "**This is strong**\n\nRegular text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(
            result[0]
                .message
                .contains("Emphasis used instead of a heading: 'This is strong'")
        );
    }

    #[test]
    fn test_double_underscore_strong() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "__This is strong__\n\nRegular text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(
            result[0]
                .message
                .contains("Emphasis used instead of a heading: 'This is strong'")
        );
    }

    #[test]
    fn test_emphasis_with_punctuation() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "**Important Note:**\n\nRegular text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Emphasis with punctuation should NOT be flagged (matches markdownlint)
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_emphasis_in_paragraph() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "This is a paragraph with *emphasis* in the middle.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag emphasis within a line
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_emphasis_in_list() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "- *List item with emphasis*\n- Another item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag emphasis in list items
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_emphasis_in_blockquote() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "> *Quote with emphasis*\n> Another line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag emphasis in blockquotes
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_emphasis_in_code_block() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "```\n*Not emphasis in code*\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag emphasis in code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_emphasis_in_html_comment() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "<!--\n**bigger**\ncomment\n-->";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag emphasis in HTML comments (matches markdownlint)
        assert_eq!(
            result.len(),
            0,
            "Expected no warnings for emphasis in HTML comment, got: {result:?}"
        );
    }

    #[test]
    fn test_toc_label() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "**Table of Contents**\n\n- Item 1\n- Item 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag common TOC labels
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_already_heading() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "# **Bold in heading**\n\nRegular text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag emphasis that's already in a heading
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_disabled_by_default() {
        // When fix is not enabled (default), no changes should be made
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "*Convert to heading*\n\nRegular text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Fix is opt-in, so by default no changes are made
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_disabled_preserves_content() {
        // When fix is not enabled, content is preserved
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "**Convert to heading**\n\nRegular text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Fix is opt-in, so by default no changes are made
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_enabled_single_asterisk() {
        // When fix is enabled, single asterisk emphasis is converted
        let rule = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 2);
        let content = "*Section Title*\n\nBody text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "## Section Title\n\nBody text.");
    }

    #[test]
    fn test_fix_enabled_double_asterisk() {
        // When fix is enabled, double asterisk emphasis is converted
        let rule = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 2);
        let content = "**Section Title**\n\nBody text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "## Section Title\n\nBody text.");
    }

    #[test]
    fn test_fix_enabled_single_underscore() {
        // When fix is enabled, single underscore emphasis is converted
        let rule = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 3);
        let content = "_Section Title_\n\nBody text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "### Section Title\n\nBody text.");
    }

    #[test]
    fn test_fix_enabled_double_underscore() {
        // When fix is enabled, double underscore emphasis is converted
        let rule = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 4);
        let content = "__Section Title__\n\nBody text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "#### Section Title\n\nBody text.");
    }

    #[test]
    fn test_fix_enabled_multiple_lines() {
        // When fix is enabled, multiple emphasis-as-heading lines are converted
        let rule = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 2);
        let content = "**First Section**\n\nSome text.\n\n**Second Section**\n\nMore text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(
            fixed,
            "## First Section\n\nSome text.\n\n## Second Section\n\nMore text."
        );
    }

    #[test]
    fn test_fix_enabled_skips_punctuation() {
        // When fix is enabled, lines ending with punctuation are skipped
        let rule = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 2);
        let content = "**Important Note:**\n\nBody text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should not be changed because it ends with punctuation (colon)
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_enabled_heading_level_1() {
        let rule = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 1);
        let content = "**Title**";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "# Title");
    }

    #[test]
    fn test_fix_enabled_heading_level_6() {
        let rule = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 6);
        let content = "**Subsubsubheading**";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "###### Subsubsubheading");
    }

    #[test]
    fn test_fix_preserves_trailing_newline_enabled() {
        let rule = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 2);
        let content = "**Heading**\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "## Heading\n");
    }

    #[test]
    fn test_fix_idempotent() {
        // A second fix run should produce no further changes
        let rule = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 2);
        let content = "**Section Title**\n\nBody text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed1 = rule.fix(&ctx).unwrap();
        assert_eq!(fixed1, "## Section Title\n\nBody text.");

        // Run fix again on the fixed content
        let ctx2 = LintContext::new(&fixed1, crate::config::MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed2, fixed1, "Fix should be idempotent");
    }

    #[test]
    fn test_fix_skips_lists() {
        let rule = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 2);
        let content = "- *List item*\n- Another item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // List items should not be converted
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_skips_blockquotes() {
        let rule = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 2);
        let content = "> **Quoted text**\n> More quote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Blockquotes should not be converted
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_skips_code_blocks() {
        let rule = MD036NoEmphasisAsHeading::new_with_fix(".,;:!?".to_string(), true, HeadingStyle::Atx, 2);
        let content = "```\n**Not a heading**\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Code blocks should not be converted
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_empty_punctuation_config() {
        let rule = MD036NoEmphasisAsHeading::new("".to_string());
        let content = "**Important Note:**\n\nRegular text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // With empty punctuation config, all emphasis is flagged
        assert_eq!(result.len(), 1);

        let fixed = rule.fix(&ctx).unwrap();
        // Fix is opt-in, so by default no changes are made
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_empty_punctuation_config_with_fix() {
        // With fix enabled and empty punctuation, all emphasis is converted
        let rule = MD036NoEmphasisAsHeading::new_with_fix("".to_string(), true, HeadingStyle::Atx, 2);
        let content = "**Important Note:**\n\nRegular text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // With empty punctuation and fix enabled, all emphasis is converted
        assert_eq!(fixed, "## Important Note:\n\nRegular text");
    }

    #[test]
    fn test_multiple_emphasized_lines() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "*First heading*\n\nSome text\n\n**Second heading**\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 5);
    }

    #[test]
    fn test_whitespace_handling() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "  **Indented emphasis**  \n\nRegular text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
    }

    #[test]
    fn test_nested_emphasis() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "***Not a simple emphasis***\n\nRegular text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Nested emphasis (3 asterisks) should not match our patterns
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_emphasis_with_newlines() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "*First line\nSecond line*\n\nRegular text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Multi-line emphasis should not be flagged
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_preserves_trailing_newline_disabled() {
        // When fix is disabled, trailing newline is preserved
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "*Convert to heading*\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Fix is opt-in, so by default no changes are made
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_default_config() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let (name, config) = rule.default_config_section().unwrap();
        assert_eq!(name, "MD036");

        let table = config.as_table().unwrap();
        assert_eq!(table.get("punctuation").unwrap().as_str().unwrap(), ".,;:!?");
        assert!(!table.get("fix").unwrap().as_bool().unwrap());
        assert_eq!(table.get("heading-style").unwrap().as_str().unwrap(), "atx");
        assert_eq!(table.get("heading-level").unwrap().as_integer().unwrap(), 2);
    }

    #[test]
    fn test_image_caption_scenario() {
        // Test the specific issue from #23 - bold text used as image caption
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "#### Métriques\n\n**commits par année : rumdl**\n\n![rumdl Commits By Year image](commits_by_year.png \"commits par année : rumdl\")";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should detect the bold text even though it's followed by an image
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
        assert!(result[0].message.contains("commits par année : rumdl"));

        // Warnings don't include inline fixes (fix is opt-in via config)
        assert!(result[0].fix.is_none());

        // Fix is opt-in, so by default the content is unchanged
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_bold_with_colon_no_punctuation_config() {
        // Test that with empty punctuation config, even text ending with colon is flagged
        let rule = MD036NoEmphasisAsHeading::new("".to_string());
        let content = "**commits par année : rumdl**\n\nSome text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // With empty punctuation config, this should be flagged
        assert_eq!(result.len(), 1);
        assert!(result[0].fix.is_none());
    }

    #[test]
    fn test_bold_with_colon_default_config() {
        // Test that with default punctuation config, text ending with colon is NOT flagged
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "**Important Note:**\n\nSome text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // With default punctuation including colon, this should NOT be flagged
        assert_eq!(result.len(), 0);
    }
}
