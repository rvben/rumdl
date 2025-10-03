use crate::rule::Rule;
use crate::rule::{Fix, LintError, LintResult, LintWarning, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::rules::heading_utils::HeadingStyle;
use crate::utils::range_utils::calculate_heading_range;
use toml;

mod md002_config;
use md002_config::MD002Config;

/// Rule MD002: First heading should be a top-level heading
///
/// See [docs/md002.md](../../docs/md002.md) for full documentation, configuration, and examples.
///
/// This rule enforces that the first heading in a document is a top-level heading (typically h1),
/// which establishes the main topic or title of the document.
///
/// ## Purpose
///
/// - **Document Structure**: Ensures proper document hierarchy with a single top-level heading
/// - **Accessibility**: Improves screen reader navigation by providing a clear document title
/// - **SEO**: Helps search engines identify the primary topic of the document
/// - **Readability**: Provides users with a clear understanding of the document's main subject
///
/// ## Configuration Options
///
/// The rule supports customizing the required level for the first heading:
///
/// ```yaml
/// MD002:
///   level: 1  # The heading level required for the first heading (default: 1)
/// ```
///
/// Setting `level: 2` would require the first heading to be an h2 instead of h1.
///
/// ## Examples
///
/// ### Correct (with default configuration)
///
/// ```markdown
/// # Document Title
///
/// ## Section 1
///
/// Content here...
///
/// ## Section 2
///
/// More content...
/// ```
///
/// ### Incorrect (with default configuration)
///
/// ```markdown
/// ## Introduction
///
/// Content here...
///
/// # Main Title
///
/// More content...
/// ```
///
/// ## Behavior
///
/// This rule:
/// - Ignores front matter (YAML metadata at the beginning of the document)
/// - Works with both ATX (`#`) and Setext (underlined) heading styles
/// - Only examines the first heading it encounters
/// - Does not apply to documents with no headings
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Changes the level of the first heading to match the configured level
/// - Preserves the original heading style (ATX, closed ATX, or Setext)
/// - Maintains indentation and other formatting
///
/// ## Rationale
///
/// Having a single top-level heading establishes the document's primary topic and creates
/// a logical structure. This follows semantic HTML principles where each page should have
/// a single `<h1>` element that defines its main subject.
///
#[derive(Debug, Clone, Default)]
pub struct MD002FirstHeadingH1 {
    config: MD002Config,
}

impl MD002FirstHeadingH1 {
    pub fn new(level: u32) -> Self {
        Self {
            config: MD002Config { level },
        }
    }

    pub fn from_config_struct(config: MD002Config) -> Self {
        Self { config }
    }
}

impl Rule for MD002FirstHeadingH1 {
    fn name(&self) -> &'static str {
        "MD002"
    }

    fn description(&self) -> &'static str {
        "First heading should be top level"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        // Early return for empty content
        if content.is_empty() {
            return Ok(vec![]);
        }

        // Find the first heading using pre-computed line info
        let first_heading = ctx
            .lines
            .iter()
            .enumerate()
            .find_map(|(line_num, line_info)| line_info.heading.as_ref().map(|h| (line_num, line_info, h)));

        if let Some((line_num, line_info, heading)) = first_heading {
            // Check if the first heading is on the first non-empty line after front matter.
            // If it is AND it's already H1, MD002 should not trigger (already correct).
            // If it is but NOT H1, MD002 still should not trigger for markdownlint compatibility
            // (MD002 is implicitly disabled when MD041 would be satisfied).
            let first_content_line = ctx
                .lines
                .iter()
                .enumerate()
                .find(|(_, line_info)| !line_info.in_front_matter && !line_info.content.trim().is_empty())
                .map(|(idx, _)| idx);

            // If the first heading is on the first content line, don't trigger MD002
            // This matches markdownlint behavior where MD002 doesn't apply to first-line headings
            if let Some(first_line_idx) = first_content_line
                && line_num == first_line_idx
            {
                return Ok(vec![]);
            }

            // Otherwise check if the heading level is correct
            if heading.level != self.config.level as u8 {
                let message = format!(
                    "First heading should be level {}, found level {}",
                    self.config.level, heading.level
                );

                // Calculate the fix
                let fix = {
                    let replacement = crate::rules::heading_utils::HeadingUtils::convert_heading_style(
                        &heading.text,
                        self.config.level,
                        match heading.style {
                            crate::lint_context::HeadingStyle::ATX => {
                                if heading.has_closing_sequence {
                                    HeadingStyle::AtxClosed
                                } else {
                                    HeadingStyle::Atx
                                }
                            }
                            crate::lint_context::HeadingStyle::Setext1 => HeadingStyle::Setext1,
                            crate::lint_context::HeadingStyle::Setext2 => HeadingStyle::Setext2,
                        },
                    );

                    // Use line content range to replace the entire heading line
                    let line_index = crate::utils::range_utils::LineIndex::new(content.to_string());
                    Some(Fix {
                        range: line_index.line_content_range(line_num + 1), // Convert to 1-indexed
                        replacement,
                    })
                };

                // Calculate precise range: highlight the entire first heading
                let (start_line, start_col, end_line, end_col) =
                    calculate_heading_range(line_num + 1, &line_info.content);

                return Ok(vec![LintWarning {
                    message,
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix,
                    rule_name: Some(self.name()),
                }]);
            }
        }

        Ok(vec![])
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Find the first heading using pre-computed line info
        let first_heading = ctx
            .lines
            .iter()
            .enumerate()
            .find_map(|(line_num, line_info)| line_info.heading.as_ref().map(|h| (line_num, line_info, h)));

        if let Some((line_num, line_info, heading)) = first_heading {
            // Check if the first heading is on the first non-empty line after front matter.
            // If it is, MD002 should not apply (markdownlint compatibility).
            let first_content_line = ctx
                .lines
                .iter()
                .enumerate()
                .find(|(_, line_info)| !line_info.in_front_matter && !line_info.content.trim().is_empty())
                .map(|(idx, _)| idx);

            if let Some(first_line_idx) = first_content_line
                && line_num == first_line_idx
            {
                return Ok(content.to_string());
            }

            // If we're here, the heading is not on the first line, so check if it needs fixing
            if heading.level == self.config.level as u8 {
                return Ok(content.to_string());
            }

            let lines: Vec<&str> = content.lines().collect();
            let mut fixed_lines = Vec::new();
            let mut i = 0;

            while i < lines.len() {
                if i == line_num {
                    // This is the first heading line that needs fixing
                    let indent = " ".repeat(line_info.indent);
                    let heading_text = heading.text.trim();

                    match heading.style {
                        crate::lint_context::HeadingStyle::ATX => {
                            let hashes = "#".repeat(self.config.level as usize);
                            if heading.has_closing_sequence {
                                // Preserve closed ATX: # Heading #
                                fixed_lines.push(format!("{indent}{hashes} {heading_text} {hashes}"));
                            } else {
                                // Standard ATX: # Heading
                                fixed_lines.push(format!("{indent}{hashes} {heading_text}"));
                            }
                            i += 1;
                        }
                        crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2 => {
                            // For Setext, we need to update the underline
                            fixed_lines.push(lines[i].to_string()); // Keep heading text as-is
                            i += 1;
                            if i < lines.len() {
                                // Replace the underline
                                let underline = if self.config.level == 1 { "=======" } else { "-------" };
                                fixed_lines.push(underline.to_string());
                                i += 1;
                            }
                        }
                    }
                    continue;
                }

                fixed_lines.push(lines[i].to_string());
                i += 1;
            }

            Ok(fixed_lines.join("\n"))
        } else {
            // No headings found
            Ok(content.to_string())
        }
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Check for heading indicators: # for ATX, = or - for Setext
        ctx.content.is_empty() || (!ctx.has_char('#') && !ctx.has_char('=') && !ctx.has_char('-'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD002Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD002Config::RULE_NAME.to_string(), toml::Value::Table(table)))
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
        let rule_config = crate::rule_config_serde::load_rule_config::<MD002Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_default_config() {
        let rule = MD002FirstHeadingH1::default();
        assert_eq!(rule.config.level, 1);
    }

    #[test]
    fn test_custom_config() {
        let rule = MD002FirstHeadingH1::new(2);
        assert_eq!(rule.config.level, 2);
    }

    #[test]
    fn test_correct_h1_first_heading() {
        let rule = MD002FirstHeadingH1::new(1);
        let content = "# Main Title\n\n## Subsection\n\nContent here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_incorrect_h2_first_heading() {
        // When heading is on first line, MD002 doesn't trigger (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(1);
        let content = "## Introduction\n\nContent here\n\n# Main Title";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0); // MD002 doesn't trigger for first-line headings
    }

    #[test]
    fn test_empty_document() {
        let rule = MD002FirstHeadingH1::default();
        let ctx = LintContext::new("", crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_document_with_no_headings() {
        let rule = MD002FirstHeadingH1::default();
        let content = "This is just paragraph text.\n\nMore paragraph text.\n\n- List item 1\n- List item 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_setext_style_heading() {
        // When heading is on first line, MD002 doesn't trigger (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(1);
        let content = "Introduction\n------------\n\nContent here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0); // MD002 doesn't trigger for first-line headings
    }

    #[test]
    fn test_correct_setext_h1() {
        let rule = MD002FirstHeadingH1::new(1);
        let content = "Main Title\n==========\n\nContent here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_with_front_matter() {
        // When heading is immediately after front matter, MD002 doesn't trigger (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(1);
        let content = "---\ntitle: Test Document\nauthor: Test Author\n---\n## Introduction\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0); // MD002 doesn't trigger for first-line headings after front matter
    }

    #[test]
    fn test_fix_atx_heading() {
        // When heading is on first line, MD002 doesn't fix (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(1);
        let content = "## Introduction\n\nContent here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content); // No fix applied for first-line headings
    }

    #[test]
    fn test_fix_closed_atx_heading() {
        // When heading is on first line, MD002 doesn't fix (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(1);
        let content = "## Introduction ##\n\nContent here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content); // No fix applied for first-line headings
    }

    #[test]
    fn test_fix_setext_heading() {
        // When heading is on first line, MD002 doesn't fix (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(1);
        let content = "Introduction\n------------\n\nContent here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content); // No fix applied for first-line headings
    }

    #[test]
    fn test_fix_with_indented_heading() {
        // When heading is on first line, MD002 doesn't fix (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(1);
        let content = "  ## Introduction\n\nContent here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content); // No fix applied for first-line headings
    }

    #[test]
    fn test_custom_level_requirement() {
        // When heading is on first line, MD002 doesn't trigger (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(2);
        let content = "# Main Title\n\n## Subsection";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0); // MD002 doesn't trigger for first-line headings
    }

    #[test]
    fn test_fix_to_custom_level() {
        // When heading is on first line, MD002 doesn't fix (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(2);
        let content = "# Main Title\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content); // No fix applied for first-line headings
    }

    #[test]
    fn test_multiple_headings() {
        // When heading is on first line, MD002 doesn't trigger (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(1);
        let content = "### Introduction\n\n# Main Title\n\n## Section\n\n#### Subsection";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0); // MD002 doesn't trigger for first-line headings
    }

    #[test]
    fn test_should_skip_optimization() {
        let rule = MD002FirstHeadingH1::default();

        // Should skip empty content
        let ctx = LintContext::new("", crate::config::MarkdownFlavor::Standard);
        assert!(rule.should_skip(&ctx));

        // Should skip content without heading indicators
        let ctx = LintContext::new(
            "Just paragraph text\n\nMore text",
            crate::config::MarkdownFlavor::Standard,
        );
        assert!(rule.should_skip(&ctx));

        // Should not skip content with ATX heading
        let ctx = LintContext::new("Some text\n# Heading", crate::config::MarkdownFlavor::Standard);
        assert!(!rule.should_skip(&ctx));

        // Should not skip content with potential setext heading
        let ctx = LintContext::new("Title\n=====", crate::config::MarkdownFlavor::Standard);
        assert!(!rule.should_skip(&ctx));
    }

    #[test]
    fn test_rule_metadata() {
        let rule = MD002FirstHeadingH1::default();
        assert_eq!(rule.name(), "MD002");
        assert_eq!(rule.description(), "First heading should be top level");
        assert_eq!(rule.category(), RuleCategory::Heading);
    }

    #[test]
    fn test_from_config_struct() {
        let config = MD002Config { level: 3 };
        let rule = MD002FirstHeadingH1::from_config_struct(config);
        assert_eq!(rule.config.level, 3);
    }

    #[test]
    fn test_fix_preserves_content_structure() {
        // When heading is on first line, MD002 doesn't fix (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(1);
        let content = "### Heading\n\nParagraph 1\n\n## Section\n\nParagraph 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content); // No fix applied for first-line headings
    }

    #[test]
    fn test_long_setext_underline() {
        // When heading is on first line, MD002 doesn't fix (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(1);
        let content = "Short Title\n----------------------------------------\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content); // No fix applied for first-line headings
    }

    #[test]
    fn test_fix_already_correct() {
        let rule = MD002FirstHeadingH1::new(1);
        let content = "# Correct Heading\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_heading_with_special_characters() {
        // When heading is on first line, MD002 doesn't trigger (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(1);
        let content = "## Heading with **bold** and _italic_ text\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0); // MD002 doesn't trigger for first-line headings

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content); // No fix applied for first-line headings
    }

    #[test]
    fn test_atx_heading_with_extra_spaces() {
        // When heading is on first line, MD002 doesn't fix (markdownlint compatibility)
        let rule = MD002FirstHeadingH1::new(1);
        let content = "##    Introduction    \n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content); // No fix applied for first-line headings
    }

    #[test]
    fn test_md002_does_not_trigger_when_first_line_is_heading() {
        // This tests markdownlint compatibility: MD002 should not trigger
        // when the first line is a heading (even if it's not level 1)
        // because MD041 would handle this case
        let rule = MD002FirstHeadingH1::new(1);
        let content = "## Introduction\n\nContent here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // MD002 should NOT trigger because the heading is on the first line
        assert_eq!(result.len(), 0, "MD002 should not trigger when first line is a heading");
    }

    #[test]
    fn test_md002_triggers_when_heading_is_not_first_line() {
        // MD002 should still trigger when the heading is NOT on the first line
        let rule = MD002FirstHeadingH1::new(1);
        let content = "Some text before heading\n\n## Introduction\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            1,
            "MD002 should trigger when heading is not on first line"
        );
        assert!(result[0].message.contains("First heading should be level 1"));
    }

    #[test]
    fn test_md002_with_front_matter_and_first_line_heading() {
        // MD002 should not trigger when the first line after front matter is a heading
        let rule = MD002FirstHeadingH1::new(1);
        let content = "---\ntitle: Test\n---\n## Introduction\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "MD002 should not trigger when first line after front matter is a heading"
        );
    }

    #[test]
    fn test_md002_with_front_matter_and_delayed_heading() {
        // MD002 should trigger when the heading is not immediately after front matter
        let rule = MD002FirstHeadingH1::new(1);
        let content = "---\ntitle: Test\n---\nSome text\n\n## Introduction\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            1,
            "MD002 should trigger when heading is not immediately after front matter"
        );
    }

    #[test]
    fn test_md002_fix_does_not_change_first_line_heading() {
        // Fix should not change a heading that's on the first line
        let rule = MD002FirstHeadingH1::new(1);
        let content = "### Third Level Heading\n\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, content, "Fix should not change heading on first line");
    }
}
