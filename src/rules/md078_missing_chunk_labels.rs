//! Rule MD078: Executable Quarto/RMarkdown chunks should have a label.
//!
//! Labels are required for figure/table cross-references, caching, and stable
//! anchors. This rule reports executable chunks (e.g. ` ```{r} `, ` ```{python} `)
//! that have neither an inline label nor a `#| label:` hashpipe option.
//!
//! Quarto flavor only; a no-op for every other flavor.

use crate::config::MarkdownFlavor;
use crate::lint_context::LintContext;
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::quarto_chunks::{is_executable_chunk, parse_hashpipe_labels, parse_inline_chunk_header};

#[derive(Debug, Clone, Default)]
pub struct MD078MissingChunkLabels;

impl Rule for MD078MissingChunkLabels {
    fn name(&self) -> &'static str {
        "MD078"
    }

    fn description(&self) -> &'static str {
        "Executable Quarto chunks should have a label"
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        if ctx.flavor != MarkdownFlavor::Quarto {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        for detail in &ctx.code_block_details {
            if !detail.is_fenced || !is_executable_chunk(&detail.info_string) {
                continue;
            }

            // Inline label?
            let Some(header) = parse_inline_chunk_header(&detail.info_string) else {
                continue;
            };
            if !header.labels.is_empty() {
                continue;
            }

            // Hashpipe label inside the block body?
            let body = block_body(ctx.content, detail.start);
            if !parse_hashpipe_labels(body).is_empty() {
                continue;
            }

            let (line, column, end_column) = info_string_span(ctx, detail.start, &detail.info_string);
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line,
                column,
                end_line: line,
                end_column,
                severity: Severity::Warning,
                message: format!(
                    "Executable chunk `{}` has no label; add `#| label: ...` or `{{{}, label=...}}`",
                    detail.info_string.trim(),
                    header.engine,
                ),
                fix: None,
            });
        }
        Ok(warnings)
    }

    fn fix(&self, _ctx: &LintContext) -> Result<String, LintError> {
        // MD078 has no auto-fix: a label is a human-chosen identifier.
        Err(LintError::FixFailed("MD078 has no auto-fix".to_string()))
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::CodeBlock
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        ctx.flavor != MarkdownFlavor::Quarto || ctx.code_block_details.is_empty()
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

/// Slice the body of a fenced code block: everything after the opening fence
/// line. The closing fence line, if present, will be encountered by the
/// caller's scanner as a non-hashpipe line and stop further parsing.
fn block_body(content: &str, block_start: usize) -> &str {
    let rest = &content[block_start..];
    match rest.find('\n') {
        Some(idx) => &rest[idx + 1..],
        None => "",
    }
}

/// Compute the (line, start_column, end_column) span covering the chunk header
/// on its line. 1-indexed for the LSP.
fn info_string_span(ctx: &LintContext, block_start: usize, info_string: &str) -> (usize, usize, usize) {
    let line_idx = ctx
        .line_offsets
        .binary_search(&block_start)
        .unwrap_or_else(|i| i.saturating_sub(1));
    let line_start = ctx.line_offsets.get(line_idx).copied().unwrap_or(0);
    let line_end = ctx.line_offsets.get(line_idx + 1).copied().unwrap_or(ctx.content.len());
    let line_text = &ctx.content[line_start..line_end];

    let (start_col, end_col) = match line_text.find(info_string.trim()) {
        Some(off) => {
            let start = off + 1;
            let end = start + info_string.trim().chars().count();
            (start, end)
        }
        None => (1, line_text.trim_end_matches('\n').chars().count().max(1) + 1),
    };

    (line_idx + 1, start_col, end_col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    fn check_quarto(content: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);
        MD078MissingChunkLabels.check(&ctx).unwrap()
    }

    fn check_standard(content: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        MD078MissingChunkLabels.check(&ctx).unwrap()
    }

    #[test]
    fn flags_executable_chunk_without_label() {
        let warnings = check_quarto("```{r}\n1 + 1\n```\n");
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].rule_name.as_deref(), Some("MD078"));
    }

    #[test]
    fn accepts_inline_positional_label() {
        let warnings = check_quarto("```{r setup}\n1 + 1\n```\n");
        assert!(warnings.is_empty());
    }

    #[test]
    fn accepts_inline_key_label() {
        let warnings = check_quarto("```{r, label=setup}\n1 + 1\n```\n");
        assert!(warnings.is_empty());
    }

    #[test]
    fn accepts_hashpipe_label() {
        let warnings = check_quarto("```{r}\n#| label: setup\n1 + 1\n```\n");
        assert!(warnings.is_empty());
    }

    #[test]
    fn ignores_display_blocks() {
        let warnings = check_quarto("```r\n1 + 1\n```\n");
        assert!(warnings.is_empty());
    }

    #[test]
    fn no_warnings_under_standard_flavor() {
        // Even a missing label in a Quarto-looking chunk must not fire under
        // Standard, since braced info strings are non-standard CommonMark.
        let warnings = check_standard("```{r}\n1 + 1\n```\n");
        assert!(warnings.is_empty());
    }

    #[test]
    fn flags_each_unlabeled_chunk_independently() {
        let content = "```{r}\n1 + 1\n```\n\n```{python}\nprint(1)\n```\n";
        let warnings = check_quarto(content);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn hashpipe_below_code_is_not_a_label() {
        // Hashpipe options must precede any code, matching Quarto's parser.
        let content = "```{r}\n1 + 1\n#| label: too-late\n```\n";
        let warnings = check_quarto(content);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn no_auto_fix_offered() {
        let warnings = check_quarto("```{r}\n1 + 1\n```\n");
        assert!(warnings[0].fix.is_none());
    }
}
