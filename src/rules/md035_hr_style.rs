//!
//! Rule MD035: Horizontal rule style
//!
//! See [docs/md035.md](../../docs/md035.md) for full documentation, configuration, and examples.

use crate::utils::range_utils::calculate_line_range;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use toml;

mod md035_config;
use md035_config::MD035Config;

/// Represents the style for horizontal rules
#[derive(Clone, Default)]
pub struct MD035HRStyle {
    config: MD035Config,
}

impl MD035HRStyle {
    pub fn new(style: String) -> Self {
        Self {
            config: MD035Config { style },
        }
    }

    pub fn from_config_struct(config: MD035Config) -> Self {
        Self { config }
    }

    fn is_horizontal_rule(line: &str) -> bool {
        crate::utils::thematic_break::is_thematic_break(line)
    }

    /// Check if a line might be a Setext heading underline
    fn is_potential_setext_heading(lines: &[&str], i: usize) -> bool {
        if i == 0 {
            return false; // First line can't be a Setext heading underline
        }

        let line = lines[i].trim();
        let prev_line = lines[i - 1].trim();

        let is_dash_line = !line.is_empty() && line.chars().all(|c| c == '-');
        let is_equals_line = !line.is_empty() && line.chars().all(|c| c == '=');
        let prev_line_has_content = !prev_line.is_empty() && !Self::is_horizontal_rule(prev_line);
        (is_dash_line || is_equals_line) && prev_line_has_content
    }

    /// Find the most prevalent HR style in the document (excluding setext headings, code blocks, and frontmatter)
    fn most_prevalent_hr_style(lines: &[&str], ctx: &crate::lint_context::LintContext) -> Option<String> {
        use std::collections::HashMap;
        let mut counts: HashMap<&str, usize> = HashMap::new();
        let mut order: Vec<&str> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            // Skip if this line is in frontmatter, code block, or MkDocs markdown HTML div
            if let Some(line_info) = ctx.lines.get(i)
                && (line_info.in_front_matter || line_info.in_code_block || line_info.in_mkdocs_html_markdown)
            {
                continue;
            }

            if Self::is_horizontal_rule(line) && !Self::is_potential_setext_heading(lines, i) {
                let style = line.trim();
                let counter = counts.entry(style).or_insert(0);
                *counter += 1;
                if *counter == 1 {
                    order.push(style);
                }
            }
        }
        // Find the style with the highest count, breaking ties by first encountered
        counts
            .iter()
            .max_by_key(|&(style, count)| {
                (
                    *count,
                    -(order.iter().position(|&s| s == *style).unwrap_or(usize::MAX) as isize),
                )
            })
            .map(|(style, _)| style.to_string())
    }
}

impl Rule for MD035HRStyle {
    fn name(&self) -> &'static str {
        "MD035"
    }

    fn description(&self) -> &'static str {
        "Horizontal rule style"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let line_index = &ctx.line_index;

        let mut warnings = Vec::new();
        let lines = ctx.raw_lines();

        // Use the configured style or find the most prevalent HR style
        let expected_style = if self.config.style.is_empty() || self.config.style == "consistent" {
            Self::most_prevalent_hr_style(lines, ctx).unwrap_or_else(|| "---".to_string())
        } else {
            self.config.style.clone()
        };

        for (i, line) in lines.iter().enumerate() {
            // Skip if this line is in frontmatter, code block, or MkDocs markdown HTML div (grid cards use indented HRs)
            if let Some(line_info) = ctx.lines.get(i)
                && (line_info.in_front_matter || line_info.in_code_block || line_info.in_mkdocs_html_markdown)
            {
                continue;
            }

            // Skip if this is a potential Setext heading underline
            if Self::is_potential_setext_heading(lines, i) {
                continue;
            }

            if Self::is_horizontal_rule(line) {
                // Check if this HR matches the expected style
                let has_indentation = line.len() > line.trim_start().len();
                let style_mismatch = line.trim() != expected_style;

                if style_mismatch || has_indentation {
                    // Calculate precise character range for the entire horizontal rule
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, line);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: if has_indentation {
                            "Horizontal rule should not be indented".to_string()
                        } else {
                            format!("Horizontal rule style should be \"{expected_style}\"")
                        },
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range_with_length(i + 1, 1, line.chars().count()),
                            replacement: expected_style.clone(),
                        }),
                    });
                }
            }
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // HR can use -, *, or _
        ctx.content.is_empty() || (!ctx.has_char('-') && !ctx.has_char('*') && !ctx.has_char('_'))
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert("style".to_string(), toml::Value::String(self.config.style.clone()));
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style = crate::config::get_rule_config_value::<String>(config, "MD035", "style")
            .unwrap_or_else(|| "consistent".to_string());
        Box::new(MD035HRStyle::new(style))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_is_horizontal_rule() {
        // Valid horizontal rules
        assert!(MD035HRStyle::is_horizontal_rule("---"));
        assert!(MD035HRStyle::is_horizontal_rule("----"));
        assert!(MD035HRStyle::is_horizontal_rule("***"));
        assert!(MD035HRStyle::is_horizontal_rule("****"));
        assert!(MD035HRStyle::is_horizontal_rule("___"));
        assert!(MD035HRStyle::is_horizontal_rule("____"));
        assert!(MD035HRStyle::is_horizontal_rule("- - -"));
        assert!(MD035HRStyle::is_horizontal_rule("* * *"));
        assert!(MD035HRStyle::is_horizontal_rule("_ _ _"));
        assert!(MD035HRStyle::is_horizontal_rule("  ---  ")); // With surrounding whitespace

        // Invalid horizontal rules
        assert!(!MD035HRStyle::is_horizontal_rule("--")); // Too few characters
        assert!(!MD035HRStyle::is_horizontal_rule("**"));
        assert!(!MD035HRStyle::is_horizontal_rule("__"));
        assert!(!MD035HRStyle::is_horizontal_rule("- -")); // Too few repetitions
        assert!(!MD035HRStyle::is_horizontal_rule("* *"));
        assert!(!MD035HRStyle::is_horizontal_rule("_ _"));
        assert!(!MD035HRStyle::is_horizontal_rule("text"));
        assert!(!MD035HRStyle::is_horizontal_rule(""));
    }

    #[test]
    fn test_is_potential_setext_heading() {
        let lines = vec!["Heading 1", "=========", "Content", "Heading 2", "---", "More content"];

        // Valid Setext headings
        assert!(MD035HRStyle::is_potential_setext_heading(&lines, 1)); // ========= under "Heading 1"
        assert!(MD035HRStyle::is_potential_setext_heading(&lines, 4)); // --- under "Heading 2"

        // Not Setext headings
        assert!(!MD035HRStyle::is_potential_setext_heading(&lines, 0)); // First line can't be underline
        assert!(!MD035HRStyle::is_potential_setext_heading(&lines, 2)); // "Content" is not an underline

        let lines2 = vec!["", "---", "Content"];
        assert!(!MD035HRStyle::is_potential_setext_heading(&lines2, 1)); // Empty line above

        let lines3 = vec!["***", "---"];
        assert!(!MD035HRStyle::is_potential_setext_heading(&lines3, 1)); // HR above
    }

    #[test]
    fn test_most_prevalent_hr_style() {
        // Single style (with blank lines to avoid Setext interpretation)
        let content = "Content\n\n---\n\nMore\n\n---\n\nText";
        let lines: Vec<&str> = content.lines().collect();
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            MD035HRStyle::most_prevalent_hr_style(&lines, &ctx),
            Some("---".to_string())
        );

        // Multiple styles, one more prevalent
        let content = "Content\n\n---\n\nMore\n\n***\n\nText\n\n---";
        let lines: Vec<&str> = content.lines().collect();
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            MD035HRStyle::most_prevalent_hr_style(&lines, &ctx),
            Some("---".to_string())
        );

        // Multiple styles, tie broken by first encountered
        let content = "Content\n\n***\n\nMore\n\n---\n\nText";
        let lines: Vec<&str> = content.lines().collect();
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            MD035HRStyle::most_prevalent_hr_style(&lines, &ctx),
            Some("***".to_string())
        );

        // No horizontal rules
        let content = "Just\nRegular\nContent";
        let lines: Vec<&str> = content.lines().collect();
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(MD035HRStyle::most_prevalent_hr_style(&lines, &ctx), None);

        // Exclude Setext headings
        let content = "Heading\n---\nContent\n\n***";
        let lines: Vec<&str> = content.lines().collect();
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            MD035HRStyle::most_prevalent_hr_style(&lines, &ctx),
            Some("***".to_string())
        );
    }

    #[test]
    fn test_consistent_style() {
        let rule = MD035HRStyle::new("consistent".to_string());
        let content = "Content\n\n---\n\nMore\n\n***\n\nText\n\n---";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag the *** as it doesn't match the most prevalent style ---
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 7);
        assert!(result[0].message.contains("Horizontal rule style should be \"---\""));
    }

    #[test]
    fn test_specific_style_dashes() {
        let rule = MD035HRStyle::new("---".to_string());
        let content = "Content\n\n***\n\nMore\n\n___\n\nText";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag both *** and ___ as they don't match ---
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 3);
        assert_eq!(result[1].line, 7);
        assert!(result[0].message.contains("Horizontal rule style should be \"---\""));
    }

    #[test]
    fn test_indented_horizontal_rule() {
        let rule = MD035HRStyle::new("---".to_string());
        let content = "Content\n\n  ---\n\nMore";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
        assert_eq!(result[0].message, "Horizontal rule should not be indented");
    }

    #[test]
    fn test_setext_heading_not_flagged() {
        let rule = MD035HRStyle::new("***".to_string());
        let content = "Heading\n---\nContent\n***";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag the --- under "Heading" as it's a Setext heading
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_consistent_style() {
        let rule = MD035HRStyle::new("consistent".to_string());
        let content = "Content\n\n---\n\nMore\n\n***\n\nText\n\n---";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "Content\n\n---\n\nMore\n\n---\n\nText\n\n---";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_specific_style() {
        let rule = MD035HRStyle::new("***".to_string());
        let content = "Content\n\n---\n\nMore\n\n___\n\nText";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "Content\n\n***\n\nMore\n\n***\n\nText";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_preserves_setext_headings() {
        let rule = MD035HRStyle::new("***".to_string());
        let content = "Heading 1\n=========\nHeading 2\n---\nContent\n\n---";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "Heading 1\n=========\nHeading 2\n---\nContent\n\n***";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_removes_indentation() {
        let rule = MD035HRStyle::new("---".to_string());
        let content = "Content\n\n  ***\n\nMore\n\n   ___\n\nText";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = "Content\n\n---\n\nMore\n\n---\n\nText";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_spaced_styles() {
        let rule = MD035HRStyle::new("* * *".to_string());
        let content = "Content\n\n- - -\n\nMore\n\n_ _ _\n\nText";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("Horizontal rule style should be \"* * *\""));
    }

    #[test]
    fn test_empty_style_uses_consistent() {
        let rule = MD035HRStyle::new("".to_string());
        let content = "Content\n\n---\n\nMore\n\n***\n\nText";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Empty style should behave like "consistent"
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 7);
    }

    #[test]
    fn test_all_hr_styles_consistent() {
        let rule = MD035HRStyle::new("consistent".to_string());
        let content = "Content\n---\nMore\n---\nText\n---";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // All HRs are the same style, should not flag anything
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_no_horizontal_rules() {
        let rule = MD035HRStyle::new("---".to_string());
        let content = "Just regular content\nNo horizontal rules here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_mixed_spaced_and_unspaced() {
        let rule = MD035HRStyle::new("consistent".to_string());
        let content = "Content\n\n---\n\nMore\n\n- - -\n\nText";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag the spaced style as inconsistent
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 7);
    }

    #[test]
    fn test_trailing_whitespace_in_hr() {
        let rule = MD035HRStyle::new("---".to_string());
        let content = "Content\n\n---   \n\nMore";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Trailing whitespace is OK for HRs
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_hr_in_code_block_not_flagged() {
        let rule = MD035HRStyle::new("---".to_string());
        let content =
            "Text\n\n```bash\n----------------------------------------------------------------------\n```\n\nMore";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag horizontal rule patterns inside code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_hr_in_code_span_not_flagged() {
        let rule = MD035HRStyle::new("---".to_string());
        let content = "Text with inline `---` code span";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag horizontal rule patterns inside code spans
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_hr_with_extra_characters() {
        let rule = MD035HRStyle::new("---".to_string());
        let content = "Content\n-----\nMore\n--------\nText";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Extra characters in the same style should not be flagged
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_default_config() {
        let rule = MD035HRStyle::new("consistent".to_string());
        let (name, config) = rule.default_config_section().unwrap();
        assert_eq!(name, "MD035");

        let table = config.as_table().unwrap();
        assert_eq!(table.get("style").unwrap().as_str().unwrap(), "consistent");
    }

    #[test]
    fn test_fix_skips_mkdocs_html_markdown() {
        // MkDocs grid cards use `---` inside `<div markdown>` blocks as card separators
        // fix() should not replace these with a different HR style
        let rule = MD035HRStyle::new("***".to_string());

        let content = "Some content\n\n***\n\n<div class=\"grid cards\" markdown>\n\n- Card 1 content\n\n    ---\n\n    Card 1 footer\n\n</div>\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);

        // check() should not flag the --- inside the div markdown block
        let warnings = rule.check(&ctx).unwrap();
        for w in &warnings {
            assert_ne!(w.line, 9, "check() should not flag --- inside <div markdown> block");
        }

        // fix() should not modify the --- inside the div markdown block
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("    ---"),
            "fix() should preserve --- inside <div markdown> block, got: {fixed}"
        );
    }

    #[test]
    fn test_is_horizontal_rule_edge_cases() {
        // Valid: many dashes/asterisks/underscores
        assert!(MD035HRStyle::is_horizontal_rule("----------"));
        assert!(MD035HRStyle::is_horizontal_rule("**********"));
        assert!(MD035HRStyle::is_horizontal_rule("__________"));

        // Valid: spaced with 4+ markers
        assert!(MD035HRStyle::is_horizontal_rule("- - - -"));
        assert!(MD035HRStyle::is_horizontal_rule("* * * * *"));
        assert!(MD035HRStyle::is_horizontal_rule("_ _ _ _ _ _"));

        // Valid: spaced with multiple spaces between markers
        assert!(MD035HRStyle::is_horizontal_rule("*   *   *"));
        assert!(MD035HRStyle::is_horizontal_rule("-    -    -"));
        assert!(MD035HRStyle::is_horizontal_rule("_  _  _"));

        // Valid: trailing space after compact HR
        assert!(MD035HRStyle::is_horizontal_rule("--- "));
        assert!(MD035HRStyle::is_horizontal_rule("*** "));
        assert!(MD035HRStyle::is_horizontal_rule("___ "));

        // Valid: trailing space after spaced HR
        assert!(MD035HRStyle::is_horizontal_rule("- - - "));
        assert!(MD035HRStyle::is_horizontal_rule("* * * "));

        // Invalid: mixed marker characters
        assert!(!MD035HRStyle::is_horizontal_rule("-*-"));
        assert!(!MD035HRStyle::is_horizontal_rule("- * -"));
        assert!(!MD035HRStyle::is_horizontal_rule("_-_"));
        assert!(!MD035HRStyle::is_horizontal_rule("*_*"));

        // Invalid: text after markers
        assert!(!MD035HRStyle::is_horizontal_rule("---text"));
        assert!(!MD035HRStyle::is_horizontal_rule("***text"));
        assert!(!MD035HRStyle::is_horizontal_rule("- - - text"));

        // Invalid: only two markers (spaced)
        assert!(!MD035HRStyle::is_horizontal_rule("- -"));
        assert!(!MD035HRStyle::is_horizontal_rule("* *"));
        assert!(!MD035HRStyle::is_horizontal_rule("_ _"));

        // Invalid: letters mixed in
        assert!(!MD035HRStyle::is_horizontal_rule("-a-b-"));
        assert!(!MD035HRStyle::is_horizontal_rule("*x*x*"));

        // Invalid: single character
        assert!(!MD035HRStyle::is_horizontal_rule("-"));
        assert!(!MD035HRStyle::is_horizontal_rule("*"));
        assert!(!MD035HRStyle::is_horizontal_rule("_"));

        // Valid: tabs count as whitespace in spaced HRs
        assert!(MD035HRStyle::is_horizontal_rule("*\t*\t*"));
        assert!(MD035HRStyle::is_horizontal_rule("-\t-\t-"));

        // Valid: very long HR
        let long_hr = "-".repeat(200);
        assert!(MD035HRStyle::is_horizontal_rule(&long_hr));
    }

    #[test]
    fn test_frontmatter_not_treated_as_hr() {
        let rule = MD035HRStyle::new("***".to_string());
        let content = "---\ntitle: Test\n---\n\n***\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only the *** should be checked, not the frontmatter ---
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_skips_mkdocs_html_markdown_preserves_outside() {
        // Ensure fix() still changes HRs outside of MkDocs blocks
        let rule = MD035HRStyle::new("***".to_string());

        let content = "Some content\n\n---\n\n<div class=\"grid cards\" markdown>\n\n- Card content\n\n    ---\n\n    Card footer\n\n</div>\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);

        let fixed = rule.fix(&ctx).unwrap();
        // The --- on line 3 (outside div) should be changed to ***
        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines[2], "***", "fix() should change --- outside <div markdown> to ***");
        // The --- inside the div should remain unchanged
        assert!(
            fixed.contains("    ---"),
            "fix() should preserve --- inside <div markdown>"
        );
    }

    /// Helper: assert that fix() produces content with zero check() warnings
    fn assert_fix_roundtrip(rule: &MD035HRStyle, content: &str, flavor: crate::config::MarkdownFlavor) {
        let ctx = LintContext::new(content, flavor, None);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed, flavor, None);
        let warnings = rule.check(&ctx2).unwrap();
        assert!(
            warnings.is_empty(),
            "fix() output should produce zero check() warnings.\nOriginal:\n{content}\nFixed:\n{fixed}\nWarnings: {warnings:?}"
        );
    }

    #[test]
    fn test_roundtrip_consistent_style() {
        let rule = MD035HRStyle::new("consistent".to_string());
        assert_fix_roundtrip(
            &rule,
            "Content\n\n---\n\nMore\n\n***\n\nText\n\n---",
            crate::config::MarkdownFlavor::Standard,
        );
    }

    #[test]
    fn test_roundtrip_specific_style() {
        let rule = MD035HRStyle::new("***".to_string());
        assert_fix_roundtrip(
            &rule,
            "Content\n\n---\n\nMore\n\n___\n\nText",
            crate::config::MarkdownFlavor::Standard,
        );
    }

    #[test]
    fn test_roundtrip_indented_hr() {
        let rule = MD035HRStyle::new("---".to_string());
        assert_fix_roundtrip(
            &rule,
            "Content\n\n  ***\n\nMore\n\n   ___\n\nText",
            crate::config::MarkdownFlavor::Standard,
        );
    }

    #[test]
    fn test_roundtrip_setext_headings() {
        let rule = MD035HRStyle::new("***".to_string());
        assert_fix_roundtrip(
            &rule,
            "Heading 1\n=========\nHeading 2\n---\nContent\n\n---",
            crate::config::MarkdownFlavor::Standard,
        );
    }

    #[test]
    fn test_roundtrip_frontmatter() {
        let rule = MD035HRStyle::new("***".to_string());
        assert_fix_roundtrip(
            &rule,
            "---\ntitle: Test\n---\n\n***\n\nContent",
            crate::config::MarkdownFlavor::Standard,
        );
    }

    #[test]
    fn test_roundtrip_mkdocs_html_markdown() {
        let rule = MD035HRStyle::new("***".to_string());
        let content = "Some content\n\n---\n\n<div class=\"grid cards\" markdown>\n\n- Card content\n\n    ---\n\n    Card footer\n\n</div>\n";
        assert_fix_roundtrip(&rule, content, crate::config::MarkdownFlavor::MkDocs);
    }

    #[test]
    fn test_roundtrip_spaced_styles() {
        let rule = MD035HRStyle::new("* * *".to_string());
        assert_fix_roundtrip(
            &rule,
            "Content\n\n- - -\n\nMore\n\n_ _ _\n\nText",
            crate::config::MarkdownFlavor::Standard,
        );
    }

    #[test]
    fn test_roundtrip_no_warnings() {
        let rule = MD035HRStyle::new("---".to_string());
        assert_fix_roundtrip(
            &rule,
            "Content\n\n---\n\nMore\n\n---\n\nText",
            crate::config::MarkdownFlavor::Standard,
        );
    }

    #[test]
    fn test_roundtrip_trailing_newline() {
        let rule = MD035HRStyle::new("***".to_string());
        assert_fix_roundtrip(
            &rule,
            "Content\n\n---\n\nMore\n",
            crate::config::MarkdownFlavor::Standard,
        );
    }
}
