/// Rule MD028: No blank lines inside blockquotes
///
/// See [docs/md028.md](../../docs/md028.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::{LineIndex, calculate_line_range};

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
            if let Some(blockquote) = &line_info.blockquote
                && blockquote.needs_md028_fix
            {
                // Calculate precise character range for the entire empty blockquote line
                let (start_line, start_col, end_line, end_col) = calculate_line_range(line_num, &line_info.content);

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

        Ok(result.join("\n") + if ctx.content.ends_with('\n') { "\n" } else { "" })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_no_blockquotes() {
        let rule = MD028NoBlanksBlockquote;
        let content = "This is regular text\n\nWith blank lines\n\nBut no blockquotes";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag content without blockquotes");
    }

    #[test]
    fn test_valid_blockquote_no_blanks() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> This is a blockquote\n> With multiple lines\n> But no blank lines";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag blockquotes without blank lines");
    }

    #[test]
    fn test_blank_line_in_blockquote() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> First line\n>\n> Third line";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
        assert!(result[0].message.contains("Empty blockquote line"));
    }

    #[test]
    fn test_multiple_blank_lines() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> First\n>\n>\n> Fourth";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Should flag each blank line separately");
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_nested_blockquote_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = ">> Nested quote\n>>\n>> More nested";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_fix_single_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> First\n>\n> Third";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> First\n> \n> Third");
    }

    #[test]
    fn test_fix_nested_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = ">> Nested\n>>\n>> More";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, ">> Nested\n>> \n>> More");
    }

    #[test]
    fn test_fix_with_indentation() {
        let rule = MD028NoBlanksBlockquote;
        let content = "  > Indented quote\n  >\n  > More";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "  > Indented quote\n  > \n  > More");
    }

    #[test]
    fn test_mixed_levels() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> Level 1\n>\n>> Level 2\n>>\n> Level 1 again";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 4);
    }

    #[test]
    fn test_blockquote_with_code_block() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> Quote with code:\n> ```\n> code\n> ```\n>\n> More quote";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_category() {
        let rule = MD028NoBlanksBlockquote;
        assert_eq!(rule.category(), RuleCategory::Blockquote);
    }

    #[test]
    fn test_should_skip() {
        let rule = MD028NoBlanksBlockquote;
        let ctx1 = LintContext::new("No blockquotes here");
        assert!(rule.should_skip(&ctx1));

        let ctx2 = LintContext::new("> Has blockquote");
        assert!(!rule.should_skip(&ctx2));
    }

    #[test]
    fn test_get_replacement() {
        assert_eq!(MD028NoBlanksBlockquote::get_replacement("", 1), "> ");
        assert_eq!(MD028NoBlanksBlockquote::get_replacement("  ", 1), "  > ");
        assert_eq!(MD028NoBlanksBlockquote::get_replacement("", 2), ">> ");
        assert_eq!(MD028NoBlanksBlockquote::get_replacement("  ", 3), "  >>> ");
    }

    #[test]
    fn test_empty_content() {
        let rule = MD028NoBlanksBlockquote;
        let content = "";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_blank_after_blockquote() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> Quote\n\nNot a quote";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Blank line after blockquote is valid");
    }

    #[test]
    fn test_preserve_trailing_newline() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> Quote\n>\n> More\n";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.ends_with('\n'));

        let content_no_newline = "> Quote\n>\n> More";
        let ctx2 = LintContext::new(content_no_newline);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert!(!fixed2.ends_with('\n'));
    }

    #[test]
    fn test_document_structure_extension() {
        let rule = MD028NoBlanksBlockquote;
        let ctx = LintContext::new("> test");
        let doc_structure = DocumentStructure::new("> test");
        assert!(rule.has_relevant_elements(&ctx, &doc_structure));

        let ctx2 = LintContext::new("no blockquote");
        let doc_structure2 = DocumentStructure::new("no blockquote");
        assert!(!rule.has_relevant_elements(&ctx2, &doc_structure2));
    }

    #[test]
    fn test_deeply_nested_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = ">>> Deep nest\n>>>\n>>> More deep";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, ">>> Deep nest\n>>> \n>>> More deep");
    }

    #[test]
    fn test_complex_blockquote_structure() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> Level 1\n> > Nested properly\n>\n> Back to level 1";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
    }
}
