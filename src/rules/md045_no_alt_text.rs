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

    /// Generate a more context-aware placeholder text based on the image URL
    fn generate_placeholder_text(&self, url_part: &str) -> String {
        // If a custom placeholder is configured (not the default), always use it
        if self.config.placeholder_text != "TODO: Add image description" {
            return self.config.placeholder_text.clone();
        }

        // Extract the URL from the url_part (could be "(url)" or "[ref]")
        let url = if url_part.starts_with('(') && url_part.ends_with(')') {
            &url_part[1..url_part.len() - 1]
        } else {
            // For reference-style images, we can't determine the URL, use default
            return self.config.placeholder_text.clone();
        };

        // Try to extract a meaningful filename from the URL
        if let Some(filename) = url.split('/').next_back() {
            // Remove the extension and common separators to create a readable description
            if let Some(name_without_ext) = filename.split('.').next() {
                // Replace common separators with spaces and capitalize words
                let readable_name = name_without_ext
                    .replace(['-', '_'], " ")
                    .split_whitespace()
                    .map(|word| {
                        let mut chars = word.chars();
                        match chars.next() {
                            None => String::new(),
                            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                if !readable_name.is_empty() {
                    return format!("{readable_name} image");
                }
            }
        }

        // Fall back to the configured placeholder text
        self.config.placeholder_text.clone()
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

                let placeholder = if image.is_reference {
                    self.config.placeholder_text.clone()
                } else {
                    self.generate_placeholder_text(&format!("({})", &image.url))
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
                        replacement: format!("![{placeholder}]{url_part}"),
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
                // Generate a more helpful placeholder based on the image URL
                let placeholder = self.generate_placeholder_text(url_part);
                result.push_str(&format!("![{placeholder}]{url_part}"));
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
    fn test_placeholder_text_generation() {
        let rule = MD045NoAltText::new();

        // Test with a simple filename
        assert_eq!(rule.generate_placeholder_text("(sunset.jpg)"), "Sunset image");

        // Test with hyphens in filename
        assert_eq!(
            rule.generate_placeholder_text("(my-beautiful-sunset.png)"),
            "My Beautiful Sunset image"
        );

        // Test with underscores in filename
        assert_eq!(
            rule.generate_placeholder_text("(team_photo_2024.jpg)"),
            "Team Photo 2024 image"
        );

        // Test with URL path
        assert_eq!(
            rule.generate_placeholder_text("(https://example.com/images/profile-picture.png)"),
            "Profile Picture image"
        );

        // Test with reference-style (should use default)
        assert_eq!(
            rule.generate_placeholder_text("[sunset]"),
            "TODO: Add image description"
        );

        // Test with empty filename
        assert_eq!(rule.generate_placeholder_text("(.jpg)"), "TODO: Add image description");
    }

    #[test]
    fn test_fix_with_smart_placeholders() {
        let rule = MD045NoAltText::new();
        let content = "![](team-photo.jpg)\n![](product_screenshot.png)\n![Good alt](logo.png)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(
            fixed,
            "![Team Photo image](team-photo.jpg)\n![Product Screenshot image](product_screenshot.png)\n![Good alt](logo.png)"
        );
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

        assert_eq!(fixed, "![Sunset image](sunset.jpg)");
    }

    #[test]
    fn test_fix_whitespace_alt_text() {
        let rule = MD045NoAltText::new();
        let content = "![   ](sunset.jpg)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "![Sunset image](sunset.jpg)");
    }

    #[test]
    fn test_fix_multiple_images() {
        let rule = MD045NoAltText::new();
        let content = "![Good](img1.jpg) ![](img2.jpg) ![](img3.jpg)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(
            fixed,
            "![Good](img1.jpg) ![Img2 image](img2.jpg) ![Img3 image](img3.jpg)"
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

        assert_eq!(fixed, "```\n![](image.jpg)\n```\n![Real Image image](real-image.jpg)");
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

        assert_eq!(fixed, "![Image image](image.jpg \"Title text\")");
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

        // When custom placeholder is set, smart placeholders are not used
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
            // When custom placeholder is set, smart placeholders are not used
            "![Good](img1.jpg) ![MISSING ALT](img2.jpg) ![MISSING ALT](img3.jpg)"
        );
    }
}
