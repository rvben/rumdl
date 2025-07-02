use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

pub mod md045_config;
use md045_config::MD045Config;

lazy_static! {
    static ref IMAGE_REGEX: Regex = Regex::new(r"!\[([^\]]*)\](\([^)]+\))").unwrap();
}

/// Rule MD045: Images should have alt text
///
/// See [docs/md045.md](../../docs/md045.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when an image is missing alternate text (alt text).
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

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Use centralized image parsing from LintContext
        for image in &ctx.images {
            if image.alt_text.trim().is_empty() {
                let url_part = if image.is_reference {
                    if let Some(ref_id) = &image.reference_id {
                        format!("[{ref_id}]")
                    } else {
                        "[]".to_string()
                    }
                } else {
                    format!("({})", image.url)
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: image.line,
                    column: image.start_col + 1, // Convert to 1-indexed
                    end_line: image.line,
                    end_column: image.end_col + 1, // Convert to 1-indexed
                    message: "Image missing alt text (add description for accessibility: ![description](url))"
                        .to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: image.byte_offset..image.byte_offset + (image.end_col - image.start_col),
                        replacement: format!("![{}]{url_part}", self.config.placeholder_text),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        let mut result = String::new();
        let mut last_end = 0;

        for caps in IMAGE_REGEX.captures_iter(content) {
            let full_match = caps.get(0).unwrap();
            let alt_text = caps.get(1).map_or("", |m| m.as_str());
            let url_part = caps.get(2).map_or("", |m| m.as_str());

            // Add text before this match
            result.push_str(&content[last_end..full_match.start()]);

            // Check if this image is inside a code block
            if ctx.is_in_code_block_or_span(full_match.start()) {
                // Keep the original image if it's in a code block
                result.push_str(&caps[0]);
            } else if alt_text.trim().is_empty() {
                // Fix the image if it's not in a code block and has empty alt text
                result.push_str(&format!("![{}]{url_part}", self.config.placeholder_text));
            } else {
                // Keep the original if alt text is not empty
                result.push_str(&caps[0]);
            }

            last_end = full_match.end();
        }

        // Add any remaining text
        result.push_str(&content[last_end..]);

        Ok(result)
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_image_without_alt_text() {
        let rule = MD045NoAltText::new();
        let content = "![](sunset.jpg)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(result[0].message.contains("Image missing alt text"));
    }

    #[test]
    fn test_image_with_only_whitespace_alt_text() {
        let rule = MD045NoAltText::new();
        let content = "![   ](sunset.jpg)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
    }

    #[test]
    fn test_multiple_images() {
        let rule = MD045NoAltText::new();
        let content = "![Good alt text](image1.jpg)\n![](image2.jpg)\n![Another good one](image3.jpg)\n![](image4.jpg)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 4);
    }

    #[test]
    fn test_reference_style_image() {
        let rule = MD045NoAltText::new();
        let content = "![][sunset]\n\n[sunset]: sunset.jpg";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
    }

    #[test]
    fn test_reference_style_with_alt_text() {
        let rule = MD045NoAltText::new();
        let content = "![Beautiful sunset][sunset]\n\n[sunset]: sunset.jpg";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_image_in_code_block() {
        let rule = MD045NoAltText::new();
        let content = "```\n![](image.jpg)\n```";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should not flag images in code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_image_in_inline_code() {
        let rule = MD045NoAltText::new();
        let content = "Use `![](image.jpg)` syntax";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should not flag images in inline code
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_empty_alt_text() {
        let rule = MD045NoAltText::new();
        let content = "![](sunset.jpg)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "![TODO: Add image description](sunset.jpg)");
    }

    #[test]
    fn test_fix_whitespace_alt_text() {
        let rule = MD045NoAltText::new();
        let content = "![   ](sunset.jpg)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "![TODO: Add image description](sunset.jpg)");
    }

    #[test]
    fn test_fix_multiple_images() {
        let rule = MD045NoAltText::new();
        let content = "![Good](img1.jpg) ![](img2.jpg) ![](img3.jpg)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(
            fixed,
            "![Good](img1.jpg) ![TODO: Add image description](img2.jpg) ![TODO: Add image description](img3.jpg)"
        );
    }

    #[test]
    fn test_fix_preserves_existing_alt_text() {
        let rule = MD045NoAltText::new();
        let content = "![This has alt text](image.jpg)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "![This has alt text](image.jpg)");
    }

    #[test]
    fn test_fix_does_not_modify_code_blocks() {
        let rule = MD045NoAltText::new();
        let content = "```\n![](image.jpg)\n```\n![](real-image.jpg)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(
            fixed,
            "```\n![](image.jpg)\n```\n![TODO: Add image description](real-image.jpg)"
        );
    }

    #[test]
    fn test_complex_urls() {
        let rule = MD045NoAltText::new();
        let content = "![](https://example.com/path/to/image.jpg?query=value#fragment)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_nested_parentheses_in_url() {
        let rule = MD045NoAltText::new();
        let content = "![](image(1).jpg)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_image_with_title() {
        let rule = MD045NoAltText::new();
        let content = "![](image.jpg \"Title text\")";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Image missing alt text"));
    }

    #[test]
    fn test_fix_preserves_title() {
        let rule = MD045NoAltText::new();
        let content = "![](image.jpg \"Title text\")";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "![TODO: Add image description](image.jpg \"Title text\")");
    }

    #[test]
    fn test_image_with_spaces_in_url() {
        let rule = MD045NoAltText::new();
        let content = "![](my image.jpg)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_column_positions() {
        let rule = MD045NoAltText::new();
        let content = "Text before ![](image.jpg) text after";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 13); // 1-indexed column
    }

    #[test]
    fn test_multiline_content() {
        let rule = MD045NoAltText::new();
        let content = "Line 1\nLine 2 with ![](image.jpg)\nLine 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_custom_placeholder_text() {
        let config = MD045Config {
            placeholder_text: "FIXME: Add alt text".to_string(),
        };
        let rule = MD045NoAltText::from_config_struct(config);
        let content = "![](image.jpg)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "![FIXME: Add alt text](image.jpg)");
    }

    #[test]
    fn test_fix_multiple_with_custom_placeholder() {
        let config = MD045Config {
            placeholder_text: "MISSING ALT".to_string(),
        };
        let rule = MD045NoAltText::from_config_struct(config);
        let content = "![Good](img1.jpg) ![](img2.jpg) ![   ](img3.jpg)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(
            fixed,
            "![Good](img1.jpg) ![MISSING ALT](img2.jpg) ![MISSING ALT](img3.jpg)"
        );
    }
}
