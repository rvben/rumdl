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

    fn extract_headings(&self, content: &str) -> Vec<String> {
        let mut result = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;
        let mut in_code_block = false;
        let mut code_fence_char = None;

        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim();

            // Handle code block state
            if trimmed.len() >= 3 {
                let first_chars: Vec<char> = trimmed.chars().take(3).collect();
                if first_chars.iter().all(|&c| c == '`' || c == '~') {
                    if in_code_block && Some(first_chars[0]) == code_fence_char {
                        // End of code block
                        in_code_block = false;
                        code_fence_char = None;
                    } else if !in_code_block {
                        // Start of code block
                        in_code_block = true;
                        code_fence_char = Some(first_chars[0]);
                    }
                    i += 1;
                    continue;
                }
            }

            // Skip content within code blocks
            if in_code_block {
                i += 1;
                continue;
            }

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

        // If this line is in a code block, it's not a heading
        let mut in_code_block = false;
        let mut code_fence_char = None;

        for (idx, curr_line) in lines.iter().enumerate() {
            if idx > line_index {
                break;
            }

            let trimmed = curr_line.trim();

            // Handle code block state
            if trimmed.len() >= 3 {
                let first_chars: Vec<char> = trimmed.chars().take(3).collect();
                if first_chars.iter().all(|&c| c == '`' || c == '~') {
                    if in_code_block && Some(first_chars[0]) == code_fence_char {
                        // End of code block
                        in_code_block = false;
                        code_fence_char = None;
                    } else if !in_code_block {
                        // Start of code block
                        in_code_block = true;
                        code_fence_char = Some(first_chars[0]);
                    }
                }
            }
        }

        // If we're in a code block, it's not a heading
        if in_code_block {
            return false;
        }

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

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
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
            result.push_str(&format!("# {}", heading));
        }

        Ok(result)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        _ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let mut warnings = Vec::new();

        // If no required headings are specified, the rule is disabled
        if self.headings.is_empty() {
            return Ok(warnings);
        }

        // Extract actual headings using document structure
        let lines: Vec<&str> = _ctx.content.lines().collect();
        let mut actual_headings = Vec::new();

        // Detect code blocks
        let mut in_code_block = false;
        let mut code_fence_char = None;
        let mut code_block_lines = Vec::new();

        // First identify code block lines
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Handle code block state
            if trimmed.len() >= 3 {
                let first_chars: Vec<char> = trimmed.chars().take(3).collect();
                if first_chars.iter().all(|&c| c == '`' || c == '~') {
                    if in_code_block && Some(first_chars[0]) == code_fence_char {
                        // End of code block
                        in_code_block = false;
                        code_fence_char = None;
                    } else if !in_code_block {
                        // Start of code block
                        in_code_block = true;
                        code_fence_char = Some(first_chars[0]);
                    }
                }
            }

            // Track lines within code blocks
            if in_code_block {
                code_block_lines.push(idx);
            }
        }

        for (i, &line_num) in structure.heading_lines.iter().enumerate() {
            // Skip headings in front matter
            if structure.is_in_front_matter(line_num) {
                continue;
            }

            let idx = line_num - 1; // Convert to 0-indexed
            if idx >= lines.len() {
                continue;
            }

            // Skip headings in code blocks
            if code_block_lines.contains(&idx) {
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

        if actual_headings != self.headings {
            for (i, line_num) in structure.heading_lines.iter().enumerate() {
                if i < structure.heading_lines.len() && !structure.is_in_front_matter(*line_num) {
                    // Skip headings in code blocks
                    let idx = line_num - 1;
                    if code_block_lines.contains(&idx) {
                        continue;
                    }

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
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no heading requirements or content is empty
        if self.headings.is_empty() || ctx.content.is_empty() {
            return true;
        }

        // We need to properly detect headings in the content
        let lines: Vec<&str> = ctx.content.lines().collect();
        let mut has_heading = false;
        let mut in_code_block = false;
        let mut code_fence_char = None;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Handle code block state
            if trimmed.len() >= 3 {
                let first_chars: Vec<char> = trimmed.chars().take(3).collect();
                if first_chars.iter().all(|&c| c == '`' || c == '~') {
                    if in_code_block && Some(first_chars[0]) == code_fence_char {
                        // End of code block
                        in_code_block = false;
                        code_fence_char = None;
                    } else if !in_code_block {
                        // Start of code block
                        in_code_block = true;
                        code_fence_char = Some(first_chars[0]);
                    }
                    continue;
                }
            }

            // Skip content within code blocks
            if in_code_block {
                continue;
            }

            // Check for ATX headings using heading_utils
            if crate::rules::heading_utils::is_heading(line) {
                has_heading = true;
                break;
            }

            // Check for setext headings (requires next line)
            if i + 1 < lines.len() && crate::rules::heading_utils::is_setext_heading(&lines, i) {
                has_heading = true;
                break;
            }
        }

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
            crate::config::get_rule_config_value::<Vec<String>>(config, "MD043", "headings")
                .unwrap_or_default();
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
        let actual_headings = rule.extract_headings(content);
        assert_eq!(
            actual_headings,
            vec!["Test Document".to_string(), "Real heading 2".to_string()],
            "Should extract correct headings and ignore code blocks"
        );

        // Test 2: Content with invalid headings
        let content = "# Test Document\n\nThis is regular content.\n\n```markdown\n# This is a heading in a code block\n## This should be ignored\n```\n\n## Not Real heading 2\n\nSome content.";
        let actual_headings = rule.extract_headings(content);
        assert_eq!(
            actual_headings,
            vec![
                "Test Document".to_string(),
                "Not Real heading 2".to_string()
            ],
            "Should extract actual headings including mismatched ones"
        );
    }

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
        let warnings = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert!(
            warnings.is_empty(),
            "Expected no warnings for matching headings"
        );

        // Test with mismatched headings
        let content = "# Introduction\n\nContent\n\n# Results\n\nSkipped method";
        let structure = document_structure_from_str(content);
        let warnings = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert!(
            !warnings.is_empty(),
            "Expected warnings for mismatched headings"
        );

        // Test with no headings but requirements exist
        let content = "No headings here, just plain text";
        let structure = document_structure_from_str(content);
        let warnings = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert!(
            !warnings.is_empty(),
            "Expected warnings when headings are missing"
        );

        // Test with setext headings
        let content = "Introduction\n===========\n\nContent\n\nMethod\n------\n\nMore content\n\nResults\n=======\n\nFinal content";
        let structure = document_structure_from_str(content);
        let warnings = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert!(
            warnings.is_empty(),
            "Expected no warnings for matching setext headings"
        );
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
        let content =
            "Regular paragraph\n\n```markdown\n# This is not a real heading\n```\n\nMore text";
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
