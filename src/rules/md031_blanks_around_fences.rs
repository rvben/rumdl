/// Rule MD031: Blank lines around fenced code blocks
///
/// See [docs/md031.md](../../docs/md031.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref CODE_FENCE: Regex = Regex::new(r"^(```|~~~)").unwrap();
}

#[derive(Debug, Default)]
pub struct MD031BlanksAroundFences;

impl MD031BlanksAroundFences {
    fn is_code_fence_line(line: &str) -> bool {
        line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~")
    }

    fn is_empty_line(line: &str) -> bool {
        line.trim().is_empty()
    }
}

impl Rule for MD031BlanksAroundFences {
    fn name(&self) -> &'static str {
        "MD031"
    }

    fn description(&self) -> &'static str {
        "Fenced code blocks should be surrounded by blank lines"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        let mut i = 0;

        while i < lines.len() {
            if Self::is_code_fence_line(lines[i]) {
                // Check for blank line before fence
                if i > 0 && !Self::is_empty_line(lines[i - 1]) {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: i + 1,
                        column: 1,
                        message: "No blank line before fenced code block".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: format!("\n{}", lines[i]),
                        }),
                    });
                }

                // Find closing fence
                let _opening_fence = i;
                i += 1;
                while i < lines.len() && !Self::is_code_fence_line(lines[i]) {
                    i += 1;
                }

                // If we found a closing fence
                if i < lines.len() {
                    // Check for blank line after fence
                    if i + 1 < lines.len() && !Self::is_empty_line(lines[i + 1]) {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: i + 1,
                            column: 1,
                            message: "No blank line after fenced code block".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index
                                    .line_col_to_byte_range(i + 1, lines[i].len() + 1),
                                replacement: format!("{}\n", lines[i]),
                            }),
                        });
                    }
                }
            }
            i += 1;
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let lines: Vec<&str> = content.lines().collect();

        let mut result = Vec::new();

        let mut i = 0;

        while i < lines.len() {
            if Self::is_code_fence_line(lines[i]) {
                // Add blank line before fence if needed
                if i > 0 && !Self::is_empty_line(lines[i - 1]) {
                    result.push(String::new());
                }

                // Add opening fence
                result.push(lines[i].to_string());

                // Find and add content within code block
                let mut j = i + 1;
                while j < lines.len() && !Self::is_code_fence_line(lines[j]) {
                    result.push(lines[j].to_string());
                    j += 1;
                }

                // Add closing fence if found
                if j < lines.len() {
                    result.push(lines[j].to_string());

                    // Add blank line after fence if needed
                    if j + 1 < lines.len() && !Self::is_empty_line(lines[j + 1]) {
                        result.push(String::new());
                    }

                    i = j;
                } else {
                    i = j;
                }
            } else {
                result.push(lines[i].to_string());
            }
            i += 1;
        }

        Ok(result.join("\n"))
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::CodeBlock
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        // Skip if the content is empty or doesn't contain any code fence markers
        content.is_empty() || (!content.contains("```") && !content.contains("~~~"))
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return if no code blocks
        if !self.has_relevant_elements(content, structure) {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Process each code fence start and end
        for &start_line in &structure.fenced_code_block_starts {
            let line_num = start_line;

            // Check for blank line before fence
            if line_num > 1 && !Self::is_empty_line(lines[line_num - 2]) {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: line_num,
                    column: 1,
                    message: "No blank line before fenced code block".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(line_num, 1),
                        replacement: format!("\n{}", lines[line_num - 1]),
                    }),
                });
            }
        }

        for &end_line in &structure.fenced_code_block_ends {
            let line_num = end_line;

            // Check for blank line after fence
            if line_num < lines.len() && !Self::is_empty_line(lines[line_num]) {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: line_num,
                    column: 1,
                    message: "No blank line after fenced code block".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index
                            .line_col_to_byte_range(line_num, lines[line_num - 1].len() + 1),
                        replacement: format!("{}\n", lines[line_num - 1]),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl DocumentStructureExtensions for MD031BlanksAroundFences {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        !doc_structure.fenced_code_block_starts.is_empty()
            || !doc_structure.fenced_code_block_ends.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::document_structure::document_structure_from_str;

    #[test]
    fn test_with_document_structure() {
        let rule = MD031BlanksAroundFences;

        // Test with properly formatted code blocks
        let content = "# Test Code Blocks\n\n```rust\nfn main() {}\n```\n\nSome text here.";
        let structure = document_structure_from_str(content);
        let warnings = rule.check_with_structure(content, &structure).unwrap();
        assert!(
            warnings.is_empty(),
            "Expected no warnings for properly formatted code blocks"
        );

        // Test with missing blank line before
        let content = "# Test Code Blocks\n```rust\nfn main() {}\n```\n\nSome text here.";
        let structure = document_structure_from_str(content);
        let warnings = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(
            warnings.len(),
            1,
            "Expected 1 warning for missing blank line before"
        );
        assert_eq!(warnings[0].line, 2, "Warning should be on line 2");
        assert!(
            warnings[0].message.contains("before"),
            "Warning should be about blank line before"
        );

        // Test with missing blank line after
        let content = "# Test Code Blocks\n\n```rust\nfn main() {}\n```\nSome text here.";
        let structure = document_structure_from_str(content);
        let warnings = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(
            warnings.len(),
            1,
            "Expected 1 warning for missing blank line after"
        );
        assert_eq!(warnings[0].line, 5, "Warning should be on line 5");
        assert!(
            warnings[0].message.contains("after"),
            "Warning should be about blank line after"
        );

        // Test with missing blank lines both before and after
        let content = "# Test Code Blocks\n```rust\nfn main() {}\n```\nSome text here.";
        let structure = document_structure_from_str(content);
        let warnings = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(
            warnings.len(),
            2,
            "Expected 2 warnings for missing blank lines before and after"
        );
    }
}
