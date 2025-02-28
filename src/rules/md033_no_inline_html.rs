use crate::rule::{LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug)]
pub struct MD033NoInlineHtml {
    allowed_elements: Vec<String>,
}

impl Default for MD033NoInlineHtml {
    fn default() -> Self {
        Self {
            allowed_elements: Vec::new(),
        }
    }
}

impl MD033NoInlineHtml {
    pub fn new(allowed_elements: Vec<String>) -> Self {
        Self { allowed_elements }
    }

    fn find_html_tag(&self, line: &str) -> Option<String> {
        let re = Regex::new(r"<([a-zA-Z][a-zA-Z0-9]*)(?:\s+[^>]*)?(/?)>").unwrap();
        for cap in re.captures_iter(line) {
            let tag_name = cap[1].to_string();
            return Some(tag_name);
        }
        None
    }
}

impl Rule for MD033NoInlineHtml {
    fn name(&self) -> &'static str {
        "MD033"
    }

    fn description(&self) -> &'static str {
        "Inline HTML"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let in_code_block = line.trim().starts_with("```");
            let in_html_block = false;

            // Skip code blocks and HTML blocks
            if in_code_block {
                continue;
            }

            if !in_code_block && !in_html_block {
                // Look for HTML tags
                if let Some(tag) = self.find_html_tag(line) {
                    if !self.allowed_elements.contains(&tag.to_lowercase()) {
                        warnings.push(LintWarning {
                            line: line_num + 1,
                            column: line.find('<').unwrap_or(0) + 1,
                            message: format!("HTML tag '{}' is not allowed", tag),
                            fix: None,
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Fixing HTML requires manual intervention
        Ok(content.to_string())
    }
} 