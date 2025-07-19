/// Rule MD031: Blank lines around fenced code blocks
///
/// See [docs/md031.md](../../docs/md031.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::{LineIndex, calculate_line_range};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref CODE_FENCE: Regex = Regex::new(r"^(```|~~~)").unwrap();
}

/// Rule MD031: Fenced code blocks should be surrounded by blank lines
#[derive(Clone)]
pub struct MD031BlanksAroundFences;

impl MD031BlanksAroundFences {
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
        let line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        let mut in_code_block = false;
        let mut current_fence_marker: Option<String> = None;
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();

            // Determine fence marker if this is a fence line
            let fence_marker = if trimmed.starts_with("```") {
                let backtick_count = trimmed.chars().take_while(|&c| c == '`').count();
                if backtick_count >= 3 {
                    Some("`".repeat(backtick_count))
                } else {
                    None
                }
            } else if trimmed.starts_with("~~~") {
                let tilde_count = trimmed.chars().take_while(|&c| c == '~').count();
                if tilde_count >= 3 {
                    Some("~".repeat(tilde_count))
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(fence_marker) = fence_marker {
                if in_code_block {
                    // We're inside a code block, check if this closes it
                    if let Some(ref current_marker) = current_fence_marker {
                        // A fence can only close a code block if:
                        // 1. It has the same type of marker (backticks or tildes)
                        // 2. It has at least as many markers as the opening fence
                        // 3. It has no content after the fence marker
                        let same_type = (current_marker.starts_with('`') && fence_marker.starts_with('`'))
                            || (current_marker.starts_with('~') && fence_marker.starts_with('~'));

                        if same_type
                            && fence_marker.len() >= current_marker.len()
                            && trimmed[fence_marker.len()..].trim().is_empty()
                        {
                            // This closes the current code block
                            in_code_block = false;
                            current_fence_marker = None;

                            // Check for blank line after closing fence
                            if i + 1 < lines.len() && !Self::is_empty_line(lines[i + 1]) {
                                let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, lines[i]);

                                warnings.push(LintWarning {
                                    rule_name: Some(self.name()),
                                    line: start_line,
                                    column: start_col,
                                    end_line,
                                    end_column: end_col,
                                    message: "No blank line after fenced code block".to_string(),
                                    severity: Severity::Warning,
                                    fix: Some(Fix {
                                        range: line_index.line_col_to_byte_range_with_length(
                                            i + 1,
                                            lines[i].len() + 1,
                                            0,
                                        ),
                                        replacement: "\n".to_string(),
                                    }),
                                });
                            }
                        }
                        // else: This is content inside a code block (shorter fence or different type), ignore
                    }
                } else {
                    // We're outside a code block, this opens one
                    in_code_block = true;
                    current_fence_marker = Some(fence_marker);

                    // Check for blank line before opening fence
                    if i > 0 && !Self::is_empty_line(lines[i - 1]) {
                        let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, lines[i]);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "No blank line before fenced code block".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range_with_length(i + 1, 1, 0),
                                replacement: "\n".to_string(),
                            }),
                        });
                    }
                }
            }
            // If we're inside a code block, ignore all content lines
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
        let mut in_code_block = false;
        let mut current_fence_marker: Option<String> = None;

        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();

            // Determine fence marker if this is a fence line
            let fence_marker = if trimmed.starts_with("```") {
                let backtick_count = trimmed.chars().take_while(|&c| c == '`').count();
                if backtick_count >= 3 {
                    Some("`".repeat(backtick_count))
                } else {
                    None
                }
            } else if trimmed.starts_with("~~~") {
                let tilde_count = trimmed.chars().take_while(|&c| c == '~').count();
                if tilde_count >= 3 {
                    Some("~".repeat(tilde_count))
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(fence_marker) = fence_marker {
                if in_code_block {
                    // We're inside a code block, check if this closes it
                    if let Some(ref current_marker) = current_fence_marker {
                        if trimmed.starts_with(current_marker) && trimmed[current_marker.len()..].trim().is_empty() {
                            // This closes the current code block
                            result.push(line.to_string());
                            in_code_block = false;
                            current_fence_marker = None;

                            // Add blank line after closing fence if needed
                            if i + 1 < lines.len() && !Self::is_empty_line(lines[i + 1]) {
                                result.push(String::new());
                            }
                        } else {
                            // This is content inside a code block (different fence marker)
                            result.push(line.to_string());
                        }
                    } else {
                        // This shouldn't happen, but preserve as content
                        result.push(line.to_string());
                    }
                } else {
                    // We're outside a code block, this opens one
                    in_code_block = true;
                    current_fence_marker = Some(fence_marker);

                    // Add blank line before fence if needed
                    if i > 0 && !Self::is_empty_line(lines[i - 1]) {
                        result.push(String::new());
                    }

                    // Add opening fence
                    result.push(line.to_string());
                }
            } else if in_code_block {
                // We're inside a code block, preserve content as-is
                result.push(line.to_string());
            } else {
                // We're outside code blocks, normal processing
                result.push(line.to_string());
            }
            i += 1;
        }

        let fixed = result.join("\n");

        // Preserve original trailing newline if it existed
        let final_result = if had_trailing_newline && !fixed.ends_with('\n') {
            format!("{fixed}\n")
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
                let (start_line, start_col, end_line, end_col) = calculate_line_range(line_num, lines[line_num - 1]);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: "No blank line before fenced code block".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range_with_length(line_num, 1, 0),
                        replacement: "\n".to_string(),
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
                        range: line_index.line_col_to_byte_range_with_length(
                            line_num,
                            lines[line_num - 1].len() + 1,
                            0,
                        ),
                        replacement: "\n".to_string(),
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
        !doc_structure.fenced_code_block_starts.is_empty() || !doc_structure.fenced_code_block_ends.is_empty()
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
        assert_eq!(warnings.len(), 1, "Expected 1 warning for missing blank line before");
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
        assert_eq!(warnings.len(), 1, "Expected 1 warning for missing blank line after");
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
    fn test_nested_code_blocks() {
        let rule = MD031BlanksAroundFences;

        // Test that nested code blocks are not flagged
        let content = r#"````markdown
```
content
```
````"#;
        let ctx = LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "Should not flag nested code blocks");

        // Test that fixes don't corrupt nested blocks
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Fix should not modify nested code blocks");
    }

    #[test]
    fn test_nested_code_blocks_complex() {
        let rule = MD031BlanksAroundFences;

        // Test documentation example with nested code blocks
        let content = r#"# Documentation

## Examples

````markdown
```python
def hello():
    print("Hello, world!")
```

```javascript
console.log("Hello, world!");
```
````

More text here."#;

        let ctx = LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            0,
            "Should not flag any issues in properly formatted nested code blocks"
        );

        // Test with 5-backtick outer block
        let content_5 = r#"`````markdown
````python
```bash
echo "nested"
```
````
`````"#;

        let ctx_5 = LintContext::new(content_5);
        let warnings_5 = rule.check(&ctx_5).unwrap();
        assert_eq!(warnings_5.len(), 0, "Should handle deeply nested code blocks");
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let rule = MD031BlanksAroundFences;

        // Test content with trailing newline
        let content = "Some text\n```\ncode\n```\nMore text\n";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Should preserve the trailing newline
        assert!(fixed.ends_with('\n'), "Fix should preserve trailing newline");
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
