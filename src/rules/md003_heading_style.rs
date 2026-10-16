//!
//! Rule MD003: Heading style
//!
//! See [docs/md003.md](../../docs/md003.md) for full documentation, configuration, and examples.

use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::{FlavorOverrideNotice, option_is_explicit};
use crate::rules::heading_utils::HeadingStyle;
use crate::utils::range_utils::calculate_heading_range;
use toml;

mod md003_config;
use md003_config::MD003Config;

/// Reports an explicit MDG style override once per process.
static MDG_STYLE_OVERRIDE: FlavorOverrideNotice = FlavorOverrideNotice::new();

/// Rule MD003: Heading style
#[derive(Clone, Default)]
pub struct MD003HeadingStyle {
    config: MD003Config,
    /// Whether `style` was explicitly configured rather than defaulted.
    style_explicit: bool,
}

impl MD003HeadingStyle {
    pub fn new(style: HeadingStyle) -> Self {
        Self {
            config: MD003Config { style },
            style_explicit: true,
        }
    }

    pub fn from_config_struct(config: MD003Config) -> Self {
        Self {
            config,
            style_explicit: false,
        }
    }

    /// Check if we should use consistent mode (detect first style)
    fn is_consistent_mode(&self) -> bool {
        // Check for the Consistent variant explicitly
        self.config.style == HeadingStyle::Consistent
    }

    fn heading_is_in_literal_code(
        ctx: &crate::lint_context::LintContext,
        heading_idx: usize,
        heading: &crate::lint_context::HeadingInfo,
    ) -> bool {
        let marker_idx = heading_idx
            + usize::from(matches!(
                heading.style,
                crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
            ));
        ctx.lines.get(marker_idx).is_some_and(|line| {
            let marker_offset = line.byte_offset + line.indent;
            ctx.is_inside_template_code(marker_offset) || ctx.is_inside_mdx_code(marker_offset)
        })
    }

    fn heading_style(
        ctx: &crate::lint_context::LintContext,
        heading_idx: usize,
        heading: &crate::lint_context::HeadingInfo,
    ) -> HeadingStyle {
        match heading.style {
            crate::lint_context::HeadingStyle::ATX => {
                if !heading.has_closing_sequence {
                    return HeadingStyle::Atx;
                }
                let line = &ctx.lines[heading_idx];
                let content = line.content(ctx.content);
                // Closing hashes precede a trailing inline attribute list.
                let before_id = content
                    .rsplit_once(" {#")
                    .filter(|(_, id)| id.trim_end().ends_with('}'))
                    .map_or(content, |(text, _)| text);
                let marker = line.byte_offset + before_id.trim_end().len() - heading.closing_sequence.len();
                if ctx.is_inside_template_code(marker) || ctx.is_inside_mdx_code(marker) {
                    HeadingStyle::Atx
                } else {
                    HeadingStyle::AtxClosed
                }
            }
            crate::lint_context::HeadingStyle::Setext1 => HeadingStyle::Setext1,
            crate::lint_context::HeadingStyle::Setext2 => HeadingStyle::Setext2,
        }
    }

    fn conversion_is_safe(
        ctx: &crate::lint_context::LintContext,
        heading_idx: usize,
        heading: &crate::lint_context::HeadingInfo,
    ) -> bool {
        if matches!(
            heading.style,
            crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
        ) {
            return crate::rules::heading_utils::setext_to_atx_is_safe(ctx, heading_idx, heading);
        }
        let line = &ctx.lines[heading_idx];
        let boundary = line.byte_offset + line.byte_len;
        !ctx.is_inside_template_code(boundary) && !ctx.is_inside_mdx_code(boundary)
    }

    /// Gets the target heading style based on configuration and document content
    fn get_target_style(&self, ctx: &crate::lint_context::LintContext) -> HeadingStyle {
        // MDG recognizes `#{1,6} ` headings only, so plain ATX is the single
        // style a Gherkin document can be steered to.
        if ctx.flavor == crate::config::MarkdownFlavor::MDG {
            self.warn_once_about_overridden_style();
            return HeadingStyle::Atx;
        }

        if !self.is_consistent_mode() {
            return self.config.style;
        }

        // Count all heading styles to determine most prevalent (prevalence-based approach)
        let mut style_counts = std::collections::HashMap::new();

        // Headings inside blockquotes vote like any other heading.
        for parsed in ctx.headings() {
            let line_num = parsed.line_num - 1;
            if Self::heading_is_in_literal_code(ctx, line_num, parsed.heading) {
                continue;
            }
            // Map from LintContext heading style to rules heading style and count
            let style = Self::heading_style(ctx, line_num, parsed.heading);
            *style_counts.entry(style).or_insert(0) += 1;
        }

        // Return most prevalent style
        // In case of tie, prefer ATX as the default (deterministic tiebreaker)
        style_counts
            .into_iter()
            .max_by(|(style_a, count_a), (style_b, count_b)| {
                match count_a.cmp(count_b) {
                    std::cmp::Ordering::Equal => {
                        // Tiebreaker: prefer ATX (most common), then Setext1, then Setext2, then AtxClosed
                        let priority = |s: &HeadingStyle| match s {
                            HeadingStyle::Atx => 0,
                            HeadingStyle::Setext1 => 1,
                            HeadingStyle::Setext2 => 2,
                            HeadingStyle::AtxClosed => 3,
                            _ => 4,
                        };
                        priority(style_b).cmp(&priority(style_a)) // Reverse for min priority wins
                    }
                    other => other,
                }
            })
            .map_or(HeadingStyle::Atx, |(style, _)| style)
    }

    /// Tell the user when an explicit fixed style cannot be honored by MDG.
    /// `consistent` asks for no fixed spelling, and `atx` is already the form
    /// the flavor enforces, so neither is an override worth reporting.
    fn warn_once_about_overridden_style(&self) {
        if !self.style_explicit || matches!(self.config.style, HeadingStyle::Atx | HeadingStyle::Consistent) {
            return;
        }

        let configured = self.config.style.to_string();
        MDG_STYLE_OVERRIDE.report(
            "MD003",
            "style",
            &configured,
            "atx",
            "Markdown with Gherkin recognizes structure headings only in plain ATX form",
        );
    }
}

impl Rule for MD003HeadingStyle {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        (self.config == Default::default()).then_some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(3))
    }

    fn name(&self) -> &'static str {
        "MD003"
    }

    fn description(&self) -> &'static str {
        "Heading style"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut result = Vec::new();

        // Get the target style using cached heading information
        let target_style = self.get_target_style(ctx);

        // Process headings using cached heading information
        for parsed in ctx.headings() {
            let line_num = parsed.line_num - 1;
            let line_info = parsed.line_info;
            let heading = parsed.heading;
            if Self::heading_is_in_literal_code(ctx, line_num, heading) {
                continue;
            }
            let level = heading.level;

            // Map the cached heading style to the rule's HeadingStyle
            let current_style = Self::heading_style(ctx, line_num, heading);

            // Determine expected style based on level and target
            let expected_style = match target_style {
                HeadingStyle::Setext1 | HeadingStyle::Setext2 => {
                    if level > 2 {
                        // Setext only supports levels 1-2. The heading cannot
                        // comply at all, so keep the ATX flavor it already has
                        // instead of restyling it to one the config never asked
                        // for. That also keeps the fix idempotent under
                        // `consistent`: rewriting only ever moves headings into
                        // the target style, so it can never flip the prevalence
                        // count that chose the target.
                        current_style
                    } else if level == 1 {
                        HeadingStyle::Setext1
                    } else {
                        HeadingStyle::Setext2
                    }
                }
                HeadingStyle::SetextWithAtx => {
                    if level <= 2 {
                        // Use Setext for h1/h2
                        if level == 1 {
                            HeadingStyle::Setext1
                        } else {
                            HeadingStyle::Setext2
                        }
                    } else {
                        // Use ATX for h3-h6
                        HeadingStyle::Atx
                    }
                }
                HeadingStyle::SetextWithAtxClosed => {
                    if level <= 2 {
                        // Use Setext for h1/h2
                        if level == 1 {
                            HeadingStyle::Setext1
                        } else {
                            HeadingStyle::Setext2
                        }
                    } else {
                        // Use ATX closed for h3-h6
                        HeadingStyle::AtxClosed
                    }
                }
                _ => target_style,
            };

            // MDG only recognizes plain ATX headings: a setext heading never
            // becomes a Gherkin node and a closing sequence leaks into the
            // node's name (`# Feature: F #` is named "F #"). Steering every
            // heading to plain ATX keeps MD003 enforcing a style while never
            // emitting a form Gherkin cannot parse.
            let expected_style = if ctx.flavor == crate::config::MarkdownFlavor::MDG {
                HeadingStyle::Atx
            } else {
                expected_style
            };

            if current_style != expected_style {
                // The heading is recorded on the last line of its text, and a
                // setext heading's text is the paragraph its underline ends
                let first_line_num = line_num + 2 - heading.text_lines;

                // Generate fix for this heading
                // A quoted heading keeps its warning without a fix: the
                // conversions here do not rebuild the container prefix of
                // the heading and of the underline a setext heading needs.
                let fix = if !parsed.is_blockquote() && Self::conversion_is_safe(ctx, line_num, heading) {
                    use crate::rules::heading_utils::HeadingUtils;

                    // Convert heading to target style, preserving inline attribute lists
                    let converted_heading =
                        HeadingUtils::convert_heading_style(&heading.raw_text, level as u32, expected_style);

                    // Preserve everything before the heading on the line its text
                    // starts on: the indentation (including tabs) and, for a
                    // heading that opens a list item, the list marker
                    let first_line_info = &ctx.lines[first_line_num - 1];
                    let first_line = first_line_info.content(ctx.content);
                    let original_indent = &first_line[..heading.first_line_prefix_len()];

                    // An ATX heading becoming setext gains an underline line. It
                    // must sit in the same container as the text line, so it
                    // carries the same indentation.
                    let atx_to_setext = heading.style == crate::lint_context::HeadingStyle::ATX
                        && level <= 2
                        && matches!(expected_style, HeadingStyle::Setext1 | HeadingStyle::Setext2);
                    let converted_heading = if atx_to_setext {
                        converted_heading.replacen('\n', &format!("\n{original_indent}"), 1)
                    } else {
                        converted_heading
                    };
                    let final_heading = format!("{original_indent}{converted_heading}");

                    // A setext heading is the whole paragraph its underline
                    // ends, so converting away from it replaces every line of
                    // that paragraph. The underline goes with them, otherwise
                    // it survives as a thematic break.
                    let converting_from_setext =
                        matches!(
                            heading.style,
                            crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
                        ) && !matches!(expected_style, HeadingStyle::Setext1 | HeadingStyle::Setext2);
                    let last_line = if converting_from_setext {
                        line_num + 2
                    } else {
                        line_num + 1
                    };

                    let start = ctx.line_content_byte_range(first_line_num).start;
                    let end = ctx.line_content_byte_range(last_line).end;

                    // Declined conversions keep the warning without a fix: the
                    // heading stays a valid ATX heading instead of being merged
                    // into the block above or turned into another construct.
                    if atx_to_setext
                        && !crate::rules::heading_utils::atx_to_setext_is_safe(ctx, line_num, heading, &final_heading)
                    {
                        None
                    } else {
                        Some(crate::rule::Fix::new(start..end, final_heading))
                    }
                } else {
                    None
                };

                // Calculate precise character range for the heading marker
                let (start_line, start_col, end_line, end_col) =
                    calculate_heading_range(first_line_num, line_num + 1, line_info.content(ctx.content));

                result.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!(
                        "Heading style should be {}, found {}",
                        match expected_style {
                            HeadingStyle::Atx => "# Heading",
                            HeadingStyle::AtxClosed => "# Heading #",
                            HeadingStyle::Setext1 => "Heading\n=======",
                            HeadingStyle::Setext2 => "Heading\n-------",
                            HeadingStyle::Consistent => "consistent with the first heading",
                            HeadingStyle::SetextWithAtx => "setext-with-atx style",
                            HeadingStyle::SetextWithAtxClosed => "setext-with-atx-closed style",
                        },
                        match current_style {
                            HeadingStyle::Atx => "# Heading",
                            HeadingStyle::AtxClosed => "# Heading #",
                            HeadingStyle::Setext1 => "Heading (underlined with =)",
                            HeadingStyle::Setext2 => "Heading (underlined with -)",
                            HeadingStyle::Consistent => "consistent style",
                            HeadingStyle::SetextWithAtx => "setext-with-atx style",
                            HeadingStyle::SetextWithAtxClosed => "setext-with-atx-closed style",
                        }
                    ),
                    severity: Severity::Warning,
                    fix,
                });
            }
        }

        Ok(result)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Get all warnings with their fixes
        let warnings = self.check(ctx)?;
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());

        // If no warnings, return original content
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Collect all fixes and sort by range start (descending) to apply from end to beginning
        let mut fixes: Vec<_> = warnings
            .iter()
            .filter_map(|w| w.fix.as_ref().map(|f| (f.range.start, f.range.end, &f.replacement)))
            .collect();
        fixes.sort_by_key(|f| std::cmp::Reverse(f.0));

        // Apply fixes from end to beginning to preserve byte offsets
        let mut result = ctx.content.to_string();
        for (start, end, replacement) in fixes {
            if start < result.len() && end <= result.len() && start <= end {
                result.replace_range(start..end, replacement);
            }
        }

        Ok(result)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Fast path: check if document likely has headings using character frequency
        if ctx.content.is_empty() || !ctx.likely_has_headings() {
            return true;
        }
        // Verify headings actually exist (handles false positives from character frequency)
        ctx.headings().next().is_none()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_sections!(MD003Config);

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD003Config>(config);
        let style_explicit = option_is_explicit(config, "MD003", "style");

        Box::new(Self {
            config: rule_config,
            style_explicit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_atx_heading_style() {
        let rule = MD003HeadingStyle::default();
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_setext_heading_style() {
        let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
        let content = "Heading 1\n=========\n\nHeading 2\n---------";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_front_matter() {
        let rule = MD003HeadingStyle::default();
        let content = "---\ntitle: Test\n---\n\n# Heading 1\n## Heading 2";

        // Test should detect headings and apply consistent style
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "No warnings expected for content with front matter, found: {result:?}"
        );
    }

    #[test]
    fn test_consistent_heading_style() {
        // Default rule uses Atx which serves as our "consistent" mode
        let rule = MD003HeadingStyle::default();
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_with_different_styles() {
        // Test with consistent style (ATX)
        let rule = MD003HeadingStyle::new(HeadingStyle::Consistent);
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Make test more resilient
        assert!(
            result.is_empty(),
            "No warnings expected for consistent ATX style, found: {result:?}"
        );

        // Test with incorrect style
        let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
        let content = "# Heading 1 #\nHeading 2\n-----\n### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            !result.is_empty(),
            "Should have warnings for inconsistent heading styles"
        );

        // Test with setext style
        let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
        let content = "Heading 1\n=========\nHeading 2\n---------\n### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // The level 3 heading can't be setext, so it's valid as ATX
        assert!(
            result.is_empty(),
            "No warnings expected for setext style with ATX for level 3, found: {result:?}"
        );
    }

    #[test]
    fn test_setext_with_atx_style() {
        let rule = MD003HeadingStyle::new(HeadingStyle::SetextWithAtx);
        // Setext for h1/h2, ATX for h3-h6
        let content = "Heading 1\n=========\n\nHeading 2\n---------\n\n### Heading 3\n\n#### Heading 4";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "SesetxtWithAtx style should accept setext for h1/h2 and ATX for h3+"
        );

        // Test incorrect usage - ATX for h1/h2
        let content_wrong = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx_wrong = LintContext::new(content_wrong, crate::config::MarkdownFlavor::Standard, None);
        let result_wrong = rule.check(&ctx_wrong).unwrap();
        assert_eq!(
            result_wrong.len(),
            2,
            "Should flag ATX headings for h1/h2 with setext_with_atx style"
        );
    }

    #[test]
    fn test_fix_preserves_attribute_lists() {
        // ATX closed heading with attribute list, converted to ATX
        let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
        let content = "# Heading { #custom-id .class } #";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        // Should flag: found ATX closed, expected ATX
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        let fix = warnings[0].fix.as_ref().expect("Should have a fix");
        assert!(
            fix.replacement.contains("{ #custom-id .class }"),
            "check() fix should preserve attribute list, got: {}",
            fix.replacement
        );

        // Verify fix() also preserves attribute list
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("{ #custom-id .class }"),
            "fix() should preserve attribute list, got: {fixed}"
        );
        assert!(
            !fixed.contains(" #\n") && !fixed.ends_with(" #"),
            "fix() should remove ATX closed trailing hashes, got: {fixed}"
        );
    }

    #[test]
    fn test_setext_with_atx_closed_style() {
        let rule = MD003HeadingStyle::new(HeadingStyle::SetextWithAtxClosed);
        // Setext for h1/h2, ATX closed for h3-h6
        let content = "Heading 1\n=========\n\nHeading 2\n---------\n\n### Heading 3 ###\n\n#### Heading 4 ####";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "SetextWithAtxClosed style should accept setext for h1/h2 and ATX closed for h3+"
        );

        // Test incorrect usage - regular ATX for h3+
        let content_wrong = "Heading 1\n=========\n\n### Heading 3\n\n#### Heading 4";
        let ctx_wrong = LintContext::new(content_wrong, crate::config::MarkdownFlavor::Standard, None);
        let result_wrong = rule.check(&ctx_wrong).unwrap();
        assert_eq!(
            result_wrong.len(),
            2,
            "Should flag non-closed ATX headings for h3+ with setext_with_atx_closed style"
        );
    }

    #[test]
    fn test_mdg_steers_every_heading_to_plain_atx() {
        // MDG parses `#{1,6} ` headings only, so setext headings are corrected
        // into ATX and closing sequences are dropped rather than preserved.
        let cases = [
            (
                MD003HeadingStyle::new(HeadingStyle::Atx),
                "Checkout\n========\n\n## Scenario: Buy an item\n",
                "# Checkout\n\n## Scenario: Buy an item\n",
            ),
            (
                MD003HeadingStyle::new(HeadingStyle::AtxClosed),
                "# Feature: Checkout\n\n## Scenario: Buy an item ##\n",
                "# Feature: Checkout\n\n## Scenario: Buy an item\n",
            ),
            (
                MD003HeadingStyle::new(HeadingStyle::Setext1),
                "# Feature: Checkout\n\nScenario: Documentation\n-----------------------\n",
                "# Feature: Checkout\n\n## Scenario: Documentation\n",
            ),
            (
                MD003HeadingStyle::default(),
                "Checkout\n========\n\nGuide\n-----\n\n## Scenario: Buy an item\n",
                "# Checkout\n\n## Guide\n\n## Scenario: Buy an item\n",
            ),
        ];

        for (rule, content, expected) in cases {
            let mdg_ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);

            assert!(!rule.should_skip(&mdg_ctx));
            assert!(
                !rule.check(&mdg_ctx).unwrap().is_empty(),
                "MDG must still report non-ATX headings in {content:?}"
            );
            let fixed = rule.fix(&mdg_ctx).unwrap();
            assert_eq!(fixed, expected, "MDG must steer {content:?} to plain ATX");

            let fixed_ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::MDG, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), fixed, "MDG fix should be idempotent");
        }
    }

    #[test]
    fn test_mdg_tracks_only_an_explicit_style_for_override_notices() {
        let direct = MD003HeadingStyle::new(HeadingStyle::Setext1);
        assert!(direct.style_explicit);

        let defaulted = MD003HeadingStyle::from_config_struct(MD003Config {
            style: HeadingStyle::Setext1,
        });
        assert!(!defaulted.style_explicit);

        let mut config = crate::config::Config::default();
        let mut rule_config = crate::config::RuleConfig::default();
        rule_config
            .values
            .insert("style".to_string(), toml::Value::String("setext".to_string()));
        config.rules.insert("MD003".to_string(), rule_config);
        let configured = MD003HeadingStyle::from_config(&config);
        let configured = configured
            .as_any()
            .downcast_ref::<MD003HeadingStyle>()
            .expect("MD003::from_config builds MD003HeadingStyle");
        assert!(configured.style_explicit);

        let default_configured = MD003HeadingStyle::from_config(&crate::config::Config::default());
        let default_configured = default_configured
            .as_any()
            .downcast_ref::<MD003HeadingStyle>()
            .expect("MD003::from_config builds MD003HeadingStyle");
        assert!(!default_configured.style_explicit);
    }
    #[test]
    fn test_literal_headings_are_preserved_beside_visible_style_fixes() {
        for (style, literal, visible, target) in [
            (HeadingStyle::Atx, "\nFake\n---\n", "Visible\n---", "## Visible"),
            (HeadingStyle::Atx, "\n# Fake #\n", "# Visible #", "# Visible"),
            (HeadingStyle::AtxClosed, "\n# Fake\n", "# Visible", "# Visible #"),
            (HeadingStyle::Setext1, "\n# Fake\n", "# Visible", "Visible\n======="),
        ] {
            let rule = MD003HeadingStyle::new(style);
            for (flavor, code) in [
                (
                    crate::config::MarkdownFlavor::Standard,
                    format!("{{% set unused=\"first\n{literal}\nlast\" %}}{{{{ unused|length }}}}"),
                ),
                (
                    crate::config::MarkdownFlavor::Hugo,
                    format!("{{{{< note title=`first\n{literal}\nlast` >}}}}"),
                ),
            ] {
                for ending in ["\n", "\r\n"] {
                    let source = format!("{code}\n\n{visible}\n").replace('\n', ending);
                    let expected = format!("{code}\n\n{target}\n").replace('\n', ending);
                    let ctx = crate::lint_context::LintContext::new(&source, flavor, None);
                    let warnings = rule.check(&ctx).unwrap();
                    assert_eq!(warnings.len(), 1, "{source}");
                    assert_eq!(
                        crate::utils::fix_utils::apply_warning_fixes(&source, &warnings).unwrap(),
                        expected,
                        "{source}"
                    );
                    // The direct fixer inserts LF in a newly generated Setext heading.
                    let direct_expected = format!("{}{ending}{ending}{target}{ending}", code.replace('\n', ending));
                    assert_eq!(rule.fix(&ctx).unwrap(), direct_expected, "{source}");
                }
            }
        }
    }

    #[test]
    fn test_literal_heading_styles_do_not_choose_visible_style() {
        let rule = MD003HeadingStyle::default();
        for literal in ["# Fake #\n\n# Other #", "\nFake\n===\n\nOther\n===\n"] {
            for (flavor, code) in [
                (
                    crate::config::MarkdownFlavor::Standard,
                    format!("{{% set unused=\"first\n{literal}\nlast\" %}}{{{{ unused|length }}}}"),
                ),
                (
                    crate::config::MarkdownFlavor::Hugo,
                    format!("{{{{< note title=`first\n{literal}\nlast` >}}}}"),
                ),
            ] {
                let source = format!("{code}\n\n# Visible\n");
                let ctx = crate::lint_context::LintContext::new(&source, flavor, None);
                assert!(rule.check(&ctx).unwrap().is_empty(), "{source}");
                assert_eq!(rule.fix(&ctx).unwrap(), source);
            }
        }
    }

    #[test]
    fn test_dynamic_and_mdx_body_heading_styles_remain_fixable() {
        let rule = MD003HeadingStyle::new(HeadingStyle::AtxClosed);
        for (flavor, source, target) in [
            (
                crate::config::MarkdownFlavor::Standard,
                "# {{ title }}\n",
                "# {{ title }} #\n",
            ),
            (
                crate::config::MarkdownFlavor::MDX,
                "<div>\n\n# Visible\n\n</div>\n",
                "<div>\n\n# Visible #\n\n</div>\n",
            ),
        ] {
            let ctx = crate::lint_context::LintContext::new(source, flavor, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1);
            assert_eq!(rule.fix(&ctx).unwrap(), target);
        }
    }

    #[test]
    fn test_unsafe_title_conversions_leave_safe_neighbors_fixable() {
        for (style, source_title, adjacent, fixed_adjacent) in [
            (
                HeadingStyle::Setext1,
                "# {% set unused=\"first  \nlast\" %}{{ unused|length }}",
                "# Adjacent",
                "Adjacent\n========",
            ),
            (
                HeadingStyle::Setext1,
                "# {{< note title=`first  \nlast` >}}",
                "# Adjacent",
                "Adjacent\n========",
            ),
            (
                HeadingStyle::Atx,
                "{% set unused=\"first  \nlast\" %}{{ unused|length }}\n===",
                "Adjacent\n===",
                "# Adjacent",
            ),
            (
                HeadingStyle::Atx,
                "{{< note title=`first  \nlast` >}}\n===",
                "Adjacent\n===",
                "# Adjacent",
            ),
            (
                HeadingStyle::AtxClosed,
                "# {% set unused=\"first  \nlast\" %}{{ unused|length }}",
                "# Adjacent",
                "# Adjacent #",
            ),
        ] {
            let rule = MD003HeadingStyle::new(style);
            let flavor = if source_title.contains("{{<") {
                crate::config::MarkdownFlavor::Hugo
            } else {
                crate::config::MarkdownFlavor::Standard
            };
            for ending in ["\n", "\r\n"] {
                let source = format!("{source_title}\n\n{adjacent}\n").replace('\n', ending);
                let expected = format!("{source_title}\n\n{fixed_adjacent}\n").replace('\n', ending);
                let ctx = crate::lint_context::LintContext::new(&source, flavor, None);
                let warnings = rule.check(&ctx).unwrap();
                assert_eq!(warnings.len(), 2, "{source}");
                assert!(warnings[0].fix.is_none(), "{source}");
                assert!(warnings[1].fix.is_some());
                assert_eq!(
                    crate::utils::fix_utils::apply_warning_fixes(&source, &warnings).unwrap(),
                    expected
                );
                let direct_expected = format!(
                    "{}{ending}{ending}{fixed_adjacent}{ending}",
                    source_title.replace('\n', ending)
                );
                assert_eq!(rule.fix(&ctx).unwrap(), direct_expected);
            }
        }
    }

    #[test]
    fn test_setext_style_conversion_preserves_literal_markup_and_hard_breaks() {
        for title in [
            "First\\\nSecond",
            "First  \nSecond",
            "`first  \nlast`",
            "`first\n  last`",
            "<span title=\"first\nlast\">Visible</span>",
        ] {
            for style in [HeadingStyle::Atx, HeadingStyle::AtxClosed] {
                let rule = MD003HeadingStyle::new(style);
                let source = format!("{title}\n===\n\nAdjacent\n===\n");
                let adjacent = if style == HeadingStyle::Atx {
                    "# Adjacent"
                } else {
                    "# Adjacent #"
                };
                let expected = format!("{title}\n===\n\n{adjacent}\n");
                let ctx = crate::lint_context::LintContext::new(&source, crate::config::MarkdownFlavor::Standard, None);
                let warnings = rule.check(&ctx).unwrap();
                assert_eq!(warnings.len(), 2);
                assert!(warnings[0].fix.is_none(), "{source}");
                assert!(warnings[1].fix.is_some());
                assert_eq!(rule.fix(&ctx).unwrap(), expected);
            }
        }
    }

    #[test]
    fn test_safe_dynamic_and_setext_style_conversions_remain_fixable() {
        for (style, source, expected) in [
            (HeadingStyle::Setext1, "# {{ title }}\n", "{{ title }}\n===========\n"),
            (HeadingStyle::AtxClosed, "# {{ title }}\n", "# {{ title }} #\n"),
            (HeadingStyle::Atx, "`first\nlast`\n===\n", "# `first last`\n"),
        ] {
            let rule = MD003HeadingStyle::new(style);
            let ctx = crate::lint_context::LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1);
            assert!(rule.check(&ctx).unwrap()[0].fix.is_some());
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }

    #[test]
    fn test_literal_closing_hashes_do_not_choose_heading_style() {
        for (flavor, prefix, suffix) in [
            (
                crate::config::MarkdownFlavor::Standard,
                "{% set unused=\"",
                "\" %}{{ unused|length }}",
            ),
            (crate::config::MarkdownFlavor::Hugo, "{{< note title=`", "` >}}"),
        ] {
            for hashes in ["#", "###", "########"] {
                for pad in ["", "  ", "\t"] {
                    for ending in ["\n", "\r\n"] {
                        let title = format!("# {prefix}naïve {hashes}{pad}\nlast{suffix}");
                        let source = format!("{title}\n\n{title}\n\n# Adjacent\n").replace('\n', ending);
                        for style in [HeadingStyle::Atx, HeadingStyle::Consistent] {
                            let rule = MD003HeadingStyle::new(style);
                            let ctx = crate::lint_context::LintContext::new(&source, flavor, None);
                            assert!(rule.check(&ctx).unwrap().is_empty(), "{source}");
                            assert_eq!(rule.fix(&ctx).unwrap(), source);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_literal_closing_hashes_still_require_a_real_closed_marker() {
        let rule = MD003HeadingStyle::new(HeadingStyle::AtxClosed);
        let source = "# {% set unused=\"first ###\nlast\" %}{{ unused|length }}\n\n# Adjacent\n";
        let ctx = crate::lint_context::LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].fix.is_none());
        assert!(warnings[1].fix.is_some());
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "# {% set unused=\"first ###\nlast\" %}{{ unused|length }}\n\n# Adjacent #\n"
        );
    }

    #[test]
    fn test_real_closing_hashes_after_dynamic_titles_remain_closed() {
        let rule = MD003HeadingStyle::new(HeadingStyle::AtxClosed);
        for (flavor, source) in [
            (crate::config::MarkdownFlavor::Standard, "# {{ title }} ###\n"),
            (crate::config::MarkdownFlavor::Standard, "# {{ title }} ### {#custom}\n"),
            (crate::config::MarkdownFlavor::MDX, "<div>\n\n# Visible ###\n\n</div>\n"),
        ] {
            let ctx = crate::lint_context::LintContext::new(source, flavor, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "{source}");
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_multiline_inline_titles_survive_heading_style_conversion() {
        for node in ["[Link]", "![Image]"] {
            for (open, close) in [('"', '"'), ('\'', '\''), ('(', ')')] {
                for style in [HeadingStyle::Atx, HeadingStyle::AtxClosed] {
                    let rule = MD003HeadingStyle::new(style);
                    let title = format!("{node}(https://example.com {open}first\nlast{close})\n===");
                    let adjacent = if style == HeadingStyle::Atx {
                        "# Adjacent"
                    } else {
                        "# Adjacent #"
                    };
                    for ending in ["\n", "\r\n"] {
                        let source = format!("{title}\n\nAdjacent\n===\n").replace('\n', ending);
                        let expected = format!("{title}\n\n{adjacent}\n").replace('\n', ending);
                        let ctx = crate::lint_context::LintContext::new(
                            &source,
                            crate::config::MarkdownFlavor::Standard,
                            None,
                        );
                        let warnings = rule.check(&ctx).unwrap();
                        assert_eq!(warnings.len(), 2, "{source}");
                        assert!(warnings[0].fix.is_none());
                        assert!(warnings[1].fix.is_some());
                        assert_eq!(
                            crate::utils::fix_utils::apply_warning_fixes(&source, &warnings).unwrap(),
                            expected
                        );
                        assert_eq!(rule.fix(&ctx).unwrap(), expected);
                    }
                }
            }
        }
    }

    #[test]
    fn test_link_labels_and_external_titles_remain_style_fixable() {
        let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
        for (source, expected) in [
            (
                "[first\nlast](https://example.com \"single title\")\n===\n",
                "# [first last](https://example.com \"single title\")\n",
            ),
            (
                "[first\nlast][ref]\n===\n\n[ref]: https://example.com \"first\nlast\"\n",
                "# [first last][ref]\n\n[ref]: https://example.com \"first\nlast\"\n",
            ),
            (
                "![Image][ref]\n===\n\n[ref]: https://example.com/i.png \"first\nlast\"\n",
                "# ![Image][ref]\n\n[ref]: https://example.com/i.png \"first\nlast\"\n",
            ),
        ] {
            let ctx = crate::lint_context::LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1);
            assert!(warnings[0].fix.is_some(), "{source}");
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }

    #[test]
    fn test_entity_newlines_do_not_block_safe_heading_style_joins() {
        let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
        for entity in ["&NewLine;", "&#10;", "&#xA;"] {
            for (title, joined) in [
                (
                    format!("[first\nlast](https://example.com \"title{entity}line\")"),
                    format!("[first last](https://example.com \"title{entity}line\")"),
                ),
                (
                    format!("![Image](https://example.com\n\"title{entity}line\")"),
                    format!("![Image](https://example.com \"title{entity}line\")"),
                ),
                (
                    format!(
                        "[![Image](https://example.com/i.png \"image{entity}title\")\nLabel](https://example.com \"link{entity}title\")"
                    ),
                    format!(
                        "[![Image](https://example.com/i.png \"image{entity}title\") Label](https://example.com \"link{entity}title\")"
                    ),
                ),
            ] {
                for flavor in [
                    crate::config::MarkdownFlavor::Standard,
                    crate::config::MarkdownFlavor::MDX,
                ] {
                    for ending in ["\n", "\r\n"] {
                        let source = format!("{title}\n===\n").replace('\n', ending);
                        let expected = format!("# {joined}\n").replace('\n', ending);
                        let ctx = crate::lint_context::LintContext::new(&source, flavor, None);
                        let warnings = rule.check(&ctx).unwrap();
                        assert_eq!(warnings.len(), 1, "{source}");
                        assert!(warnings[0].fix.is_some(), "{source}");
                        assert_eq!(rule.fix(&ctx).unwrap(), expected);
                    }
                }
            }
        }
    }

    #[test]
    fn test_mixed_inline_title_data_still_withholds_unsafe_heading_joins() {
        let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
        let source = "[Entity](https://example.com \"title&NewLine;line\") [Raw](https://example.com \"first\nlast\")\n===\n\nAdjacent\n===\n";
        let expected = "[Entity](https://example.com \"title&NewLine;line\") [Raw](https://example.com \"first\nlast\")\n===\n\n# Adjacent\n";
        let ctx = crate::lint_context::LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].fix.is_none());
        assert!(warnings[1].fix.is_some());
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
    }
    #[test]
    fn plain_atx_fixes_preserve_literal_terminal_hashes() {
        let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
        for title in ["Title #", "标题 ###", "#######", "Title\t####", "**Title** ###"] {
            let first_hash = title.len() - title.bytes().rev().take_while(|&byte| byte == b'#').count();
            let escaped = format!("{}\\{}", &title[..first_hash], &title[first_hash..]);
            for ending in ["\n", "\r\n"] {
                for body in [format!("{title}\n===\n"), format!("# {title} #\n")] {
                    let source = body.replace('\n', ending);
                    let expected = format!("# {escaped}{ending}");
                    let ctx = LintContext::new(&source, crate::config::MarkdownFlavor::Standard, None);
                    let warnings = rule.check(&ctx).unwrap();
                    assert_eq!(warnings.len(), 1, "{source}");
                    assert_eq!(
                        crate::utils::fix_utils::apply_warning_fixes(&source, &warnings).unwrap(),
                        expected
                    );
                    assert_eq!(rule.fix(&ctx).unwrap(), expected);
                    let fixed_ctx = LintContext::new(&expected, crate::config::MarkdownFlavor::Standard, None);
                    assert!(rule.check(&fixed_ctx).unwrap().is_empty());
                    assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
                }
            }
        }
    }

    mod atx_to_setext_safety {
        use super::*;
        use crate::config::MarkdownFlavor;
        use crate::utils::fix_utils::apply_warning_fixes;

        /// Level and plain text of every heading as pulldown-cmark renders it.
        fn headings(content: &str) -> Vec<(u8, String)> {
            use pulldown_cmark::{Event, Parser, Tag, TagEnd};
            let mut out = Vec::new();
            let mut current: Option<(u8, String)> = None;
            for event in Parser::new_ext(content, crate::utils::parser_options::rumdl_parser_options_for(content)) {
                match event {
                    Event::Start(Tag::Heading { level, .. }) => current = Some((level as u8, String::new())),
                    Event::End(TagEnd::Heading(_)) => out.extend(current.take()),
                    Event::Text(t) | Event::Code(t) => {
                        if let Some((_, text)) = current.as_mut() {
                            text.push_str(&t);
                        }
                    }
                    _ => {}
                }
            }
            out
        }

        fn run(style: HeadingStyle, flavor: MarkdownFlavor, src: &str) -> (Vec<LintWarning>, String, String) {
            let rule = MD003HeadingStyle::new(style);
            let ctx = LintContext::new(src, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            let by_warning = apply_warning_fixes(src, &warnings).unwrap();
            let by_fix = rule.fix(&ctx).unwrap();
            (warnings, by_warning, by_fix)
        }

        fn assert_declined_with(style: HeadingStyle, flavor: MarkdownFlavor, src: &str) {
            let (warnings, by_warning, by_fix) = run(style, flavor, src);
            assert!(!warnings.is_empty(), "{src:?}: expected a warning to remain");
            assert!(
                warnings.iter().all(|w| w.fix.is_none()),
                "{src:?}: expected no fix, got {warnings:?}"
            );
            assert_eq!(by_warning, src, "{src:?}: warning fixes changed the document");
            assert_eq!(by_fix, src, "{src:?}: fix() changed the document");
        }

        fn assert_declined(src: &str) {
            assert_declined_with(HeadingStyle::Setext1, MarkdownFlavor::Standard, src);
        }

        fn assert_converted_with(style: HeadingStyle, flavor: MarkdownFlavor, src: &str, expected: &str) {
            let (warnings, by_warning, by_fix) = run(style, flavor, src);
            assert!(!warnings.is_empty(), "{src:?}: expected a warning");
            assert_eq!(by_warning, expected, "{src:?}: warning fixes");
            assert_eq!(by_fix, expected, "{src:?}: fix()");
            assert_eq!(
                headings(src),
                headings(expected),
                "{src:?}: converting changed the heading set"
            );
        }

        fn assert_converted(src: &str, expected: &str) {
            assert_converted_with(HeadingStyle::Setext1, MarkdownFlavor::Standard, src, expected);
        }

        #[test]
        fn paragraph_above_is_not_absorbed() {
            assert_declined("para\n# H\n");
            assert_declined("para\n## H\n");
            assert_declined("para\n# H #\n");
            assert_declined("a\n# H\nb\n");
            assert_declined("para\r\n# H\r\n");
        }

        #[test]
        fn list_and_table_above_decline() {
            assert_declined("- item\n# H\n");
            assert_declined("- item\n  # H\n");
            assert_declined("| a | b |\n|---|---|\n| 1 | 2 |\n# H\n");
        }

        #[test]
        fn meta_styles_decline_with_paragraph_above() {
            assert_declined_with(HeadingStyle::SetextWithAtx, MarkdownFlavor::Standard, "para\n# H\n");
            assert_declined_with(
                HeadingStyle::SetextWithAtxClosed,
                MarkdownFlavor::Standard,
                "para\n# H\n",
            );
        }

        #[test]
        fn empty_heading_is_not_deleted() {
            assert_declined("para\n#\n");
            assert_declined("\n#\n");
            assert_declined("\n##\n");
            assert_declined("# \n");
        }

        #[test]
        fn heading_text_that_is_block_syntax_declines() {
            for text in [
                "- a",
                "+ x",
                "* x",
                "1.",
                "1) x",
                ">",
                "> q",
                "---",
                "***",
                "```js",
                "~~~",
                "<div>",
                "<!-- c -->",
                "[x]: /u",
            ] {
                assert_declined(&format!("\n# {text}\n"));
            }
            assert_declined("\n## -\n");
        }

        #[test]
        fn list_continuation_underline_keeps_the_container_indent() {
            assert_converted("- a\n\n  # H\n", "- a\n\n  H\n  =\n");
            assert_converted("1. a\n\n   ## H\n", "1. a\n\n   H\n   -\n");
            assert_converted("\n   # H\n", "\n   H\n   =\n");
        }

        #[test]
        fn blockquote_heading_is_left_alone() {
            assert_declined("> # H\n");
        }

        #[test]
        fn safe_positions_still_convert() {
            assert_converted(
                "# Heading 1\n## Heading 2\n",
                "Heading 1\n=========\nHeading 2\n---------\n",
            );
            assert_converted("# A\n# B\n", "A\n=\nB\n=\n");
            assert_converted("# H\n", "H\n=\n");
            assert_converted("para\n\n# H\n", "para\n\nH\n=\n");
            assert_converted("---\n\n# H\n", "---\n\nH\n=\n");
            assert_converted("***\n# H\n", "***\nH\n=\n");
            assert_converted("```\ncode\n```\n# H\n", "```\ncode\n```\nH\n=\n");
            assert_converted("Foo\n===\n# H\n", "Foo\n===\nH\n=\n");
            assert_converted("---\ntitle: x\n---\n# H\n", "---\ntitle: x\n---\nH\n=\n");
            assert_converted("# H #\n", "H\n=\n");
            assert_converted("para\n\n## H\n", "para\n\nH\n-\n");
        }

        #[test]
        fn setext_with_atx_converts_only_levels_one_and_two() {
            assert_converted_with(
                HeadingStyle::SetextWithAtx,
                MarkdownFlavor::Standard,
                "# A\n## B\n### C\n",
                "A\n=\nB\n-\n### C\n",
            );
            assert_converted_with(
                HeadingStyle::SetextWithAtx,
                MarkdownFlavor::Standard,
                "\n# A #\n",
                "\nA\n=\n",
            );
        }

        #[test]
        fn levels_three_and_up_stay_atx() {
            let (warnings, by_warning, _) = run(HeadingStyle::Setext1, MarkdownFlavor::Standard, "para\n### H\n");
            assert!(warnings.is_empty());
            assert_eq!(by_warning, "para\n### H\n");
        }

        #[test]
        fn text_that_only_looks_like_syntax_converts() {
            assert_converted("\n# ===\n", "\n===\n===\n");
            assert_converted("\n# \\-\n", "\n\\-\n==\n");
            assert_converted("\n# foo\\\n", "\nfoo\\\n====\n");
            assert_converted("\n# ::: note\n", "\n::: note\n========\n");
            assert_converted("\n# Normal title\n", "\nNormal title\n============\n");
        }

        #[test]
        fn flavor_block_syntax_declines() {
            for (flavor, text) in [
                (MarkdownFlavor::Pandoc, "::: note"),
                (MarkdownFlavor::Quarto, "::: note"),
                (MarkdownFlavor::MkDocs, "!!! note"),
                (MarkdownFlavor::MkDocs, "??? note"),
                (MarkdownFlavor::MkDocs, "=== \"Tab\""),
                (MarkdownFlavor::MDX, "<Foo />"),
                (MarkdownFlavor::MDX, "import x from 'y'"),
                (MarkdownFlavor::MDX, "{expr}"),
                (MarkdownFlavor::Quarto, "$$"),
                (MarkdownFlavor::Pandoc, "$$ x"),
                (MarkdownFlavor::Standard, "+++"),
            ] {
                let src = format!("\n# {text}\n");
                let (_, by_warning, by_fix) = run(HeadingStyle::Setext1, flavor, &src);
                assert_eq!(
                    by_warning, src,
                    "{flavor:?} {text:?}: warning fixes changed the document"
                );
                assert_eq!(by_fix, src, "{flavor:?} {text:?}: fix() changed the document");
            }
        }

        #[test]
        fn fixed_output_keeps_headings_for_every_flavor() {
            for flavor in [
                MarkdownFlavor::Standard,
                MarkdownFlavor::Pandoc,
                MarkdownFlavor::Quarto,
                MarkdownFlavor::MkDocs,
                MarkdownFlavor::MDX,
            ] {
                let src = "\n# Plain title\n\n## Second\n";
                let (_, by_warning, _) = run(HeadingStyle::Setext1, flavor, src);
                assert_eq!(headings(src), headings(&by_warning), "{flavor:?}");
                assert!(by_warning.contains("Plain title\n===="), "{flavor:?}: {by_warning:?}");
            }
        }
    }
    mod quoted_headings {
        use super::*;
        use crate::config::MarkdownFlavor;

        fn check_with(style: HeadingStyle, src: &str) -> Vec<LintWarning> {
            let ctx = LintContext::new(src, MarkdownFlavor::Standard, None);
            MD003HeadingStyle::new(style).check(&ctx).unwrap()
        }

        fn lines_of(warnings: &[LintWarning]) -> Vec<usize> {
            warnings.iter().map(|w| w.line).collect()
        }

        fn assert_unchanged_by_fix(style: HeadingStyle, src: &str) {
            let ctx = LintContext::new(src, MarkdownFlavor::Standard, None);
            let rule = MD003HeadingStyle::new(style);
            assert_eq!(rule.fix(&ctx).unwrap(), src, "{src:?}: fix() changed the document");
        }

        #[test]
        fn quoted_headings_are_judged_against_the_consistent_style() {
            // Setext1, Setext2 and ATX each have one heading, and a tie goes to
            // ATX: the unquoted setext heading is fixable, the quoted one is not.
            let src = "Top\n===\n\n> ## Quote atx\n>\n> Setext in quote\n> ---\n";
            let warnings = check_with(HeadingStyle::Consistent, src);
            assert_eq!(lines_of(&warnings), vec![1, 6], "{warnings:?}");
            assert!(warnings[0].fix.is_some());
            assert!(warnings[1].fix.is_none());

            // The quoted ATX heading is the odd one out when setext prevails.
            let src = "Top\n===\n\nNext\n====\n\n> ## Quote atx\n";
            let warnings = check_with(HeadingStyle::Consistent, src);
            assert_eq!(lines_of(&warnings), vec![7], "{warnings:?}");
            assert!(warnings[0].fix.is_none());
            assert_unchanged_by_fix(HeadingStyle::Consistent, "> ## Quote atx\n\nTop\n===\n\nNext\n====\n");
        }

        #[test]
        fn quoted_setext_heading_is_flagged_in_an_atx_document() {
            let src = "# Top\n\n> Setext in quote\n> ---\n";
            let warnings = check_with(HeadingStyle::Consistent, src);
            // One ATX and one setext heading tie, ATX wins.
            assert_eq!(lines_of(&warnings), vec![3], "{warnings:?}");
            assert!(warnings[0].message.contains("# Heading"), "{}", warnings[0].message);
            assert!(warnings.iter().all(|w| w.fix.is_none()));
            assert_unchanged_by_fix(HeadingStyle::Consistent, src);
        }

        #[test]
        fn unquoted_setext_in_the_same_shape_is_flagged_too() {
            let warnings = check_with(HeadingStyle::Consistent, "# Top\n\nPlain\n---\n");
            assert_eq!(lines_of(&warnings), vec![3]);
        }

        #[test]
        fn nested_quotes_are_checked() {
            let src = "# Top\n\n> > Inner\n> > ---\n";
            let warnings = check_with(HeadingStyle::Consistent, src);
            assert_eq!(lines_of(&warnings), vec![3], "{warnings:?}");
            assert!(warnings[0].fix.is_none());
        }

        #[test]
        fn quote_inside_a_list_item_is_checked() {
            let src = "# Top\n\n- item\n\n  > Quoted\n  > ---\n";
            let warnings = check_with(HeadingStyle::Consistent, src);
            assert_eq!(lines_of(&warnings), vec![5], "{warnings:?}");
            assert!(warnings[0].fix.is_none());
            let src = "- > Quoted\n  > ---\n\n# Top\n";
            let warnings = check_with(HeadingStyle::Atx, src);
            assert_eq!(lines_of(&warnings), vec![1], "{warnings:?}");
        }

        #[test]
        fn list_item_inside_a_quote_is_checked() {
            let src = "# Top\n\n> - # Item heading #\n";
            let warnings = check_with(HeadingStyle::Atx, src);
            assert_eq!(lines_of(&warnings), vec![3], "{warnings:?}");
            assert!(warnings[0].fix.is_none());
        }

        #[test]
        fn fixed_style_flags_quoted_headings_without_a_fix() {
            for (style, src) in [
                (HeadingStyle::Setext1, "> # H\n"),
                (HeadingStyle::Atx, "> H\n> =\n"),
                (HeadingStyle::AtxClosed, "> ## H\n"),
                (HeadingStyle::Atx, "> > ## H ##\n"),
            ] {
                let warnings = check_with(style, src);
                assert_eq!(warnings.len(), 1, "{style:?} {src:?}: {warnings:?}");
                assert!(warnings[0].fix.is_none(), "{style:?} {src:?}");
                assert_unchanged_by_fix(style, src);
            }
        }

        #[test]
        fn quote_only_document_is_not_skipped() {
            let ctx = LintContext::new("> # H\n", MarkdownFlavor::Standard, None);
            let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
            assert!(!rule.should_skip(&ctx));
        }

        #[test]
        fn quoted_headings_vote_in_the_consistent_style() {
            // Two quoted setext headings outvote one unquoted ATX heading, so the
            // ATX heading is the one reported (and it is fixable).
            let src = "# Top\n\n> A\n> -\n\n> B\n> -\n";
            let warnings = check_with(HeadingStyle::Consistent, src);
            assert_eq!(lines_of(&warnings), vec![1], "{warnings:?}");
            assert!(warnings[0].fix.is_some(), "the unquoted heading stays fixable");
        }

        #[test]
        fn quoted_headings_alone_can_set_the_style() {
            let src = "> A\n> -\n\n> B\n> -\n";
            assert!(check_with(HeadingStyle::Consistent, src).is_empty());
        }

        #[test]
        fn consistent_quoted_and_unquoted_headings_are_clean() {
            assert!(check_with(HeadingStyle::Consistent, "# A\n\n> ## B\n\n> > ### C\n").is_empty());
            assert!(check_with(HeadingStyle::Consistent, "A\n=\n\n> B\n> -\n").is_empty());
        }

        #[test]
        fn setext_levels_above_two_stay_atx_in_quotes() {
            assert!(check_with(HeadingStyle::Setext1, "> ### H\n").is_empty());
            assert!(check_with(HeadingStyle::SetextWithAtx, "> ### H\n").is_empty());
        }

        #[test]
        fn code_and_html_look_alikes_are_not_headings() {
            assert!(check_with(HeadingStyle::Setext1, "> ```\n> # H\n> ```\n").is_empty());
            assert!(check_with(HeadingStyle::Setext1, ">     # H\n").is_empty());
            assert!(check_with(HeadingStyle::Setext1, "> text # not a heading\n").is_empty());
        }
    }
}
