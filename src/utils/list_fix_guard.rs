//! Keep list fixes from changing what a document renders.
//!
//! The list indentation and spacing rules (MD005, MD007, MD029, MD030) move
//! markers and the lines they own, and MD032 inserts blank lines around lists.
//! Whether a moved line still belongs to the same item, list or paragraph
//! depends on every container around it, and no per-rule model of those
//! containers covers every shape a document can take.
//! The parser is the authority, so this guard asks it: the fixes are applied,
//! both texts are parsed, and each outermost list whose fixes change the
//! parsed structure keeps its warnings but loses their fixes. The list is the
//! unit because its fixes come from one reading of it: when that reading is
//! wrong for one line, the rest of its fixes are not to be trusted either.

use crate::lint_context::LintContext;
use crate::rule::LintWarning;
use crate::utils::fix_utils::apply_warning_fixes;
use crate::utils::parser_options::rumdl_parser_options;
use pulldown_cmark::{Event, Parser, Tag};
use std::ops::Range;

/// What a rule's fixes are allowed to change in the parsed document.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Allowed {
    /// Nothing: the fixes only move whitespace the parser discards.
    Nothing,
    /// The number an ordered list starts at, which renumbering sets.
    ListStart,
}

/// Whether a marker or diagnostic position belongs to template/MDX source code.
pub(crate) fn is_inside_literal_code(ctx: &LintContext, position: usize) -> bool {
    ctx.is_inside_template_code(position) || ctx.is_inside_mdx_code(position)
}

/// Ignore list formatting inside template/MDX code and protect literal values.
/// Template-owned continuation edits invalidate the whole atomic fix. Native
/// MDX deindents list-owned prefixes while reading expressions, so those safe
/// container moves remain eligible; its primary marker edits are protected.
pub(crate) fn protect_literal_code(ctx: &LintContext, warnings: &mut Vec<LintWarning>) {
    warnings.retain_mut(|warning| {
        let position = ctx.line_column_byte_range(warning.line, warning.column).start;
        if is_inside_literal_code(ctx, position) {
            return false;
        }
        if warning.fix.as_ref().is_some_and(|fix| {
            std::iter::once(fix).chain(&fix.additional_edits).any(|edit| {
                ctx.overlaps_template_code(edit.range.start, edit.range.end)
                    || (edit.range.is_empty() && ctx.is_inside_template_code(edit.range.start))
            }) || ctx.overlaps_mdx_code(fix.range.start, fix.range.end)
                || (fix.range.is_empty() && ctx.is_inside_mdx_code(fix.range.start))
        }) {
            warning.fix = None;
        }
        true
    });
}

/// Remove the fixes whose outermost list would parse differently once fixed.
///
/// When every fix together preserves the structure, which is the case for
/// ordinary documents, this costs two parses. Otherwise each line whose parse
/// changed points at the list responsible, those lists lose their fixes, and
/// the rest are applied and compared again, so a document with many broken
/// lists costs a parse per round rather than per list.
pub(crate) fn drop_structure_changing_fixes(ctx: &LintContext, warnings: &mut [LintWarning], allowed: Allowed) {
    let content = ctx.content;
    if warnings.iter().all(|w| w.fix.is_none()) {
        return;
    }

    // Outermost lists do not overlap, so they are ordered by start.
    let lists = outermost_lists(content);
    let list_at = |offset: usize| {
        lists
            .partition_point(|l| l.start <= offset)
            .checked_sub(1)
            .filter(|&i| offset < lists[i].end)
    };
    let group_of = |w: &LintWarning| -> Option<usize> {
        let start = w.fix.as_ref()?.range.start;
        // A fix outside every list forms a group of its own.
        Some(list_at(start).unwrap_or(lists.len() + start))
    };

    let mut groups: Vec<usize> = warnings.iter().filter_map(group_of).collect();
    groups.sort_unstable();
    groups.dedup();

    let before = Parsed::new(content, allowed);
    let fixed_with = |kept: &[usize]| -> Option<Parsed> {
        let subset: Vec<LintWarning> = warnings
            .iter()
            .filter(|w| group_of(w).is_some_and(|g| kept.binary_search(&g).is_ok()))
            .cloned()
            .collect();
        apply_warning_fixes(content, &subset)
            .ok()
            .map(|fixed| Parsed::new(&fixed, allowed))
    };
    // The line of every edit a fix makes, with the fix's group, by line. A
    // fix's additional edits count too: one can reach past its list.
    let before_ref = &before;
    let mut fix_lines: Vec<(usize, usize)> = warnings
        .iter()
        .filter_map(|w| Some((w.fix.as_ref()?, group_of(w)?)))
        .flat_map(|(fix, group)| {
            std::iter::once(fix)
                .chain(&fix.additional_edits)
                .map(move |edit| (before_ref.line_of(edit.range.start), group))
        })
        .collect();
    fix_lines.sort_unstable();
    // The group to blame for a line whose parse changed: the list around it,
    // else the one whose fix is nearest, since a moved line can change the
    // blocks on either side of it.
    let culprit = |line: usize, kept: &[usize]| -> Option<usize> {
        let is_kept = |g: &usize| kept.binary_search(g).is_ok();
        list_at(before.line_starts[line]).filter(is_kept).or_else(|| {
            let split = fix_lines.partition_point(|&(l, _)| l < line);
            let above = fix_lines[..split].iter().rev().find(|(_, g)| is_kept(g));
            let below = fix_lines[split..].iter().find(|(_, g)| is_kept(g));
            match (above, below) {
                (Some(a), Some(b)) => Some(if line - a.0 <= b.0 - line { a.1 } else { b.1 }),
                (a, b) => a.or(b).map(|&(_, g)| g),
            }
        })
    };

    let mut kept = groups;
    while let Some(after) = fixed_with(&kept) {
        if after.events == before.events {
            break;
        }
        let mut blamed: Vec<usize> = match before.changed_lines(&after) {
            Some(lines) => lines.into_iter().filter_map(|line| culprit(line, &kept)).collect(),
            None => Vec::new(),
        };
        if blamed.is_empty() {
            // The change could not be traced to a list: search for it.
            let preserves = |kept: &[usize]| fixed_with(kept).is_some_and(|after| after.events == before.events);
            kept = select_groups(&kept, &preserves);
            break;
        }
        blamed.sort_unstable();
        kept.retain(|g| blamed.binary_search(g).is_err());
    }

    for warning in warnings.iter_mut() {
        if group_of(warning).is_some_and(|g| kept.binary_search(&g).is_err()) {
            warning.fix = None;
        }
    }
}

/// The part of `groups` (sorted) found to preserve the structure, still
/// sorted: bisection finds the members that fail, and if the rest still fail
/// together, they are re-added one at a time.
fn select_groups(groups: &[usize], preserves: &impl Fn(&[usize]) -> bool) -> Vec<usize> {
    let mut kept = bisect(groups, preserves);
    // Lists that are sound one at a time can still interact once all are
    // fixed, so the survivors must also pass together.
    if kept.len() > 1 && !preserves(&kept) {
        let candidates = std::mem::take(&mut kept);
        for group in candidates {
            kept.push(group);
            if !preserves(&kept) {
                kept.pop();
            }
        }
    }
    kept
}

/// The groups of `groups` (sorted) that do not change the structure, found by
/// splitting any set that does.
fn bisect(groups: &[usize], preserves: &impl Fn(&[usize]) -> bool) -> Vec<usize> {
    if groups.is_empty() || preserves(groups) {
        return groups.to_vec();
    }
    if groups.len() == 1 {
        return Vec::new();
    }
    let (left, right) = groups.split_at(groups.len() / 2);
    let mut kept = bisect(left, preserves);
    kept.extend(bisect(right, preserves));
    kept
}

/// Byte ranges of the lists not nested in another list, in document order.
fn outermost_lists(content: &str) -> Vec<Range<usize>> {
    let mut lists = Vec::new();
    let mut depth = 0usize;
    for (event, range) in Parser::new_ext(content, rumdl_parser_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::List(_)) => {
                if depth == 0 {
                    lists.push(range);
                }
                depth += 1;
            }
            Event::End(pulldown_cmark::TagEnd::List(_)) => depth -= 1,
            _ => {}
        }
    }
    lists
}

/// A parse of a document, with enough position to say which lines changed.
struct Parsed {
    /// The events, less what `allowed` lets a fix change. Source positions are
    /// not kept: moving them is the point. Text is compared exactly: where a
    /// line's indentation lands inside a code span, renderers disagree on
    /// whether it counts, and a fix that moves it is declined rather than
    /// guessed at.
    events: Vec<Event<'static>>,
    /// For each event, the line it belongs to (an end tag belongs to the line
    /// its element ends on) and how many elements enclose it.
    places: Vec<(usize, usize)>,
    line_starts: Vec<usize>,
}

/// The line holding `offset`, given where each line starts.
fn line_at(line_starts: &[usize], offset: usize) -> usize {
    line_starts.partition_point(|&start| start <= offset) - 1
}

#[cfg(test)]
thread_local! {
    static PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Parsed {
    fn new(text: &str, allowed: Allowed) -> Self {
        #[cfg(test)]
        PARSES.with(|parses| parses.set(parses.get() + 1));
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        let mut events = Vec::new();
        let mut places = Vec::new();
        let mut depth = 0usize;
        for (event, range) in Parser::new_ext(text, rumdl_parser_options()).into_offset_iter() {
            let line = match event {
                Event::End(_) => {
                    depth -= 1;
                    line_at(&line_starts, range.end.saturating_sub(1).max(range.start))
                }
                _ => line_at(&line_starts, range.start),
            };
            places.push((line, depth));
            if matches!(event, Event::Start(_)) {
                depth += 1;
            }
            events.push(match (allowed, event.into_static()) {
                (Allowed::ListStart, Event::Start(Tag::List(Some(_)))) => Event::Start(Tag::List(Some(1))),
                (_, event) => event,
            });
        }
        Self {
            events,
            places,
            line_starts,
        }
    }

    fn line_of(&self, offset: usize) -> usize {
        line_at(&self.line_starts, offset)
    }

    /// Each line's events, with how deeply each is nested.
    fn by_line(&self) -> Vec<Vec<(&Event<'static>, usize)>> {
        let mut lines = vec![Vec::new(); self.line_starts.len()];
        for (event, &(line, depth)) in self.events.iter().zip(&self.places) {
            lines[line].push((event, depth));
        }
        lines
    }

    /// The lines whose events differ in `other`, which must have as many
    /// lines; `None` when it does not, since lines then do not correspond.
    fn changed_lines(&self, other: &Parsed) -> Option<Vec<usize>> {
        if self.line_starts.len() != other.line_starts.len() {
            return None;
        }
        let (mine, theirs) = (self.by_line(), other.by_line());
        Some((0..mine.len()).filter(|&line| mine[line] != theirs[line]).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::rule::Fix;

    fn edit(line: usize, range: Range<usize>, replacement: &str) -> LintWarning {
        LintWarning {
            message: String::new(),
            line,
            column: 1,
            end_line: line,
            end_column: 1,
            severity: crate::rule::Severity::Warning,
            fix: Some(Fix::new(range, replacement.to_string())),
            rule_name: None,
        }
    }

    fn guard(content: &str, warnings: &mut [LintWarning], allowed: Allowed) {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        drop_structure_changing_fixes(&ctx, warnings, allowed);
    }

    #[test]
    fn keeps_fixes_that_only_move_discarded_whitespace() {
        let content = "- a\n   - b\n";
        let mut warnings = vec![edit(2, 4..7, "  ")];
        guard(content, &mut warnings, Allowed::Nothing);
        assert!(warnings[0].fix.is_some());
    }

    #[test]
    fn drops_only_the_fixes_of_the_list_whose_structure_changes() {
        // Dedenting the lazy `+ item` line would open a list outside the quote.
        // The later list's fix is sound and stays.
        let content = ">   - item\n     + item\n\ntext\n\n- a\n   - b\n";
        let lazy = content.find("     + item").unwrap();
        let nested = content.rfind("   - b").unwrap();
        let mut warnings = vec![edit(2, lazy..lazy + 5, ""), edit(7, nested..nested + 3, "  ")];
        guard(content, &mut warnings, Allowed::Nothing);
        assert!(warnings[0].fix.is_none());
        assert!(warnings[1].fix.is_some());
    }

    #[test]
    fn a_list_start_changes_only_when_allowed() {
        let content = "3. a\n4. b\n";
        let renumber = || vec![edit(1, 0..1, "1"), edit(2, 5..6, "2")];

        let mut warnings = renumber();
        guard(content, &mut warnings, Allowed::ListStart);
        assert!(warnings.iter().all(|w| w.fix.is_some()));

        // Only the first number sets the start, but the list's fixes stand or
        // fall together.
        let mut warnings = renumber();
        guard(content, &mut warnings, Allowed::Nothing);
        assert!(warnings.iter().all(|w| w.fix.is_none()));
    }

    #[test]
    fn indentation_inside_a_code_span_is_compared_exactly() {
        // Lazy lines keep their indentation inside the span, so dedenting the
        // closing line narrows what renders.
        let content = ">  - item\n          ```\n          ```\n";
        let closing = content.rfind("          ```").unwrap();
        let mut warnings = vec![edit(3, closing..closing + 10, "         ")];
        guard(content, &mut warnings, Allowed::Nothing);
        assert!(warnings[0].fix.is_none());
    }

    #[test]
    fn a_change_is_blamed_on_the_nearest_fix_below_it_too() {
        // Turning `  - c` into an underline makes `text` a heading. `text` is in
        // no list, and the fix nearest to it is the one below, not the sound
        // one in the list above.
        let content = "- a\n   - b\n\ntext\n  - c\n";
        let nested = content.find("   - b").unwrap();
        let below = content.find("  - c").unwrap();
        let mut warnings = vec![edit(2, nested..nested + 3, "  "), edit(5, below..below + 5, "  ===")];
        guard(content, &mut warnings, Allowed::Nothing);
        assert!(warnings[0].fix.is_some());
        assert!(warnings[1].fix.is_none());
    }

    #[test]
    fn a_change_is_blamed_on_the_fix_that_edits_its_line() {
        // The quoted list's fix also dedents the line below the quote, which
        // turns its code block into a paragraph. That line's nearest primary
        // edit is the sound fix of the last list, but the edit on the line
        // itself belongs to the quoted list's fix.
        let content = "> - a\n>\n>       code\n        text\n *  c\n";
        let quoted = content.find("- a").unwrap();
        let inner = content.find("      code").unwrap();
        let outer = content.find("        text").unwrap();
        let last = content.find(" *  c").unwrap();
        let mut quoted_fix = edit(1, quoted..quoted, " ");
        let fix = quoted_fix.fix.as_mut().unwrap();
        fix.additional_edits = vec![
            Fix::new(inner..inner + 4, String::new()),
            Fix::new(outer..outer + 5, String::new()),
        ];
        let mut warnings = vec![quoted_fix, edit(5, last..last + 1, "")];
        guard(content, &mut warnings, Allowed::Nothing);
        assert!(warnings[0].fix.is_none());
        assert!(warnings[1].fix.is_some());
    }

    #[test]
    fn a_fix_that_adds_lines_is_found_by_search() {
        // Lines no longer correspond once one is added, so the change cannot be
        // traced by line and the lists are searched instead.
        let content = "- a\n   - b\n\ntext\n\n- c\n";
        let nested = content.find("   - b").unwrap();
        let last = content.rfind("- c").unwrap();
        let mut warnings = vec![
            edit(2, nested..nested + 3, "  "),
            edit(6, last..last + 3, "- c\n\n    code"),
        ];
        guard(content, &mut warnings, Allowed::Nothing);
        assert!(warnings[0].fix.is_some());
        assert!(warnings[1].fix.is_none());
    }

    #[test]
    fn many_broken_lists_cost_a_few_parses_not_one_per_list() {
        let broken = ">   - item\n     + item\n\n";
        let sound = "- a\n   - b\n\n";
        let content = format!("{broken}{sound}").repeat(200);
        let mut warnings = Vec::new();
        for (block, start) in content.match_indices(">   - item").map(|(i, _)| i).enumerate() {
            let lazy = start + ">   - item\n".len();
            let nested = start + broken.len() + "- a\n".len();
            warnings.push(edit(4 * block + 2, lazy..lazy + 5, ""));
            warnings.push(edit(4 * block + 4, nested..nested + 3, "  "));
        }

        PARSES.with(|parses| parses.set(0));
        guard(&content, &mut warnings, Allowed::Nothing);
        let parses = PARSES.with(std::cell::Cell::get);

        assert!(warnings.iter().step_by(2).all(|w| w.fix.is_none()));
        assert!(warnings.iter().skip(1).step_by(2).all(|w| w.fix.is_some()));
        assert!(parses <= 4, "{parses} parses");
    }

    #[test]
    fn groups_sound_alone_must_also_pass_together() {
        // 1 and 3 are each sound, 2 never is, and 1 with 3 conflict: the first
        // of the conflicting pair wins.
        let preserves = |kept: &[usize]| !(kept.contains(&2) || (kept.contains(&1) && kept.contains(&3)));
        assert_eq!(select_groups(&[1, 2, 3], &preserves), vec![1]);
        assert_eq!(select_groups(&[1, 3], &|_: &[usize]| true), vec![1, 3]);
        assert_eq!(select_groups(&[4], &|_: &[usize]| false), Vec::<usize>::new());
    }
}
