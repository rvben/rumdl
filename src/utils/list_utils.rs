//! Shared list-structure analysis used by the list rules.
//!
//! These helpers answer "what content belongs to a given list item" in one
//! place so that rules which need it — MD030 (spaces after markers) and MD013
//! (line length / reflow) — agree exactly instead of maintaining parallel
//! implementations that can drift out of sync.
//!
//! All offsets come from the pre-parsed [`crate::lint_context::LintContext`]
//! (`ListItemInfo`/`BlockquoteInfo`), and blockquote-relative indentation is
//! measured with [`effective_indent_in_blockquote`] so the thresholds match the
//! coordinate system the rest of the code uses.

use crate::lint_context::LintContext;
use crate::utils::blockquote::effective_indent_in_blockquote;

/// How a following line relates to the list item being scanned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListContinuation {
    /// Part of the item (a continuation line or nested content).
    Belongs,
    /// A blank line, which neither continues nor ends the item.
    Skip,
    /// The item is over (a sibling/ancestor marker or under-indented content).
    Ends,
}

/// The marker column, blockquote nesting level, and minimum (blockquote-aware)
/// indent a following line needs to continue the list item on `line_num`
/// (1-based). These are the inputs shared by every continuation scan. `None` if
/// the line isn't a list item. Inside a blockquote the indent excludes the
/// prefix so it stays in the coordinate system of [`effective_indent_in_blockquote`].
pub fn continuation_params(ctx: &LintContext, line_num: usize) -> Option<(usize, usize, usize)> {
    let info = ctx.line_info(line_num)?;
    let list = info.list_item.as_ref()?;
    let (bq_level, min_indent) = match &info.blockquote {
        Some(bq) if bq.nesting_level > 0 => (bq.nesting_level, list.content_column.saturating_sub(bq.prefix.len())),
        _ => (0, list.content_column),
    };
    Some((list.marker_column, bq_level, min_indent))
}

/// Classify the line at `next_line_num` (1-based) relative to a list item whose
/// marker is at `marker_column` with continuation threshold (`bq_level`,
/// `min_indent`). The single source of truth for what belongs to a list item.
pub fn classify_continuation(
    ctx: &LintContext,
    next_line_num: usize,
    lines: &[&str],
    marker_column: usize,
    bq_level: usize,
    min_indent: usize,
) -> ListContinuation {
    let Some(info) = ctx.line_info(next_line_num) else {
        return ListContinuation::Skip;
    };
    // A deeper marker is nested content; one at the same or a shallower column
    // ends the item.
    if let Some(next_list) = &info.list_item {
        return if next_list.marker_column <= marker_column {
            ListContinuation::Ends
        } else {
            ListContinuation::Belongs
        };
    }
    let content = lines.get(next_line_num - 1).copied().unwrap_or("");
    if content.trim().is_empty() {
        return ListContinuation::Skip; // Blank lines don't decide on their own.
    }
    let raw_indent = content.len() - content.trim_start().len();
    if effective_indent_in_blockquote(content, bq_level, raw_indent) < min_indent {
        ListContinuation::Ends
    } else {
        ListContinuation::Belongs
    }
}

/// Whether any line after `line_num` (1-based) belongs to an item with the given
/// continuation threshold, scanning until the item ends. Shared by the multi-line
/// check and MD030's inline-bullet check.
pub fn has_continuation(
    ctx: &LintContext,
    line_num: usize,
    lines: &[&str],
    marker_column: usize,
    bq_level: usize,
    min_indent: usize,
) -> bool {
    for next in (line_num + 1)..=lines.len() {
        match classify_continuation(ctx, next, lines, marker_column, bq_level, min_indent) {
            ListContinuation::Belongs => return true,
            ListContinuation::Ends => break,
            ListContinuation::Skip => {}
        }
    }
    false
}

/// Whether the list item on `line_num` (1-based) spans multiple lines (has
/// continuation or nested content). `false` when the line is not a list item.
pub fn is_multi_line_list_item(ctx: &LintContext, line_num: usize, lines: &[&str]) -> bool {
    let Some((marker_column, bq_level, min_indent)) = continuation_params(ctx, line_num) else {
        return false;
    };
    has_continuation(ctx, line_num, lines, marker_column, bq_level, min_indent)
}
