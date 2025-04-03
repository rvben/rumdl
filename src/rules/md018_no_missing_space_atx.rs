use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::markdown_elements::{ElementType, MarkdownElements};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref ATX_NO_SPACE_PATTERN: Regex = Regex::new(r"^(#+)([^#\s])").unwrap();
}

#[derive(Debug, Default)]
pub struct MD018NoMissingSpaceAtx;

impl MD018NoMissingSpaceAtx {
    pub fn new() -> Self {
        Self
    }

    fn is_atx_heading_without_space(&self, line: &str) -> bool {
        ATX_NO_SPACE_PATTERN.is_match(line)
    }

    fn fix_atx_heading(&self, line: &str) -> String {
        let captures = ATX_NO_SPACE_PATTERN.captures(line).unwrap();

        let hashes = captures.get(1).unwrap();

        let content = &line[hashes.end()..];
        format!("{} {}", hashes.as_str(), content)
    }

    // Calculate the byte range for a specific line in the content
    fn get_line_byte_range(&self, content: &str, line_num: usize) -> std::ops::Range<usize> {
        let mut current_line = 1;
        let mut start_byte = 0;

        for (i, c) in content.char_indices() {
            if current_line == line_num && c == '\n' {
                return start_byte..i;
            } else if c == '\n' {
                current_line += 1;
                if current_line == line_num {
                    start_byte = i + 1;
                }
            }
        }

        // If we're looking for the last line and it doesn't end with a newline
        if current_line == line_num {
            return start_byte..content.len();
        }

        // Fallback if line not found (shouldn't happen)
        0..0
    }
}

impl Rule for MD018NoMissingSpaceAtx {
    fn name(&self) -> &'static str {
        "MD018"
    }

    fn description(&self) -> &'static str {
        "No space after hash on ATX style heading"
    }

    fn check(&self, content: &str) -> LintResult {
        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Use MarkdownElements to detect all headings
        let headings = MarkdownElements::detect_headings(content);

        // Process each line to check for ATX headings without proper spacing
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            // Skip lines in code blocks
            if line_index.is_code_block(line_num + 1) {
                continue;
            }

            // Check if this is an ATX heading without space
            if self.is_atx_heading_without_space(line) {
                // Make sure this is a heading, not just a line starting with #
                let is_heading = headings
                    .iter()
                    .any(|h| h.element_type == ElementType::Heading && h.start_line == line_num);

                if is_heading {
                    let hashes = ATX_NO_SPACE_PATTERN.captures(line).unwrap().get(1).unwrap();
                    let line_range = self.get_line_byte_range(content, line_num + 1);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!(
                            "No space after {} in ATX style heading",
                            "#".repeat(hashes.as_str().len())
                        ),
                        line: line_num + 1,
                        column: hashes.end() + 1,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_range,
                            replacement: self.fix_atx_heading(line),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Early return for empty content
        if content.is_empty() {
            return Ok(String::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut result = String::new();

        // Use MarkdownElements to detect all headings
        let headings = MarkdownElements::detect_headings(content);

        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            // Skip lines in code blocks
            if line_index.is_code_block(i + 1) {
                result.push_str(line);
            } else if self.is_atx_heading_without_space(line) {
                // Make sure this is a heading, not just a line starting with #
                let is_heading = headings
                    .iter()
                    .any(|h| h.element_type == ElementType::Heading && h.start_line == i);

                if is_heading {
                    result.push_str(&self.fix_atx_heading(line));
                } else {
                    result.push_str(line);
                }
            } else {
                result.push_str(line);
            }

            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        // Preserve trailing newline if original had it
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return if no headings
        if structure.heading_lines.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Process only heading lines using structure.heading_lines
        for &line_num in &structure.heading_lines {
            let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed

            // Skip if out of bounds
            if line_idx >= lines.len() {
                continue;
            }

            let line = lines[line_idx];

            // Check if this is an ATX heading without space
            if self.is_atx_heading_without_space(line) {
                let hashes = ATX_NO_SPACE_PATTERN.captures(line).unwrap().get(1).unwrap();
                let line_range = self.get_line_byte_range(content, line_num);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!(
                        "No space after {} in ATX style heading",
                        "#".repeat(hashes.as_str().len())
                    ),
                    line: line_num,
                    column: hashes.end() + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_range,
                        replacement: self.fix_atx_heading(line),
                    }),
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
        content.is_empty() || !content.contains('#')
    }
}

impl DocumentStructureExtensions for MD018NoMissingSpaceAtx {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // This rule is only relevant if there are headings
        !doc_structure.heading_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_document_structure() {
        let rule = MD018NoMissingSpaceAtx;

        // Test with correct space
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty());

        // Test with missing space
        let content = "#Heading 1\n## Heading 2\n###Heading 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should flag the two headings with missing spaces
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }
}
