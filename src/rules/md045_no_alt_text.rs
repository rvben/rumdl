
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref IMAGE_REGEX: Regex = Regex::new(r"!\[([^\]]*)\](\([^)]+\))").unwrap();
}

/// Rule MD045: Images should have alt text
///
/// See [docs/md045.md](../../docs/md045.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when an image is missing alternate text (alt text).
#[derive(Clone)]
pub struct MD045NoAltText;

impl Default for MD045NoAltText {
    fn default() -> Self {
        Self::new()
    }
}

impl MD045NoAltText {
    pub fn new() -> Self {
        Self
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
                        format!("[{}]", ref_id)
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
                    message: "Image missing alt text (add description for accessibility: ![description](url))".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: image.byte_offset..image.byte_offset + (image.end_col - image.start_col),
                        replacement: format!("![TODO: Add image description]{}", url_part),
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
                result.push_str(&format!("![TODO: Add image description]{}", url_part));
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

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD045NoAltText::new())
    }
}
