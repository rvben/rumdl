//!
//! Rule MD003: Heading style
//!
//! See [docs/md003.md](../../docs/md003.md) for full documentation, configuration, and examples.

use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::rules::heading_utils::HeadingStyle;
use crate::utils::range_utils::calculate_heading_range;
use toml;

mod md003_config;
use md003_config::MD003Config;

/// Rule MD003: Heading style
#[derive(Clone, Default)]
pub struct MD003HeadingStyle {
    config: MD003Config,
}

impl MD003HeadingStyle {
    pub fn new(style: HeadingStyle) -> Self {
        Self {
            config: MD003Config { style },
        }
    }

    pub fn from_config_struct(config: MD003Config) -> Self {
        Self { config }
    }

    /// Check if we should use consistent mode (detect first style)
    fn is_consistent_mode(&self) -> bool {
        // Check for the Consistent variant explicitly
        self.config.style == HeadingStyle::Consistent
    }

    /// Gets the target heading style based on configuration and document content
    fn get_target_style(&self, ctx: &crate::lint_context::LintContext) -> HeadingStyle {
        if !self.is_consistent_mode() {
            return self.config.style;
        }

        // Count all heading styles to determine most prevalent (prevalence-based approach)
        let mut style_counts = std::collections::HashMap::new();

        for line_info in &ctx.lines {
            if let Some(heading) = &line_info.heading {
                // Skip invalid headings (e.g., `#NoSpace` which lacks required space after #)
                if !heading.is_valid {
                    continue;
                }

                // Map from LintContext heading style to rules heading style and count
                let style = match heading.style {
                    crate::lint_context::HeadingStyle::ATX => {
                        if heading.has_closing_sequence {
                            HeadingStyle::AtxClosed
                        } else {
                            HeadingStyle::Atx
                        }
                    }
                    crate::lint_context::HeadingStyle::Setext1 => HeadingStyle::Setext1,
                    crate::lint_context::HeadingStyle::Setext2 => HeadingStyle::Setext2,
                };
                *style_counts.entry(style).or_insert(0) += 1;
            }
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
            .map(|(style, _)| style)
            .unwrap_or(HeadingStyle::Atx)
    }
}

impl Rule for MD003HeadingStyle {
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
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                // Skip invalid headings (e.g., `#NoSpace` which lacks required space after #)
                if !heading.is_valid {
                    continue;
                }

                let level = heading.level;

                // Map the cached heading style to the rule's HeadingStyle
                let current_style = match heading.style {
                    crate::lint_context::HeadingStyle::ATX => {
                        if heading.has_closing_sequence {
                            HeadingStyle::AtxClosed
                        } else {
                            HeadingStyle::Atx
                        }
                    }
                    crate::lint_context::HeadingStyle::Setext1 => HeadingStyle::Setext1,
                    crate::lint_context::HeadingStyle::Setext2 => HeadingStyle::Setext2,
                };

                // Determine expected style based on level and target
                let expected_style = match target_style {
                    HeadingStyle::Setext1 | HeadingStyle::Setext2 => {
                        if level > 2 {
                            // Setext only supports levels 1-2, so levels 3+ must be ATX
                            HeadingStyle::Atx
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

                if current_style != expected_style {
                    // Generate fix for this heading
                    let fix = {
                        use crate::rules::heading_utils::HeadingUtils;

                        // Convert heading to target style
                        let converted_heading =
                            HeadingUtils::convert_heading_style(&heading.text, level as u32, expected_style);

                        // Preserve original indentation (including tabs)
                        let line = line_info.content(ctx.content);
                        let original_indent = &line[..line_info.indent];
                        let final_heading = format!("{original_indent}{converted_heading}");

                        // Calculate the correct range for the heading
                        let range = ctx.line_index.line_content_range(line_num + 1);

                        Some(crate::rule::Fix {
                            range,
                            replacement: final_heading,
                        })
                    };

                    // Calculate precise character range for the heading marker
                    let (start_line, start_col, end_line, end_col) =
                        calculate_heading_range(line_num + 1, line_info.content(ctx.content));

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
                                HeadingStyle::SetextWithAtx => "setext_with_atx style",
                                HeadingStyle::SetextWithAtxClosed => "setext_with_atx_closed style",
                            },
                            match current_style {
                                HeadingStyle::Atx => "# Heading",
                                HeadingStyle::AtxClosed => "# Heading #",
                                HeadingStyle::Setext1 => "Heading (underlined with =)",
                                HeadingStyle::Setext2 => "Heading (underlined with -)",
                                HeadingStyle::Consistent => "consistent style",
                                HeadingStyle::SetextWithAtx => "setext_with_atx style",
                                HeadingStyle::SetextWithAtxClosed => "setext_with_atx_closed style",
                            }
                        ),
                        severity: Severity::Warning,
                        fix,
                    });
                }
            }
        }

        Ok(result)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Get all warnings with their fixes
        let warnings = self.check(ctx)?;

        // If no warnings, return original content
        if warnings.is_empty() {
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

    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Fast path: check if document likely has headings using character frequency
        if ctx.content.is_empty() || !ctx.likely_has_headings() {
            return true;
        }
        // Verify headings actually exist (handles false positives from character frequency)
        !ctx.lines.iter().any(|line| line.heading.is_some())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD003Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD003Config::RULE_NAME.to_string(), toml::Value::Table(table)))
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
        let rule_config = crate::rule_config_serde::load_rule_config::<MD003Config>(config);
        Box::new(Self::from_config_struct(rule_config))
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
}
