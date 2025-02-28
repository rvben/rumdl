use crate::rule::{LintError, LintResult, LintWarning, Rule};
use crate::rules::heading_utils::HeadingUtils;
use std::collections::HashMap;

#[derive(Debug)]
pub struct MD024MultipleHeadings {
    pub allow_different_nesting: bool,
}

impl Default for MD024MultipleHeadings {
    fn default() -> Self {
        Self {
            allow_different_nesting: false,
        }
    }
}

impl MD024MultipleHeadings {
    pub fn new(allow_different_nesting: bool) -> Self {
        Self {
            allow_different_nesting,
        }
    }
}

impl Rule for MD024MultipleHeadings {
    fn name(&self) -> &'static str {
        "MD024"
    }

    fn description(&self) -> &'static str {
        "Multiple headings with the same content should be avoided"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut seen_headings: HashMap<String, usize> = HashMap::new();

        for (line_num, line) in content.lines().enumerate() {
            if let Some(heading) = HeadingUtils::parse_heading(line, line_num + 1) {
                let key = if self.allow_different_nesting {
                    format!("{}:{}", heading.level, heading.text)
                } else {
                    heading.text.clone()
                };

                if let Some(prev_line) = seen_headings.get(&key) {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: 1,
                        message: format!(
                            "Duplicate heading '{}' found (previous occurrence at line {})",
                            heading.text, prev_line
                        ),
                        fix: None, // No automatic fix for duplicate headings
                    });
                } else {
                    seen_headings.insert(key, line_num + 1);
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // No automatic fix for duplicate headings
        Ok(content.to_string())
    }
} 