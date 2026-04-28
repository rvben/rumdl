//!
//! Rule MD077: List continuation content indentation
//!
//! See [docs/md077.md](../../docs/md077.md) for full documentation, configuration, and examples.

use std::ops::ControlFlow;

use crate::lint_context::{LineInfo, LintContext};
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

/// Rule MD077: List continuation content indentation
///
/// Checks two cases:
/// - **Loose continuation** (after a blank line): content must be indented to the
///   item's content column (W+N rule), or it falls out of the list.
/// - **Tight continuation** (no blank line): content must not be over-indented
///   beyond the item's content column.
///
/// Under the MkDocs flavor, a minimum of 4 spaces is enforced for ordered list
/// items to satisfy Python-Markdown.
#[derive(Clone, Default)]
pub struct MD077ListContinuationIndent;

impl MD077ListContinuationIndent {
    /// Width of a GFM task checkbox prefix including its trailing space:
    /// `[ ] `, `[x] `, or `[X] ` — always exactly 4 bytes.
    const TASK_CHECKBOX_PREFIX_LEN: usize = 4;

    /// Returns true if the item line starts a GFM task list item, i.e. its
    /// content column begins with `[ ] `, `[x] `, or `[X] `. The trailing
    /// space is part of the match — `- [ ]` with no body is an empty list
    /// item, not a task.
    ///
    /// Task items have a second, conventionally-accepted continuation column
    /// at `content_col + 4` (aligned after the checkbox). MD013 reflow
    /// produces this column for wrapped task lines, so MD077 has to accept
    /// it to avoid a fix loop with MD013.
    ///
    /// `content_col` is a byte offset into `line`, not a visual column. The
    /// CommonMark list parser produces byte-offset content columns, and the
    /// checkbox prefix `[ ] ` is pure ASCII, so this byte-level comparison
    /// is correct. Leading indent mixing tabs and spaces is irrelevant here
    /// because `content_col` already points past any leading whitespace.
    fn is_task_list_item(line: &str, content_col: usize) -> bool {
        line.as_bytes()
            .get(content_col..content_col + Self::TASK_CHECKBOX_PREFIX_LEN)
            .is_some_and(|window| matches!(window, b"[ ] " | b"[x] " | b"[X] "))
    }

    /// Check if a trimmed line is a block-level construct (not list continuation).
    fn is_block_level_construct(trimmed: &str) -> bool {
        // Footnote definition: [^label]:
        if trimmed.starts_with("[^") && trimmed.contains("]:") {
            return true;
        }
        // Abbreviation definition: *[text]:
        if trimmed.starts_with("*[") && trimmed.contains("]:") {
            return true;
        }
        // Reference link definition: [label]: url
        // Must start with [ but not be a regular link, footnote, or abbreviation
        if trimmed.starts_with('[') && !trimmed.starts_with("[^") && trimmed.contains("]: ") {
            return true;
        }
        false
    }

    /// Check if a trimmed line is a fenced code block delimiter (opener or closer).
    fn is_code_fence(trimmed: &str) -> bool {
        let bytes = trimmed.as_bytes();
        if bytes.len() < 3 {
            return false;
        }
        let ch = bytes[0];
        (ch == b'`' || ch == b'~') && bytes[1] == ch && bytes[2] == ch
    }

    /// Check if a trimmed line starts with a list marker (*, -, +, or ordered).
    /// Used to avoid flagging deeply indented list items that the parser doesn't
    /// recognize as list items (e.g., with indent=8 configured in MD007).
    fn starts_with_list_marker(trimmed: &str) -> bool {
        let bytes = trimmed.as_bytes();
        match bytes.first() {
            Some(b'*' | b'-' | b'+') => bytes.get(1).is_some_and(|&b| b == b' ' || b == b'\t'),
            Some(b'0'..=b'9') => {
                let rest = trimmed.trim_start_matches(|c: char| c.is_ascii_digit());
                rest.starts_with(". ") || rest.starts_with(") ")
            }
            _ => false,
        }
    }

    /// Given the line number of a fenced code block opener, walk forward and
    /// return the line number of the matching closer. Returns the opener itself
    /// if no following line is in the code block (degenerate single-line block).
    fn find_fence_closer(ctx: &LintContext, opener_line: usize) -> usize {
        let mut closer_line = opener_line;
        for peek in (opener_line + 1)..=ctx.lines.len() {
            let Some(peek_info) = ctx.line_info(peek) else { break };
            if peek_info.in_code_block {
                closer_line = peek;
            } else {
                break;
            }
        }
        closer_line
    }

    /// Build an atomic fix that reindents a fenced code block from its opener
    /// through its matching closer.
    ///
    /// - **Opener and closer** are moved to `required` (the list item's
    ///   content column, which is what MD077 actually flagged).
    /// - **Interior lines** are *promoted* to `required` only if they sit
    ///   below it; interior content at or above `required` is left at its
    ///   original column. This preserves authored interior indentation when
    ///   possible while guaranteeing fence pairing: every non-blank line in
    ///   the block ends at column ≥ `required`, so the block stays inside
    ///   the list item's scope after the fix.
    ///
    /// Why a compound fix rather than three independent fixes? MD077 and
    /// MD031 run in the same iterative fix loop. If we only moved the
    /// delimiters, an intermediate state would have mismatched
    /// opener/closer indentation and MD031 would misread the block as
    /// unpaired, injecting stray blank lines (issue #574).
    ///
    /// Why `max(interior, required)` instead of `interior + delta`? The
    /// delta-shift version was not idempotent: if interior started below
    /// the list scope (e.g., col 0 under an opener at col 2 that needs to
    /// move to col 3), delta-shift landed interior at col 1 — still below
    /// the list scope — and the next MD077 pass would re-flag it
    /// individually and snap it to `required`. The promote-up rule reaches
    /// that end state in a single pass.
    ///
    /// Leading tabs are normalized to spaces: CommonMark expands a tab to
    /// the next column that's a multiple of 4, so simply prepending spaces
    /// before a tab would let the tab snap back and cancel the shift. We
    /// replace the whole leading-whitespace byte range with spaces.
    fn build_compound_fence_fix(
        ctx: &LintContext,
        opener_line: usize,
        closer_line: usize,
        opener_actual: usize,
        required: usize,
    ) -> Option<Fix> {
        if required <= opener_actual {
            return None;
        }
        let opener_info = ctx.line_info(opener_line)?;
        let closer_info = ctx.line_info(closer_line)?;

        let fix_start = opener_info.byte_offset;
        let fix_end = closer_info.byte_offset + closer_info.byte_len;

        let mut replacement = String::new();
        for i in opener_line..=closer_line {
            let info = ctx.line_info(i)?;
            if i > opener_line {
                replacement.push('\n');
            }
            let line = info.content(ctx.content);
            if info.is_blank {
                // Blank lines have no content to shift; preserve verbatim.
                replacement.push_str(line);
            } else {
                let new_visual = if i == opener_line || i == closer_line {
                    required
                } else {
                    info.visual_indent.max(required)
                };
                for _ in 0..new_visual {
                    replacement.push(' ');
                }
                replacement.push_str(&line[info.indent..]);
            }
        }

        Some(Fix::new(fix_start..fix_end, replacement))
    }

    /// Walk the continuation lines owned by a single list item, invoking
    /// `per_line` for each *in-scope, non-blank, non-nested, non-skipped*
    /// line with its pre-computed visual column and loose/tight state.
    ///
    /// This is the **single source of truth** for MD077's item-scope
    /// traversal: both the sibling-column pre-pass and the main check loop
    /// route through this method so their termination semantics cannot
    /// drift. The callback sees only lines the rule actually needs to
    /// reason about; it can return `ControlFlow::Break` for early exit.
    ///
    /// Termination conditions (applied before the callback fires):
    /// - Headings and horizontal rules end the item unconditionally.
    /// - After a blank line, content at or below the marker column has
    ///   escaped the item; further lines are not delivered.
    ///
    /// Skipped silently (do not fire the callback):
    /// - Blank lines (toggle `saw_blank`).
    /// - Nested list items (reset `saw_blank`, track their content column).
    /// - Lines inside a nested item's scope (`col >= nested_content_col`).
    /// - Reference/footnote/abbreviation definitions and similar block
    ///   constructs that aren't list continuation.
    /// - Lines that `should_skip_line` rejects (code-block interior etc.).
    fn walk_item_continuation<F>(
        ctx: &LintContext,
        item_line: usize,
        range_end: usize,
        marker_col: usize,
        mut per_line: F,
    ) where
        F: FnMut(&ContinuationLine<'_>) -> ControlFlow<()>,
    {
        let mut saw_blank = false;
        let mut nested_content_col: Option<usize> = None;

        for line_num in (item_line + 1)..=range_end {
            let Some(info) = ctx.line_info(line_num) else {
                continue;
            };

            let trimmed = info.content(ctx.content).trim_start();

            if Self::should_skip_line(info, trimmed) {
                continue;
            }

            if info.is_blank {
                saw_blank = true;
                continue;
            }

            if let Some(ref li) = info.list_item {
                nested_content_col = (li.marker_column > marker_col).then_some(li.content_column);
                saw_blank = false;
                continue;
            }

            if info.heading.is_some() || info.is_horizontal_rule {
                break;
            }

            if Self::is_block_level_construct(trimmed) {
                continue;
            }

            let col = info.visual_indent;

            if let Some(ncc) = nested_content_col {
                if col >= ncc {
                    continue;
                }
                nested_content_col = None;
            }

            if saw_blank && col <= marker_col {
                break;
            }

            let line = ContinuationLine {
                line_num,
                info,
                trimmed,
                actual: col,
                saw_blank,
            };
            if per_line(&line).is_break() {
                break;
            }
        }
    }

    /// Scan an item's owned range and report whether any *other* continuation
    /// line in the item uses the content column or the post-checkbox column.
    ///
    /// Used exclusively for tie-breaking the auto-fix target when an
    /// over-indented line is exactly equidistant from `content_col` and
    /// `task_col`. In that case the author's intent is ambiguous, so we
    /// snap to whichever valid column *they're already using* elsewhere in
    /// the same item. When neither or both columns are in use, the caller
    /// falls back to a canonical default.
    fn sibling_column_usage(
        ctx: &LintContext,
        item_line: usize,
        range_end: usize,
        marker_col: usize,
        content_col: usize,
        task_col: usize,
    ) -> (bool, bool) {
        let mut uses_content = false;
        let mut uses_task = false;

        Self::walk_item_continuation(ctx, item_line, range_end, marker_col, |line| {
            if line.actual == content_col {
                uses_content = true;
            }
            if line.actual == task_col {
                uses_task = true;
            }
            if uses_content && uses_task {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });

        (uses_content, uses_task)
    }

    /// Compute the auto-fix target for an over-indented continuation line.
    /// Snaps to the nearer of the two valid columns (content_col / task_col)
    /// for task items, and on an exact tie uses sibling-column context to
    /// pick whichever column the author is already using elsewhere in this
    /// item. Non-task items always snap to `required`.
    fn compute_fix_target(
        actual: usize,
        required: usize,
        task_col: Option<usize>,
        uses_content_col: bool,
        uses_task_col: bool,
    ) -> usize {
        let Some(t) = task_col else { return required };
        match actual.abs_diff(t).cmp(&actual.abs_diff(required)) {
            std::cmp::Ordering::Less => t,
            std::cmp::Ordering::Greater => required,
            std::cmp::Ordering::Equal => match (uses_task_col, uses_content_col) {
                (true, false) => t,
                _ => required,
            },
        }
    }

    /// Check if a line should be skipped (inside code, HTML, frontmatter, etc.)
    ///
    /// Code block *content* is skipped, but fence opener/closer lines are not —
    /// their indentation matters for list continuation in MkDocs.
    fn should_skip_line(info: &crate::lint_context::LineInfo, trimmed: &str) -> bool {
        if info.in_code_block && !Self::is_code_fence(trimmed) {
            return true;
        }
        info.in_front_matter
            || info.in_html_block
            || info.in_html_comment
            || info.in_mdx_comment
            || info.in_mkdocstrings
            || info.in_esm_block
            || info.in_math_block
            || info.in_admonition
            || info.in_content_tab
            || info.in_pymdown_block
            || info.in_definition_list
            || info.in_mkdocs_html_markdown
            || info.in_kramdown_extension_block
    }

    /// Build the warning for a tight-mode over-indented continuation line.
    fn build_over_indent_warning(
        ctx: &LintContext,
        line: &ContinuationLine<'_>,
        fix_target: usize,
        message: String,
    ) -> LintWarning {
        let line_content = line.info.content(ctx.content);
        let fix_start = line.info.byte_offset;
        let fix_end = fix_start + line.info.indent;
        LintWarning {
            rule_name: Some("MD077".to_string()),
            line: line.line_num,
            column: 1,
            end_line: line.line_num,
            end_column: line_content.len() + 1,
            message,
            severity: Severity::Warning,
            fix: Some(Fix::new(fix_start..fix_end, " ".repeat(fix_target))),
        }
    }

    /// Build the warning for a loose-mode under-indented continuation line.
    /// When the line is the opener of a fenced code block, emit a compound
    /// fix that reindents opener + interior + closer atomically so MD031
    /// doesn't see a transiently-broken fence pair (see #574).
    ///
    /// Returns the warning plus, when the fix is compound, the closer
    /// line number so the caller can mark it flagged (preventing the
    /// main loop from double-flagging the closer as its own under-indent
    /// case). Keeping the "also flag this line" signal out of band keeps
    /// this function pure — it reads from `ctx` only and returns a
    /// plain value.
    fn build_under_indent_warning(
        ctx: &LintContext,
        line: &ContinuationLine<'_>,
        required: usize,
        message: String,
    ) -> UnderIndentOutcome {
        let line_content = line.info.content(ctx.content);
        let is_fence_opener = line.info.in_code_block
            && Self::is_code_fence(line.trimmed)
            && ctx.line_info(line.line_num - 1).is_none_or(|p| !p.in_code_block);

        let (fix, warn_end_line, warn_end_column, compound_closer) = if is_fence_opener {
            let closer_line = Self::find_fence_closer(ctx, line.line_num);
            let fix = Self::build_compound_fence_fix(ctx, line.line_num, closer_line, line.actual, required);
            let end_column = ctx
                .line_info(closer_line)
                .map_or(line_content.len() + 1, |ci| ci.content(ctx.content).len() + 1);
            let extra_flag = (closer_line != line.line_num).then_some(closer_line);
            (fix, closer_line, end_column, extra_flag)
        } else {
            let fix_start = line.info.byte_offset;
            let fix_end = fix_start + line.info.indent;
            let fix = Some(Fix::new(fix_start..fix_end, " ".repeat(required)));
            (fix, line.line_num, line_content.len() + 1, None)
        };

        UnderIndentOutcome {
            warning: LintWarning {
                rule_name: Some("MD077".to_string()),
                line: line.line_num,
                column: 1,
                end_line: warn_end_line,
                end_column: warn_end_column,
                message,
                severity: Severity::Warning,
                fix,
            },
            also_flag_line: compound_closer,
        }
    }
}

/// A continuation line yielded by `walk_item_continuation`. Bundles the
/// per-line facts both checker branches need so helper functions don't
/// balloon their argument lists.
struct ContinuationLine<'a> {
    line_num: usize,
    info: &'a LineInfo,
    trimmed: &'a str,
    actual: usize,
    saw_blank: bool,
}

/// Result of `build_under_indent_warning`. Carries both the warning and,
/// when the fix is compound (fence opener → promote-to-required over the
/// whole block), the closer line so the caller can record it as already
/// handled. This keeps the warning builder free of external mutation.
struct UnderIndentOutcome {
    warning: LintWarning,
    also_flag_line: Option<usize>,
}

impl Rule for MD077ListContinuationIndent {
    fn name(&self) -> &'static str {
        "MD077"
    }

    fn description(&self) -> &'static str {
        "List continuation content indentation"
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        if ctx.content.is_empty() {
            return Ok(Vec::new());
        }

        let strict_indent = ctx.flavor.requires_strict_list_indent();
        let total_lines = ctx.lines.len();
        let mut warnings = Vec::new();
        let mut flagged_lines = std::collections::HashSet::new();

        // Collect all list item lines sorted, with their content_column,
        // marker_column, and — if the item is a GFM task — its post-checkbox
        // column. Precomputing task_col here (instead of re-reading line_info
        // inside the hot inner loop) keeps the per-item cost O(1).
        //
        // We need the owned range to extend past block.end_line because the
        // parser excludes under-indented continuation from the block, and
        // MD077 specifically has to evaluate those escaped lines.
        let mut items: Vec<(usize, usize, usize, Option<usize>)> = Vec::new();
        for block in &ctx.list_blocks {
            for &item_line in &block.item_lines {
                if let Some(info) = ctx.line_info(item_line)
                    && let Some(ref li) = info.list_item
                {
                    let line = info.content(ctx.content);
                    let task_col = Self::is_task_list_item(line, li.content_column)
                        .then_some(li.content_column + Self::TASK_CHECKBOX_PREFIX_LEN);
                    items.push((item_line, li.marker_column, li.content_column, task_col));
                }
            }
        }
        items.sort_unstable();
        items.dedup_by_key(|&mut (ln, _, _, _)| ln);

        for (item_idx, &(item_line, marker_col, content_col, task_col)) in items.iter().enumerate() {
            let required = if strict_indent { content_col.max(4) } else { content_col };

            // Owned range ends at the line before the next sibling-or-higher
            // item, or end of document.
            let range_end = items
                .iter()
                .skip(item_idx + 1)
                .find(|&&(_, mc, _, _)| mc <= marker_col)
                .map_or(total_lines, |&(ln, _, _, _)| ln - 1);

            // For task items, gather sibling-column usage once per item so
            // the auto-fix can tie-break equidistant over-indents toward
            // whichever valid column the author is already using.
            let (uses_content_col, uses_task_col) = match task_col {
                Some(t) => Self::sibling_column_usage(ctx, item_line, range_end, marker_col, content_col, t),
                None => (false, false),
            };

            Self::walk_item_continuation(ctx, item_line, range_end, marker_col, |line| {
                let actual = line.actual;
                if !line.saw_blank {
                    // Tight continuation: flag over-indented lines.
                    if actual > required
                        && Some(actual) != task_col
                        && !Self::starts_with_list_marker(line.trimmed)
                        && flagged_lines.insert(line.line_num)
                    {
                        let fix_target =
                            Self::compute_fix_target(actual, required, task_col, uses_content_col, uses_task_col);
                        let message = match task_col {
                            Some(t) => format!(
                                "Continuation line over-indented \
                                 (expected {required} or {t}, found {actual})"
                            ),
                            None => {
                                format!("Continuation line over-indented (expected {required}, found {actual})")
                            }
                        };
                        warnings.push(Self::build_over_indent_warning(ctx, line, fix_target, message));
                    }
                } else if actual < required && flagged_lines.insert(line.line_num) {
                    // Loose continuation: flag under-indented lines.
                    let message = if strict_indent {
                        format!(
                            "Content inside list item needs {required} spaces of indentation \
                             for MkDocs compatibility (found {actual})",
                        )
                    } else {
                        format!(
                            "Content after blank line in list item needs {required} spaces of \
                             indentation to remain part of the list (found {actual})",
                        )
                    };
                    let outcome = Self::build_under_indent_warning(ctx, line, required, message);
                    if let Some(closer_line) = outcome.also_flag_line {
                        flagged_lines.insert(closer_line);
                    }
                    warnings.push(outcome.warning);
                }
                ControlFlow::Continue(())
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        let warnings = self.check(ctx)?;
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Sort fixes by byte position descending to apply from end to start
        let mut fixes: Vec<Fix> = warnings.into_iter().filter_map(|w| w.fix).collect();
        fixes.sort_by_key(|f| std::cmp::Reverse(f.range.start));

        let mut content = ctx.content.to_string();
        for fix in fixes {
            if fix.range.start <= content.len() && fix.range.end <= content.len() {
                content.replace_range(fix.range, &fix.replacement);
            }
        }

        Ok(content)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || ctx.list_blocks.is_empty()
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

    fn check(content: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let rule = MD077ListContinuationIndent;
        rule.check(&ctx).unwrap()
    }

    fn check_mkdocs(content: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let rule = MD077ListContinuationIndent;
        rule.check(&ctx).unwrap()
    }

    fn fix(content: &str) -> String {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let rule = MD077ListContinuationIndent;
        rule.fix(&ctx).unwrap()
    }

    fn fix_mkdocs(content: &str) -> String {
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let rule = MD077ListContinuationIndent;
        rule.fix(&ctx).unwrap()
    }

    // ── Tight continuation (no blank line) ─────────────────────────────

    #[test]
    fn tight_lazy_continuation_zero_indent_not_flagged() {
        // Zero-indent lazy continuation is valid CommonMark
        let content = "- Item\ncontinuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn tight_continuation_correct_indent_not_flagged() {
        // Correctly indented tight continuation (aligns with content column)
        let content = "1. Item\n   continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn tight_continuation_over_indented_ordered() {
        // "1. " = 3 chars, but continuation has 4 spaces
        let content = "1. This is a list item with multiple lines.\n    The second line is over-indented.\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 2);
        assert!(warnings[0].message.contains("over-indented"));
    }

    #[test]
    fn tight_continuation_over_indented_unordered() {
        // "- " = 2 chars, but continuation has 3 spaces
        let content = "- Item\n   over-indented\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 2);
    }

    #[test]
    fn tight_continuation_multiple_over_indented_lines() {
        let content = "1. Item\n    line one\n    line two\n    line three\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 3);
    }

    #[test]
    fn tight_continuation_mixed_correct_and_over() {
        let content = "1. Item\n   correct\n    over-indented\n   correct again\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
    }

    #[test]
    fn tight_continuation_nested_over_indented() {
        // L2 "- " at column 2, content_column = 4. Continuation at 5 is over-indented for L2.
        let content = "- L1\n  - L2\n     over-indented continuation of L2\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
        // Must report expected=4 (L2's content_col), not expected=2 (L1's)
        assert!(warnings[0].message.contains("expected 4"));
        assert!(warnings[0].message.contains("found 5"));
    }

    #[test]
    fn tight_continuation_nested_correct_indent_not_flagged() {
        // Continuation at 4 spaces is correct for L2 (content_col=4). Must NOT be
        // flagged as over-indented relative to L1 (content_col=2).
        let content = "- L1\n  - L2\n    correctly indented continuation of L2\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn fix_tight_continuation_nested_over_indented() {
        // Fix should reduce to 4 spaces (L2's content_col), not 2 (L1's)
        let content = "- L1\n  - L2\n     over-indented continuation of L2\n";
        let fixed = fix(content);
        assert_eq!(fixed, "- L1\n  - L2\n    over-indented continuation of L2\n");
    }

    #[test]
    fn tight_continuation_under_indented_not_flagged() {
        // 2 spaces instead of 3 for "1. " — under-indented, not over-indented.
        // Valid lazy continuation in CommonMark, so not flagged.
        let content = "1. Item\n  under-indented\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn tight_continuation_tab_over_indented() {
        // A tab expands to 4 visual columns, which exceeds content_col=2 for "- "
        let content = "- Item\n\tover-indented\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn fix_tight_continuation_over_indented_ordered() {
        let content = "1. This is a list item with multiple lines.\n    The second line is over-indented.\n";
        let fixed = fix(content);
        assert_eq!(
            fixed,
            "1. This is a list item with multiple lines.\n   The second line is over-indented.\n"
        );
    }

    #[test]
    fn fix_tight_continuation_over_indented_unordered() {
        let content = "- Item\n   over-indented\n";
        let fixed = fix(content);
        assert_eq!(fixed, "- Item\n  over-indented\n");
    }

    #[test]
    fn fix_tight_continuation_multiple_lines() {
        let content = "1. Item\n    line one\n    line two\n";
        let fixed = fix(content);
        assert_eq!(fixed, "1. Item\n   line one\n   line two\n");
    }

    #[test]
    fn tight_continuation_mkdocs_4space_ordered_not_flagged() {
        // MkDocs requires max(3, 4) = 4 spaces for "1. " items.
        // 4-space tight continuation is correct, not over-indented.
        let content = "1. Item\n    continuation\n";
        assert!(check_mkdocs(content).is_empty());
    }

    #[test]
    fn tight_continuation_mkdocs_5space_ordered_flagged() {
        // 5 spaces exceeds the MkDocs required indent of 4
        let content = "1. Item\n     over-indented\n";
        let warnings = check_mkdocs(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("expected 4"));
        assert!(warnings[0].message.contains("found 5"));
    }

    #[test]
    fn fix_tight_continuation_mkdocs_over_indented() {
        let content = "1. Item\n     over-indented\n";
        let fixed = fix_mkdocs(content);
        assert_eq!(fixed, "1. Item\n    over-indented\n");
    }

    #[test]
    fn tight_continuation_deeply_indented_list_markers_not_flagged() {
        // Deeply indented list markers (e.g., indent=8 in MD007) may not be
        // recognized as list items by the parser. MD077 must not flag them.
        let content = "* Level 0\n        * Level 1\n                * Level 2\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn tight_continuation_ordered_marker_not_flagged() {
        // Indented ordered list marker should not be flagged
        let content = "- Parent\n      1. Child item\n";
        assert!(check(content).is_empty());
    }

    // ── Unordered list: correct indent after blank ────────────────────

    #[test]
    fn unordered_correct_indent_no_warning() {
        let content = "- Item\n\n  continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn unordered_partial_indent_warns() {
        // Content with some indent (above marker column) but less than
        // content_column is likely an indentation mistake.
        let content = "- Item\n\n continuation\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
        assert!(warnings[0].message.contains("2 spaces"));
        assert!(warnings[0].message.contains("found 1"));
    }

    #[test]
    fn unordered_zero_indent_is_new_paragraph() {
        // Content at 0 indent after a top-level list is a new paragraph, not
        // under-indented continuation.
        let content = "- Item\n\ncontinuation\n";
        assert!(check(content).is_empty());
    }

    // ── Ordered list: CommonMark W+N ──────────────────────────────────

    #[test]
    fn ordered_3space_correct_commonmark() {
        // "1. " is 3 chars, content_column = 3
        let content = "1. Item\n\n   continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn ordered_2space_under_indent_commonmark() {
        let content = "1. Item\n\n  continuation\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("3 spaces"));
        assert!(warnings[0].message.contains("found 2"));
    }

    // ── Multi-digit ordered markers ───────────────────────────────────

    #[test]
    fn multi_digit_marker_correct() {
        // "10. " is 4 chars, content_column = 4
        let content = "10. Item\n\n    continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn multi_digit_marker_under_indent() {
        let content = "10. Item\n\n   continuation\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("4 spaces"));
    }

    // ── MkDocs flavor: 4-space minimum ────────────────────────────────

    #[test]
    fn mkdocs_3space_ordered_warns() {
        // In MkDocs mode, 3-space indent on "1. " is not enough
        let content = "1. Item\n\n   continuation\n";
        let warnings = check_mkdocs(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("4 spaces"));
        assert!(warnings[0].message.contains("MkDocs"));
    }

    #[test]
    fn mkdocs_4space_ordered_no_warning() {
        let content = "1. Item\n\n    continuation\n";
        assert!(check_mkdocs(content).is_empty());
    }

    #[test]
    fn mkdocs_unordered_2space_ok() {
        // Unordered "- " has content_column = 2; max(2, 4) = 4 in mkdocs
        let content = "- Item\n\n    continuation\n";
        assert!(check_mkdocs(content).is_empty());
    }

    #[test]
    fn mkdocs_unordered_2space_warns() {
        // "- " has content_column 2; MkDocs requires max(2,4) = 4
        let content = "- Item\n\n  continuation\n";
        let warnings = check_mkdocs(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("4 spaces"));
    }

    // ── Auto-fix ──────────────────────────────────────────────────────

    #[test]
    fn fix_unordered_indent() {
        // Partial indent (above marker column, below content column) gets fixed
        let content = "- Item\n\n continuation\n";
        let fixed = fix(content);
        assert_eq!(fixed, "- Item\n\n  continuation\n");
    }

    #[test]
    fn fix_ordered_indent() {
        let content = "1. Item\n\n continuation\n";
        let fixed = fix(content);
        assert_eq!(fixed, "1. Item\n\n   continuation\n");
    }

    #[test]
    fn fix_mkdocs_indent() {
        let content = "1. Item\n\n   continuation\n";
        let fixed = fix_mkdocs(content);
        assert_eq!(fixed, "1. Item\n\n    continuation\n");
    }

    // ── Nested lists: only flag continuation, not sub-items ───────────

    #[test]
    fn nested_list_items_not_flagged() {
        let content = "- Parent\n\n  - Child\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn nested_list_zero_indent_is_new_paragraph() {
        // Content at 0 indent ends the list, not continuation
        let content = "- Parent\n  - Child\n\ncontinuation of parent\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn nested_list_partial_indent_flagged() {
        // Content with partial indent (above parent marker, below content col)
        let content = "- Parent\n  - Child\n\n continuation of parent\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("2 spaces"));
    }

    // ── Code blocks inside items ─────────────────────────────────────

    #[test]
    fn code_block_correctly_indented_no_warning() {
        // Fence lines and content all at correct indent for "- " (content_column = 2)
        let content = "- Item\n\n  ```\n  code\n  ```\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn code_fence_under_indented_warns() {
        // Fence opener has 1-space indent, but "- " needs 2.
        // Only the opener is flagged — its compound fix also covers the
        // interior content and the matching closer (see issue #574).
        let content = "- Item\n\n ```\n code\n ```\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
    }

    #[test]
    fn code_fence_under_indented_ordered_mkdocs() {
        // Ordered list in MkDocs: "1. " needs max(3, 4) = 4 spaces
        // Fence at 3 spaces is correct for CommonMark but wrong for MkDocs
        let content = "1. Item\n\n   ```toml\n   key = \"value\"\n   ```\n";
        assert!(check(content).is_empty()); // Standard mode: 3 is fine
        let warnings = check_mkdocs(content);
        assert_eq!(warnings.len(), 1); // MkDocs: opener's compound fix covers the whole block
        assert_eq!(warnings[0].line, 3);
        assert!(warnings[0].message.contains("4 spaces"));
        assert!(warnings[0].message.contains("MkDocs"));
    }

    #[test]
    fn code_fence_tilde_under_indented() {
        let content = "- Item\n\n ~~~\n code\n ~~~\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1); // Tilde fences: single compound-fix warning on opener
        assert_eq!(warnings[0].line, 3);
    }

    // ── Multiple blank lines ──────────────────────────────────────────

    #[test]
    fn multiple_blank_lines_zero_indent_is_new_paragraph() {
        // Even with multiple blanks, 0-indent content is a new paragraph
        let content = "- Item\n\n\ncontinuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn multiple_blank_lines_partial_indent_flags() {
        let content = "- Item\n\n\n continuation\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
    }

    // ── Empty items: no continuation to check ─────────────────────────

    #[test]
    fn empty_item_no_warning() {
        let content = "- \n- Second\n";
        assert!(check(content).is_empty());
    }

    // ── Multiple items, only some under-indented ──────────────────────

    #[test]
    fn multiple_items_mixed_indent() {
        let content = "1. First\n\n   correct continuation\n\n2. Second\n\n  wrong continuation\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 7);
    }

    // ── Task list items ───────────────────────────────────────────────

    #[test]
    fn task_list_correct_indent() {
        // "- [ ] " = content_column is typically at col 6
        let content = "- [ ] Task\n\n      continuation\n";
        assert!(check(content).is_empty());
    }

    // ── Frontmatter skipped ───────────────────────────────────────────

    #[test]
    fn frontmatter_not_flagged() {
        let content = "---\ntitle: test\n---\n\n- Item\n\n  continuation\n";
        assert!(check(content).is_empty());
    }

    // ── Fix produces valid output with multiple fixes ─────────────────

    #[test]
    fn fix_multiple_items() {
        let content = "1. First\n\n wrong1\n\n2. Second\n\n wrong2\n";
        let fixed = fix(content);
        assert_eq!(fixed, "1. First\n\n   wrong1\n\n2. Second\n\n   wrong2\n");
    }

    #[test]
    fn fix_multiline_loose_continuation_all_lines() {
        let content = "1. Item\n\n  line one\n  line two\n  line three\n";
        let fixed = fix(content);
        assert_eq!(fixed, "1. Item\n\n   line one\n   line two\n   line three\n");
    }

    // ── No false positive when content is after sibling item ──────────

    #[test]
    fn sibling_item_boundary_respected() {
        // The "continuation" after a blank belongs to "- Second", not "- First"
        let content = "- First\n- Second\n\n  continuation\n";
        assert!(check(content).is_empty());
    }

    // ── Blockquote-nested lists ────────────────────────────────────────

    #[test]
    fn blockquote_list_correct_indent_no_warning() {
        // Lists inside blockquotes: visual_indent includes the blockquote
        // prefix, so comparisons work on raw line columns.
        let content = "> - Item\n>\n>   continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn blockquote_list_under_indent_no_false_positive() {
        // Under-indented continuation inside a blockquote: visual_indent
        // starts at 0 (the `>` char) which is <= marker_col, so the scan
        // breaks and no warning is emitted. This is a known false negative
        // (not a false positive), which is the safer default.
        let content = "> - Item\n>\n> continuation\n";
        assert!(check(content).is_empty());
    }

    // ── Deep nesting (3+ levels) ──────────────────────────────────────

    #[test]
    fn deeply_nested_correct_indent() {
        let content = "- L1\n  - L2\n    - L3\n\n      continuation of L3\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn deeply_nested_under_indent() {
        // L3 starts at column 4 with "- " marker, content_column = 6
        // Continuation with 5 spaces is under-indented for L3.
        let content = "- L1\n  - L2\n    - L3\n\n     continuation of L3\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("6 spaces"));
        assert!(warnings[0].message.contains("found 5"));
    }

    // ── Tab indentation ───────────────────────────────────────────────

    #[test]
    fn tab_indent_correct() {
        // A tab at the start expands to 4 visual columns, which satisfies
        // "- " (content_column = 2).
        let content = "- Item\n\n\tcontinuation\n";
        assert!(check(content).is_empty());
    }

    // ── Multiple continuation paragraphs ──────────────────────────────

    #[test]
    fn multiple_continuations_correct() {
        let content = "- Item\n\n  para 1\n\n  para 2\n\n  para 3\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn multiple_continuations_second_under_indent() {
        // First continuation is correct, second is under-indented
        let content = "- Item\n\n  para 1\n\n continuation 2\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 5);
    }

    // ── Ordered list with `)` marker style ────────────────────────────

    #[test]
    fn ordered_paren_marker_correct() {
        // "1) " is 3 chars, content_column = 3
        let content = "1) Item\n\n   continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn ordered_paren_marker_under_indent() {
        let content = "1) Item\n\n  continuation\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("3 spaces"));
    }

    // ── Star and plus markers ─────────────────────────────────────────

    #[test]
    fn star_marker_correct() {
        let content = "* Item\n\n  continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn star_marker_under_indent() {
        let content = "* Item\n\n continuation\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn plus_marker_correct() {
        let content = "+ Item\n\n  continuation\n";
        assert!(check(content).is_empty());
    }

    // ── Heading breaks scan ───────────────────────────────────────────

    #[test]
    fn heading_after_list_no_warning() {
        let content = "- Item\n\n# Heading\n";
        assert!(check(content).is_empty());
    }

    // ── Horizontal rule breaks scan ───────────────────────────────────

    #[test]
    fn hr_after_list_no_warning() {
        let content = "- Item\n\n---\n";
        assert!(check(content).is_empty());
    }

    // ── Reference link definitions skip ───────────────────────────────

    #[test]
    fn reference_link_def_not_flagged() {
        let content = "- Item\n\n [link]: https://example.com\n";
        assert!(check(content).is_empty());
    }

    // ── Footnote definitions skip ─────────────────────────────────────

    #[test]
    fn footnote_def_not_flagged() {
        let content = "- Item\n\n [^1]: footnote text\n";
        assert!(check(content).is_empty());
    }

    // ── Fix preserves correct content ─────────────────────────────────

    #[test]
    fn fix_deeply_nested() {
        let content = "- L1\n  - L2\n    - L3\n\n     under-indented\n";
        let fixed = fix(content);
        assert_eq!(fixed, "- L1\n  - L2\n    - L3\n\n      under-indented\n");
    }

    #[test]
    fn fix_mkdocs_unordered() {
        // MkDocs: "- " has content_column 2, but MkDocs requires max(2,4) = 4
        let content = "- Item\n\n  continuation\n";
        let fixed = fix_mkdocs(content);
        assert_eq!(fixed, "- Item\n\n    continuation\n");
    }

    #[test]
    fn fix_code_fence_indent() {
        // Fence opener, interior, and closer all shift by the same delta so
        // the parser keeps pairing the fences and MD031 doesn't misfire.
        let content = "- Item\n\n ```\n code\n ```\n";
        let fixed = fix(content);
        assert_eq!(fixed, "- Item\n\n  ```\n  code\n  ```\n");
    }

    #[test]
    fn fix_mkdocs_code_fence_indent() {
        // MkDocs ordered list: fence at 3 spaces needs 4; interior shifts too
        let content = "1. Item\n\n   ```toml\n   key = \"val\"\n   ```\n";
        let fixed = fix_mkdocs(content);
        assert_eq!(fixed, "1. Item\n\n    ```toml\n    key = \"val\"\n    ```\n");
    }

    // ── Empty document / whitespace-only ──────────────────────────────

    #[test]
    fn empty_document_no_warning() {
        assert!(check("").is_empty());
    }

    #[test]
    fn whitespace_only_no_warning() {
        assert!(check("   \n\n  \n").is_empty());
    }

    // ── No list at all ────────────────────────────────────────────────

    #[test]
    fn no_list_no_warning() {
        let content = "# Heading\n\nSome paragraph.\n\nAnother paragraph.\n";
        assert!(check(content).is_empty());
    }

    // ── Multi-line continuation (additional coverage) ──────────────

    #[test]
    fn multiline_continuation_all_lines_flagged() {
        let content = "1. This is a list item.\n\n  This is continuation text and\n  it has multiple lines.\n  This is yet another line.\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 3);
        assert_eq!(warnings[0].line, 3);
        assert_eq!(warnings[1].line, 4);
        assert_eq!(warnings[2].line, 5);
    }

    #[test]
    fn multiline_continuation_with_frontmatter_fix() {
        let content = "---\ntitle: Heading\n---\n\nSome introductory text:\n\n1. This is a list item.\n\n  This is list continuation text and\n  it has multiple lines that aren't indented properly.\n  This is yet another line that isn't indented properly.\n1. This is a list item.\n\n  This is list continuation text and\n  it has multiple lines that aren't indented properly.\n  This is yet another line that isn't indented properly.\n";
        let fixed = fix(content);
        assert_eq!(
            fixed,
            "---\ntitle: Heading\n---\n\nSome introductory text:\n\n1. This is a list item.\n\n   This is list continuation text and\n   it has multiple lines that aren't indented properly.\n   This is yet another line that isn't indented properly.\n1. This is a list item.\n\n   This is list continuation text and\n   it has multiple lines that aren't indented properly.\n   This is yet another line that isn't indented properly.\n"
        );
    }

    #[test]
    fn multiline_continuation_correct_indent_no_warning() {
        let content = "1. Item\n\n   line one\n   line two\n   line three\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn multiline_continuation_mixed_indent() {
        let content = "1. Item\n\n   correct\n  wrong\n   correct\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 4);
    }

    #[test]
    fn multiline_continuation_unordered() {
        let content = "- Item\n\n continuation 1\n continuation 2\n continuation 3\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 3);
        let fixed = fix(content);
        assert_eq!(
            fixed,
            "- Item\n\n  continuation 1\n  continuation 2\n  continuation 3\n"
        );
    }

    #[test]
    fn multiline_continuation_two_items_fix() {
        let content = "1. First\n\n  cont a\n  cont b\n\n2. Second\n\n  cont c\n  cont d\n";
        let fixed = fix(content);
        assert_eq!(
            fixed,
            "1. First\n\n   cont a\n   cont b\n\n2. Second\n\n   cont c\n   cont d\n"
        );
    }

    #[test]
    fn fence_fix_does_not_break_pairing_for_md031() {
        // Regression for issue #574: previously MD077 only reindented the
        // fence delimiter lines while leaving the code block's interior at
        // the old indent. Between iterations of the fix loop the parser
        // saw an opener-closer mismatch, and MD031 then injected stray
        // blank lines at the fence boundaries. MD077's compound fix must
        // now rewrite the whole block atomically so the fences stay paired.
        let content = "#### title\n\nabc\n\n\
                       1. ab\n\n\
                       \x20\x20`aabbccdd`\n\n\
                       2. cd\n\n\
                       \x20\x20`bbcc dd ee`\n\n\
                       \x20\x20```\n\
                       \x20\x20abcd\n\
                       \x20\x20ef gh\n\
                       \x20\x20```\n\n\
                       \x20\x20uu\n\n\
                       \x20\x20```\n\
                       \x20\x20cdef\n\
                       \x20\x20gh ij\n\
                       \x20\x20```\n";
        let expected = "#### title\n\nabc\n\n\
                        1. ab\n\n\
                        \x20\x20\x20`aabbccdd`\n\n\
                        2. cd\n\n\
                        \x20\x20\x20`bbcc dd ee`\n\n\
                        \x20\x20\x20```\n\
                        \x20\x20\x20abcd\n\
                        \x20\x20\x20ef gh\n\
                        \x20\x20\x20```\n\n\
                        \x20\x20\x20uu\n\n\
                        \x20\x20\x20```\n\
                        \x20\x20\x20cdef\n\
                        \x20\x20\x20gh ij\n\
                        \x20\x20\x20```\n";
        assert_eq!(fix(content), expected);
    }

    #[test]
    fn multiline_continuation_separated_by_blank() {
        let content = "1. Item\n\n  para1 line1\n  para1 line2\n\n  para2 line1\n  para2 line2\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 4);
        let fixed = fix(content);
        assert_eq!(
            fixed,
            "1. Item\n\n   para1 line1\n   para1 line2\n\n   para2 line1\n   para2 line2\n"
        );
    }

    #[test]
    fn tab_indented_fence_is_normalized_to_spaces() {
        // Leading tabs expand to the next multiple-of-4 column under
        // CommonMark, so simply prepending spaces before a tab would
        // silently no-op (the tab snaps back to column 4). The compound
        // fence fix must replace the leading whitespace with a fresh
        // (visual_indent + delta) run of spaces. A `100. ` item has
        // content_column = 5, so a tab-indented fence (visual col 4) is
        // under-indented by 1 and must end up at 5 spaces after the fix.
        let content = "100. ab\n\n\t```\n\tabcd\n\t```\n";
        let expected = "100. ab\n\n     ```\n     abcd\n     ```\n";
        assert_eq!(fix(content), expected);
    }

    // ── GFM task list items: post-checkbox continuation column ───────
    //
    // MD013's reflow indents wrapped task-list lines at `content_col + 4`
    // (the column after the checkbox). MD077 must accept that column for
    // both tight and loose continuation, for every marker flavour, so the
    // two rules don't fight over well-formed task items (issue #579).

    #[test]
    fn task_list_tight_continuation_post_checkbox_reproducer_579() {
        // Exact reproducer from the bug report: content wraps to the
        // post-checkbox column (6) with no blank line.
        let content = "- [ ] Lorem ipsum dolor sit amet, consectetur adipiscing\n      tempor incididunt ut labore.\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn task_list_tight_continuation_dash_unchecked() {
        let content = "- [ ] Task\n      continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn task_list_tight_continuation_dash_checked_lower() {
        let content = "- [x] Task\n      continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn task_list_tight_continuation_dash_checked_upper() {
        let content = "- [X] Task\n      continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn task_list_tight_continuation_star_marker() {
        let content = "* [ ] Task\n      continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn task_list_tight_continuation_plus_marker() {
        let content = "+ [ ] Task\n      continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn task_list_tight_continuation_content_column_still_valid() {
        // Column 2 is the CommonMark-canonical indent for "- " and remains
        // valid for task items too.
        let content = "- [ ] Task\n  continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn task_list_tight_continuation_between_columns_still_flagged() {
        // Column 4 matches neither content_col (2) nor post-checkbox (6).
        // A genuine indentation mistake — must remain flagged.
        let content = "- [ ] Task\n    continuation\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        // Task items advertise both valid columns to the user.
        assert!(warnings[0].message.contains("expected 2 or 6"));
        assert!(warnings[0].message.contains("found 4"));
    }

    #[test]
    fn task_list_tight_continuation_overshoot_still_flagged() {
        // Column 7 overshoots the post-checkbox column. Genuine mistake.
        let content = "- [ ] Task\n       continuation\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("expected 2 or 6"));
        assert!(warnings[0].message.contains("found 7"));
    }

    // ── Task-list fix output: snap to nearer valid column ────────────

    #[test]
    fn fix_task_list_overshoot_snaps_to_task_col() {
        // Col 7 is 1 away from post-checkbox (6), 5 away from content (2).
        // Snap to 6 — the author's intent was almost certainly the
        // post-checkbox alignment, not the content column.
        let content = "- [ ] Task\n       continuation\n";
        let fixed = fix(content);
        assert_eq!(fixed, "- [ ] Task\n      continuation\n");
    }

    #[test]
    fn fix_task_list_col_5_snaps_to_task_col() {
        // Col 5 is 1 away from post-checkbox (6), 3 away from content (2).
        let content = "- [ ] Task\n     continuation\n";
        let fixed = fix(content);
        assert_eq!(fixed, "- [ ] Task\n      continuation\n");
    }

    #[test]
    fn fix_task_list_col_3_snaps_to_content_col() {
        // Col 3 is 1 away from content (2), 3 away from post-checkbox (6).
        let content = "- [ ] Task\n   continuation\n";
        let fixed = fix(content);
        assert_eq!(fixed, "- [ ] Task\n  continuation\n");
    }

    #[test]
    fn fix_task_list_col_4_ties_to_content_col() {
        // Col 4 is equidistant (±2) from both columns. Tie breaks to the
        // CommonMark-canonical content column — that's the default indent
        // MD077 would produce for a non-task item, so prefer it when the
        // author's intent is ambiguous.
        let content = "- [ ] Task\n    continuation\n";
        let fixed = fix(content);
        assert_eq!(fixed, "- [ ] Task\n  continuation\n");
    }

    #[test]
    fn fix_task_list_ordered_overshoot_snaps_to_task_col() {
        // "1. [ ] " → content_col = 3, post-checkbox = 7.
        // Col 8 is nearer to 7.
        let content = "1. [ ] Task\n        continuation\n";
        let fixed = fix(content);
        assert_eq!(fixed, "1. [ ] Task\n       continuation\n");
    }

    #[test]
    fn fix_task_list_ordered_under_overshoot_snaps_to_content_col() {
        // "1. [ ] " → content_col = 3, post-checkbox = 7.
        // Col 4 is nearer to 3.
        let content = "1. [ ] Task\n    continuation\n";
        let fixed = fix(content);
        assert_eq!(fixed, "1. [ ] Task\n   continuation\n");
    }

    #[test]
    fn task_list_tight_continuation_ordered_single_digit() {
        // "1. [ ] " → content_col = 3, post-checkbox = 7
        let content = "1. [ ] Task\n       continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn task_list_tight_continuation_ordered_multi_digit() {
        // "10. [ ] " → content_col = 4, post-checkbox = 8
        let content = "10. [ ] Task\n        continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn task_list_tight_continuation_nested_dash() {
        // Nested "  - [ ] " at marker_col=2 → content_col=4, post-checkbox=8
        let content = "- Parent\n  - [ ] Nested task\n        continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn task_list_loose_continuation_post_checkbox_column_not_flagged() {
        // Loose continuation (blank line) at col 6 is also valid. This
        // already passed before the fix, but pin the intent: the 6-space
        // indent is accepted because it's the task-alignment column, not
        // because the under-indent check happens to let it through.
        let content = "- [ ] Task\n\n      continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn task_list_empty_body_is_not_a_task() {
        // "- [ ]" with nothing after is an empty regular list item, not a
        // task. Column 4 continuation has no task alignment to justify it
        // and must still be flagged as over-indented. (Col 6 would turn
        // the continuation into an indented code block inside the item,
        // which is a different code path.)
        let content = "- [ ]\n    continuation\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("found 4"));
    }

    #[test]
    fn task_list_malformed_checkbox_is_not_a_task() {
        // `[~] ` is not a GFM checkbox; only `[ ] `, `[x] `, `[X] ` count.
        let content = "- [~] Not a task\n      continuation\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
    }

    // ── MkDocs flavor × task checkbox ─────────────────────────────────
    //
    // MkDocs strict-indent and task alignment interact: required_min is
    // max(content_col, 4), and post-checkbox is content_col + 4. Both are
    // independently valid; values between them are flagged.

    #[test]
    fn task_list_mkdocs_unordered_required_min_valid() {
        // "- [ ]" MkDocs: required_min = max(2, 4) = 4, post-checkbox = 6.
        let content = "- [ ] Task\n    continuation\n";
        assert!(check_mkdocs(content).is_empty());
    }

    #[test]
    fn task_list_mkdocs_unordered_post_checkbox_valid() {
        let content = "- [ ] Task\n      continuation\n";
        assert!(check_mkdocs(content).is_empty());
    }

    #[test]
    fn task_list_mkdocs_unordered_between_flagged() {
        // Column 5 is between required_min=4 and post-checkbox=6.
        let content = "- [ ] Task\n     continuation\n";
        let warnings = check_mkdocs(content);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn task_list_mkdocs_ordered_both_columns_valid() {
        // "1. [ ]" MkDocs: required_min = max(3, 4) = 4, post-checkbox = 7.
        let at_4 = "1. [ ] Task\n    continuation\n";
        assert!(check_mkdocs(at_4).is_empty());
        let at_7 = "1. [ ] Task\n       continuation\n";
        assert!(check_mkdocs(at_7).is_empty());
    }

    #[test]
    fn task_list_mkdocs_ordered_between_flagged() {
        // Column 5 and 6 are between required_min=4 and post-checkbox=7.
        let at_5 = "1. [ ] Task\n     continuation\n";
        assert_eq!(check_mkdocs(at_5).len(), 1);
        let at_6 = "1. [ ] Task\n      continuation\n";
        assert_eq!(check_mkdocs(at_6).len(), 1);
    }

    // ── Context-aware tie-break ──────────────────────────────────────
    //
    // When a flagged line is exactly equidistant from `content_col` and
    // `task_col`, the author's intent is ambiguous. Before picking a
    // canonical default, look at whether other continuation lines in the
    // same item already use one of the valid columns — if so, snap to the
    // column they're using so the fix preserves the author's visible
    // convention.

    #[test]
    fn fix_task_list_tie_sibling_at_task_col_snaps_to_task_col() {
        // Col 4 is equidistant from content_col (2) and task_col (6).
        // A valid sibling at col 6 proves the author is aligning under the
        // checkbox, so the tie resolves to col 6.
        let content = "- [ ] Task\n      aligned continuation\n    tied continuation\n";
        let fixed = fix(content);
        assert_eq!(
            fixed,
            "- [ ] Task\n      aligned continuation\n      tied continuation\n"
        );
    }

    #[test]
    fn fix_task_list_tie_sibling_at_content_col_snaps_to_content_col() {
        // Valid sibling at col 2 proves the author is aligning to the
        // content column, so the col-4 tie resolves to col 2.
        let content = "- [ ] Task\n  aligned continuation\n    tied continuation\n";
        let fixed = fix(content);
        assert_eq!(fixed, "- [ ] Task\n  aligned continuation\n  tied continuation\n");
    }

    #[test]
    fn fix_task_list_tie_both_siblings_snaps_to_content_col() {
        // When siblings exist at both valid columns, the author's pattern
        // is self-contradictory. Fall back to the CommonMark-canonical
        // content column.
        let content = "- [ ] Task\n  at content col\n      at task col\n    tied continuation\n";
        let fixed = fix(content);
        assert_eq!(
            fixed,
            "- [ ] Task\n  at content col\n      at task col\n  tied continuation\n"
        );
    }

    #[test]
    fn fix_task_list_tie_sees_task_col_through_tight_lazy_continuation() {
        // CommonMark allows tight lazy continuation at col ≤ marker_col
        // (zero-indent continuation) inside a list item. The pre-pass
        // must MIRROR the main check loop's termination semantics: in
        // tight mode (no preceding blank) col ≤ marker_col is NOT a
        // termination signal — the lazy line still belongs to the item.
        //
        // This test pins that mirroring: a `lazy` line at col 0 is
        // followed by a legitimate task-col sibling at col 6, then a
        // tied col-4 line. If the pre-pass terminated eagerly at the
        // lazy line, the task-col sibling would be missed and the tied
        // line would fall back to content column. With correct
        // mirroring, the task-col sibling is seen and the tie resolves
        // to col 6.
        let content = concat!("- [ ] Task\n", "lazy\n", "      aligned at task col\n", "    tied\n",);
        let fixed = fix(content);
        assert!(
            fixed.contains("\n      tied\n"),
            "tied line should snap to col 6 (task col) because a task-col \
             sibling is visible past the tight lazy-continuation line; got:\n{fixed}"
        );
    }

    // ── Tab-indented task continuation ───────────────────────────────
    //
    // Leading tabs expand to the next column that's a multiple of 4 under
    // CommonMark. The fix replaces the leading whitespace bytes wholesale,
    // turning tabs into space-indented output.

    #[test]
    fn task_list_tab_indented_continuation_flagged() {
        // Two tabs → visual col 8, which overshoots both valid columns
        // for `- [ ] ` (content_col=2, task_col=6).
        let content = "- [ ] Task\n\t\twrap\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("expected 2 or 6"));
        assert!(warnings[0].message.contains("found 8"));
    }

    #[test]
    fn fix_task_list_tab_indented_snaps_to_task_col() {
        // abs_diff(8, 6) = 2 < abs_diff(8, 2) = 6 → snap to task_col (6).
        let content = "- [ ] Task\n\t\twrap\n";
        let fixed = fix(content);
        assert_eq!(fixed, "- [ ] Task\n      wrap\n");
    }

    #[test]
    fn fix_task_list_single_tab_equidistant_snaps_to_content_col() {
        // One tab → visual col 4, equidistant from content_col (2) and
        // task_col (6). No siblings → tie-break to content_col.
        let content = "- [ ] Task\n\twrap\n";
        let fixed = fix(content);
        assert_eq!(fixed, "- [ ] Task\n  wrap\n");
    }

    // ── Blockquote × task-list ───────────────────────────────────────
    //
    // Blockquote-nested lists are a known limitation on MD077: the list
    // parser doesn't always expose them with the same column semantics as
    // top-level lists, and the rule prefers a false-negative default to
    // avoid spurious warnings inside blockquotes (see
    // `blockquote_list_under_indent_no_false_positive`). These tests pin
    // the current behavior so any future change is intentional.

    #[test]
    fn task_list_blockquote_post_checkbox_not_flagged() {
        // Post-checkbox alignment inside a blockquote — accepted as valid.
        let content = "> - [ ] Task\n>       continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn task_list_blockquote_between_cols_documented_limitation() {
        // Col-4-equivalent inside a blockquote is silently accepted — a
        // known MD077 limitation on blockquote-nested lists, not a task-
        // list-specific choice. Pinning the current behavior.
        let content = "> - [ ] Task\n>     continuation\n";
        assert!(check(content).is_empty());
    }

    #[test]
    fn task_list_blockquote_overshoot_documented_limitation() {
        // Overshoot inside a blockquote — same known limitation.
        let content = "> - [ ] Task\n>        continuation\n";
        assert!(check(content).is_empty());
    }

    // ── MkDocs × task × fix output ───────────────────────────────────
    //
    // MkDocs strict-indent raises `required` to max(content_col, 4) while
    // task_col stays at content_col + 4. The snap logic operates on the
    // raised required, not on the underlying content_col.

    #[test]
    fn fix_task_list_mkdocs_unordered_overshoot_snaps_to_task_col() {
        // `- [ ]` MkDocs: required=4, task_col=6. Col 7 → abs_diff(7,6)=1
        // < abs_diff(7,4)=3. Snap to task_col.
        let content = "- [ ] Task\n       continuation\n";
        let fixed = fix_mkdocs(content);
        assert_eq!(fixed, "- [ ] Task\n      continuation\n");
    }

    #[test]
    fn fix_task_list_mkdocs_unordered_tie_snaps_to_required() {
        // `- [ ]` MkDocs: required=4, task_col=6. Col 5 → abs_diff(5,6)=1
        // == abs_diff(5,4)=1. Tie with no siblings → required (4).
        let content = "- [ ] Task\n     continuation\n";
        let fixed = fix_mkdocs(content);
        assert_eq!(fixed, "- [ ] Task\n    continuation\n");
    }

    #[test]
    fn fix_task_list_mkdocs_ordered_overshoot_snaps_to_task_col() {
        // `1. [ ]` MkDocs: required=4, task_col=7. Col 8 → abs_diff(8,7)=1
        // < abs_diff(8,4)=4. Snap to task_col.
        let content = "1. [ ] Task\n        continuation\n";
        let fixed = fix_mkdocs(content);
        assert_eq!(fixed, "1. [ ] Task\n       continuation\n");
    }

    #[test]
    fn fix_task_list_mkdocs_ordered_near_required_snaps_to_required() {
        // `1. [ ]` MkDocs: required=4, task_col=7. Col 5 → abs_diff(5,7)=2
        // > abs_diff(5,4)=1. Snap to required (4). `1. [ ] Task\n     wrap`
        // has actual=5 which is over `required=4` so it's flagged in
        // strict mode, while in standard mode it falls under the lazy-
        // continuation window and isn't flagged at all.
        let content = "1. [ ] Task\n     continuation\n";
        let fixed = fix_mkdocs(content);
        assert_eq!(fixed, "1. [ ] Task\n    continuation\n");
    }

    #[test]
    fn fix_task_list_mkdocs_ordered_between_cols_snaps_to_task_col() {
        // `1. [ ]` MkDocs: required=4, task_col=7. Col 6 → abs_diff(6,7)=1
        // < abs_diff(6,4)=2. Snap to task_col (7).
        let content = "1. [ ] Task\n      continuation\n";
        let fixed = fix_mkdocs(content);
        assert_eq!(fixed, "1. [ ] Task\n       continuation\n");
    }

    // ── Fix idempotency (property test) ──────────────────────────────
    //
    // A fix pass on already-fixed content must produce the same content
    // — otherwise MD077 would oscillate on repeated invocations. This is
    // the core property that issue #579 was about (MD077 vs. MD013 fix
    // loop), and the integration test covers the MD013 interaction. The
    // property tests below pin the *internal* idempotency of MD077's own
    // fix, so any future change that introduces oscillation fails fast.

    fn assert_idempotent(content: &str) {
        let once = fix(content);
        let twice = fix(&once);
        assert_eq!(once, twice, "MD077 fix was not idempotent on input: {content:?}");
    }

    fn assert_idempotent_mkdocs(content: &str) {
        let once = fix_mkdocs(content);
        let twice = fix_mkdocs(&once);
        assert_eq!(
            once, twice,
            "MD077 (MkDocs) fix was not idempotent on input: {content:?}"
        );
    }

    #[test]
    fn idempotent_task_list_between_cols() {
        assert_idempotent("- [ ] Task\n    continuation\n");
    }

    #[test]
    fn idempotent_task_list_overshoot() {
        assert_idempotent("- [ ] Task\n       continuation\n");
    }

    #[test]
    fn idempotent_task_list_under_post_checkbox() {
        assert_idempotent("- [ ] Task\n   continuation\n");
    }

    #[test]
    fn idempotent_task_list_near_post_checkbox() {
        assert_idempotent("- [ ] Task\n     continuation\n");
    }

    #[test]
    fn idempotent_task_list_tab_overshoot() {
        assert_idempotent("- [ ] Task\n\t\twrap\n");
    }

    #[test]
    fn idempotent_task_list_single_tab() {
        assert_idempotent("- [ ] Task\n\twrap\n");
    }

    #[test]
    fn idempotent_task_list_ordered_overshoot() {
        assert_idempotent("1. [ ] Task\n        continuation\n");
    }

    #[test]
    fn idempotent_task_list_ordered_under() {
        assert_idempotent("1. [ ] Task\n    continuation\n");
    }

    #[test]
    fn idempotent_task_list_tie_with_sibling_at_task_col() {
        assert_idempotent("- [ ] Task\n      aligned\n    tied\n");
    }

    #[test]
    fn idempotent_task_list_tie_with_sibling_at_content_col() {
        assert_idempotent("- [ ] Task\n  aligned\n    tied\n");
    }

    #[test]
    fn idempotent_task_list_mkdocs_unordered_overshoot() {
        assert_idempotent_mkdocs("- [ ] Task\n       continuation\n");
    }

    #[test]
    fn idempotent_task_list_mkdocs_unordered_tie() {
        assert_idempotent_mkdocs("- [ ] Task\n     continuation\n");
    }

    #[test]
    fn idempotent_task_list_mkdocs_ordered_overshoot() {
        assert_idempotent_mkdocs("1. [ ] Task\n        continuation\n");
    }

    #[test]
    fn idempotent_task_list_mkdocs_ordered_between() {
        assert_idempotent_mkdocs("1. [ ] Task\n      continuation\n");
    }

    #[test]
    fn idempotent_task_list_reproducer_579() {
        // The exact reproducer from issue #579 already has correct indent
        // (col 6 = post-checkbox), so idempotency is trivially true. Pin
        // it anyway as a smoke test against future regressions.
        assert_idempotent(
            "- [ ] Lorem ipsum dolor sit amet, consectetur adipiscing\n      tempor incididunt ut labore.\n",
        );
    }

    #[test]
    fn idempotent_non_task_list_still_holds() {
        // Non-task items never enter the task_col code path; sanity-check
        // that idempotency is preserved for them too.
        assert_idempotent("1. Item\n    over-indented\n");
        assert_idempotent("- Item\n\n continuation\n");
    }

    // ── Non-task idempotency: loose-mode under-indent ────────────────
    //
    // When a blank line precedes the continuation (loose mode),
    // under-indented content is flagged and fixed up to the content
    // column. Idempotency pins that one pass of the fix is sufficient.

    #[test]
    fn idempotent_non_task_loose_under_indent_ordered() {
        // 1. Item → content col 3; "  x" is 2 spaces, under content col.
        assert_idempotent("1. Item\n\n  continuation\n");
    }

    #[test]
    fn idempotent_non_task_loose_under_indent_multi_digit() {
        // 10. Item → content col 4; single-space continuation needs 4.
        assert_idempotent("10. Item\n\n continuation\n");
    }

    #[test]
    fn idempotent_non_task_tight_over_indent_ordered() {
        // Tight-mode over-indent: 5 spaces where content col is 3.
        assert_idempotent("1. Item\n     over-indented\n");
    }

    // ── Non-task idempotency: fenced code block compound fix ─────────
    //
    // A fence opener that needs re-indenting is repaired by the
    // compound-fence fix which shifts opener + interior + closer
    // together. Idempotency pins that the compound fix settles in one
    // pass and does not oscillate between runs.

    #[test]
    fn idempotent_non_task_fence_ordered_loose() {
        // 1. Item → content col 3; fence at col 2 needs to shift to 3.
        assert_idempotent("1. Item\n\n  ```rust\n  let x = 1;\n  ```\n");
    }

    #[test]
    fn idempotent_non_task_fence_tilde_under_indent() {
        // Tilde fences use the same compound-fix path as backtick fences.
        // Interior below the list scope (col 0 here, required col 3) must
        // be promoted up in the same pass as the fence delimiters —
        // otherwise a second pass would flag the interior individually
        // and defeat idempotency.
        assert_idempotent("1. Item\n\n  ~~~\nplain text\n  ~~~\n");
    }

    #[test]
    fn idempotent_non_task_fence_interior_above_required() {
        // Interior already above the required column must not be pushed
        // further up by the compound fix — authored interior indentation
        // is preserved when it doesn't threaten fence pairing.
        assert_idempotent("1. Item\n\n  ```\n    deeply indented code\n  ```\n");
    }

    #[test]
    fn fence_fix_promotes_interior_below_scope_in_single_pass() {
        // Concrete behavioral check, not just idempotency:
        // interior at col 0 with opener at col 2, required 3, must land
        // at col 3 (same as opener) so fence pairing is preserved.
        let content = "1. Item\n\n  ```\ncode\n  ```\n";
        let fixed = fix(content);
        assert_eq!(fixed, "1. Item\n\n   ```\n   code\n   ```\n");
    }

    #[test]
    fn fence_fix_preserves_interior_above_required() {
        // Opener at col 2 → col 3 (required). Interior at col 4 stays at
        // col 4 (above required, no need to push it).
        let content = "1. Item\n\n  ```\n    code\n  ```\n";
        let fixed = fix(content);
        assert_eq!(fixed, "1. Item\n\n   ```\n    code\n   ```\n");
    }

    // ── Non-task idempotency: MkDocs strict-indent ───────────────────
    //
    // Under MkDocs flavor, continuation requires max(content_col, 4),
    // which can force a fix even when CommonMark would accept the
    // content. Pin idempotency for the non-task path there too.

    #[test]
    fn idempotent_non_task_mkdocs_ordered_at_3_spaces() {
        // CommonMark-valid (3 spaces) but MkDocs demands 4 → fix runs.
        assert_idempotent_mkdocs("1. Item\n\n   continuation\n");
    }

    #[test]
    fn idempotent_non_task_mkdocs_unordered_at_2_spaces() {
        // "- Item" → content col 2, but MkDocs raises the floor to 4.
        assert_idempotent_mkdocs("- Item\n\n  continuation\n");
    }

    #[test]
    fn idempotent_non_task_mkdocs_fence_compound() {
        // MkDocs non-task fence: opener/interior/closer shift together.
        assert_idempotent_mkdocs("1. Item\n\n   ```toml\n   k = 1\n   ```\n");
    }
}
