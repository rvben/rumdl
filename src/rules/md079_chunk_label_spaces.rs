//! Rule MD079: Quarto chunk labels must not contain whitespace.
//!
//! Whitespace in chunk labels silently breaks Quarto cross-references
//! (`@fig-foo`) and produces unstable HTML anchors. This rule catches:
//!
//! - Implicit-positional spaces: ` ```{r several words} ` — multiple bare
//!   words before any `key=value` are interpreted by knitr/Quarto as a
//!   single space-separated label.
//! - Quoted-value spaces: ` ```{r, label="my label"} `.
//! - Hashpipe spaces: `#| label: my label`.
//!
//! Quarto flavor only; a no-op for every other flavor. No auto-fix —
//! renaming a label is a semantic choice (hyphen vs underscore vs collapse).

use crate::config::MarkdownFlavor;
use crate::lint_context::LintContext;
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::quarto_chunks::{
    ChunkLabelSource, is_executable_chunk, parse_hashpipe_labels, parse_inline_chunk_header,
};

#[derive(Debug, Clone, Default)]
pub struct MD079ChunkLabelSpaces;

impl Rule for MD079ChunkLabelSpaces {
    fn name(&self) -> &'static str {
        "MD079"
    }

    fn description(&self) -> &'static str {
        "Quarto chunk labels must not contain whitespace"
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

            // Inline labels.
            if let Some(header) = parse_inline_chunk_header(&detail.info_string) {
                // Implicit-positional run: two or more bare words before any
                // key=value parse as one space-separated label per Quarto.
                let positional: Vec<_> = header
                    .labels
                    .iter()
                    .filter(|l| l.source == ChunkLabelSource::InlinePositional)
                    .collect();
                if positional.len() >= 2 {
                    let combined = positional
                        .iter()
                        .map(|l| l.value.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    warnings.push(make_warning(
                        self.name(),
                        ctx,
                        detail.start,
                        &detail.info_string,
                        &combined,
                    ));
                }

                // Quoted `label="..."` containing spaces.
                for label in header.labels.iter().filter(|l| l.source == ChunkLabelSource::InlineKey) {
                    if label.value.chars().any(char::is_whitespace) {
                        warnings.push(make_warning(
                            self.name(),
                            ctx,
                            detail.start,
                            &detail.info_string,
                            &label.value,
                        ));
                    }
                }
            }

            // Hashpipe `#| label: ...` containing spaces.
            let body = block_body(ctx.content, detail.start);
            for label in parse_hashpipe_labels(body) {
                if label.value.chars().any(char::is_whitespace) {
                    warnings.push(make_warning(
                        self.name(),
                        ctx,
                        detail.start,
                        &detail.info_string,
                        &label.value,
                    ));
                }
            }
        }
        Ok(warnings)
    }

    fn fix(&self, _ctx: &LintContext) -> Result<String, LintError> {
        // Renaming a label is a human decision (hyphen, underscore, or collapse).
        Err(LintError::FixFailed("MD079 has no auto-fix".to_string()))
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

fn block_body(content: &str, block_start: usize) -> &str {
    let rest = &content[block_start..];
    match rest.find('\n') {
        Some(idx) => &rest[idx + 1..],
        None => "",
    }
}

fn make_warning(
    rule_name: &str,
    ctx: &LintContext,
    block_start: usize,
    info_string: &str,
    label_value: &str,
) -> LintWarning {
    let line_idx = ctx
        .line_offsets
        .binary_search(&block_start)
        .unwrap_or_else(|i| i.saturating_sub(1));
    let line_start = ctx.line_offsets.get(line_idx).copied().unwrap_or(0);
    let line_end = ctx.line_offsets.get(line_idx + 1).copied().unwrap_or(ctx.content.len());
    let line_text = &ctx.content[line_start..line_end];

    let trimmed = info_string.trim();
    let (start_col, end_col) = match line_text.find(trimmed) {
        Some(off) => {
            let start = off + 1;
            let end = start + trimmed.chars().count();
            (start, end)
        }
        None => (1, line_text.trim_end_matches('\n').chars().count().max(1) + 1),
    };

    LintWarning {
        rule_name: Some(rule_name.to_string()),
        line: line_idx + 1,
        column: start_col,
        end_line: line_idx + 1,
        end_column: end_col,
        severity: Severity::Warning,
        message: format!("Chunk label `{label_value}` contains whitespace; use a hyphen or underscore instead"),
        fix: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    fn check_quarto(content: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);
        MD079ChunkLabelSpaces.check(&ctx).unwrap()
    }

    fn check_standard(content: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        MD079ChunkLabelSpaces.check(&ctx).unwrap()
    }

    #[test]
    fn flags_implicit_positional_spaces() {
        let warnings = check_quarto("```{r several words}\n1 + 1\n```\n");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("several words"));
    }

    #[test]
    fn flags_quoted_label_with_spaces() {
        let warnings = check_quarto("```{r, label=\"my label\"}\n1 + 1\n```\n");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("my label"));
    }

    #[test]
    fn flags_hashpipe_label_with_spaces() {
        let warnings = check_quarto("```{r}\n#| label: my label\n1 + 1\n```\n");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("my label"));
    }

    #[test]
    fn accepts_single_positional_label() {
        let warnings = check_quarto("```{r setup}\n1 + 1\n```\n");
        assert!(warnings.is_empty());
    }

    #[test]
    fn accepts_hyphenated_or_underscored_labels() {
        assert!(check_quarto("```{r my-label}\n1\n```\n").is_empty());
        assert!(check_quarto("```{r, label=my_label}\n1\n```\n").is_empty());
        assert!(check_quarto("```{r}\n#| label: my-label\n1\n```\n").is_empty());
    }

    #[test]
    fn ignores_display_blocks() {
        // Plain ` ```r several words ` is a display block, not a chunk.
        // The trailing text is an info-string class list, not a label.
        let warnings = check_quarto("```r several words\n1 + 1\n```\n");
        assert!(warnings.is_empty());
    }

    #[test]
    fn no_warnings_under_standard_flavor() {
        let warnings = check_standard("```{r several words}\n1 + 1\n```\n");
        assert!(warnings.is_empty());
    }

    #[test]
    fn does_not_flag_options_after_label() {
        // First bare word is the label, subsequent key=value args are options.
        let warnings = check_quarto("```{r setup, echo=FALSE}\n1 + 1\n```\n");
        assert!(warnings.is_empty());
    }

    #[test]
    fn no_auto_fix_offered() {
        let warnings = check_quarto("```{r several words}\n1 + 1\n```\n");
        assert!(warnings[0].fix.is_none());
    }
}
