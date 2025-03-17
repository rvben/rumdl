use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Pattern for ATX headings
    static ref ATX_HEADING: Regex = Regex::new(r"^(#+)\s+(.+)$").unwrap();
    // Pattern for setext heading underlines
    static ref SETEXT_UNDERLINE: Regex = Regex::new(r"^([=-]+)$").unwrap();
}

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
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            // Check for ATX heading
            if let Some(cap) = ATX_HEADING.captures(line) {
                if let Some(heading_text) = cap.get(2) {
                    result.push(heading_text.as_str().trim().to_string());
                }
            }
            // Check for setext heading (requires looking at next line)
            else if i + 1 < lines.len() && !line.trim().is_empty() {
                let next_line = lines[i + 1];
                if SETEXT_UNDERLINE.is_match(next_line) {
                    result.push(line.trim().to_string());
                    i += 1; // Skip the underline
                }
            }

            i += 1;
        }

        result
    }

    fn is_heading(&self, content: &str, line_index: usize) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        let line = lines[line_index];

        // Check for ATX heading
        if ATX_HEADING.is_match(line) {
            return true;
        }

        // Check for setext heading (requires looking at next line)
        if line_index + 1 < lines.len() && !line.trim().is_empty() {
            let next_line = lines[line_index + 1];
            if SETEXT_UNDERLINE.is_match(next_line) {
                return true;
            }
        }

        false
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
            let lines: Vec<&str> = content.lines().collect();
            for (i, _) in lines.iter().enumerate() {
                if self.is_heading(content, i) {
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: 1,
                        message: "Heading structure does not match the required structure"
                            .to_string(),
                        severity: Severity::Warning,
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

        // Add required headings
        for (idx, heading) in self.headings.iter().enumerate() {
            if idx > 0 {
                result.push_str("\n\n");
            }
            result.push_str(&format!("# {}", heading));
        }

        Ok(result)
    }
}
