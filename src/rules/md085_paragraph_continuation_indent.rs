//! Rule MD085: Paragraph continuation lines should not be indented.
//!
//! CommonMark strips leading whitespace from every line of a paragraph after the
//! first, so indentation there is invisible in the rendered output and only makes
//! the source inconsistent. This rule removes it.
//!
//! Indentation is structural everywhere else, so the rule is deliberately narrow:
//! it only touches paragraphs that no container owns. Inside a list item, a
//! blockquote, a definition list, a footnote body, a table or an MkDocs container,
//! the leading whitespace carries meaning and is left alone.

use crate::lint_context::{HeadingStyle, LineInfo, LintContext};
use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

/// What the line just examined was, which decides how to read the next one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    /// No block is open: the next non-blank line starts one.
    Between,
    /// A top-level paragraph is open, so its next line is a continuation.
    Paragraph,
    /// Some other block is open. Its later lines can look like bare prose (a lazy
    /// blockquote or list continuation carries no markers of its own), so the state
    /// has to outlive them.
    Other,
}

#[derive(Debug, Clone, Default)]
pub struct MD085ParagraphContinuationIndent;

impl MD085ParagraphContinuationIndent {
    pub fn new() -> Self {
        Self
    }

    /// Whether this line can belong to a paragraph that no container owns.
    ///
    /// `is_paragraph_context` rules out the non-paragraph blocks (code, HTML, math,
    /// headings, front matter, extension blocks); the rest of the list rules out the
    /// containers, where the leading whitespace is what puts the line inside them.
    fn is_top_level_prose(line: &LineInfo) -> bool {
        line.is_paragraph_context()
            && !line.is_blank
            && !line.in_list_block
            && line.blockquote.is_none()
            && !line.in_table_block
            && !line.in_definition_list
            && !line.in_footnote_definition
            && !line.in_admonition
            && !line.in_content_tab
            && !line.in_mkdocs_html_markdown
            && !line.in_pandoc_div
            && !line.in_mkdocstrings
            && !line.in_myst_directive
            && !line.in_jsx_block
            && !line.in_jsx_expression
            && !line.in_esm_block
            && !line.in_mdx_comment
            && !line.in_obsidian_comment
    }

    /// Whether a non-paragraph line is a block all by itself, so the line after it
    /// starts fresh. A setext heading is not: its underline is still to come.
    fn is_self_contained(line: &LineInfo) -> bool {
        if let Some(heading) = &line.heading {
            return heading.style == HeadingStyle::ATX;
        }
        line.is_horizontal_rule || line.is_kramdown_block_ial || line.is_myst_comment
    }
}

impl Rule for MD085ParagraphContinuationIndent {
    fn name(&self) -> &'static str {
        "MD085"
    }

    fn description(&self) -> &'static str {
        "Paragraph continuation lines should not be indented"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::FullyFixable
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        // Nothing to strip unless some line begins with whitespace.
        !ctx.lines.iter().any(|line| line.indent > 0 && !line.is_blank)
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let mut state = Block::Between;

        for (idx, line) in ctx.lines.iter().enumerate() {
            if line.is_blank {
                state = Block::Between;
                continue;
            }

            if !Self::is_top_level_prose(line) {
                state = if Self::is_self_contained(line) {
                    Block::Between
                } else {
                    Block::Other
                };
                continue;
            }

            match state {
                // The line that opens a paragraph is its first line, whose indentation
                // decides what the block is. Only later lines are free whitespace.
                Block::Between => state = Block::Paragraph,
                // Prose reached while another block is open is a lazy continuation of
                // that block, not a paragraph of its own.
                Block::Other => {}
                Block::Paragraph => {
                    // Leading whitespace inside a multi-line code span is span content.
                    if line.indent == 0 || line.in_code_span_continuation {
                        continue;
                    }

                    let line_num = idx + 1;
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: line_num,
                        column: 1,
                        end_line: line_num,
                        end_column: 1 + line.content(ctx.content)[..line.indent].chars().count(),
                        severity: Severity::Warning,
                        message: "Paragraph continuation line should not be indented".to_string(),
                        fix: Some(Fix::new(
                            line.byte_offset..line.byte_offset + line.indent,
                            String::new(),
                        )),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings).map_err(LintError::InvalidInput)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;

    fn warned_lines(content: &str, flavor: MarkdownFlavor) -> Vec<usize> {
        let ctx = LintContext::new(content, flavor, None);
        MD085ParagraphContinuationIndent::new()
            .check(&ctx)
            .unwrap()
            .iter()
            .map(|w| w.line)
            .collect()
    }

    fn fixed(content: &str, flavor: MarkdownFlavor) -> String {
        let ctx = LintContext::new(content, flavor, None);
        let rule = MD085ParagraphContinuationIndent::new();
        let out = rule.fix(&ctx).unwrap();
        // A warning and a rewrite have to agree, in both directions.
        assert_eq!(
            rule.check(&ctx).unwrap().is_empty(),
            out == content,
            "warnings and fix disagree for {content:?}"
        );
        out
    }

    /// Content the rule must leave exactly as it found it.
    fn assert_unchanged(content: &str, flavor: MarkdownFlavor) {
        assert!(
            warned_lines(content, flavor).is_empty(),
            "unexpected warning for {content:?}"
        );
        assert_eq!(fixed(content, flavor), content);
    }

    #[test]
    fn strips_indentation_from_continuation_lines() {
        let content = "This is some paragraph\n with line breaks\n  and indentation.\n";
        assert_eq!(warned_lines(content, MarkdownFlavor::Standard), vec![2, 3]);
        assert_eq!(
            fixed(content, MarkdownFlavor::Standard),
            "This is some paragraph\nwith line breaks\nand indentation.\n"
        );
    }

    #[test]
    fn leaves_the_first_line_of_a_paragraph_alone() {
        // The first line's indentation is what decides which block this is, so it is
        // never free whitespace. Only the lines after it are.
        assert_eq!(
            fixed("  Indented start\n   continuation\n", MarkdownFlavor::Standard),
            "  Indented start\ncontinuation\n"
        );
    }

    #[test]
    fn strips_a_continuation_indented_like_code() {
        // An indented code block cannot interrupt a paragraph, so four spaces here are
        // prose, and removing them renders identically.
        assert_eq!(
            fixed("para\n    four spaces\n", MarkdownFlavor::Standard),
            "para\nfour spaces\n"
        );
        assert_eq!(fixed("para\n\ttab\n", MarkdownFlavor::Standard), "para\ntab\n");
    }

    #[test]
    fn leaves_an_indented_code_block_alone() {
        assert_unchanged("para\n\n    real code block\n", MarkdownFlavor::Standard);
        assert_unchanged("```\n  fenced\n```\n", MarkdownFlavor::Standard);
    }

    #[test]
    fn leaves_container_content_alone() {
        // Indentation is structural in every one of these: removing it reparents or
        // ends the construct.
        for content in [
            "- item\n  continuation\n",
            "1. item\n   continuation\n",
            "> quote\n>  continuation\n",
            "| a | b |\n|---|---|\n|  1 |  2 |\n",
            "[^1]: note\n  more of the note\n",
            "<div>\n  inner\n</div>\n",
            "$$\n  x = 1\n$$\n",
        ] {
            assert_unchanged(content, MarkdownFlavor::Standard);
        }
    }

    #[test]
    fn leaves_lazy_continuation_of_a_container_alone() {
        // A lazy continuation line carries no marker of its own, so it reads exactly
        // like top-level prose. What it continues is the container above it.
        assert_unchanged("> quote\n  lazy one\n  lazy two\n", MarkdownFlavor::Standard);
        assert_unchanged("- item\n lazy one space\n", MarkdownFlavor::Standard);
    }

    #[test]
    fn leaves_a_setext_underline_and_what_follows_alone() {
        // The underline carries no markers of its own, so without tracking the heading
        // it would read as a paragraph and the line after it as a continuation.
        assert_unchanged("Title\n=====\n  after underline\n", MarkdownFlavor::Standard);
        assert_unchanged("Title\n---\n  after underline\n", MarkdownFlavor::Standard);
    }

    #[test]
    fn a_paragraph_after_a_self_contained_block_is_still_checked() {
        for prefix in ["# Heading\n", "***\n"] {
            let content = format!("{prefix}para\n  cont\n");
            assert_eq!(
                fixed(&content, MarkdownFlavor::Standard),
                format!("{prefix}para\ncont\n")
            );
        }
    }

    #[test]
    fn leaves_a_multi_line_code_span_alone() {
        // The leading whitespace on the closing line sits inside the span, so it is
        // content rather than indentation.
        assert_eq!(
            fixed("para `code\n  more` tail\n  cont\n", MarkdownFlavor::Standard),
            "para `code\n  more` tail\ncont\n"
        );
    }

    #[test]
    fn leaves_mkdocs_containers_alone() {
        for content in [
            "!!! note\n    body line\n    more body\n",
            "=== \"Tab\"\n    body line\n    more body\n",
            "Term\n\n: definition\n  more of the definition\n",
        ] {
            assert_unchanged(content, MarkdownFlavor::MkDocs);
        }
    }

    #[test]
    fn preserves_hard_line_breaks() {
        // The rule only removes leading whitespace, so a two-space or backslash break
        // at the end of the previous line survives.
        assert_eq!(fixed("para  \n  next\n", MarkdownFlavor::Standard), "para  \nnext\n");
        assert_eq!(fixed("para\\\n  next\n", MarkdownFlavor::Standard), "para\\\nnext\n");
    }

    #[test]
    fn preserves_trailing_blank_lines_and_a_missing_final_newline() {
        assert_eq!(
            fixed("para\n  cont\n\n\n", MarkdownFlavor::Standard),
            "para\ncont\n\n\n"
        );
        assert_eq!(fixed("para\n  cont", MarkdownFlavor::Standard), "para\ncont");
    }

    #[test]
    fn fix_is_idempotent() {
        for content in [
            "This is some paragraph\n with line breaks\n  and indentation.\n",
            "  Indented start\n   continuation\n",
            "para\n  cont\n\n\n",
            "> quote\n  lazy\n",
        ] {
            let once = fixed(content, MarkdownFlavor::Standard);
            let ctx = LintContext::new(&once, MarkdownFlavor::Standard, None);
            assert_eq!(
                MD085ParagraphContinuationIndent::new().fix(&ctx).unwrap(),
                once,
                "fix is not idempotent for {content:?}"
            );
        }
    }

    #[test]
    fn warning_spans_the_indentation_it_removes() {
        let ctx = LintContext::new("para\n   cont\n", MarkdownFlavor::Standard, None);
        let warnings = MD085ParagraphContinuationIndent::new().check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 2);
        assert_eq!(warnings[0].column, 1);
        assert_eq!(warnings[0].end_line, 2);
        assert_eq!(warnings[0].end_column, 4);
    }

    #[test]
    fn empty_and_blank_documents_are_untouched() {
        for content in ["", "\n", "   \n", "\n\n\n"] {
            assert_unchanged(content, MarkdownFlavor::Standard);
        }
    }
}
