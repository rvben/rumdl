use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{
    CodeBlockType, DocumentStructure, DocumentStructureExtensions,
};
use crate::utils::range_utils::{calculate_line_range, LineIndex};

/// Rule MD040: Fenced code blocks should have a language
///
/// See [docs/md040.md](../../docs/md040.md) for full documentation, configuration, and examples.

#[derive(Debug, Default, Clone)]
pub struct MD040FencedCodeLanguage;

impl Rule for MD040FencedCodeLanguage {
    fn name(&self) -> &'static str {
        "MD040"
    }

    fn description(&self) -> &'static str {
        "Code blocks should have a language specified"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let mut in_code_block = false;

        let mut fence_char = None;

        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            if let Some(ref current_fence) = fence_char {
                if trimmed.starts_with(current_fence) {
                    in_code_block = false;
                    fence_char = None;
                }
            } else if !in_code_block && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
                // Opening fence
                let fence = if trimmed.starts_with("```") {
                    "```"
                } else {
                    "~~~"
                };
                fence_char = Some(fence.to_string());

                // Check if language is specified
                let after_fence = trimmed[fence.len()..].trim();
                if after_fence.is_empty() {
                    // Calculate precise character range for the entire fence line that needs a language
                    let (start_line, start_col, end_line, end_col) =
                        calculate_line_range(i + 1, line);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Code block (```) missing language"
                            .to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: {
                                // Replace just the fence marker with fence+language
                                let trimmed_start = line.len() - line.trim_start().len();
                                let fence_len = fence.len();
                                let line_start_byte = ctx.line_offsets.get(i).copied().unwrap_or(0);
                                let fence_start_byte = line_start_byte + trimmed_start;
                                let fence_end_byte = fence_start_byte + fence_len;
                                fence_start_byte..fence_end_byte
                            },
                            replacement: if fence == "```" {
                                "```text".to_string()
                            } else {
                                "~~~text".to_string()
                            },
                        }),
                    });
                }
                in_code_block = true;
            }
        }

        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;
        // Early return if no code blocks
        if !_doc_structure.has_code_blocks {
            return Ok(vec![]);
        }

        let _line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Use the code blocks from document structure
        for block in &_doc_structure.code_blocks {
            // Only check fenced code blocks
            match block.block_type {
                CodeBlockType::Fenced => {
                    // Check if language is specified
                    if block.language.as_ref().map_or(true, |lang| lang.is_empty()) {
                        // Get the opening fence line
                        let fence_line = content.lines().nth(block.start_line - 1).unwrap_or("");

                        // Calculate precise character range for the entire fence line that needs a language
                        let (start_line, start_col, end_line, end_col) =
                            calculate_line_range(block.start_line, fence_line);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "Code block (```) missing language"
                                .to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: {
                                    // Replace just the fence marker with fence+language
                                    let trimmed_start = fence_line.len() - fence_line.trim_start().len();
                                    let fence_len = if fence_line.trim_start().starts_with("```") { 3 } else { 3 };
                                    let line_start_byte = ctx.line_offsets.get(block.start_line - 1).copied().unwrap_or(0);
                                    let fence_start_byte = line_start_byte + trimmed_start;
                                    let fence_end_byte = fence_start_byte + fence_len;
                                    fence_start_byte..fence_end_byte
                                },
                                replacement: if fence_line.trim_start().starts_with("```") {
                                    "```text".to_string()
                                } else {
                                    "~~~text".to_string()
                                },
                            }),
                        });
                    }
                }
                CodeBlockType::Indented => {
                    // Indented code blocks don't have languages, so skip them
                    continue;
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut result = String::new();
        let mut in_code_block = false;
        let mut fence_char = None;
        let mut fence_needs_language = false;
        let mut original_indent = String::new();

        let lines: Vec<&str> = content.lines().collect();

        // Helper function to check if we're in a nested context
        let is_in_nested_context = |line_idx: usize| -> bool {
            // Look for blockquote or list context above this line
            for i in (0..line_idx).rev() {
                let line = lines.get(i).unwrap_or(&"");
                let trimmed = line.trim();

                // If we hit a blank line, check if context continues
                if trimmed.is_empty() {
                    continue;
                }

                // Check for blockquote markers
                if line.trim_start().starts_with('>') {
                    return true;
                }

                // Check for list markers with sufficient indentation
                if line.len() - line.trim_start().len() >= 2 {
                    let after_indent = line.trim_start();
                    if after_indent.starts_with("- ") || after_indent.starts_with("* ") ||
                       after_indent.starts_with("+ ") ||
                       (after_indent.len() > 2 && after_indent.chars().nth(0).unwrap_or(' ').is_ascii_digit() &&
                        after_indent.chars().nth(1).unwrap_or(' ') == '.' &&
                        after_indent.chars().nth(2).unwrap_or(' ') == ' ') {
                        return true;
                    }
                }

                // If we find content that's not indented, we're not in nested context
                if line.starts_with(|c: char| !c.is_whitespace()) {
                    break;
                }
            }
            false
        };

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            if let Some(ref current_fence) = fence_char {
                if trimmed.starts_with(current_fence) {
                    // This is a closing fence
                    if fence_needs_language {
                        // Use the same indentation as the opening fence
                        result.push_str(&format!("{}{}\n", original_indent, trimmed));
                    } else {
                        // Preserve original line as-is
                        result.push_str(line);
                        result.push('\n');
                    }
                    in_code_block = false;
                    fence_char = None;
                    fence_needs_language = false;
                    original_indent.clear();
                    continue;
                }

                // This is content inside a code block - keep original indentation
                result.push_str(line);
                result.push('\n');
            } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    let fence = if trimmed.starts_with("```") {
                        "```"
                    } else {
                        "~~~"
                    };
                    fence_char = Some(fence.to_string());

                    // Capture the original indentation
                    let line_indent = line[..line.len() - line.trim_start().len()].to_string();

                    // Add 'text' as default language for opening fence if no language specified
                    let after_fence = trimmed[fence.len()..].trim();
                    if after_fence.is_empty() {
                        // Decide whether to preserve indentation based on context
                        let should_preserve_indent = is_in_nested_context(i);

                        if should_preserve_indent {
                            // Preserve indentation for nested contexts
                            original_indent = line_indent;
                            result.push_str(&format!("{}{}text\n", original_indent, fence));
                        } else {
                            // Remove indentation for standalone code blocks
                            original_indent = String::new();
                            result.push_str(&format!("{}text\n", fence));
                        }
                        fence_needs_language = true;
                    } else {
                        // Keep original line as-is since it already has a language
                        result.push_str(line);
                        result.push('\n');
                        fence_needs_language = false;
                    }
                } else {
                    result.push_str(line);
                    result.push('\n');
                }
                in_code_block = true;
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD040FencedCodeLanguage)
    }
}

impl DocumentStructureExtensions for MD040FencedCodeLanguage {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> bool {
        let content = ctx.content;
        // Rule is only relevant if content contains code fences
        content.contains("```") || content.contains("~~~")
    }
}
