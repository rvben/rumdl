use crate::HeadingStyle;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::rules::heading_utils::HeadingUtils;
use crate::utils::range_utils::calculate_heading_range;
use regex::Regex;

/// Rule MD001: Heading levels should only increment by one level at a time
///
/// See [docs/md001.md](../../docs/md001.md) for full documentation, configuration, and examples.
///
/// This rule enforces a fundamental principle of document structure: heading levels
/// should increase by exactly one level at a time to maintain a proper document hierarchy.
///
/// ## Purpose
///
/// Proper heading structure creates a logical document outline and improves:
/// - Readability for humans
/// - Accessibility for screen readers
/// - Navigation in rendered documents
/// - Automatic generation of tables of contents
///
/// ## Examples
///
/// ### Correct Heading Structure
/// ```markdown
/// # Heading 1
/// ## Heading 2
/// ### Heading 3
/// ## Another Heading 2
/// ```
///
/// ### Incorrect Heading Structure
/// ```markdown
/// # Heading 1
/// ### Heading 3 (skips level 2)
/// #### Heading 4
/// ```
///
/// ## Behavior
///
/// This rule:
/// - Tracks the heading level throughout the document
/// - Validates that each new heading is at most one level deeper than the previous heading
/// - Allows heading levels to decrease by any amount (e.g., going from ### to #)
/// - Works with both ATX (`#`) and Setext (underlined) heading styles
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Changes the level of non-compliant headings to be one level deeper than the previous heading
/// - Preserves the original heading style (ATX or Setext)
/// - Maintains indentation and other formatting
///
/// ## Rationale
///
/// Skipping heading levels (e.g., from `h1` to `h3`) can confuse readers and screen readers
/// by creating gaps in the document structure. Consistent heading increments create a proper
/// hierarchical outline essential for well-structured documents.
///
/// ## Front Matter Title Support
///
/// When `front_matter_title` is enabled (default: true), this rule recognizes a `title:` field
/// in YAML/TOML frontmatter as an implicit level-1 heading. This allows documents like:
///
/// ```markdown
/// ---
/// title: My Document
/// ---
///
/// ## First Section
/// ```
///
/// Without triggering a warning about skipping from H1 to H2, since the frontmatter title
/// counts as the H1.
///
#[derive(Debug, Clone)]
pub struct MD001HeadingIncrement {
    /// Whether to treat frontmatter title field as an implicit H1
    pub front_matter_title: bool,
    /// Optional regex pattern to match custom title fields in frontmatter
    pub front_matter_title_pattern: Option<Regex>,
}

impl Default for MD001HeadingIncrement {
    fn default() -> Self {
        Self {
            front_matter_title: true,
            front_matter_title_pattern: None,
        }
    }
}

impl MD001HeadingIncrement {
    /// Create a new instance with specified settings
    pub fn new(front_matter_title: bool) -> Self {
        Self {
            front_matter_title,
            front_matter_title_pattern: None,
        }
    }

    /// Create a new instance with a custom pattern for matching title fields
    pub fn with_pattern(front_matter_title: bool, pattern: Option<String>) -> Self {
        let front_matter_title_pattern = pattern.and_then(|p| match Regex::new(&p) {
            Ok(regex) => Some(regex),
            Err(e) => {
                log::warn!("Invalid front_matter_title_pattern regex for MD001: {e}");
                None
            }
        });

        Self {
            front_matter_title,
            front_matter_title_pattern,
        }
    }

    /// Check if the document has a front matter title field
    fn has_front_matter_title(&self, content: &str) -> bool {
        if !self.front_matter_title {
            return false;
        }

        // If we have a custom pattern, use it to search front matter content
        if let Some(ref pattern) = self.front_matter_title_pattern {
            let front_matter_lines = FrontMatterUtils::extract_front_matter(content);
            for line in front_matter_lines {
                if pattern.is_match(line) {
                    return true;
                }
            }
            return false;
        }

        // Default behavior: check for "title:" field
        FrontMatterUtils::has_front_matter_field(content, "title:")
    }
}

impl Rule for MD001HeadingIncrement {
    fn name(&self) -> &'static str {
        "MD001"
    }

    fn description(&self) -> &'static str {
        "Heading levels should only increment by one level at a time"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // If frontmatter has a title field, treat it as an implicit H1
        let mut prev_level: Option<usize> = if self.has_front_matter_title(ctx.content) {
            Some(1)
        } else {
            None
        };

        // Process valid headings using the filtered iterator
        for valid_heading in ctx.valid_headings() {
            let heading = valid_heading.heading;
            let line_info = valid_heading.line_info;
            let level = heading.level as usize;

            // Check if this heading level is more than one level deeper than the previous
            if let Some(prev) = prev_level
                && level > prev + 1
            {
                // Preserve original indentation (including tabs)
                let line = line_info.content(ctx.content);
                let original_indent = &line[..line_info.indent];
                let heading_text = &heading.text;

                // Map heading style
                let style = match heading.style {
                    crate::lint_context::HeadingStyle::ATX => HeadingStyle::Atx,
                    crate::lint_context::HeadingStyle::Setext1 => HeadingStyle::Setext1,
                    crate::lint_context::HeadingStyle::Setext2 => HeadingStyle::Setext2,
                };

                // Create a fix with the correct heading level
                let fixed_level = prev + 1;
                let replacement = HeadingUtils::convert_heading_style(heading_text, fixed_level as u32, style);

                // Calculate precise range: highlight the entire heading
                let line_content = line_info.content(ctx.content);
                let (start_line, start_col, end_line, end_col) =
                    calculate_heading_range(valid_heading.line_num, line_content);

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!("Expected heading level {}, but found heading level {}", prev + 1, level),
                    severity: Severity::Error,
                    fix: Some(Fix {
                        range: ctx.line_index.line_content_range(valid_heading.line_num),
                        replacement: format!("{original_indent}{replacement}"),
                    }),
                });
            }

            prev_level = Some(level);
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut fixed_lines = Vec::new();

        // If frontmatter has a title field, treat it as an implicit H1
        let mut prev_level: Option<usize> = if self.has_front_matter_title(ctx.content) {
            Some(1)
        } else {
            None
        };

        for line_info in ctx.lines.iter() {
            if let Some(heading) = &line_info.heading {
                // Skip invalid headings (e.g., `#NoSpace` which lacks required space after #)
                if !heading.is_valid {
                    fixed_lines.push(line_info.content(ctx.content).to_string());
                    continue;
                }

                let level = heading.level as usize;
                let mut fixed_level = level;

                // Check if this heading needs fixing
                if let Some(prev) = prev_level
                    && level > prev + 1
                {
                    fixed_level = prev + 1;
                }

                // Map heading style - when fixing, we may need to change Setext style based on level
                let style = match heading.style {
                    crate::lint_context::HeadingStyle::ATX => HeadingStyle::Atx,
                    crate::lint_context::HeadingStyle::Setext1 => {
                        if fixed_level == 1 {
                            HeadingStyle::Setext1
                        } else {
                            HeadingStyle::Setext2
                        }
                    }
                    crate::lint_context::HeadingStyle::Setext2 => {
                        if fixed_level == 1 {
                            HeadingStyle::Setext1
                        } else {
                            HeadingStyle::Setext2
                        }
                    }
                };

                let replacement = HeadingUtils::convert_heading_style(&heading.text, fixed_level as u32, style);
                // Preserve original indentation (including tabs)
                let line = line_info.content(ctx.content);
                let original_indent = &line[..line_info.indent];
                fixed_lines.push(format!("{original_indent}{replacement}"));

                prev_level = Some(fixed_level);
            } else {
                fixed_lines.push(line_info.content(ctx.content).to_string());
            }
        }

        let mut result = fixed_lines.join("\n");
        if ctx.content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Fast path: check if document likely has headings
        if ctx.content.is_empty() || !ctx.likely_has_headings() {
            return true;
        }
        // Verify valid headings actually exist
        !ctx.has_valid_headings()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        // Get MD001 config section
        let (front_matter_title, front_matter_title_pattern) = if let Some(rule_config) = config.rules.get("MD001") {
            let fmt = rule_config
                .values
                .get("front-matter-title")
                .or_else(|| rule_config.values.get("front_matter_title"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let pattern = rule_config
                .values
                .get("front-matter-title-pattern")
                .or_else(|| rule_config.values.get("front_matter_title_pattern"))
                .and_then(|v| v.as_str())
                .filter(|s: &&str| !s.is_empty())
                .map(String::from);

            (fmt, pattern)
        } else {
            (true, None)
        };

        Box::new(MD001HeadingIncrement::with_pattern(
            front_matter_title,
            front_matter_title_pattern,
        ))
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        Some((
            "MD001".to_string(),
            toml::toml! {
                front-matter-title = true
            }
            .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_basic_functionality() {
        let rule = MD001HeadingIncrement::default();

        // Test with valid headings
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with invalid headings
        let content = "# Heading 1\n### Heading 3\n#### Heading 4";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_frontmatter_title_counts_as_h1() {
        let rule = MD001HeadingIncrement::default();

        // Frontmatter with title, followed by H2 - should pass
        let content = "---\ntitle: My Document\n---\n\n## First Section";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "H2 after frontmatter title should not trigger warning"
        );

        // Frontmatter with title, followed by H3 - should warn (skips H2)
        let content = "---\ntitle: My Document\n---\n\n### Third Level";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "H3 after frontmatter title should warn");
        assert!(result[0].message.contains("Expected heading level 2"));
    }

    #[test]
    fn test_frontmatter_without_title() {
        let rule = MD001HeadingIncrement::default();

        // Frontmatter without title, followed by H2 - first heading has no predecessor
        // so it should pass (no increment check for the first heading)
        let content = "---\nauthor: John\n---\n\n## First Section";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "First heading after frontmatter without title has no predecessor"
        );
    }

    #[test]
    fn test_frontmatter_title_disabled() {
        let rule = MD001HeadingIncrement::new(false);

        // Frontmatter with title, but feature disabled - H2 has no predecessor
        let content = "---\ntitle: My Document\n---\n\n## First Section";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "With front_matter_title disabled, first heading has no predecessor"
        );
    }

    #[test]
    fn test_frontmatter_title_with_subsequent_headings() {
        let rule = MD001HeadingIncrement::default();

        // Complete document with frontmatter title
        let content = "---\ntitle: My Document\n---\n\n## Introduction\n\n### Details\n\n## Conclusion";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Valid heading progression after frontmatter title");
    }

    #[test]
    fn test_frontmatter_title_fix() {
        let rule = MD001HeadingIncrement::default();

        // Frontmatter with title, H3 should be fixed to H2
        let content = "---\ntitle: My Document\n---\n\n### Third Level";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("## Third Level"),
            "H3 should be fixed to H2 when frontmatter has title"
        );
    }

    #[test]
    fn test_toml_frontmatter_title() {
        let rule = MD001HeadingIncrement::default();

        // TOML frontmatter with title
        let content = "+++\ntitle = \"My Document\"\n+++\n\n## First Section";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "TOML frontmatter title should count as H1");
    }

    #[test]
    fn test_no_frontmatter_no_h1() {
        let rule = MD001HeadingIncrement::default();

        // No frontmatter, starts with H2 - first heading has no predecessor, so no warning
        let content = "## First Section\n\n### Subsection";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "First heading (even if H2) has no predecessor to compare against"
        );
    }
}
