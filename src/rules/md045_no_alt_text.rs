use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, Severity};

pub mod md045_config;
use md045_config::MD045Config;

/// Rule MD045: Images should have alt text
///
/// See [docs/md045.md](../../docs/md045.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when an image is missing alternate text (alt text).
/// This rule is diagnostic-only — it does not offer auto-fix because meaningful
/// alt text requires human judgment. Automated placeholders are harmful for
/// accessibility (screen readers would read fabricated text to users).
#[derive(Clone)]
pub struct MD045NoAltText {
    config: MD045Config,
}

impl Default for MD045NoAltText {
    fn default() -> Self {
        Self::new()
    }
}

impl MD045NoAltText {
    pub fn new() -> Self {
        Self {
            config: MD045Config::default(),
        }
    }

    pub fn from_config_struct(config: MD045Config) -> Self {
        Self { config }
    }
}

impl Rule for MD045NoAltText {
    fn name(&self) -> &'static str {
        "MD045"
    }

    fn description(&self) -> &'static str {
        "Images should have alternate text (alt text)"
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no image syntax present
        !ctx.likely_has_links_or_images()
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        for image in &ctx.images {
            if image.alt_text.trim().is_empty() {
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: image.line,
                    column: image.start_col + 1,
                    end_line: image.line,
                    end_column: image.end_col + 1,
                    message: "Image missing alt text (add description for accessibility: ![description](url))"
                        .to_string(),
                    severity: Severity::Error,
                    fix: None,
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        Ok(ctx.content.to_string())
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
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
        let rule_config = crate::rule_config_serde::load_rule_config::<MD045Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_image_with_alt_text() {
        let rule = MD045NoAltText::new();
        let content = "![A beautiful sunset](sunset.jpg)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_image_without_alt_text() {
        let rule = MD045NoAltText::new();
        let content = "![](sunset.jpg)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(result[0].message.contains("Image missing alt text"));
    }

    #[test]
    fn test_no_fix_offered() {
        let rule = MD045NoAltText::new();
        let content = "![](sunset.jpg)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(
            result[0].fix.is_none(),
            "MD045 should not offer auto-fix (alt text requires human judgment)"
        );
    }

    #[test]
    fn test_image_with_only_whitespace_alt_text() {
        let rule = MD045NoAltText::new();
        let content = "![   ](sunset.jpg)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(result[0].fix.is_none());
    }

    #[test]
    fn test_multiple_images() {
        let rule = MD045NoAltText::new();
        let content = "![Good alt text](image1.jpg)\n![](image2.jpg)\n![Another good one](image3.jpg)\n![](image4.jpg)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 4);
    }

    #[test]
    fn test_reference_style_image() {
        let rule = MD045NoAltText::new();
        let content = "![][sunset]\n\n[sunset]: sunset.jpg";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
    }

    #[test]
    fn test_reference_style_with_alt_text() {
        let rule = MD045NoAltText::new();
        let content = "![Beautiful sunset][sunset]\n\n[sunset]: sunset.jpg";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_image_in_code_block() {
        let rule = MD045NoAltText::new();
        let content = "```\n![](image.jpg)\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_image_in_inline_code() {
        let rule = MD045NoAltText::new();
        let content = "Use `![](image.jpg)` syntax";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_complex_urls() {
        let rule = MD045NoAltText::new();
        let content = "![](https://example.com/path/to/image.jpg?query=value#fragment)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_image_with_title() {
        let rule = MD045NoAltText::new();
        let content = "![](image.jpg \"Title text\")";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Image missing alt text"));
    }

    #[test]
    fn test_column_positions() {
        let rule = MD045NoAltText::new();
        let content = "Text before ![](image.jpg) text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 13);
    }

    #[test]
    fn test_multiline_content() {
        let rule = MD045NoAltText::new();
        let content = "Line 1\nLine 2 with ![](image.jpg)\nLine 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }
}
