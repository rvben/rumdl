use crate::utils::range_utils::LineIndex;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity, RuleCategory};
use crate::utils::markdown_elements::{ElementType, MarkdownElements};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref ATX_MULTIPLE_SPACE_PATTERN: Regex = Regex::new(r"^(#+)\s{2,}").unwrap();
}

#[derive(Debug, Default)]
pub struct MD019NoMultipleSpaceAtx;

impl MD019NoMultipleSpaceAtx {
    pub fn new() -> Self {
        Self
    }

    fn is_atx_heading_with_multiple_spaces(&self, line: &str) -> bool {
        ATX_MULTIPLE_SPACE_PATTERN.is_match(line)
    }

    fn fix_atx_heading(&self, line: &str) -> String {
        let captures = ATX_MULTIPLE_SPACE_PATTERN.captures(line).unwrap();

        let hashes = captures.get(1).unwrap();

        let content = line[hashes.end()..].trim_start();
        format!("{} {}", hashes.as_str(), content)
    }

    fn count_spaces_after_hashes(&self, line: &str) -> usize {
        let captures = ATX_MULTIPLE_SPACE_PATTERN.captures(line).unwrap();

        let hashes = captures.get(1).unwrap();
        line[hashes.end()..]
            .chars()
            .take_while(|c| c.is_whitespace())
            .count()
    }
}

impl Rule for MD019NoMultipleSpaceAtx {
    fn name(&self) -> &'static str {
        "MD019"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces after hash on ATX style heading"
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
        
        // Process each line to check for ATX headings with multiple spaces
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            // Skip lines in code blocks
            if line_index.is_code_block(line_num) {
                continue;
            }

            // Check if this is an ATX heading with multiple spaces
            if self.is_atx_heading_with_multiple_spaces(line) {
                // Make sure this is a heading, not just a line starting with #
                let is_heading = headings.iter().any(|h| 
                    h.element_type == ElementType::Heading && 
                    h.start_line == line_num
                );
                
                if is_heading {
                    let hashes = ATX_MULTIPLE_SPACE_PATTERN
                        .captures(line)
                        .unwrap()
                        .get(1)
                        .unwrap();
                    let spaces = self.count_spaces_after_hashes(line);
                    warnings.push(LintWarning {
            rule_name: Some(self.name()),
                        message: format!(
                            "Multiple spaces ({}) after {} in ATX style heading",
                            spaces,
                            "#".repeat(hashes.as_str().len())
                        ),
                        line: line_num + 1,
                        column: hashes.end() + 1,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(line_num + 1, 1),
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
            if line_index.is_code_block(i) {
                result.push_str(line);
            } else if self.is_atx_heading_with_multiple_spaces(line) {
                // Make sure this is a heading, not just a line starting with #
                let is_heading = headings.iter().any(|h| 
                    h.element_type == ElementType::Heading && 
                    h.start_line == i
                );
                
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
        
        let line_index = LineIndex::new(content.to_string());
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
            
            // Check if this is an ATX heading with multiple spaces
            if self.is_atx_heading_with_multiple_spaces(line) {
                let hashes = ATX_MULTIPLE_SPACE_PATTERN
                    .captures(line)
                    .unwrap()
                    .get(1)
                    .unwrap();
                let spaces = self.count_spaces_after_hashes(line);
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!(
                        "Multiple spaces ({}) after {} in ATX style heading",
                        spaces,
                        "#".repeat(hashes.as_str().len())
                    ),
                    line: line_num,
                    column: hashes.end() + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(line_num, 1),
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

impl DocumentStructureExtensions for MD019NoMultipleSpaceAtx {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // This rule is only relevant if there are headings
        !doc_structure.heading_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::document_structure::document_structure_from_str;

    #[test]
    fn test_with_document_structure() {
        let rule = MD019NoMultipleSpaceAtx::new();
        
        // Test with heading that has multiple spaces
        let content = "#  Multiple Spaces\n\nRegular content\n\n##   More Spaces";
        let structure = document_structure_from_str(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should flag both headings
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 5);
        
        // Test with proper headings
        let content = "# Single Space\n\n## Also correct";
        let structure = document_structure_from_str(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty(), "Properly formatted headings should not generate warnings");
    }
}
