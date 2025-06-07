use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{
    DocumentStructure, DocumentStructureExtensions,
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
        let mut current_fence_marker: Option<String> = None;
        let mut opening_fence_indent: usize = 0;

        // Pre-compute disabled state to avoid O(n²) complexity
        let mut is_disabled = false;

        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            // Update disabled state incrementally
            if let Some(rules) = crate::rule::parse_disable_comment(trimmed) {
                if rules.is_empty() || rules.contains(&self.name()) {
                    is_disabled = true;
                }
            }
            if let Some(rules) = crate::rule::parse_enable_comment(trimmed) {
                if rules.is_empty() || rules.contains(&self.name()) {
                    is_disabled = false;
                }
            }

            // Skip processing if rule is disabled
            if is_disabled {
                continue;
            }

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
                        let current_indent = line.len() - line.trim_start().len();
                        // Only close if the fence marker exactly matches the opening marker AND has no content after
                        // AND the indentation is not greater than the opening fence
                        if fence_marker == *current_marker &&
                           trimmed[current_marker.len()..].trim().is_empty() &&
                           current_indent <= opening_fence_indent {
                            // This closes the current code block
                            in_code_block = false;
                            current_fence_marker = None;
                            opening_fence_indent = 0;
                        }
                        // else: This is content inside a code block, ignore completely
                    }
                } else {
                    // We're outside a code block, this opens one
                    // Check if language is specified
                    let after_fence = trimmed[fence_marker.len()..].trim();
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
                                    let fence_len = fence_marker.len();
                                    let line_start_byte = ctx.line_offsets.get(i).copied().unwrap_or(0);
                                    let fence_start_byte = line_start_byte + trimmed_start;
                                    let fence_end_byte = fence_start_byte + fence_len;
                                    fence_start_byte..fence_end_byte
                                },
                                replacement: format!("{}text", fence_marker),
                            }),
                        });
                    }

                    in_code_block = true;
                    current_fence_marker = Some(fence_marker);
                    opening_fence_indent = line.len() - line.trim_start().len();
                }
            }
            // If we're inside a code block and this line is not a fence, ignore it
        }

        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> LintResult {
        // For now, just delegate to the regular check method to ensure consistent behavior
        // The document structure optimization can be re-added later once the logic is stable
        self.check(ctx)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut result = String::new();
        let mut in_code_block = false;
        let mut current_fence_marker: Option<String> = None;
        let mut fence_needs_language = false;
        let mut original_indent = String::new();
        let mut opening_fence_indent: usize = 0;

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

        // Pre-compute disabled state to avoid O(n²) complexity
        let mut is_disabled = false;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Update disabled state incrementally
            if let Some(rules) = crate::rule::parse_disable_comment(trimmed) {
                if rules.is_empty() || rules.contains(&self.name()) {
                    is_disabled = true;
                }
            }
            if let Some(rules) = crate::rule::parse_enable_comment(trimmed) {
                if rules.is_empty() || rules.contains(&self.name()) {
                    is_disabled = false;
                }
            }

            // Skip processing if rule is disabled, preserve the line as-is
            if is_disabled {
                result.push_str(line);
                result.push('\n');
                continue;
            }

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
                        let current_indent = line.len() - line.trim_start().len();
                        if fence_marker == *current_marker &&
                           trimmed[current_marker.len()..].trim().is_empty() &&
                           current_indent <= opening_fence_indent {
                            // This closes the current code block
                            if fence_needs_language {
                                // Use the same indentation as the opening fence
                                result.push_str(&format!("{}{}\n", original_indent, trimmed));
                            } else {
                                // Preserve original line as-is
                                result.push_str(line);
                                result.push('\n');
                            }
                            in_code_block = false;
                            current_fence_marker = None;
                            fence_needs_language = false;
                            original_indent.clear();
                            opening_fence_indent = 0;
                        } else {
                            // This is content inside a code block (different fence marker) - preserve exactly as-is
                            result.push_str(line);
                            result.push('\n');
                        }
                    } else {
                        // This shouldn't happen, but preserve as content
                        result.push_str(line);
                        result.push('\n');
                    }
                } else {
                    // We're outside a code block, this opens one
                    // Capture the original indentation
                    let line_indent = line[..line.len() - line.trim_start().len()].to_string();

                    // Add 'text' as default language for opening fence if no language specified
                    let after_fence = trimmed[fence_marker.len()..].trim();
                    if after_fence.is_empty() {
                        // Decide whether to preserve indentation based on context
                        let should_preserve_indent = is_in_nested_context(i);

                        if should_preserve_indent {
                            // Preserve indentation for nested contexts
                            original_indent = line_indent;
                            result.push_str(&format!("{}{}text\n", original_indent, fence_marker));
                        } else {
                            // Remove indentation for standalone code blocks
                            original_indent = String::new();
                            result.push_str(&format!("{}text\n", fence_marker));
                        }
                        fence_needs_language = true;
                    } else {
                        // Keep original line as-is since it already has a language
                        result.push_str(line);
                        result.push('\n');
                        fence_needs_language = false;
                    }

                    in_code_block = true;
                    current_fence_marker = Some(fence_marker);
                    opening_fence_indent = line.len() - line.trim_start().len();
                }
            } else if in_code_block {
                // We're inside a code block and this is not a fence line - preserve exactly as-is
                result.push_str(line);
                result.push('\n');
            } else {
                // We're outside code blocks and this is not a fence line - preserve as-is
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
