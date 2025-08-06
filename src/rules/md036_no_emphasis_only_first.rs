//!
//! Rule MD036: No emphasis used as a heading
//!
//! See [docs/md036.md](../../docs/md036.md) for full documentation, configuration, and examples.

use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::document_structure::DocumentStructure;
use crate::utils::range_utils::calculate_emphasis_range;
use lazy_static::lazy_static;
use regex::Regex;
use toml;

mod md036_config;
use md036_config::MD036Config;

lazy_static! {
    // Optimize regex patterns with compilation once at startup
    static ref RE_ASTERISK_SINGLE: Regex = Regex::new(r"^\s*\*([^*\n]+)\*\s*$").unwrap();
    static ref RE_UNDERSCORE_SINGLE: Regex = Regex::new(r"^\s*_([^_\n]+)_\s*$").unwrap();
    static ref RE_ASTERISK_DOUBLE: Regex = Regex::new(r"^\s*\*\*([^*\n]+)\*\*\s*$").unwrap();
    static ref RE_UNDERSCORE_DOUBLE: Regex = Regex::new(r"^\s*__([^_\n]+)__\s*$").unwrap();
    static ref LIST_MARKER: Regex = Regex::new(r"^\s*(?:[*+-]|\d+\.)\s+").unwrap();
    static ref BLOCKQUOTE_MARKER: Regex = Regex::new(r"^\s*>").unwrap();
    static ref FENCED_CODE_BLOCK_START: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,})").unwrap();
    static ref HEADING_MARKER: Regex = Regex::new(r"^#+\s").unwrap();
    static ref HEADING_WITH_EMPHASIS: Regex = Regex::new(r"^(#+\s+).*(?:\*\*|\*|__|_)").unwrap();
    // Pattern to match common Table of Contents labels that should not be converted to headings
    static ref TOC_LABEL_PATTERN: Regex = Regex::new(r"^\s*(?:\*\*|\*|__|_)(?:Table of Contents|Contents|TOC|Index)(?:\*\*|\*|__|_)\s*$").unwrap();
}

/// Rule MD036: Emphasis used instead of a heading
#[derive(Clone, Default)]
pub struct MD036NoEmphasisAsHeading {
    config: MD036Config,
}

impl MD036NoEmphasisAsHeading {
    pub fn new(punctuation: String) -> Self {
        Self {
            config: MD036Config { punctuation },
        }
    }

    pub fn from_config_struct(config: MD036Config) -> Self {
        Self { config }
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

    fn is_entire_line_emphasized(
        &self,
        line: &str,
        doc_structure: &DocumentStructure,
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

        // Skip if line is in a list, blockquote, or code block using DocumentStructure
        if LIST_MARKER.is_match(line)
            || BLOCKQUOTE_MARKER.is_match(line)
            || doc_structure.is_in_code_block(line_num + 1)
        // line_num is 0-based, but DocumentStructure expects 1-based
        {
            return None;
        }

        // Check specific patterns directly without additional requirements
        // Check for *emphasis* pattern (entire line)
        if let Some(caps) = RE_ASTERISK_SINGLE.captures(line) {
            let text = caps.get(1).unwrap().as_str();
            // Check if text ends with punctuation - if so, don't flag it
            if !self.config.punctuation.is_empty() && self.ends_with_punctuation(text) {
                return None;
            }
            let _full_match = caps.get(0).unwrap();
            // Find position in original line by looking for the emphasis pattern
            let pattern = format!("*{text}*");
            let start_pos = original_line.find(&pattern).unwrap_or(0);
            let end_pos = start_pos + pattern.len();
            return Some((1, text.to_string(), start_pos, end_pos));
        }

        // Check for _emphasis_ pattern (entire line)
        if let Some(caps) = RE_UNDERSCORE_SINGLE.captures(line) {
            let text = caps.get(1).unwrap().as_str();
            // Check if text ends with punctuation - if so, don't flag it
            if !self.config.punctuation.is_empty() && self.ends_with_punctuation(text) {
                return None;
            }
            let _full_match = caps.get(0).unwrap();
            // Find position in original line by looking for the emphasis pattern
            let pattern = format!("_{text}_");
            let start_pos = original_line.find(&pattern).unwrap_or(0);
            let end_pos = start_pos + pattern.len();
            return Some((1, text.to_string(), start_pos, end_pos));
        }

        // Check for **strong** pattern (entire line)
        if let Some(caps) = RE_ASTERISK_DOUBLE.captures(line) {
            let text = caps.get(1).unwrap().as_str();
            // Check if text ends with punctuation - if so, don't flag it
            if !self.config.punctuation.is_empty() && self.ends_with_punctuation(text) {
                return None;
            }
            let _full_match = caps.get(0).unwrap();
            // Find position in original line by looking for the emphasis pattern
            let pattern = format!("**{text}**");
            let start_pos = original_line.find(&pattern).unwrap_or(0);
            let end_pos = start_pos + pattern.len();
            return Some((2, text.to_string(), start_pos, end_pos));
        }

        // Check for __strong__ pattern (entire line)
        if let Some(caps) = RE_UNDERSCORE_DOUBLE.captures(line) {
            let text = caps.get(1).unwrap().as_str();
            // Check if text ends with punctuation - if so, don't flag it
            if !self.config.punctuation.is_empty() && self.ends_with_punctuation(text) {
                return None;
            }
            let _full_match = caps.get(0).unwrap();
            // Find position in original line by looking for the emphasis pattern
            let pattern = format!("__{text}__");
            let start_pos = original_line.find(&pattern).unwrap_or(0);
            let end_pos = start_pos + pattern.len();
            return Some((2, text.to_string(), start_pos, end_pos));
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

        // Use the optimized document structure approach
        let doc_structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &doc_structure)
    }

    /// Optimized check using pre-computed document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> LintResult {
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

            if let Some((_level, text, start_pos, end_pos)) = self.is_entire_line_emphasized(line, doc_structure, i) {
                let (start_line, start_col, end_line, end_col) =
                    calculate_emphasis_range(i + 1, line, start_pos, end_pos);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!("Emphasis used instead of a heading: '{text}'"),
                    severity: Severity::Warning,
                    fix: None, // No automatic fix - too risky to convert to heading
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // MD036 does not provide automatic fixes
        // Converting bold text to headings is too risky and can corrupt documents
        // Users should manually decide if bold text should be a heading
        Ok(ctx.content.to_string())
    }

    /// Check if this rule should be skipped for performance
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or has no emphasis markers
        ctx.content.is_empty() || (!ctx.content.contains('*') && !ctx.content.contains('_'))
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
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let punctuation = crate::config::get_rule_config_value::<String>(config, "MD036", "punctuation")
            .unwrap_or_else(|| ".,;:!?".to_string());

        Box::new(MD036NoEmphasisAsHeading::new(punctuation))
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Emphasis with punctuation should NOT be flagged (matches markdownlint)
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_emphasis_in_paragraph() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "This is a paragraph with *emphasis* in the middle.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should not flag emphasis within a line
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_emphasis_in_list() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "- *List item with emphasis*\n- Another item";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should not flag emphasis in list items
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_emphasis_in_blockquote() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "> *Quote with emphasis*\n> Another line";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should not flag emphasis in blockquotes
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_emphasis_in_code_block() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "```\n*Not emphasis in code*\n```";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should not flag emphasis in code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_toc_label() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "**Table of Contents**\n\n- Item 1\n- Item 2";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should not flag common TOC labels
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_already_heading() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "# **Bold in heading**\n\nRegular text";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should not flag emphasis that's already in a heading
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_no_changes() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "*Convert to heading*\n\nRegular text";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // MD036 no longer provides automatic fixes
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_preserves_content() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "**Convert to heading**\n\nRegular text";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // MD036 no longer provides automatic fixes
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_empty_punctuation_config() {
        let rule = MD036NoEmphasisAsHeading::new("".to_string());
        let content = "**Important Note:**\n\nRegular text";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // With empty punctuation config, all emphasis is flagged
        assert_eq!(result.len(), 1);

        let fixed = rule.fix(&ctx).unwrap();
        // MD036 no longer provides automatic fixes
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_multiple_emphasized_lines() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "*First heading*\n\nSome text\n\n**Second heading**\n\nMore text";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 5);
    }

    #[test]
    fn test_whitespace_handling() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "  **Indented emphasis**  \n\nRegular text";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
    }

    #[test]
    fn test_nested_emphasis() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "***Not a simple emphasis***\n\nRegular text";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Nested emphasis (3 asterisks) should not match our patterns
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_emphasis_with_newlines() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "*First line\nSecond line*\n\nRegular text";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Multi-line emphasis should not be flagged
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "*Convert to heading*\n";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // MD036 no longer provides automatic fixes
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_default_config() {
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let (name, config) = rule.default_config_section().unwrap();
        assert_eq!(name, "MD036");

        let table = config.as_table().unwrap();
        assert_eq!(table.get("punctuation").unwrap().as_str().unwrap(), ".,;:!?");
    }

    #[test]
    fn test_image_caption_scenario() {
        // Test the specific issue from #23 - bold text used as image caption
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
        let content = "#### Métriques\n\n**commits par année : rumdl**\n\n![rumdl Commits By Year image](commits_by_year.png \"commits par année : rumdl\")";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should detect the bold text even though it's followed by an image
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
        assert!(result[0].message.contains("commits par année : rumdl"));

        // But should NOT provide a fix
        assert!(result[0].fix.is_none());

        // And the fix method should return unchanged content
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_bold_with_colon_no_punctuation_config() {
        // Test that with empty punctuation config, even text ending with colon is flagged
        let rule = MD036NoEmphasisAsHeading::new("".to_string());
        let content = "**commits par année : rumdl**\n\nSome text";
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // With default punctuation including colon, this should NOT be flagged
        assert_eq!(result.len(), 0);
    }
}
