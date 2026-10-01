//! Moving the lines a list item owns when a fix moves the item's content column.
//!
//! A list item's content column decides what its following lines mean: a fence
//! indented up to three columns past it is a fence, four or more is an indented
//! code block, and a nested marker is a child only when it sits at or past it.
//! A fix that re-spaces a marker (MD030) or re-indents one (MD007) moves that
//! column, so every line the item owns has to move by the same amount, or the
//! document renders differently: a nested fence that ends up four columns past
//! the new content column turns into an indented code block holding the fence
//! markers as text.
//!
//! Several items can move one line: it moves with each item that owns it, outer
//! to inner. [`indent_shift_edits`] turns those contributions into edits that do
//! not overlap, so each one can ride on the fix of the item that caused it and
//! still compose when every fix is applied together.

use crate::lint_context::LintContext;
use crate::rule::{Fix, LintWarning};
use crate::utils::blockquote::{effective_indent_in_blockquote, parse_blockquote_prefix};

/// How a following line relates to a list item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Continuation {
    /// Part of the item (a continuation line or nested content).
    Belongs,
    /// A blank line, which neither continues nor ends the item.
    Skip,
    /// The item is over (a sibling/ancestor marker or under-indented content).
    Ends,
}

/// An open list item whose content the lines being walked may continue, kept on
/// a stack from outermost to innermost. `own` is how far the fix moves this
/// item's content column; `warning` indexes the warning whose fix carries the
/// moves of the lines the item owns, and is set whenever `own` is non-zero.
#[derive(Debug, Clone, Copy)]
pub struct OwnerFrame {
    pub marker_column: usize,
    pub bq_level: usize,
    pub min_indent: usize,
    pub own: isize,
    pub warning: Option<usize>,
}

impl OwnerFrame {
    /// The frame for the list item on `line_num` (1-based), or `None` if the line
    /// holds no list item.
    pub fn for_item(ctx: &LintContext, line_num: usize, own: isize, warning: Option<usize>) -> Option<Self> {
        let (marker_column, bq_level, min_indent) = continuation_params(ctx, line_num)?;
        Some(Self {
            marker_column,
            bq_level,
            min_indent,
            own,
            warning,
        })
    }
}

/// The marker column, blockquote nesting level, and minimum (blockquote-aware)
/// indent a following line needs to continue the list item on `line_num`
/// (1-based). `None` if the line isn't a list item. Inside a blockquote the
/// indent excludes the prefix so it stays in the coordinate system of
/// [`effective_indent_in_blockquote`].
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
) -> Continuation {
    let Some(info) = ctx.line_info(next_line_num) else {
        return Continuation::Skip;
    };
    // A deeper marker is nested content; one at the same or a shallower column
    // ends the item.
    if let Some(next_list) = &info.list_item {
        return if next_list.marker_column <= marker_column {
            Continuation::Ends
        } else {
            Continuation::Belongs
        };
    }
    let content = lines.get(next_line_num - 1).copied().unwrap_or("");
    // Blank lines don't decide on their own, and inside the item's blockquote a
    // bare `>` is a blank line.
    let blank_in_blockquote = bq_level > 0
        && info
            .blockquote
            .as_ref()
            .is_some_and(|bq| bq.nesting_level == bq_level && bq.content.trim().is_empty());
    if content.trim().is_empty() || blank_in_blockquote {
        return Continuation::Skip;
    }
    let raw_indent = content.len() - content.trim_start().len();
    if effective_indent_in_blockquote(content, bq_level, raw_indent) < min_indent {
        Continuation::Ends
    } else {
        Continuation::Belongs
    }
}

/// Pop the items the line at `line_num` (1-based) ends. A lazy continuation
/// line, one that falls short of the innermost item's content column yet still
/// continues its open paragraph, ends nothing. Returns whether the line is such
/// a lazy continuation, which also means it does not move with the items.
pub fn close_ended_items(ctx: &LintContext, line_num: usize, lines: &[&str], stack: &mut Vec<OwnerFrame>) -> bool {
    let mut innermost = true;
    while let Some(frame) = stack.last() {
        let continuation = classify_continuation(
            ctx,
            line_num,
            lines,
            frame.marker_column,
            frame.bq_level,
            frame.min_indent,
        );
        if continuation != Continuation::Ends {
            break;
        }
        if innermost && is_lazy_continuation(ctx, line_num - 1) {
            return true;
        }
        innermost = false;
        stack.pop();
    }
    false
}

/// What keeps a line that ends list items outside them once the fix moves them.
#[derive(Debug, Clone, Copy)]
pub enum EndedItems {
    /// The line still falls short of every item it ends.
    StayClosed,
    /// The line would reach an item's narrowed content column and join it, so it
    /// has to move by this extra frame's `own` as well.
    ShiftOut(OwnerFrame),
    /// The line would join an item and cannot be moved on its own: it opens a
    /// code or math block whose content its indentation positions.
    Unfixable,
}

/// How far a fix moves the content column of each item on `stack`, outermost
/// first. With [`Nesting::Absolute`] a rule places every marker itself, so an
/// item's column moves by its own amount; with [`Nesting::Relative`] it also
/// moves with every enclosing item.
fn content_column_shifts(stack: &[OwnerFrame], nesting: Nesting) -> Vec<isize> {
    let mut total = 0;
    stack
        .iter()
        .map(|frame| match nesting {
            Nesting::Absolute => frame.own,
            Nesting::Relative => {
                total += frame.own;
                total
            }
        })
        .collect()
}

/// Check the line at `line_idx` (0-based), which ends the items `before[kept..]`
/// and stays in `before[..kept]`, against the columns the fix moves those items
/// to. A line ends an item by falling short of its content column, so when the
/// fix narrows that column far enough the line lands inside the item instead: a
/// paragraph that followed a nested list at its parent's indentation would
/// become part of the nested list's last item. Such a line moves to the content
/// column of the item that keeps it: it is not code, so indentation short of
/// four columns past that column carries no meaning, and the narrowed item's
/// marker sits at or past it.
pub fn keep_ended_items_closed(
    ctx: &LintContext,
    line_idx: usize,
    before: &[OwnerFrame],
    kept: usize,
    nesting: Nesting,
) -> EndedItems {
    let Some(info) = ctx.lines.get(line_idx) else {
        return EndedItems::StayClosed;
    };
    if kept >= before.len() || is_marker_line(ctx, line_idx) {
        return EndedItems::StayClosed;
    }
    let lines = ctx.raw_lines();
    let Some(&content) = lines.get(line_idx) else {
        return EndedItems::StayClosed;
    };
    let bq_level = info.blockquote.as_ref().map_or(0, |bq| bq.nesting_level);
    let shifts = content_column_shifts(before, nesting);
    let raw_indent = content.len() - content.trim_start().len();
    let line_shift = kept.checked_sub(1).map_or(0, |owner| shifts[owner]);
    let indent = effective_indent_in_blockquote(content, bq_level, raw_indent) as isize + line_shift;

    // An item the line ends by leaving its blockquote stays ended whatever the
    // indentation, so only items in the line's own blockquote count.
    let Some((captor, limit)) = (kept..before.len())
        .filter(|&i| before[i].bq_level == bq_level)
        .map(|i| (i, before[i].min_indent as isize + shifts[i]))
        .min_by_key(|&(_, column)| column)
    else {
        return EndedItems::StayClosed;
    };
    if indent < limit {
        return EndedItems::StayClosed;
    }
    if info.in_code_block || info.in_math_block {
        return EndedItems::Unfixable;
    }
    let floor = kept
        .checked_sub(1)
        .map_or(0, |owner| before[owner].min_indent as isize + shifts[owner]);
    let Some(warning) = before[..=captor].iter().rev().find_map(|frame| frame.warning) else {
        return EndedItems::Unfixable;
    };
    if floor >= limit {
        return EndedItems::Unfixable;
    }
    EndedItems::ShiftOut(OwnerFrame {
        marker_column: 0,
        bq_level,
        min_indent: 0,
        own: floor - indent,
        warning: Some(warning),
    })
}

/// Byte offset on `line` where an item's shiftable indent begins: column 0 when
/// the item is at top level, or just past the blockquote prefix when it sits
/// inside a blockquote (its indent lives after the `>` markers).
fn write_offset(bq_level: usize, line: &str) -> usize {
    match bq_level {
        0 => 0,
        _ => parse_blockquote_prefix(line).map_or(0, |p| p.prefix.len()),
    }
}

/// Move the owned `line` (0-based `line_idx`) with every item in `owners` whose
/// content column moves, attaching each item's edit to that item's own warning.
/// Each item's move applies within its own coordinate system: past the
/// blockquote prefix when the item sits inside a blockquote.
pub fn move_owned_line(
    ctx: &LintContext,
    line: &str,
    line_idx: usize,
    owners: &[OwnerFrame],
    warnings: &mut [LintWarning],
) {
    if owners.iter().all(|frame| frame.own == 0) {
        return;
    }
    let line_start = ctx.line_offsets.get(line_idx).copied().unwrap_or(0);
    let offsets: Vec<usize> = owners.iter().map(|f| write_offset(f.bq_level, line)).collect();
    let mut done: Vec<usize> = Vec::new();
    for &offset in &offsets {
        if done.contains(&offset) {
            continue;
        }
        done.push(offset);
        let contributions: Vec<isize> = owners
            .iter()
            .zip(&offsets)
            .map(|(frame, &o)| if o == offset { frame.own } else { 0 })
            .collect();
        let edits = indent_shift_edits(line_start, line, offset, &contributions);
        for (frame, edit) in owners.iter().zip(edits) {
            if let (Some(edit), Some(index)) = (edit, frame.warning)
                && let Some(fix) = warnings.get_mut(index).and_then(|w| w.fix.as_mut())
            {
                fix.additional_edits.push(edit);
            }
        }
    }
}

/// The edits that shift the indentation of `line` by each of `contributions` in
/// turn. `line_start` is the line's byte offset in the document and `offset`
/// the byte offset within the line where its shiftable indentation begins (past
/// any blockquote prefix).
///
/// Returns one entry per contribution, `None` where it moves nothing. A negative
/// contribution deletes its own slice of the leading spaces, counted from the
/// front, so the deletions of different contributions never touch the same
/// bytes; a deletion never removes more than the spaces actually present, and
/// stops at a tab. A positive contribution inserts spaces at its own position,
/// counted back from the end of the leading spaces and never inside a deletion:
/// two identical inserts at one position would be merged into one when the
/// fixes are applied together.
pub fn indent_shift_edits(line_start: usize, line: &str, offset: usize, contributions: &[isize]) -> Vec<Option<Fix>> {
    let Some(rest) = line.get(offset..) else {
        return vec![None; contributions.len()];
    };
    let available = rest.len() - rest.trim_start_matches(' ').len();
    let at = line_start + offset;
    let mut consumed = 0;
    let mut edits: Vec<Option<Fix>> = contributions
        .iter()
        .map(|&delta| {
            if delta >= 0 {
                return None;
            }
            let take = delta.unsigned_abs().min(available - consumed);
            if take == 0 {
                return None;
            }
            let start = at + consumed;
            consumed += take;
            Some(Fix::new(start..start + take, String::new()))
        })
        .collect();
    let mut inserted = 0;
    for (edit, &delta) in edits.iter_mut().zip(contributions) {
        if delta > 0 {
            let position = at + available.saturating_sub(inserted).max(consumed);
            inserted += 1;
            *edit = Some(Fix::new(position..position, " ".repeat(delta as usize)));
        }
    }
    edits
}

/// Whether the 0-based line `line_idx` continues the paragraph on the line
/// right above it. CommonMark keeps such a line in the open paragraph whatever
/// its indentation, so when it falls short of the item's content column (a lazy
/// continuation) it neither ends the list item nor needs to move with it.
pub fn is_lazy_continuation(ctx: &LintContext, line_idx: usize) -> bool {
    let Some(prev_idx) = line_idx.checked_sub(1) else {
        return false;
    };
    let (Some(prev), Some(cur)) = (ctx.lines.get(prev_idx), ctx.lines.get(line_idx)) else {
        return false;
    };
    let bq_depth = |info: &crate::lint_context::LineInfo| info.blockquote.as_ref().map_or(0, |bq| bq.nesting_level);
    let opens_block = |info: &crate::lint_context::LineInfo| {
        info.list_item.is_some()
            || info.in_code_block
            || info.heading.is_some()
            || info.is_horizontal_rule
            || info.in_html_block
            || info.in_html_comment
            || info.is_div_marker
    };
    // The line above must be open paragraph text, which a list item's own marker
    // line is when it carries text after the marker. A task checkbox alone is
    // not text, the item holds no paragraph yet. A line of an HTML block is
    // HTML, also where the block opens on a marker line, and only a paragraph
    // continues lazily.
    let prev_is_paragraph = !ctx.line_holds_html_block(prev_idx)
        && match &prev.list_item {
            Some(item) => {
                !prev.in_code_block
                    && prev
                        .content(ctx.content)
                        .get(item.content_column..)
                        .map(str::trim)
                        .is_some_and(|text| !text.is_empty() && !matches!(text, "[ ]" | "[x]" | "[X]"))
            }
            None => !opens_block(prev),
        };
    !prev.is_blank && !cur.is_blank && bq_depth(prev) == bq_depth(cur) && !opens_block(cur) && prev_is_paragraph
}

/// How a rule positions nested list markers, which decides whether they move
/// with their parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nesting {
    /// Nested markers keep their place relative to the parent's content, so they
    /// move with every enclosing item that moves (MD029 renumbering).
    Relative,
    /// The rule places every marker at an absolute column through its own
    /// warning, so nested marker lines are left alone and any other line moves
    /// with its innermost owning item only (MD005, MD007).
    Absolute,
}

/// Whether the line at `line_idx` (0-based) is a list item's marker line. A
/// fence can open on the marker line (`- ```js`), which the parser flags as
/// code, but the line is still the item's.
fn is_marker_line(ctx: &LintContext, line_idx: usize) -> bool {
    let Some(info) = ctx.lines.get(line_idx) else {
        return false;
    };
    let Some(item) = &info.list_item else {
        return false;
    };
    !info.in_code_block
        || info
            .content(ctx.content)
            .get(item.content_column..)
            .is_some_and(|rest| rest.starts_with("```") || rest.starts_with("~~~"))
}

/// Move every line a list item owns along with that item's content column.
/// `moves` maps the 0-based line of each item whose content column the rule's
/// fix moves to the index of the warning carrying that fix and the number of
/// columns it moves; each line's edit is attached to that warning's fix, so a
/// single quick fix is correct on its own and all of them compose.
pub fn move_owned_lines(
    ctx: &LintContext,
    moves: &std::collections::HashMap<usize, (usize, isize)>,
    nesting: Nesting,
    warnings: &mut [LintWarning],
) {
    if moves.values().all(|&(_, delta)| delta == 0) {
        return;
    }
    let lines = ctx.raw_lines();
    let mut stack: Vec<OwnerFrame> = Vec::new();
    // Where the outermost list being walked starts, and whether a move in it
    // cannot be made. Its items shift only against each other, so a list that
    // cannot be fixed leaves the lists around it fixable.
    let mut list_start = 0;
    let mut list_unfixable = false;
    for (line_idx, line) in lines.iter().enumerate() {
        let before = stack.clone();
        let lazy = close_ended_items(ctx, line_idx + 1, lines, &mut stack);
        if lazy || line.trim().is_empty() {
            continue;
        }
        let marker = is_marker_line(ctx, line_idx);
        if marker && stack.is_empty() {
            if std::mem::take(&mut list_unfixable) {
                decline_fixes(list_start..line_idx, moves, warnings);
            }
            list_start = line_idx;
        }
        let mut owners = match (marker, nesting) {
            (true, Nesting::Absolute) => Vec::new(),
            (false, Nesting::Absolute) => stack[stack.len().saturating_sub(1)..].to_vec(),
            (_, Nesting::Relative) => stack.clone(),
        };
        match keep_ended_items_closed(ctx, line_idx, &before, stack.len(), nesting) {
            EndedItems::StayClosed => {}
            EndedItems::ShiftOut(frame) => owners.push(frame),
            EndedItems::Unfixable => list_unfixable = true,
        }
        move_owned_line(ctx, line, line_idx, &owners, warnings);
        if marker {
            let (warning, own) = moves.get(&line_idx).map_or((None, 0), |&(w, d)| (Some(w), d));
            stack.extend(OwnerFrame::for_item(ctx, line_idx + 1, own, warning));
        }
    }
    if list_unfixable {
        decline_fixes(list_start..lines.len(), moves, warnings);
    }
}

/// Drop the fix of every warning that moves an item on the `list` lines.
/// Moving only some of a list's items would shift them relative to the ones
/// left alone and change the nesting, so when one move cannot be made safely
/// the list's warnings stay unfixed together.
fn decline_fixes(
    list: std::ops::Range<usize>,
    moves: &std::collections::HashMap<usize, (usize, isize)>,
    warnings: &mut [LintWarning],
) {
    for (line_idx, &(index, _)) in moves {
        if list.contains(line_idx)
            && let Some(warning) = warnings.get_mut(index)
        {
            warning.fix = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(line: &str, edits: &[Option<Fix>]) -> String {
        let mut edits: Vec<&Fix> = edits.iter().flatten().collect();
        edits.sort_by_key(|f| std::cmp::Reverse((f.range.start, f.range.end)));
        let mut out = line.to_string();
        for fix in edits {
            out.replace_range(fix.range.clone(), &fix.replacement);
        }
        out
    }

    #[test]
    fn narrowing_contributions_delete_disjoint_slices() {
        let line = "        text";
        let edits = indent_shift_edits(0, line, 0, &[-2, -2]);
        let ranges: Vec<_> = edits.iter().map(|e| e.as_ref().unwrap().range.clone()).collect();
        assert_eq!(ranges, vec![0..2, 2..4]);
        assert_eq!(apply(line, &edits), "    text");
    }

    #[test]
    fn each_narrowing_contribution_is_correct_alone() {
        let line = "        text";
        let edits = indent_shift_edits(0, line, 0, &[-2, -2]);
        assert_eq!(apply(line, &edits[..1]), "      text");
        assert_eq!(apply(line, &edits[1..]), "      text");
    }

    #[test]
    fn widening_and_narrowing_compose() {
        let line = "    text";
        let edits = indent_shift_edits(0, line, 0, &[3, -2]);
        assert_eq!(apply(line, &edits), "     text");
    }

    #[test]
    fn deletion_never_exceeds_the_leading_spaces() {
        let edits = indent_shift_edits(0, "  text", 0, &[-2, -2]);
        assert!(edits[0].is_some());
        assert!(edits[1].is_none());
    }

    #[test]
    fn deletion_stops_at_a_tab() {
        let edits = indent_shift_edits(0, " \ttext", 0, &[-2]);
        assert_eq!(edits[0].as_ref().unwrap().range, 0..1);
    }

    #[test]
    fn edits_start_after_the_offset() {
        let line = ">     text";
        let edits = indent_shift_edits(10, line, 2, &[-2]);
        assert_eq!(edits[0].as_ref().unwrap().range, 12..14);
        let edits = indent_shift_edits(10, line, 2, &[1]);
        assert_eq!(edits[0].as_ref().unwrap().range, 16..16);
    }

    #[test]
    fn equal_widening_contributions_survive_applying_every_fix_together() {
        // Two owners widening one line by the same amount must not produce two
        // identical edits, which applying the fixes together merges into one.
        let content = "1. a\n   1. b\n      z\n";
        let line_start = content.find("      z").unwrap();
        let edits = indent_shift_edits(line_start, "      z", 0, &[1, 1]);
        let warnings: Vec<LintWarning> = edits
            .into_iter()
            .map(|fix| LintWarning {
                message: String::new(),
                line: 3,
                column: 1,
                end_line: 3,
                end_column: 1,
                severity: crate::rule::Severity::Warning,
                fix,
                rule_name: None,
            })
            .collect();
        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(fixed, "1. a\n   1. b\n        z\n");
        for warning in &warnings {
            let alone = crate::utils::fix_utils::apply_warning_fixes(content, std::slice::from_ref(warning)).unwrap();
            assert_eq!(alone, "1. a\n   1. b\n       z\n");
        }
    }

    #[test]
    fn inserts_never_land_inside_a_deletion() {
        let edits = indent_shift_edits(0, "   text", 0, &[-2, 1]);
        let ranges: Vec<_> = edits.iter().map(|e| e.as_ref().unwrap().range.clone()).collect();
        assert_eq!(ranges, vec![0..2, 3..3]);
        assert_eq!(apply("   text", &edits), "  text");
    }

    #[test]
    fn lazy_continuation_is_paragraph_text_right_after_the_item() {
        let content = "*   item\nlazy\n\nnot lazy\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(is_lazy_continuation(&ctx, 1));
        assert!(!is_lazy_continuation(&ctx, 3));
    }

    #[test]
    fn a_line_after_an_item_html_block_is_not_a_lazy_continuation() {
        let content = "- <div>\n  b\n}\n- a\n  b\n}\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(!is_lazy_continuation(&ctx, 2));
        assert!(is_lazy_continuation(&ctx, 5));
    }

    #[test]
    fn a_line_after_a_lone_task_checkbox_is_not_a_lazy_continuation() {
        let content = "- [ ]\nfoo\n- [x]  \nfoo\n- [ ] a\nfoo\n- [-]\nfoo\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(!is_lazy_continuation(&ctx, 1));
        assert!(!is_lazy_continuation(&ctx, 3));
        assert!(is_lazy_continuation(&ctx, 5));
        assert!(is_lazy_continuation(&ctx, 7));
    }

    #[test]
    fn a_block_start_is_not_a_lazy_continuation() {
        let content = "*   item\n# heading\n*   next\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(!is_lazy_continuation(&ctx, 1));
        assert!(!is_lazy_continuation(&ctx, 2));
    }
}
