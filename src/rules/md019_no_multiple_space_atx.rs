use crate::utils::range_utils::LineIndex;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::markdown_elements::{ElementType, MarkdownElements};
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
}
