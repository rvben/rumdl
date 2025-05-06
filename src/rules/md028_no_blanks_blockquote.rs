/// Rule MD028: No blank lines inside blockquotes
///
/// See [docs/md028.md](../../docs/md028.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::blockquote_utils::BlockquoteUtils;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;

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

impl Rule for MD028NoBlanksBlockquote {
    fn name(&self) -> &'static str {
        "MD028"
    }

    fn description(&self) -> &'static str {
        "Blank line inside blockquote"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let line_index = LineIndex::new(ctx.content.to_string());
        let mut warnings = Vec::new();
        let lines: Vec<&str> = ctx.content.lines().collect();
        for (i, &line) in lines.iter().enumerate() {
            if BlockquoteUtils::is_blockquote(line) {
                let level = BlockquoteUtils::get_nesting_level(line);
                let indent = BlockquoteUtils::extract_indentation(line);
                // Canonical blank blockquote line: marker(s) + single space, no content
                let expected = Self::get_replacement(&indent, level);
                if BlockquoteUtils::is_empty_blockquote(line) && line != expected {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: "Blank line inside blockquote".to_string(),
                        line: i + 1,
                        column: 1,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: expected,
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
        structure: &DocumentStructure,
    ) -> LintResult {
        if structure.blockquotes.is_empty() {
            return Ok(Vec::new());
        }
        let line_index = LineIndex::new(ctx.content.to_string());
        let mut warnings = Vec::new();
        let lines: Vec<&str> = ctx.content.lines().collect();
        for blockquote in &structure.blockquotes {
            for line_num in blockquote.start_line..=blockquote.end_line {
                if line_num == 0 || line_num > lines.len() {
                    continue;
                }
                let line_idx = line_num - 1;
                let line = lines[line_idx];
                if BlockquoteUtils::is_blockquote(line)
                    && BlockquoteUtils::is_empty_blockquote(line)
                {
                    let level = BlockquoteUtils::get_nesting_level(line);
                    let indent = BlockquoteUtils::extract_indentation(line);
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: "Blank line inside blockquote".to_string(),
                        line: line_num,
                        column: 1,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(line_num, 1),
                            replacement: Self::get_replacement(&indent, level),
                        }),
                    });
                }
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let lines: Vec<&str> = ctx.content.lines().collect();
        let mut result = Vec::with_capacity(lines.len());
        for line in lines.iter() {
            if BlockquoteUtils::is_blockquote(line) && BlockquoteUtils::is_empty_blockquote(line) {
                let level = BlockquoteUtils::get_nesting_level(line);
                let indent = BlockquoteUtils::extract_indentation(line);
                let replacement = Self::get_replacement(&indent, level);
                result.push(replacement);
            } else {
                result.push(line.to_string());
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
