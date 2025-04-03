use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
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
                        rule_name: Some(self.name()),
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

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        let mut warnings = Vec::new();

        // If no required headings are specified, the rule is disabled
        if self.headings.is_empty() {
            return Ok(warnings);
        }

        // Extract actual headings using document structure
        let lines: Vec<&str> = content.lines().collect();
        let mut actual_headings = Vec::new();

        for (i, &line_num) in structure.heading_lines.iter().enumerate() {
            // Skip headings in front matter
            if structure.is_in_front_matter(line_num) {
                continue;
            }

            let idx = line_num - 1; // Convert to 0-indexed
            if idx >= lines.len() {
                continue;
            }

            let line = lines[idx];

            // Extract heading text based on heading style
            let heading_text = if line.trim_start().starts_with('#') {
                // ATX heading - extract text after '#' marks
                if let Some(cap) = ATX_HEADING.captures(line) {
                    if let Some(heading_text) = cap.get(2) {
                        heading_text.as_str().trim().to_string()
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            } else if i + 1 < structure.heading_lines.len()
                && structure.heading_lines[i + 1] == line_num + 1
                && idx + 1 < lines.len()
                && SETEXT_UNDERLINE.is_match(lines[idx + 1])
            {
                // Setext heading
                line.trim().to_string()
            } else {
                line.trim().to_string()
            };

            actual_headings.push(heading_text);
        }

        // If no headings found but we have required headings, create a warning
        if actual_headings.is_empty() && !self.headings.is_empty() {
            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: 1,
                column: 1,
                message: format!("Required headings not found: {:?}", self.headings),
                severity: Severity::Warning,
                fix: None,
            });
            return Ok(warnings);
        }

        // Compare with required headings
        if actual_headings != self.headings {
            for (i, line_num) in structure.heading_lines.iter().enumerate() {
                if i < structure.heading_lines.len() && !structure.is_in_front_matter(*line_num) {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: *line_num,
                        column: 1,
                        message: format!(
                            "Heading structure does not match required structure. Expected: {:?}, Found: {:?}",
                            self.headings, actual_headings
                        ),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }
            }

            // If we have no warnings but headings don't match (could happen if we have no headings),
            // add a warning at the beginning of the file
            if warnings.is_empty() {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: 1,
                    column: 1,
                    message: format!(
                        "Heading structure does not match required structure. Expected: {:?}, Found: {:?}",
                        self.headings, actual_headings
                    ),
                    severity: Severity::Warning,
                    fix: None,
                });
            }
        }

        Ok(warnings)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        // Skip if no heading requirements or content is empty
        self.headings.is_empty()
            || content.trim().is_empty()
        // Quick check for heading markers
        (!content.contains('#') && !content.contains('=') && !content.contains('-'))
    }
}

impl DocumentStructureExtensions for MD043RequiredHeadings {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        !doc_structure.heading_lines.is_empty() || !self.headings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::document_structure::document_structure_from_str;

    #[test]
    fn test_with_document_structure() {
        // Test with required headings
        let required = vec![
            "Introduction".to_string(),
            "Method".to_string(),
            "Results".to_string(),
        ];
        let rule = MD043RequiredHeadings::new(required);

        // Test with matching headings
        let content =
            "# Introduction\n\nContent\n\n# Method\n\nMore content\n\n# Results\n\nFinal content";
        let structure = document_structure_from_str(content);
        let warnings = rule.check_with_structure(content, &structure).unwrap();
        assert!(
            warnings.is_empty(),
            "Expected no warnings for matching headings"
        );

        // Test with mismatched headings
        let content = "# Introduction\n\nContent\n\n# Results\n\nSkipped method";
        let structure = document_structure_from_str(content);
        let warnings = rule.check_with_structure(content, &structure).unwrap();
        assert!(
            !warnings.is_empty(),
            "Expected warnings for mismatched headings"
        );

        // Test with no headings but requirements exist
        let content = "No headings here, just plain text";
        let structure = document_structure_from_str(content);
        let warnings = rule.check_with_structure(content, &structure).unwrap();
        assert!(
            !warnings.is_empty(),
            "Expected warnings when headings are missing"
        );

        // Test with setext headings
        let content = "Introduction\n===========\n\nContent\n\nMethod\n------\n\nMore content\n\nResults\n=======\n\nFinal content";
        let structure = document_structure_from_str(content);
        let warnings = rule.check_with_structure(content, &structure).unwrap();
        assert!(
            warnings.is_empty(),
            "Expected no warnings for matching setext headings"
        );
    }
}
