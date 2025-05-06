use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use regex::Regex;

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
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let image_regex = Regex::new(r"!\[([^\]]*)\](\([^)]+\))").unwrap();

        for (line_num, line) in content.lines().enumerate() {
            for cap in image_regex.captures_iter(line) {
                let alt_text = cap.get(1).map_or("", |m| m.as_str());
                if alt_text.trim().is_empty() {
                    let full_match = cap.get(0).unwrap();
                    let _url_part = cap.get(2).unwrap();
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: line_num + 1,
                        column: full_match.start() + 1,
                        message: "Image should have alternate text".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index
                                .line_col_to_byte_range(line_num + 1, full_match.start() + 1),
                            replacement: format!(
                                "![Image description]{}",
                                &line[full_match.start() + 2..full_match.end()]
                            ),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let image_regex = Regex::new(r"!\[([^\]]*)\](\([^)]+\))").unwrap();

        let result = image_regex.replace_all(content, |caps: &regex::Captures| {
            let alt_text = caps.get(1).map_or("", |m| m.as_str());
            let _url_part = caps.get(2).map_or("", |m| m.as_str());

            if alt_text.trim().is_empty() {
                format!("![Image description]{}", _url_part)
            } else {
                caps[0].to_string()
            }
        });

        Ok(result.to_string())
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
