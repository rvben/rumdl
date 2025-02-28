use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

/// Rule MD045: Images should have alternate text
///
/// This rule is triggered when an image is missing alternate text (alt text).
pub struct MD045NoAltText;

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

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let image_regex = Regex::new(r"!\[([^\]]*)\](\([^)]+\))").unwrap();

        for (line_num, line) in content.lines().enumerate() {
            for cap in image_regex.captures_iter(line) {
                let alt_text = cap.get(1).map_or("", |m| m.as_str());
                if alt_text.trim().is_empty() {
                    let full_match = cap.get(0).unwrap();
                    let url_part = cap.get(2).unwrap();
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: full_match.start() + 1,
                        message: "Image should have alternate text".to_string(),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: full_match.start() + 1,
                            replacement: format!("![Image description]{}", url_part.as_str()),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let image_regex = Regex::new(r"!\[([^\]]*)\](\([^)]+\))").unwrap();
        
        let result = image_regex.replace_all(content, |caps: &regex::Captures| {
            let alt_text = caps.get(1).map_or("", |m| m.as_str());
            let url_part = caps.get(2).map_or("", |m| m.as_str());
            
            if alt_text.trim().is_empty() {
                format!("![Image description]{}", url_part)
            } else {
                caps[0].to_string()
            }
        });

        Ok(result.to_string())
    }
} 