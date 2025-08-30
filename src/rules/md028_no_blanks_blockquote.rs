/// Rule MD028: No blank lines inside blockquotes
///
/// See [docs/md028.md](../../docs/md028.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::{LineIndex, calculate_line_range};

#[derive(Clone)]
pub struct MD028NoBlanksBlockquote;

impl MD028NoBlanksBlockquote {
    /// Check if a line is a blockquote line (has > markers)
    fn is_blockquote_line(line: &str) -> bool {
        line.trim_start().starts_with('>')
    }

    /// Get the blockquote level (number of > markers)
    fn get_blockquote_level(line: &str) -> usize {
        let trimmed = line.trim_start();
        let mut level = 0;
        let chars = trimmed.chars();

        for ch in chars {
            if ch == '>' {
                level += 1;
            } else if ch != ' ' && ch != '\t' {
                break;
            }
        }

        level
    }

    /// Get the leading whitespace before the first >
    fn get_leading_whitespace(line: &str) -> &str {
        let trimmed_len = line.trim_start().len();
        let total_len = line.len();
        &line[..total_len - trimmed_len]
    }

    /// Check if a blank line is inside a blockquote context
    fn is_blank_line_in_blockquote(lines: &[&str], index: usize) -> Option<(usize, String)> {
        let current_line = lines[index];

        // Must be a truly blank line (no blockquote markers)
        if !current_line.trim().is_empty() || Self::is_blockquote_line(current_line) {
            return None;
        }

        // Look backward for a blockquote line
        let mut prev_blockquote_level = None;
        let mut prev_indent = "";

        for i in (0..index).rev() {
            let line = lines[i];
            if Self::is_blockquote_line(line) {
                prev_blockquote_level = Some(Self::get_blockquote_level(line));
                prev_indent = Self::get_leading_whitespace(line);
                break;
            }
            // If we hit a non-blank, non-blockquote line, stop
            if !line.trim().is_empty() {
                return None;
            }
        }

        // No previous blockquote found
        let level = prev_blockquote_level?;

        // Look forward for a blockquote line
        for line in lines.iter().skip(index + 1) {
            if Self::is_blockquote_line(line) {
                let next_level = Self::get_blockquote_level(line);
                // The blank line is inside a blockquote if the next blockquote has the same or higher level
                if next_level >= level {
                    // Generate the fix: add appropriate blockquote markers
                    let mut fix = prev_indent.to_string();
                    for _ in 0..level {
                        fix.push('>');
                    }
                    return Some((level, fix));
                }
                return None;
            }
            // If we hit a non-blank, non-blockquote line, stop
            if !line.trim().is_empty() {
                return None;
            }
        }

        None
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

        // Get all lines
        let lines: Vec<&str> = ctx.content.lines().collect();

        // Check each line
        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1;

            // Skip lines in code blocks
            if line_idx < ctx.lines.len() && ctx.lines[line_idx].in_code_block {
                continue;
            }

            // Check if this is a blank line inside a blockquote
            if let Some((level, fix_content)) = Self::is_blank_line_in_blockquote(&lines, line_idx) {
                let (start_line, start_col, end_line, end_col) = calculate_line_range(line_num, line);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!("Blank line inside blockquote (level {level})"),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range_with_length(line_num, 1, line.len()),
                        replacement: fix_content,
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
        // Just delegate to the main check method
        self.check(ctx)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut result = Vec::with_capacity(ctx.lines.len());
        let lines: Vec<&str> = ctx.content.lines().collect();

        for (line_idx, line) in lines.iter().enumerate() {
            // Check if this blank line needs fixing
            if let Some((_, fix_content)) = Self::is_blank_line_in_blockquote(&lines, line_idx) {
                result.push(fix_content);
            } else {
                result.push(line.to_string());
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag content without blockquotes");
    }

    #[test]
    fn test_valid_blockquote_no_blanks() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> This is a blockquote\n> With multiple lines\n> But no blank lines";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag blockquotes without blank lines");
    }

    #[test]
    fn test_blockquote_with_empty_line_marker() {
        let rule = MD028NoBlanksBlockquote;
        // Lines with just > are valid and should NOT be flagged
        let content = "> First line\n>\n> Third line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lines with just > marker");
    }

    #[test]
    fn test_blockquote_with_empty_line_marker_and_space() {
        let rule = MD028NoBlanksBlockquote;
        // Lines with > and space are also valid
        let content = "> First line\n> \n> Third line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lines with > and space");
    }

    #[test]
    fn test_blank_line_in_blockquote() {
        let rule = MD028NoBlanksBlockquote;
        // Truly blank line (no >) inside blockquote should be flagged
        let content = "> First line\n\n> Third line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag truly blank line inside blockquote");
        assert_eq!(result[0].line, 2);
        assert!(result[0].message.contains("Blank line inside blockquote"));
    }

    #[test]
    fn test_multiple_blank_lines() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> First\n\n\n> Fourth";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Should flag each blank line separately");
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_nested_blockquote_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = ">> Nested quote\n\n>> More nested";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_nested_blockquote_with_marker() {
        let rule = MD028NoBlanksBlockquote;
        // Lines with >> are valid
        let content = ">> Nested quote\n>>\n>> More nested";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lines with >> marker");
    }

    #[test]
    fn test_fix_single_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> First\n\n> Third";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> First\n>\n> Third");
    }

    #[test]
    fn test_fix_nested_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = ">> Nested\n\n>> More";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, ">> Nested\n>>\n>> More");
    }

    #[test]
    fn test_fix_with_indentation() {
        let rule = MD028NoBlanksBlockquote;
        let content = "  > Indented quote\n\n  > More";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "  > Indented quote\n  >\n  > More");
    }

    #[test]
    fn test_mixed_levels() {
        let rule = MD028NoBlanksBlockquote;
        // Blank lines between different levels
        let content = "> Level 1\n\n>> Level 2\n\n> Level 1 again";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Line 2 is a blank between > and >>, should be flagged as level 1
        // Line 4 is a blank between >> and >, NOT inside blockquote (different context)
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_blockquote_with_code_block() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> Quote with code:\n> ```\n> code\n> ```\n>\n> More quote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Line 5 has > marker, so it's not a blank line
        assert!(result.is_empty(), "Should not flag line with > marker");
    }

    #[test]
    fn test_category() {
        let rule = MD028NoBlanksBlockquote;
        assert_eq!(rule.category(), RuleCategory::Blockquote);
    }

    #[test]
    fn test_should_skip() {
        let rule = MD028NoBlanksBlockquote;
        let ctx1 = LintContext::new("No blockquotes here", crate::config::MarkdownFlavor::Standard);
        assert!(rule.should_skip(&ctx1));

        let ctx2 = LintContext::new("> Has blockquote", crate::config::MarkdownFlavor::Standard);
        assert!(!rule.should_skip(&ctx2));
    }

    #[test]
    fn test_empty_content() {
        let rule = MD028NoBlanksBlockquote;
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_blank_after_blockquote() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> Quote\n\nNot a quote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Blank line after blockquote ends is valid");
    }

    #[test]
    fn test_blank_before_blockquote() {
        let rule = MD028NoBlanksBlockquote;
        let content = "Not a quote\n\n> Quote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Blank line before blockquote starts is valid");
    }

    #[test]
    fn test_preserve_trailing_newline() {
        let rule = MD028NoBlanksBlockquote;
        let content = "> Quote\n\n> More\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.ends_with('\n'));

        let content_no_newline = "> Quote\n\n> More";
        let ctx2 = LintContext::new(content_no_newline, crate::config::MarkdownFlavor::Standard);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert!(!fixed2.ends_with('\n'));
    }

    #[test]
    fn test_document_structure_extension() {
        let rule = MD028NoBlanksBlockquote;
        let ctx = LintContext::new("> test", crate::config::MarkdownFlavor::Standard);
        let doc_structure = DocumentStructure::new("> test");
        assert!(rule.has_relevant_elements(&ctx, &doc_structure));

        let ctx2 = LintContext::new("no blockquote", crate::config::MarkdownFlavor::Standard);
        let doc_structure2 = DocumentStructure::new("no blockquote");
        assert!(!rule.has_relevant_elements(&ctx2, &doc_structure2));
    }

    #[test]
    fn test_deeply_nested_blank() {
        let rule = MD028NoBlanksBlockquote;
        let content = ">>> Deep nest\n\n>>> More deep";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, ">>> Deep nest\n>>>\n>>> More deep");
    }

    #[test]
    fn test_deeply_nested_with_marker() {
        let rule = MD028NoBlanksBlockquote;
        // Lines with >>> are valid
        let content = ">>> Deep nest\n>>>\n>>> More deep";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lines with >>> marker");
    }

    #[test]
    fn test_complex_blockquote_structure() {
        let rule = MD028NoBlanksBlockquote;
        // Line with > is valid, not a blank line
        let content = "> Level 1\n> > Nested properly\n>\n> Back to level 1";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag line with > marker");
    }

    #[test]
    fn test_complex_with_blank() {
        let rule = MD028NoBlanksBlockquote;
        // Blank line between different nesting levels is not flagged
        // (going from >> back to > is a context change)
        let content = "> Level 1\n> > Nested\n\n> Back to level 1";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            0,
            "Blank between different nesting levels is not inside blockquote"
        );
    }
}
