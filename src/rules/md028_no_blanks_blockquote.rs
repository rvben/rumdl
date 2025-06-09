/// Rule MD028: No blank lines inside blockquotes
///
/// See [docs/md028.md](../../docs/md028.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::{calculate_line_range, LineIndex};

#[derive(Clone)]
pub struct MD028NoBlanksBlockquote;

impl MD028NoBlanksBlockquote {
    /// Generates the replacement for a blank blockquote line
    fn get_replacement(indent: &str, level: usize) -> String {
        let mut result = indent.to_string();

        // For nested blockquotes: ">>" or ">" based on level
        for _ in 0..level {
            result.push('>');
        }
        // Add a single space after the last '>'
        result.push(' ');

        result
    }
}

impl Default for MD028NoBlanksBlockquote {
    fn default() -> Self {
        Self
    }
}

impl Rule for MD028NoBlanksBlockquote {
    fn name(&self) -> &'static str {
        "MD028"
    }

    fn description(&self) -> &'static str {
        "Blank line inside blockquote"
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Early return for content without blockquotes
        if !ctx.content.contains('>') {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(ctx.content.to_string());
        let mut warnings = Vec::new();

        // Process all lines using cached blockquote information
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            let line_num = line_idx + 1;
            
            // Skip lines in code blocks
            if line_info.in_code_block {
                continue;
            }

            // Check if this is a blockquote that needs MD028 fix
            if let Some(blockquote) = &line_info.blockquote {
                if blockquote.needs_md028_fix {
                    // Calculate precise character range for the entire empty blockquote line
                    let (start_line, start_col, end_line, end_col) =
                        calculate_line_range(line_num, &line_info.content);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: "Empty blockquote line should contain '>' marker".to_string(),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range_with_length(line_num, 1, line_info.content.len()),
                            replacement: Self::get_replacement(&blockquote.indent, blockquote.nesting_level),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        _structure: &DocumentStructure,
    ) -> LintResult {
        // Just delegate to the main check method since it now uses cached data
        self.check(ctx)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut result = Vec::with_capacity(ctx.lines.len());
        
        for line_info in &ctx.lines {
            if let Some(blockquote) = &line_info.blockquote {
                if blockquote.needs_md028_fix {
                    let replacement = Self::get_replacement(&blockquote.indent, blockquote.nesting_level);
                    result.push(replacement);
                } else {
                    result.push(line_info.content.clone());
                }
            } else {
                result.push(line_info.content.clone());
            }
        }
        
        Ok(result.join("\n")
            + if ctx.content.ends_with('\n') {
                "\n"
            } else {
                ""
            })
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Blockquote
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        !ctx.content.contains('>')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD028NoBlanksBlockquote)
    }
}

impl DocumentStructureExtensions for MD028NoBlanksBlockquote {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        !doc_structure.blockquotes.is_empty()
    }
}
