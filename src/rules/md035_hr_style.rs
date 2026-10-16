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

    pub(crate) fn is_horizontal_rule(line: &str) -> bool {
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

    /// Whether `replacement` is a setext underline when it follows a paragraph line: only
    /// `-` characters, ignoring surrounding spaces and tabs. A spaced run such as `- - -`
    /// is not an underline, and neither is a run of `*` or `_`.
    fn is_setext_underline_text(replacement: &str) -> bool {
        let trimmed = replacement.trim_matches(|c| c == ' ' || c == '\t');
        !trimmed.is_empty() && trimmed.chars().all(|c| c == '-')
    }

    /// Whether writing `replacement` on line `index` could turn the line above into a
    /// setext heading. Any non-blank line above counts, whatever block it belongs to: the
    /// rule cannot tell every paragraph continuation apart from other text, and a declined
    /// fix is retried once a blank line separates the two.
    fn would_form_setext_underline(lines: &[&str], index: usize, replacement: &str) -> bool {
        Self::is_setext_underline_text(replacement)
            && index > 0
            && lines.get(index - 1).is_some_and(|prev| !prev.trim().is_empty())
    }

    /// Whether writing `replacement` on the first line would make the document open with
    /// front matter. A `---` first line followed by another `---` line is a front matter
    /// block, so the rewrite would turn two breaks into metadata and hide them from every
    /// later pass.
    fn would_open_front_matter(ctx: &crate::lint_context::LintContext, index: usize, replacement: &str) -> bool {
        if index != 0 || ctx.front_matter_end_line() > 0 {
            return false;
        }
        let rest = ctx.content.find('\n').map_or("", |end| &ctx.content[end..]);
        let candidate = format!("{replacement}{rest}");
        crate::lint_context::LintContext::new(&candidate, ctx.flavor, None).front_matter_end_line() > 0
    }

    /// Zero-based indices of the lines holding a thematic break that the parser places
    /// inside a list item or block quote. Their indentation positions them in the
    /// container and is not a style error.
    fn container_break_lines(ctx: &crate::lint_context::LintContext) -> std::collections::HashSet<usize> {
        use crate::lint_context::document_parser::blocks::BlockKind;
        let mut depth = 0usize;
        let mut result = std::collections::HashSet::new();
        for (kind, range) in &ctx.parser_blocks().events {
            match kind {
                BlockKind::ItemStart | BlockKind::QuoteStart => depth += 1,
                BlockKind::ItemEnd | BlockKind::QuoteEnd => depth = depth.saturating_sub(1),
                BlockKind::Rule if depth > 0 => {
                    let line = ctx.lines.partition_point(|l| l.byte_offset <= range.start);
                    result.insert(line.saturating_sub(1));
                }
                _ => {}
            }
        }
        result
    }

    /// Exclude markers owned by literal syntax before checking or inferring style.
    fn should_skip_hr_line(ctx: &crate::lint_context::LintContext, index: usize) -> bool {
        let Some(line) = ctx.lines.get(index) else {
            return false;
        };
        let marker_offset = line.byte_offset + line.indent;
        line.in_front_matter
            || line.in_code_block
            || line.in_mkdocs_html_markdown
            || line.in_html_block
            || ctx.line_holds_html_block(index)
            || line.in_html_comment
            || ctx.is_inside_template_code(marker_offset)
            || ctx.is_inside_mdx_code(marker_offset)
    }

    /// Find the most prevalent style among visible horizontal rules.
    fn most_prevalent_hr_style(lines: &[&str], ctx: &crate::lint_context::LintContext) -> Option<String> {
        use std::collections::HashMap;
        let mut counts: HashMap<&str, usize> = HashMap::new();
        let mut order: Vec<&str> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if !Self::is_horizontal_rule(line) || Self::should_skip_hr_line(ctx, i) {
                continue;
            }

            if !Self::is_potential_setext_heading(lines, i) {
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
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        (self.config == Default::default()).then_some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(35))
    }

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
        let mut warnings = Vec::new();
        let lines = ctx.raw_lines();

        // Use the configured style or find the most prevalent HR style
        let expected_style = if self.config.style.is_empty() || self.config.style == "consistent" {
            Self::most_prevalent_hr_style(lines, ctx).unwrap_or_else(|| "---".to_string())
        } else {
            self.config.style.clone()
        };

        let mut container_breaks: Option<std::collections::HashSet<usize>> = None;

        for (i, line) in lines.iter().enumerate() {
            if !Self::is_horizontal_rule(line) || Self::should_skip_hr_line(ctx, i) {
                continue;
            }

            // Skip if this is a potential Setext heading underline
            if Self::is_potential_setext_heading(lines, i) {
                continue;
            }

            // Check if this HR matches the expected style
            let leading = &line[..line.len() - line.trim_start().len()];
            // Indentation is only an error for a break that sits in no list item or quote;
            // inside one it is the break's membership in the container and is kept.
            let in_container = !leading.is_empty()
                && container_breaks
                    .get_or_insert_with(|| Self::container_break_lines(ctx))
                    .contains(&i);
            let has_indentation = !leading.is_empty() && !in_container;
            let style_mismatch = line.trim() != expected_style;

            if style_mismatch || has_indentation {
                // Calculate precise character range for the entire horizontal rule
                let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, line);

                let replacement = if in_container {
                    format!("{leading}{expected_style}")
                } else {
                    expected_style.clone()
                };
                let fix = if Self::would_form_setext_underline(lines, i, &replacement)
                    || Self::would_open_front_matter(ctx, i, &replacement)
                {
                    None
                } else {
                    Some(Fix::new(
                        ctx.line_column_byte_range_with_length(i + 1, 1, line.chars().count()),
                        replacement,
                    ))
                };

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
                    fix,
                });
            }
        }

        Ok(warnings)
    }

    fn fixes_from_diagnostics(&self) -> bool {
        true
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

    crate::impl_rule_config_methods!(MD035Config);
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
        // The section publishes the rule's DEFAULT, not the instance's setting: every
        // consumer builds its rules from `Config::default()` and prints this as the
        // value a user would get without configuring anything.
        let style = |rule: &MD035HRStyle| {
            let (name, config) = rule.default_config_section().unwrap();
            assert_eq!(name, "MD035");
            config
                .as_table()
                .unwrap()
                .get("style")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        };

        // "consistent" is what an unconfigured MD035 enforces, so it is what the
        // published default must say.
        assert_eq!(style(&MD035HRStyle::default()), "consistent");
        assert_eq!(
            style(&MD035HRStyle::new("***".to_string())),
            "consistent",
            "a configured instance must still publish the default"
        );
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

    #[test]
    fn test_literal_rules_are_preserved_beside_visible_rules() {
        use crate::config::MarkdownFlavor;
        let rule = MD035HRStyle::new("---".to_string());
        for (flavor, code) in [
            (
                MarkdownFlavor::Standard,
                "{% set unused=\"first\n\n***\n\nlast\" %}{{ unused|length }}",
            ),
            (MarkdownFlavor::Hugo, "{{< note title=`first\n\n***\n\nlast` >}}"),
            (
                MarkdownFlavor::MDX,
                "export const text = `first\n\n***\n\nlast`\n\n{text}",
            ),
            (MarkdownFlavor::MDX, "{`first\n\n***\n\nlast`}"),
            (MarkdownFlavor::Standard, "<pre>\n\n***\n\n</pre>"),
            (MarkdownFlavor::Standard, "<!--\n\n***\n\n-->"),
        ] {
            for ending in ["\n", "\r\n"] {
                let source = format!("{code}\n\n***\n").replace('\n', ending);
                let expected = format!("{code}\n\n---\n").replace('\n', ending);
                let ctx = LintContext::new(&source, flavor, None);
                assert_eq!(rule.check(&ctx).unwrap().len(), 1, "{source}");
                assert_eq!(rule.fix(&ctx).unwrap(), expected, "{source}");
            }
        }
    }

    #[test]
    fn test_literal_rules_do_not_choose_visible_style() {
        use crate::config::MarkdownFlavor;
        let rule = MD035HRStyle::default();
        for (flavor, code) in [
            (
                MarkdownFlavor::Standard,
                "{% set unused=\"first\n\n___\n\n___\n\nlast\" %}{{ unused|length }}",
            ),
            (
                MarkdownFlavor::MDX,
                "export const text = `first\n\n___\n\n___\n\nlast`\n\n{text}",
            ),
            (MarkdownFlavor::Standard, "<pre>\n\n___\n\n___\n\n</pre>"),
            (MarkdownFlavor::Standard, "<!--\n\n___\n\n___\n\n-->"),
        ] {
            let source = format!("{code}\n\n***\n");
            let ctx = LintContext::new(&source, flavor, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "{source}");
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_visible_mdx_jsx_body_rules_remain_fixable() {
        let rule = MD035HRStyle::new("---".to_string());
        for tag in ["div", "pre"] {
            let source = format!("<{tag}>\n\n***\n\n</{tag}>\n\n***\n");
            let ctx = LintContext::new(&source, crate::config::MarkdownFlavor::MDX, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 2);
            assert_eq!(rule.fix(&ctx).unwrap(), source.replace("***", "---"));
        }
    }

    /// Parser events of a document with source offsets dropped, so two texts can be
    /// compared for identical block structure.
    fn structure(text: &str) -> Vec<String> {
        pulldown_cmark::Parser::new_ext(text, crate::utils::parser_options::rumdl_parser_options_for(text))
            .map(|event| format!("{event:?}"))
            .collect()
    }

    fn fix_with_style(style: &str, content: &str) -> (String, Vec<crate::rule::LintWarning>) {
        let rule = MD035HRStyle::new(style.to_string());
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        (rule.fix(&ctx).unwrap(), rule.check(&ctx).unwrap())
    }

    #[test]
    fn test_fix_declined_when_dash_run_would_underline_paragraph() {
        let cases: &[(&str, &str)] = &[
            ("underscores", "Text\n___\nMore\n"),
            ("asterisks", "Text\n***\nMore\n"),
            ("spaced asterisks", "Text\n* * *\nMore\n"),
            ("spaced underscores", "Text\n_ _ _\nMore\n"),
            ("long underscores", "Text\n_____\nMore\n"),
            ("indent 1", "Text\n ___\nMore\n"),
            ("indent 3", "Text\n   ***\nMore\n"),
            ("trailing whitespace", "Text\n***   \nMore\n"),
            (
                "two-line paragraph",
                "First line.\nSecond line.\n___\nNext paragraph.\n",
            ),
            ("one-row table", "| a |\n***\n"),
            ("div marker above", "Text\n:::\n***\n"),
            ("reference definition above", "[a]: http://example.com\n***\n"),
            ("heading above", "# H\n***\n\nB\n"),
            ("fence close above", "```\ncode\n```\n***\n"),
            ("list item paragraph", "- item\n  para\n  ___\n"),
            ("ordered list item paragraph", "1. one\n   two\n   ***\n"),
            ("list item at column 0", "- item\n___\n"),
            ("lazy blockquote", "> quote\n___\n"),
        ];
        for (name, content) in cases {
            let (fixed, warnings) = fix_with_style("---", content);
            assert_eq!(&fixed, content, "{name}: the fix must leave the document unchanged");
            assert_eq!(warnings.len(), 1, "{name}: the style warning must remain");
            assert!(warnings[0].fix.is_none(), "{name}: the warning must carry no fix");
        }
    }

    #[test]
    fn test_fix_declined_on_issue_input_keeps_paragraph() {
        let content = "# Doc\n\nIntro.\n\n---\n\nFirst line.\nSecond line.\n___\nNext paragraph.\n";
        let rule = MD035HRStyle::new("consistent".to_string());
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 9);
        assert!(warnings[0].fix.is_none());
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
        assert!(
            !structure(&fixed)
                .iter()
                .any(|e| e.contains("Heading") && e.contains("H2")),
            "the paragraph must not become a heading"
        );
    }

    #[test]
    fn test_fix_declined_with_padded_dash_style() {
        for style in ["--- ", " ---", "\t---\t"] {
            let content = "Text\n***\nMore\n";
            let (fixed, warnings) = fix_with_style(style, content);
            assert_eq!(fixed, content, "style {style:?}");
            assert!(warnings.iter().all(|w| w.fix.is_none()), "style {style:?}");
        }
    }

    #[test]
    fn test_fix_still_applies_when_replacement_is_not_an_underline() {
        // Non-dash styles, and the spaced dash style, are thematic breaks under a paragraph.
        for (style, content, expected) in [
            ("***", "Text\n___\nMore\n", "Text\n***\nMore\n"),
            ("___", "Text\n***\nMore\n", "Text\n___\nMore\n"),
            ("- - -", "Text\n***\nMore\n", "Text\n- - -\nMore\n"),
        ] {
            let (fixed, _) = fix_with_style(style, content);
            assert_eq!(fixed, expected, "style {style:?}");
            assert_eq!(structure(&fixed), structure(content), "style {style:?}");
        }
    }

    #[test]
    fn test_fix_still_applies_after_blank_line() {
        for (content, expected) in [
            ("Text\n\n***\n\nMore\n", "Text\n\n---\n\nMore\n"),
            ("***\n\n***\n", "---\n\n---\n"),
        ] {
            let (fixed, _) = fix_with_style("---", content);
            assert_eq!(fixed, expected);
            assert_eq!(structure(&fixed), structure(content));
        }
    }

    #[test]
    fn test_indentation_inside_list_item_is_kept() {
        let content = "- a\n\n  ***\n\n  more\n";
        let (fixed, warnings) = fix_with_style("---", content);
        assert_eq!(fixed, "- a\n\n  ---\n\n  more\n");
        assert_eq!(
            structure(&fixed),
            structure(content),
            "the break must stay inside the item"
        );
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].message.contains("style should be"),
            "{}",
            warnings[0].message
        );

        let ordered = "1. one\n\n   ___\n\n   more\n";
        let (fixed, _) = fix_with_style("---", ordered);
        assert_eq!(fixed, "1. one\n\n   ---\n\n   more\n");
        assert_eq!(structure(&fixed), structure(ordered));
    }

    #[test]
    fn test_conforming_break_inside_list_item_is_not_indentation_error() {
        let (_, warnings) = fix_with_style("---", "- a\n\n  ---\n\n  more\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        let (_, warnings) = fix_with_style("---", "1. one\n\n   ---\n\n   more\n");
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn test_indentation_outside_list_item_is_still_stripped() {
        // Indented less than the item's content column: the break ends the list.
        let content = "- a\n\n ***\n\nmore\n";
        let (fixed, _) = fix_with_style("---", content);
        assert_eq!(fixed, "- a\n\n---\n\nmore\n");
        assert_eq!(structure(&fixed), structure(content));

        let content = "Text\n\n  ---\n\nMore\n";
        let (fixed, _) = fix_with_style("---", content);
        assert_eq!(fixed, "Text\n\n---\n\nMore\n");
    }

    #[test]
    fn test_fix_is_idempotent_on_consecutive_breaks_with_declined_fix() {
        let content = "___\n---\n***\n---\n";
        let rule = MD035HRStyle::default();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed1 = rule.fix(&ctx).unwrap();
        let ctx = LintContext::new(&fixed1, crate::config::MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx).unwrap();
        assert_eq!(fixed1, fixed2, "first fix: {fixed1:?}");
    }

    #[test]
    fn test_fix_declined_when_first_line_would_open_front_matter() {
        for content in ["___\n---\n***\n---\n", "***\n---\ntext\n\n---\n"] {
            let rule = MD035HRStyle::new("---".to_string());
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert!(
                warnings.iter().any(|w| w.line == 1 && w.fix.is_none()),
                "{content:?}: {warnings:?}"
            );
            assert_eq!(rule.fix(&ctx).unwrap().lines().next(), content.lines().next());
        }
    }

    #[test]
    fn test_fix_applies_on_first_line_when_no_front_matter_results() {
        let rule = MD035HRStyle::new("---".to_string());
        let content = "***\n\ntext\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), "---\n\ntext\n");
    }
}
