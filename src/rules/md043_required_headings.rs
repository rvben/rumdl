use crate::rule::{LintError, LintResult, LintWarning, Rule};
use regex::Regex;

/// Rule MD043: Required headings
///
/// This rule is triggered when the headings in a markdown document don't match the specified structure.
pub struct MD043RequiredHeadings {
    headings: Vec<String>,
}

impl MD043RequiredHeadings {
    pub fn new(headings: Vec<String>) -> Self {
        Self { headings }
    }

    fn extract_headings(&self, content: &str) -> Vec<String> {
        let mut result = Vec::new();
        let heading_regex = Regex::new(r"^(#+)\s+(.+)$|^(.+)\n([=-]+)$").unwrap();

        for line in content.lines() {
            if let Some(cap) = heading_regex.captures(line) {
                let heading_text = if let Some(atx_text) = cap.get(2) {
                    atx_text.as_str()
                } else if let Some(setext_text) = cap.get(3) {
                    setext_text.as_str()
                } else {
                    continue;
                };
                result.push(heading_text.trim().to_string());
            }
        }
        result
    }

    fn is_heading(&self, line: &str) -> bool {
        let heading_regex = Regex::new(r"^(#+)\s+(.+)$|^(.+)\n([=-]+)$").unwrap();
        heading_regex.is_match(line)
    }
}

impl Rule for MD043RequiredHeadings {
    fn name(&self) -> &'static str {
        "MD043"
    }

    fn description(&self) -> &'static str {
        "Required heading structure"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let actual_headings = self.extract_headings(content);
        
        // If no required headings are specified, the rule is disabled
        if self.headings.is_empty() {
            return Ok(warnings);
        }

        if actual_headings != self.headings {
            for (line_num, line) in content.lines().enumerate() {
                if self.is_heading(line) {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: 1,
                        message: "Heading structure does not match the required structure".to_string(),
                        fix: None, // Cannot automatically fix as we don't know the intended structure
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // If no required headings are specified, return content as is
        if self.headings.is_empty() {
            return Ok(content.to_string());
        }

        let mut result = String::new();
        let mut heading_idx = 0;

        // Add required headings
        for heading in &self.headings {
            if heading_idx > 0 {
                result.push_str("\n\n");
            }
            result.push_str(&format!("# {}", heading));
            heading_idx += 1;
        }

        Ok(result)
    }
} 