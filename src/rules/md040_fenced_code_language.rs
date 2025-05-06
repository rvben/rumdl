use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{
    CodeBlockType, DocumentStructure, DocumentStructureExtensions,
};
use crate::utils::range_utils::LineIndex;

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
        "Fenced code blocks should have a language specified"
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
                    let _indent = line.len() - line.trim_start().len();
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: i + 1,
                        column: 1,
                        message: "Fenced code blocks should have a language specified".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: if line.starts_with("```") {
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

        let line_index = LineIndex::new(content.to_string());
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
                        let trimmed = fence_line.trim();
                        let _fence = if trimmed.starts_with("```") {
                            "```"
                        } else {
                            "~~~"
                        };

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: block.start_line,
                            column: 1,
                            message: "Fenced code blocks should have a language specified"
                                .to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range(block.start_line, 1),
                                replacement: if fence_line.starts_with("```") {
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

        let lines: Vec<&str> = content.lines().collect();
        for line in lines.iter() {
            let trimmed = line.trim();

            if let Some(ref current_fence) = fence_char {
                if trimmed.starts_with(current_fence) {
                    // This is a closing fence - use no indentation
                    result.push_str(&format!("{}\n", current_fence));
                    in_code_block = false;
                    fence_char = None;
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

                    // Add 'text' as default language for opening fence if no language specified
                    let after_fence = trimmed[fence.len()..].trim();
                    if after_fence.is_empty() {
                        // Use no indentation for the opening fence with language
                        result.push_str(&format!("{}text\n", fence));
                    } else {
                        // Keep original indentation for fences that already have a language
                        result.push_str(line);
                        result.push('\n');
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
