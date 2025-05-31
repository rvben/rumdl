/// Rule MD031: Blank lines around fenced code blocks
///
/// See [docs/md031.md](../../docs/md031.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::{calculate_line_range, LineIndex};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref CODE_FENCE: Regex = Regex::new(r"^(```|~~~)").unwrap();
}

/// Rule MD031: Fenced code blocks should be surrounded by blank lines
#[derive(Clone)]
pub struct MD031BlanksAroundFences;

impl MD031BlanksAroundFences {
    fn is_code_fence_line(line: &str) -> bool {
        line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~")
    }

    fn is_empty_line(line: &str) -> bool {
        line.trim().is_empty()
    }
}

impl Default for MD031BlanksAroundFences {
    fn default() -> Self {
        Self
    }
}

impl Rule for MD031BlanksAroundFences {
    fn name(&self) -> &'static str {
        "MD031"
    }

    fn description(&self) -> &'static str {
        "Fenced code blocks should be surrounded by blank lines"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        let mut i = 0;

        while i < lines.len() {
            if Self::is_code_fence_line(lines[i]) {
                // Check for blank line before fence
                if i > 0 && !Self::is_empty_line(lines[i - 1]) {
                    // Calculate precise character range for the entire fence line that needs a blank line before it
                    let (start_line, start_col, end_line, end_col) =
                        calculate_line_range(i + 1, lines[i]);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
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
                        // Calculate precise character range for the entire fence line that needs a blank line after it
                        let (start_line_fence, start_col_fence, end_line_fence, end_col_fence) =
                            calculate_line_range(i + 1, lines[i]);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line_fence,
                            column: start_col_fence,
                            end_line: end_line_fence,
                            end_column: end_col_fence,
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

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        // Check if original content ended with newline
        let had_trailing_newline = content.ends_with('\n');

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

        let fixed = result.join("\n");

        // Preserve original trailing newline if it existed
        let final_result = if had_trailing_newline && !fixed.ends_with('\n') {
            format!("{}\n", fixed)
        } else {
            fixed
        };

        Ok(final_result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::CodeBlock
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty() || (!content.contains("```") && !content.contains("~~~"))
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;
        // Early return if no code blocks
        if !self.has_relevant_elements(ctx, structure) {
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
                // Calculate precise character range for the entire fence line that needs a blank line before it
                let (start_line, start_col, end_line, end_col) =
                    calculate_line_range(line_num, lines[line_num - 1]);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
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
                // Calculate precise character range for the entire fence line that needs a blank line after it
                let (start_line_fence, start_col_fence, end_line_fence, end_col_fence) =
                    calculate_line_range(line_num, lines[line_num - 1]);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: start_line_fence,
                    column: start_col_fence,
                    end_line: end_line_fence,
                    end_column: end_col_fence,
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

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD031BlanksAroundFences)
    }
}

impl DocumentStructureExtensions for MD031BlanksAroundFences {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        !doc_structure.fenced_code_block_starts.is_empty()
            || !doc_structure.fenced_code_block_ends.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::utils::document_structure::document_structure_from_str;

    #[test]
    fn test_with_document_structure() {
        let rule = MD031BlanksAroundFences;

        // Test with properly formatted code blocks
        let content = "# Test Code Blocks\n\n```rust\nfn main() {}\n```\n\nSome text here.";
        let structure = document_structure_from_str(content);
        let ctx = LintContext::new(content);
        let warnings = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(
            warnings.is_empty(),
            "Expected no warnings for properly formatted code blocks"
        );

        // Test with missing blank line before
        let content = "# Test Code Blocks\n```rust\nfn main() {}\n```\n\nSome text here.";
        let structure = document_structure_from_str(content);
        let ctx = LintContext::new(content);
        let warnings = rule.check_with_structure(&ctx, &structure).unwrap();
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
        let ctx = LintContext::new(content);
        let warnings = rule.check_with_structure(&ctx, &structure).unwrap();
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
        let ctx = LintContext::new(content);
        let warnings = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(
            warnings.len(),
            2,
            "Expected 2 warnings for missing blank lines before and after"
        );
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let rule = MD031BlanksAroundFences;

        // Test content with trailing newline
        let content = "Some text\n```\ncode\n```\nMore text\n";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Should preserve the trailing newline
        assert!(
            fixed.ends_with('\n'),
            "Fix should preserve trailing newline"
        );
        assert_eq!(fixed, "Some text\n\n```\ncode\n```\n\nMore text\n");
    }

    #[test]
    fn test_fix_preserves_no_trailing_newline() {
        let rule = MD031BlanksAroundFences;

        // Test content without trailing newline
        let content = "Some text\n```\ncode\n```\nMore text";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Should not add trailing newline if original didn't have one
        assert!(
            !fixed.ends_with('\n'),
            "Fix should not add trailing newline if original didn't have one"
        );
        assert_eq!(fixed, "Some text\n\n```\ncode\n```\n\nMore text");
    }
}
