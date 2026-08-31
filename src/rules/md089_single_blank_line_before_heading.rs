//! Rule MD089: Single blank line before heading.
//!
//! Checks that every heading (ATX and Setext) is preceded by exactly one
//! blank line, except the first heading in the document (configurable).
//!
//! - 0 blank lines before heading → flagged (needs 1)
//! - 2+ blank lines before heading → flagged (needs 1)
//! - First heading in document → exempt (default)
//! - Headings inside fenced code blocks → ignored
//! - Headings inside front matter → ignored
//! - Single-line HTML comments between content and heading → transparent
//!   (not counted as content or blank)
//!
//! See [docs/md089.md](../../docs/md089.md) for full documentation and examples.

use crate::lint_context::HeadingStyle;
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

pub(crate) mod md089_config;
use md089_config::{MD089Config, MD089HeadingStyle};

/// The rule struct.
#[derive(Clone)]
pub struct MD089SingleBlankLineBeforeHeading {
    config: MD089Config,
}

impl MD089SingleBlankLineBeforeHeading {
    pub fn new() -> Self {
        Self::from_config_struct(MD089Config::default())
    }

    fn from_config_struct(config: MD089Config) -> Self {
        Self { config }
    }
}

impl Default for MD089SingleBlankLineBeforeHeading {
    fn default() -> Self {
        Self::new()
    }
}

impl MD089SingleBlankLineBeforeHeading {
    /// Whether the heading style is checked by this rule.
    fn style_checked(&self, style: &HeadingStyle) -> bool {
        let atx = self.config.heading_styles.contains(&MD089HeadingStyle::Atx);
        let setext = self.config.heading_styles.contains(&MD089HeadingStyle::Setext);
        match style {
            HeadingStyle::ATX => atx,
            HeadingStyle::Setext1 | HeadingStyle::Setext2 => setext,
        }
    }

    /// Count blank lines before `heading_idx` in `ctx.lines`.  Walks backward
    /// from the heading, counting consecutive blank lines.  Single-line HTML
    /// comments (`<!-- ... -->`) are transparent - they are not counted as
    /// content and do not reset the blank count.
    fn count_blanks_above(&self, ctx: &crate::lint_context::LintContext, heading_idx: usize) -> usize {
        let mut count = 0;
        let mut idx = heading_idx as isize - 1;
        while idx >= 0 {
            let prev = &ctx.lines[idx as usize];
            let content = prev.content(ctx.content);
            let trimmed = content.trim();
            if trimmed.is_empty() {
                count += 1;
                idx -= 1;
            } else if trimmed.starts_with("<!--") && trimmed.ends_with("-->") {
                // Single-line HTML comment - transparent
                idx -= 1;
            } else {
                break;
            }
        }
        count
    }

    /// Whether there is any non-blank, non-transparent content at or before
    /// `heading_idx` in the document.  Used to avoid requiring a blank line
    /// before a heading that opens the document (nothing can precede it).
    fn has_content_before(&self, ctx: &crate::lint_context::LintContext, heading_idx: usize) -> bool {
        for j in 0..heading_idx {
            let line = &ctx.lines[j];
            let content = line.content(ctx.content);
            let trimmed = content.trim();
            if !(trimmed.is_empty() || (trimmed.starts_with("<!--") && trimmed.ends_with("-->"))) {
                return true;
            }
        }
        false
    }

    /// Find the index of the first valid heading in the document (outside
    /// code blocks and front matter).
    fn first_heading_idx(&self, ctx: &crate::lint_context::LintContext) -> Option<usize> {
        for (i, line) in ctx.lines.iter().enumerate() {
            if line.in_code_block || line.in_front_matter {
                continue;
            }
            if let Some(ref h) = line.heading
                && h.is_valid
                && self.style_checked(&h.style)
            {
                return Some(i);
            }
        }
        None
    }
}

impl Rule for MD089SingleBlankLineBeforeHeading {
    fn name(&self) -> &'static str {
        "MD089"
    }

    fn description(&self) -> &'static str {
        "Single blank line before heading"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    fn fix_capability(&self) -> crate::rule::FixCapability {
        crate::rule::FixCapability::FullyFixable
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let first_idx = self.first_heading_idx(ctx);

        for (i, line_info) in ctx.lines.iter().enumerate() {
            // Skip code blocks, front matter, and non-heading lines
            if line_info.in_code_block || line_info.in_front_matter {
                continue;
            }
            let Some(ref heading) = line_info.heading else {
                continue;
            };
            if !heading.is_valid || !self.style_checked(&heading.style) {
                continue;
            }

            // First heading exempt?
            if self.config.first_heading_exempt && Some(i) == first_idx {
                continue;
            }

            // Count blank lines above
            let blanks = self.count_blanks_above(ctx, i);

            // Heading at the very start of the document with no content before
            // it cannot have a blank line - skip.
            if !self.has_content_before(ctx, i) {
                continue;
            }

            if blanks == 0 {
                let line_num = i + 1;
                let col = heading.marker_column + 1;
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: "Expected 1 blank line before heading, found 0".to_string(),
                    line: line_num,
                    column: col,
                    end_line: line_num,
                    end_column: col + 1,
                    severity: Severity::Warning,
                    fix: None,
                });
            } else if blanks > 1 {
                let line_num = i + 1;
                let col = heading.marker_column + 1;
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: format!("Expected 1 blank line before heading, found {blanks}"),
                    line: line_num,
                    column: col,
                    end_line: line_num,
                    end_column: col + 1,
                    severity: Severity::Warning,
                    fix: None,
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        Ok(self.fix_content(ctx))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD089Config);
}

impl MD089SingleBlankLineBeforeHeading {
    /// Build the fixed document: each heading gets exactly one blank line
    /// before it (unless it is the first heading and exempt, or heads the
    /// document and has no preceding content).
    fn fix_content(&self, ctx: &crate::lint_context::LintContext) -> String {
        let line_ending = "\n";
        let had_trailing_newline = ctx.content.ends_with('\n');
        let mut result: Vec<String> = Vec::new();
        let mut skip_count = 0;
        let first_idx = self.first_heading_idx(ctx);

        for (i, line_info) in ctx.lines.iter().enumerate() {
            if skip_count > 0 {
                skip_count -= 1;
                continue;
            }

            if line_info.in_code_block || line_info.in_front_matter {
                result.push(line_info.content(ctx.content).to_string());
                continue;
            }

            let Some(ref heading) = line_info.heading else {
                result.push(line_info.content(ctx.content).to_string());
                continue;
            };

            if !heading.is_valid || !self.style_checked(&heading.style) {
                result.push(line_info.content(ctx.content).to_string());
                // For Setext, also push the underline line
                if matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2) && i + 1 < ctx.lines.len() {
                    result.push(ctx.lines[i + 1].content(ctx.content).to_string());
                    skip_count += 1;
                }
                continue;
            }

            let is_setext = matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2);

            // First heading exempt?
            if self.config.first_heading_exempt && Some(i) == first_idx {
                result.push(line_info.content(ctx.content).to_string());
                if is_setext && i + 1 < ctx.lines.len() {
                    result.push(ctx.lines[i + 1].content(ctx.content).to_string());
                    skip_count += 1;
                }
                continue;
            }

            // Does the document have content before this heading?
            if !self.has_content_before(ctx, i) {
                result.push(line_info.content(ctx.content).to_string());
                if is_setext && i + 1 < ctx.lines.len() {
                    result.push(ctx.lines[i + 1].content(ctx.content).to_string());
                    skip_count += 1;
                }
                continue;
            }

            // Count blank lines in the result (the already-built portion),
            // applying the same transparent-line logic as check().
            let mut blanks = 0;
            let mut check_idx = result.len();
            while check_idx > 0 {
                let prev = &result[check_idx - 1];
                let trimmed = prev.trim();
                if trimmed.is_empty() {
                    blanks += 1;
                    check_idx -= 1;
                } else if trimmed.starts_with("<!--") && trimmed.ends_with("-->") {
                    check_idx -= 1;
                } else {
                    break;
                }
            }

            // Normalize: if 0 blanks, insert one; if >1, remove extras.
            if blanks == 0 {
                result.push(String::new());
            } else if blanks > 1 {
                for _ in 0..(blanks - 1) {
                    result.pop();
                }
            }

            // Push the heading line
            result.push(line_info.content(ctx.content).to_string());

            // For Setext, push the underline line
            if is_setext && i + 1 < ctx.lines.len() {
                result.push(ctx.lines[i + 1].content(ctx.content).to_string());
                skip_count += 1;
            }
        }

        // Join
        let mut output = result.join(line_ending);
        if had_trailing_newline {
            output.push('\n');
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    fn check(content: &str, config: MD089Config) -> Vec<LintWarning> {
        let rule = MD089SingleBlankLineBeforeHeading { config };
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        rule.check(&ctx).unwrap()
    }

    fn fix(content: &str, config: MD089Config) -> String {
        let rule = MD089SingleBlankLineBeforeHeading { config };
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        rule.fix(&ctx).unwrap()
    }

    fn default_config() -> MD089Config {
        MD089Config::default()
    }

    // --- Basic detection ---

    #[test]
    fn test_no_blank_before_heading() {
        let warnings = check("# Title\n## Heading\n", default_config());
        assert_eq!(warnings.len(), 1, "expected 1 warning, got {warnings:?}");
    }

    #[test]
    fn test_one_blank_before_heading() {
        let warnings = check("# Title\n\n## Heading\n", default_config());
        assert_eq!(warnings.len(), 0, "expected clean, got {warnings:?}");
    }

    #[test]
    fn test_two_blanks_before_heading() {
        let warnings = check("# Title\n\n\n## Heading\n", default_config());
        assert_eq!(warnings.len(), 1, "expected 1 warning, got {warnings:?}");
    }

    #[test]
    fn test_three_blanks_before_heading() {
        let warnings = check("# Title\n\n\n\n## Heading\n", default_config());
        assert_eq!(warnings.len(), 1, "expected 1 warning, got {warnings:?}");
    }

    // --- First heading exemption ---

    #[test]
    fn test_first_heading_exempt_by_default() {
        // First heading at line 1 - exempt, no preceding content anyway.
        // Second heading has no blank → flagged.
        let warnings = check("# Title\n## Section\n", default_config());
        assert_eq!(warnings.len(), 1, "expected 1 (second heading), got {warnings:?}");
    }

    #[test]
    fn test_first_heading_not_exempt_when_has_content_before() {
        // With first_heading_exempt = false, even the first heading must
        // have 1 blank before it if there is content before it.
        let mut config = default_config();
        config.first_heading_exempt = false;
        // First heading is at line 1 - no content before it, so no flag.
        let warnings = check("# Title\n## Section\n", config);
        assert_eq!(warnings.len(), 1, "expected 1 (second heading), got {warnings:?}");
    }

    #[test]
    fn test_first_heading_not_exempt_with_content_before() {
        let mut config = default_config();
        config.first_heading_exempt = false;
        // "text" before the heading means there IS content before it.
        let warnings = check("text\n# Title\n", config);
        assert_eq!(warnings.len(), 1, "expected 1 warning, got {warnings:?}");
    }

    // --- Heading at document start ---

    #[test]
    fn test_heading_at_line_1_clean() {
        let mut config = default_config();
        config.first_heading_exempt = false;
        let warnings = check("# Title\n\n## Section\n", config);
        assert_eq!(warnings.len(), 0, "expected clean, got {warnings:?}");
    }

    // --- Style filtering ---

    #[test]
    fn test_atx_only_skip_setext() {
        let mut config = default_config();
        config.heading_styles = vec![MD089HeadingStyle::Atx];
        let content = "# Title\n\n## ATX\n\nSome text\n======\n";
        let warnings = check(content, config);
        assert_eq!(warnings.len(), 0, "expected clean (setext ignored), got {warnings:?}");
    }

    #[test]
    fn test_setext_only_skip_atx() {
        let mut config = default_config();
        config.heading_styles = vec![MD089HeadingStyle::Setext];
        let content = "# Title\n\n## ATX\n\nSome text\n======\n";
        let warnings = check(content, config);
        // Only the setext heading "Some text / ======" is checked.
        // It has 1 blank before → clean.
        assert_eq!(warnings.len(), 0, "expected clean (atx ignored), got {warnings:?}");
    }

    // --- Code block safety ---

    #[test]
    fn test_heading_inside_code_block_ignored() {
        let content = "# Title\n\n```\n## Heading inside code block\n```\n";
        let warnings = check(content, default_config());
        assert_eq!(warnings.len(), 0, "expected clean, got {warnings:?}");
    }

    // --- Fix ---

    #[test]
    fn test_fix_no_blank() {
        let result = fix("# Title\n## Heading\n", default_config());
        assert_eq!(result, "# Title\n\n## Heading\n", "got: {result:?}");
    }

    #[test]
    fn test_fix_two_blanks() {
        let result = fix("# Title\n\n\n## Heading\n", default_config());
        assert_eq!(result, "# Title\n\n## Heading\n", "got: {result:?}");
    }

    #[test]
    fn test_fix_three_blanks() {
        let result = fix("# Title\n\n\n\n## Heading\n", default_config());
        assert_eq!(result, "# Title\n\n## Heading\n", "got: {result:?}");
    }

    #[test]
    fn test_fix_idempotent() {
        let input = "# Title\n## Heading\n\n### Sub\n";
        let first = fix(input, default_config());
        let second = fix(&first, default_config());
        assert_eq!(first, second, "fix should be idempotent");
    }

    #[test]
    fn test_fix_first_heading_not_modified() {
        let result = fix("# Title\n## Section\n", default_config());
        // First heading stays at line 1. Second heading gets a blank.
        assert_eq!(result, "# Title\n\n## Section\n", "got: {result:?}");
    }

    #[test]
    fn test_fix_setext() {
        let content = "Some text\n\n## Section\n\ntext\n====\n";
        let result = fix(content, default_config());
        // "text / ====" is a setext heading. It should have 1 blank before.
        // The content before it has 1 blank already → clean.
        assert_eq!(result, "Some text\n\n## Section\n\ntext\n====\n", "got: {result:?}");
    }

    #[test]
    fn test_fix_setext_no_blank() {
        // With first_heading_exempt = false, the ATX first heading "## Section"
        // (which follows content with no blank) gets fixed; the setext heading
        // "text / ====" already has its blank and stays untouched.
        let mut config = default_config();
        config.first_heading_exempt = false;
        let content = "Some text\n## Section\n\ntext\n====\n";
        let result = fix(content, config);
        assert_eq!(result, "Some text\n\n## Section\n\ntext\n====\n", "got: {result:?}");
    }

    #[test]
    fn test_fix_setext_heading_missing_blank() {
        // The setext heading "no blank here / ====" follows "## Section" with
        // no blank line and must get one inserted.
        let content = "# Title\n\n## Section\nno blank here\n====\n";
        let result = fix(content, default_config());
        assert_eq!(
            result, "# Title\n\n## Section\n\nno blank here\n====\n",
            "got: {result:?}"
        );
    }
}
