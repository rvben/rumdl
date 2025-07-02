use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::calculate_heading_range;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Pattern for ATX headings
    static ref ATX_HEADING: Regex = Regex::new(r"^(#+)\s+(.+)$").unwrap();
    // Pattern for setext heading underlines
    static ref SETEXT_UNDERLINE: Regex = Regex::new(r"^([=-]+)$").unwrap();
}

/// Rule MD043: Required headings present
///
/// See [docs/md043.md](../../docs/md043.md) for full documentation, configuration, and examples.
#[derive(Clone)]
pub struct MD043RequiredHeadings {
    headings: Vec<String>,
}

impl MD043RequiredHeadings {
    pub fn new(headings: Vec<String>) -> Self {
        Self { headings }
    }

    fn extract_headings(&self, ctx: &crate::lint_context::LintContext) -> Vec<String> {
        let mut result = Vec::new();

        for line_info in &ctx.lines {
            if let Some(heading) = &line_info.heading {
                result.push(heading.text.trim().to_string());
            }
        }

        result
    }

    fn is_heading(&self, line_index: usize, ctx: &crate::lint_context::LintContext) -> bool {
        if line_index < ctx.lines.len() {
            ctx.lines[line_index].heading.is_some()
        } else {
            false
        }
    }
}

impl Rule for MD043RequiredHeadings {
    fn name(&self) -> &'static str {
        "MD043"
    }

    fn description(&self) -> &'static str {
        "Required heading structure"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let actual_headings = self.extract_headings(ctx);

        // If no required headings are specified, the rule is disabled
        if self.headings.is_empty() {
            return Ok(warnings);
        }

        if actual_headings != self.headings {
            // If no headings found but we have required headings, create a warning
            if actual_headings.is_empty() && !self.headings.is_empty() {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 2,
                    message: format!("Required headings not found: {:?}", self.headings),
                    severity: Severity::Warning,
                    fix: None,
                });
                return Ok(warnings);
            }

            // Create warnings for each heading that doesn't match
            for (i, line_info) in ctx.lines.iter().enumerate() {
                if self.is_heading(i, ctx) {
                    // Calculate precise character range for the entire heading
                    let (start_line, start_col, end_line, end_col) = calculate_heading_range(i + 1, &line_info.content);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Heading structure does not match the required structure".to_string(),
                        severity: Severity::Warning,
                        fix: None, // Cannot automatically fix as we don't know the intended structure
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
                    end_line: 1,
                    end_column: 2,
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

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
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
            result.push_str(&format!("# {heading}"));
        }

        Ok(result)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        _ctx: &crate::lint_context::LintContext,
        _structure: &DocumentStructure,
    ) -> LintResult {
        // Just use the regular check method which now uses cached headings
        self.check(_ctx)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no heading requirements or content is empty
        if self.headings.is_empty() || ctx.content.is_empty() {
            return true;
        }

        // Check if any heading exists using cached information
        let has_heading = ctx.lines.iter().any(|line| line.heading.is_some());

        !has_heading
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let headings =
            crate::config::get_rule_config_value::<Vec<String>>(config, "MD043", "headings").unwrap_or_default();
        Box::new(MD043RequiredHeadings::new(headings))
    }
}

impl DocumentStructureExtensions for MD043RequiredHeadings {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        !doc_structure.heading_lines.is_empty() || !self.headings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::utils::document_structure::document_structure_from_str;

    #[test]
    fn test_extract_headings_code_blocks() {
        // Create rule with required headings
        let required = vec!["Test Document".to_string(), "Real heading 2".to_string()];
        let rule = MD043RequiredHeadings::new(required);

        // Test 1: Basic content with code block
        let content = "# Test Document\n\nThis is regular content.\n\n```markdown\n# This is a heading in a code block\n## Another heading in code block\n```\n\n## Real heading 2\n\nSome content.";
        let ctx = crate::lint_context::LintContext::new(content);
        let actual_headings = rule.extract_headings(&ctx);
        assert_eq!(
            actual_headings,
            vec!["Test Document".to_string(), "Real heading 2".to_string()],
            "Should extract correct headings and ignore code blocks"
        );

        // Test 2: Content with invalid headings
        let content = "# Test Document\n\nThis is regular content.\n\n```markdown\n# This is a heading in a code block\n## This should be ignored\n```\n\n## Not Real heading 2\n\nSome content.";
        let ctx = crate::lint_context::LintContext::new(content);
        let actual_headings = rule.extract_headings(&ctx);
        assert_eq!(
            actual_headings,
            vec!["Test Document".to_string(), "Not Real heading 2".to_string()],
            "Should extract actual headings including mismatched ones"
        );
    }

    #[test]
    fn test_with_document_structure() {
        // Test with required headings
        let required = vec!["Introduction".to_string(), "Method".to_string(), "Results".to_string()];
        let rule = MD043RequiredHeadings::new(required);

        // Test with matching headings
        let content = "# Introduction\n\nContent\n\n# Method\n\nMore content\n\n# Results\n\nFinal content";
        let structure = document_structure_from_str(content);
        let warnings = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert!(warnings.is_empty(), "Expected no warnings for matching headings");

        // Test with mismatched headings
        let content = "# Introduction\n\nContent\n\n# Results\n\nSkipped method";
        let structure = document_structure_from_str(content);
        let warnings = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert!(!warnings.is_empty(), "Expected warnings for mismatched headings");

        // Test with no headings but requirements exist
        let content = "No headings here, just plain text";
        let structure = document_structure_from_str(content);
        let warnings = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert!(!warnings.is_empty(), "Expected warnings when headings are missing");

        // Test with setext headings
        let content = "Introduction\n===========\n\nContent\n\nMethod\n------\n\nMore content\n\nResults\n=======\n\nFinal content";
        let structure = document_structure_from_str(content);
        let warnings = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert!(warnings.is_empty(), "Expected no warnings for matching setext headings");
    }

    #[test]
    fn test_should_skip_no_false_positives() {
        // Create rule with required headings
        let required = vec!["Test".to_string()];
        let rule = MD043RequiredHeadings::new(required);

        // Test 1: Content with '#' character in normal text (not a heading)
        let content = "This paragraph contains a # character but is not a heading";
        assert!(
            rule.should_skip(&LintContext::new(content)),
            "Should skip content with # in normal text"
        );

        // Test 2: Content with code block containing heading-like syntax
        let content = "Regular paragraph\n\n```markdown\n# This is not a real heading\n```\n\nMore text";
        assert!(
            rule.should_skip(&LintContext::new(content)),
            "Should skip content with heading-like syntax in code blocks"
        );

        // Test 3: Content with list items using '-' character
        let content = "Some text\n\n- List item 1\n- List item 2\n\nMore text";
        assert!(
            rule.should_skip(&LintContext::new(content)),
            "Should skip content with list items using dash"
        );

        // Test 4: Content with horizontal rule that uses '---'
        let content = "Some text\n\n---\n\nMore text below the horizontal rule";
        assert!(
            rule.should_skip(&LintContext::new(content)),
            "Should skip content with horizontal rule"
        );

        // Test 5: Content with equals sign in normal text
        let content = "This is a normal paragraph with equals sign x = y + z";
        assert!(
            rule.should_skip(&LintContext::new(content)),
            "Should skip content with equals sign in normal text"
        );

        // Test 6: Content with dash/minus in normal text
        let content = "This is a normal paragraph with minus sign x - y = z";
        assert!(
            rule.should_skip(&LintContext::new(content)),
            "Should skip content with minus sign in normal text"
        );
    }

    #[test]
    fn test_should_skip_heading_detection() {
        // Create rule with required headings
        let required = vec!["Test".to_string()];
        let rule = MD043RequiredHeadings::new(required);

        // Test 1: Content with ATX heading
        let content = "# This is a heading\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(content)),
            "Should not skip content with ATX heading"
        );

        // Test 2: Content with Setext heading (equals sign)
        let content = "This is a heading\n================\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(content)),
            "Should not skip content with Setext heading (=)"
        );

        // Test 3: Content with Setext heading (dash)
        let content = "This is a subheading\n------------------\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(content)),
            "Should not skip content with Setext heading (-)"
        );

        // Test 4: Content with ATX heading with closing hashes
        let content = "## This is a heading ##\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(content)),
            "Should not skip content with ATX heading with closing hashes"
        );
    }
}
