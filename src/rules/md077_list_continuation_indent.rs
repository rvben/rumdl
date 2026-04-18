//!
//! Rule MD077: List continuation content indentation
//!
//! See [docs/md077.md](../../docs/md077.md) for full documentation, configuration, and examples.

use crate::lint_context::LintContext;
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
    /// through its matching closer, shifting every non-blank line's leading
    /// whitespace by the same visual delta. Preserves relative indentation of
    /// code content inside the block so the parser keeps pairing the fences.
    ///
    /// Leading tabs are normalized to spaces: CommonMark expands a tab to the
    /// next column that's a multiple of 4, so simply prepending spaces before
    /// a tab would let the tab snap back and cancel the shift. We replace the
    /// whole leading-whitespace byte range with `(visual_indent + delta)`
    /// spaces instead.
    ///
    /// This prevents other rules (notably MD031) from seeing a transiently
    /// broken fence pair between iterations of the fix loop (see issue #574).
    fn build_compound_fence_fix(
        ctx: &LintContext,
        opener_line: usize,
        closer_line: usize,
        opener_actual: usize,
        required: usize,
    ) -> Option<Fix> {
        let opener_info = ctx.line_info(opener_line)?;
        let closer_info = ctx.line_info(closer_line)?;
        let delta = required.saturating_sub(opener_actual);
        if delta == 0 {
            return None;
        }

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
                let new_visual = info.visual_indent + delta;
                for _ in 0..new_visual {
                    replacement.push(' ');
                }
                replacement.push_str(&line[info.indent..]);
            }
        }

        Some(Fix {
            range: fix_start..fix_end,
            replacement,
        })
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

        // Collect all list item lines sorted, with their content_column and marker_column.
        // We need this to compute owned ranges that extend past block.end_line
        // (the parser excludes under-indented continuation from the block).
        let mut items: Vec<(usize, usize, usize)> = Vec::new(); // (line_num, marker_col, content_col)
        for block in &ctx.list_blocks {
            for &item_line in &block.item_lines {
                if let Some(info) = ctx.line_info(item_line)
                    && let Some(ref li) = info.list_item
                {
                    items.push((item_line, li.marker_column, li.content_column));
                }
            }
        }
        items.sort_unstable();
        items.dedup_by_key(|&mut (ln, _, _)| ln);

        for (item_idx, &(item_line, marker_col, content_col)) in items.iter().enumerate() {
            let required = if strict_indent { content_col.max(4) } else { content_col };

            // Owned range ends at the line before the next sibling-or-higher
            // item, or end of document.
            let range_end = items
                .iter()
                .skip(item_idx + 1)
                .find(|&&(_, mc, _)| mc <= marker_col)
                .map_or(total_lines, |&(ln, _, _)| ln - 1);

            let mut saw_blank = false;
            // Track nested child items so we don't check their continuation
            // lines against the parent's content column.
            let mut nested_content_col: Option<usize> = None;

            for line_num in (item_line + 1)..=range_end {
                let Some(line_info) = ctx.line_info(line_num) else {
                    continue;
                };

                let trimmed = line_info.content(ctx.content).trim_start();

                if Self::should_skip_line(line_info, trimmed) {
                    continue;
                }

                if line_info.is_blank {
                    saw_blank = true;
                    continue;
                }

                // Nested list items are not continuation content
                if let Some(ref li) = line_info.list_item {
                    if li.marker_column > marker_col {
                        nested_content_col = Some(li.content_column);
                    } else {
                        nested_content_col = None;
                    }
                    saw_blank = false;
                    continue;
                }

                // Skip headings - they clearly aren't list continuation
                if line_info.heading.is_some() {
                    break;
                }

                // Skip horizontal rules
                if line_info.is_horizontal_rule {
                    break;
                }

                // Skip block-level constructs that aren't list continuation:
                // reference definitions, footnote definitions, abbreviation definitions
                if Self::is_block_level_construct(trimmed) {
                    continue;
                }

                let actual = line_info.visual_indent;

                // Lines belonging to a nested item's scope are handled by
                // that item's own iteration — skip them here.
                if let Some(ncc) = nested_content_col {
                    if actual >= ncc {
                        continue;
                    }
                    nested_content_col = None;
                }

                // Tight continuation (no blank line): flag over-indented lines
                if !saw_blank {
                    if actual > required && !Self::starts_with_list_marker(trimmed) && flagged_lines.insert(line_num) {
                        let line_content = line_info.content(ctx.content);
                        let fix_start = line_info.byte_offset;
                        let fix_end = fix_start + line_info.indent;

                        warnings.push(LintWarning {
                            rule_name: Some("MD077".to_string()),
                            line: line_num,
                            column: 1,
                            end_line: line_num,
                            end_column: line_content.len() + 1,
                            message: format!("Continuation line over-indented (expected {required}, found {actual})",),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: fix_start..fix_end,
                                replacement: " ".repeat(required),
                            }),
                        });
                    }
                    continue;
                }

                // Content at or below the marker column is not continuation —
                // it starts a new paragraph (top-level) or belongs to a
                // parent item (nested).
                if actual <= marker_col {
                    break;
                }

                if actual < required && flagged_lines.insert(line_num) {
                    let line_content = line_info.content(ctx.content);

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

                    // If this is the opener of a fenced code block, emit a
                    // compound fix that reindents the whole block (opener +
                    // interior + closer). Fixing only the fence lines while
                    // leaving interior content at the old indent creates a
                    // transiently-broken fence pair that MD031 (which runs in
                    // the same iterative-fix loop) misreads as orphan fences
                    // and "repairs" by inserting stray blank lines.
                    let is_fence_opener = line_info.in_code_block
                        && Self::is_code_fence(trimmed)
                        && ctx.line_info(line_num - 1).is_none_or(|p| !p.in_code_block);

                    let (fix, warn_end_line, warn_end_column) = if is_fence_opener {
                        let closer_line = Self::find_fence_closer(ctx, line_num);
                        if closer_line != line_num {
                            flagged_lines.insert(closer_line);
                        }
                        let fix = Self::build_compound_fence_fix(ctx, line_num, closer_line, actual, required);
                        let end_line = closer_line;
                        let end_column = ctx
                            .line_info(closer_line)
                            .map_or(line_content.len() + 1, |ci| ci.content(ctx.content).len() + 1);
                        (fix, end_line, end_column)
                    } else {
                        let fix_start = line_info.byte_offset;
                        let fix_end = fix_start + line_info.indent;
                        let fix = Some(Fix {
                            range: fix_start..fix_end,
                            replacement: " ".repeat(required),
                        });
                        (fix, line_num, line_content.len() + 1)
                    };

                    warnings.push(LintWarning {
                        rule_name: Some("MD077".to_string()),
                        line: line_num,
                        column: 1,
                        end_line: warn_end_line,
                        end_column: warn_end_column,
                        message,
                        severity: Severity::Warning,
                        fix,
                    });
                }

                // Intentionally keep `saw_blank = true` while scanning this
                // owned item range so that *all* lines in a loose continuation
                // paragraph are validated/fixed, not just the first line after
                // the blank.
            }
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
}
