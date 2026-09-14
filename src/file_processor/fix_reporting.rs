//! Deciding which of a document's warnings the fix pass actually resolved.
//!
//! The fix pass rewrites a whole document and the result is re-linted, so nothing
//! links a pre-fix warning to its post-fix self. Four properties of that gap
//! shape the reconciliation:
//!
//! - A rule can resolve a violation without attaching a `Fix` to the warning.
//!   MD046 rewrites the document from `Rule::fix()` and reports `fix: None`, so
//!   "carries a fix" answers whether an editor can offer a code action, not
//!   whether the CLI resolved anything. One rule's fix also resolves other rules'
//!   findings - removing trailing spaces shortens the line MD013 was reporting -
//!   which no record of which rules fixed something can predict.
//! - A fix that changes the line count moves every warning below it, so a
//!   survivor sits somewhere else afterwards. Matching on the reported line calls
//!   a warning that merely moved "fixed".
//! - What a warning says is not stable either. A message quoting a length or a
//!   line number is rewritten when a fix changes either, so a survivor can read
//!   as a disappearance.
//! - Two warnings can say exactly the same thing, one resolved and one not. The
//!   report names the resolved one by its position, so a count of survivors is
//!   not enough to say which.
//!
//! So the re-lint decides, and it is read three ways. How many findings a rule
//! lost bounds how many of its warnings can be credited, which is what an
//! unstable message cannot inflate. A warning on a line the fix pass left
//! untouched is the same finding as a survivor saying the same thing at that
//! line's new position, which is what tells identical warnings apart. What each
//! remaining warning says picks among the rest, which is what keeps a warning
//! that merely moved out of the count. Between them, a rule's share of the report
//! is always `max(before, after)` entries: whatever a fix run did, every finding
//! the file had is either reported as fixed or still there.

use std::collections::HashMap;

use rumdl_lib::rule::{LintWarning, Severity};

/// What a warning says, independent of where it sits.
///
/// Message text is the closest thing a rule offers to a stable identity, and it
/// is not a contract: a rule that names a position in its message (MD053 reports
/// the line of the definition it conflicts with) reads as a different warning
/// once that line moves. That is why identity only ever picks *which* warnings to
/// credit, never how many.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct WarningIdentity<'a> {
    rule_name: Option<&'a str>,
    message: &'a str,
    severity: Severity,
}

impl<'a> WarningIdentity<'a> {
    fn of(warning: &'a LintWarning) -> Self {
        Self {
            rule_name: warning.rule_name.as_deref(),
            message: &warning.message,
            severity: warning.severity,
        }
    }
}

/// Where each line the fix pass left untouched sits in the document it produced.
///
/// Lines are paired by aligning the two documents the way a diff aligns them, so
/// a line keeps its partner however many lines were inserted or removed above
/// it. A line the pass rewrote has no partner.
struct KeptLines {
    /// Pre-fix line number to post-fix line number, both 1-based, or `None` when
    /// the pass changed nothing and every line is where it was.
    moved: Option<HashMap<usize, usize>>,
}

impl KeptLines {
    fn between(original_content: &str, fixed_content: &str) -> Self {
        if original_content == fixed_content {
            return Self { moved: None };
        }
        let old: Vec<&str> = original_content.split_inclusive('\n').collect();
        let new: Vec<&str> = fixed_content.split_inclusive('\n').collect();
        let mut moved = HashMap::new();
        for op in similar::TextDiff::configure().diff_slices(&old, &new).ops() {
            if let similar::DiffOp::Equal {
                old_index,
                new_index,
                len,
            } = *op
            {
                moved.extend((1..=len).map(|offset| (old_index + offset, new_index + offset)));
            }
        }
        Self { moved: Some(moved) }
    }

    /// The post-fix line number of pre-fix `line`, if the pass left it untouched.
    fn now_at(&self, line: usize) -> Option<usize> {
        match &self.moved {
            None => Some(line),
            Some(moved) => moved.get(&line).copied(),
        }
    }
}

/// Which of a document's pre-fix warnings the fix pass resolved.
pub struct FixReconciliation {
    fixed: Vec<bool>,
}

impl FixReconciliation {
    /// Whether each pre-fix warning was resolved, in the order it was reported.
    pub fn per_warning(&self) -> &[bool] {
        &self.fixed
    }

    /// How many pre-fix warnings the fix pass resolved.
    pub fn fixed_count(&self) -> usize {
        self.fixed.iter().filter(|&&was_fixed| was_fixed).count()
    }

    /// The pre-fix warnings the fix pass left, in the order they were reported.
    ///
    /// `all_warnings` is the list this reconciliation was built from. Each warning
    /// keeps its position in the document the fix pass started from and loses its
    /// fix: the pass left it in place, so marking it fixable would promise a fix
    /// run resolves it.
    pub fn unfixed(&self, all_warnings: &[LintWarning]) -> Vec<LintWarning> {
        all_warnings
            .iter()
            .zip(&self.fixed)
            .filter(|&(_, &was_fixed)| !was_fixed)
            .map(|(warning, _)| LintWarning {
                fix: None,
                ..warning.clone()
            })
            .collect()
    }
}

/// Reconcile a document's pre-fix warnings against the ones that survived.
///
/// `remaining_warnings` has to come from linting the fixed document with the same
/// rules that produced `all_warnings`, or a rule missing from one side reads as a
/// document whose findings all disappeared. `original_content` is the document
/// `all_warnings` was reported against and `fixed_content` the one
/// `remaining_warnings` was, both with the line endings the lint read.
pub fn reconcile_fixed_warnings(
    all_warnings: &[LintWarning],
    remaining_warnings: &[LintWarning],
    original_content: &str,
    fixed_content: &str,
) -> FixReconciliation {
    // How many findings each rule lost. A rule that reported four and still
    // reports one resolved three of them, whichever three they were, and a rule
    // that gained findings resolved none.
    let mut net_resolved: HashMap<Option<&str>, usize> = HashMap::new();
    for warning in all_warnings {
        *net_resolved.entry(warning.rule_name.as_deref()).or_insert(0) += 1;
    }

    let mut survivors: HashMap<WarningIdentity<'_>, usize> = HashMap::new();
    let mut survivors_at: HashMap<(WarningIdentity<'_>, usize, usize), usize> = HashMap::new();
    for warning in remaining_warnings {
        let identity = WarningIdentity::of(warning);
        *survivors.entry(identity).or_insert(0) += 1;
        *survivors_at
            .entry((identity, warning.line, warning.column))
            .or_insert(0) += 1;
        let remaining_for_rule = net_resolved.entry(warning.rule_name.as_deref()).or_insert(0);
        *remaining_for_rule = remaining_for_rule.saturating_sub(1);
    }

    // A warning on a line the fix pass left untouched pairs first with a
    // survivor saying the same thing at the same column of that line's new
    // position: the text is the same, so the finding is too. Two warnings that
    // say the same thing are told apart here, when a fix resolved one of them.
    let kept_lines = KeptLines::between(original_content, fixed_content);
    let mut paired = vec![false; all_warnings.len()];
    for (index, warning) in all_warnings.iter().enumerate() {
        let identity = WarningIdentity::of(warning);
        if let Some(line) = kept_lines.now_at(warning.line)
            && let Some(at_position) = survivors_at.get_mut(&(identity, line, warning.column))
            && *at_position > 0
            && let Some(saying_the_same) = survivors.get_mut(&identity)
        {
            *at_position -= 1;
            *saying_the_same -= 1;
            paired[index] = true;
        }
    }

    // Pair each remaining pre-fix warning with a survivor saying the same thing.
    // Every warning claims at most one, so when two of them are indistinguishable
    // the order they were reported in would otherwise decide which one is called
    // the survivor. Letting the ones the CLI could not have acted on claim first
    // leaves each disappearance to a warning a fix can account for.
    let mut claim_order: Vec<usize> = (0..all_warnings.len()).filter(|&index| !paired[index]).collect();
    claim_order.sort_by_key(|&index| (all_warnings[index].fix.is_some(), index));

    let mut unmatched: Vec<usize> = Vec::new();
    for index in claim_order {
        if !claim_survivor(&mut survivors, &all_warnings[index]) {
            unmatched.push(index);
        }
    }

    // Credit the disappearances, up to how many the rule actually had. More
    // warnings can lose their text than the rule lost findings, because a
    // survivor the fix pass reworded no longer matches anything, so the count is
    // what settles it. A warning carrying a fix goes first: it is the one the run
    // was able to act on directly.
    unmatched.sort_by_key(|&index| (all_warnings[index].fix.is_none(), index));

    let mut fixed = vec![false; all_warnings.len()];
    for index in unmatched {
        let rule_budget = net_resolved
            .entry(all_warnings[index].rule_name.as_deref())
            .or_insert(0);
        if *rule_budget > 0 {
            *rule_budget -= 1;
            fixed[index] = true;
        }
    }

    FixReconciliation { fixed }
}

/// Take one survivor equivalent to `warning`, reporting whether there was one.
fn claim_survivor<'a>(survivors: &mut HashMap<WarningIdentity<'a>, usize>, warning: &'a LintWarning) -> bool {
    match survivors.get_mut(&WarningIdentity::of(warning)) {
        Some(count) if *count > 0 => {
            *count -= 1;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rumdl_lib::rule::{Fix, LintWarning};

    fn warning(rule: &str, line: usize, message: &str, fix: Option<Fix>) -> LintWarning {
        LintWarning {
            message: message.to_string(),
            line,
            column: 1,
            end_line: line,
            end_column: 2,
            severity: Severity::Warning,
            fix,
            rule_name: Some(rule.to_string()),
        }
    }

    fn escalated(mut warning: LintWarning) -> LintWarning {
        warning.severity = Severity::Error;
        warning
    }

    fn some_fix() -> Option<Fix> {
        Some(Fix::new(0..1, String::new()))
    }

    /// Reconciles through a fix pass that rewrote every line, so what each
    /// warning says is all that pairs it with a survivor.
    fn reconcile(all: &[LintWarning], remaining: &[LintWarning]) -> Vec<bool> {
        reconcile_fixed_warnings(all, remaining, "before\n", "after\n")
            .per_warning()
            .to_vec()
    }

    /// A document, and the document after a fix added a blank line below the
    /// heading and trimmed line 2: line 2 is rewritten as line 3, and line 4 is
    /// untouched and now line 5.
    const BEFORE: &str = "# T\nfirst   \n\nsecond\n";
    const AFTER: &str = "# T\n\nfirst\n\nsecond\n";
    const TOO_LONG: &str = "Line length 90 exceeds 80 characters";

    #[test]
    fn lines_keep_their_partners_across_inserted_and_removed_lines() {
        let kept = KeptLines::between("a\nb\nc\nd\n", "a\nX\nY\nc\n");
        let positions: Vec<_> = (1..=4).map(|line| kept.now_at(line)).collect();
        assert_eq!(positions, vec![Some(1), None, Some(4), None]);

        let unchanged = KeptLines::between("a\nb\n", "a\nb\n");
        assert_eq!(unchanged.now_at(2), Some(2));
    }

    #[test]
    fn an_identical_warning_on_an_untouched_line_is_the_survivor() {
        let all = vec![warning("MD013", 2, TOO_LONG, None), warning("MD013", 4, TOO_LONG, None)];
        let remaining = vec![warning("MD013", 5, TOO_LONG, None)];
        let reconciled = reconcile_fixed_warnings(&all, &remaining, BEFORE, AFTER);
        assert_eq!(reconciled.per_warning(), [true, false]);
    }

    #[test]
    fn an_identical_warning_on_an_untouched_line_is_the_survivor_whichever_is_reported_first() {
        let all = vec![warning("MD013", 4, TOO_LONG, None), warning("MD013", 2, TOO_LONG, None)];
        let remaining = vec![warning("MD013", 5, TOO_LONG, None)];
        let reconciled = reconcile_fixed_warnings(&all, &remaining, BEFORE, AFTER);
        assert_eq!(reconciled.per_warning(), [false, true]);
    }

    #[test]
    fn a_survivor_away_from_an_untouched_line_is_not_that_lines_finding() {
        // The survivor sits on the rewritten line, so the finding on the
        // untouched line is the one that disappeared.
        let all = vec![warning("MD013", 2, TOO_LONG, None), warning("MD013", 4, TOO_LONG, None)];
        let remaining = vec![warning("MD013", 3, TOO_LONG, None)];
        let reconciled = reconcile_fixed_warnings(&all, &remaining, BEFORE, AFTER);
        assert_eq!(reconciled.per_warning(), [false, true]);
    }

    #[test]
    fn identical_warnings_on_one_untouched_line_are_told_apart_by_column() {
        let at_column = |column| LintWarning {
            column,
            ..warning("MD049", 4, "Emphasis style should be asterisk", None)
        };
        let all = vec![at_column(1), at_column(8)];
        let remaining = vec![LintWarning {
            line: 5,
            ..at_column(8)
        }];
        let reconciled = reconcile_fixed_warnings(&all, &remaining, BEFORE, AFTER);
        assert_eq!(reconciled.per_warning(), [true, false]);
    }

    #[test]
    fn the_unfixed_warnings_keep_their_reported_positions_and_lose_their_fixes() {
        let all = vec![
            warning("MD022", 1, "Expected 1 blank line below heading", some_fix()),
            warning("MD009", 3, "Trailing spaces", some_fix()),
            warning("MD052", 7, "Reference 'zz' not found", None),
        ];
        let remaining = vec![
            warning("MD022", 1, "Expected 1 blank line below heading", some_fix()),
            warning("MD052", 8, "Reference 'zz' not found", None),
        ];
        let unfixed = reconcile_fixed_warnings(&all, &remaining, "before\n", "after\n").unfixed(&all);
        let summary: Vec<_> = unfixed
            .iter()
            .map(|w| (w.rule_name.as_deref(), w.line, w.fix.is_some()))
            .collect();
        assert_eq!(summary, vec![(Some("MD022"), 1, false), (Some("MD052"), 7, false)]);
    }

    #[test]
    fn a_warning_that_moved_but_survived_is_not_fixed() {
        let all = vec![warning("MD052", 7, "Reference 'zz' not found", None)];
        let remaining = vec![warning("MD052", 9, "Reference 'zz' not found", None)];
        assert_eq!(reconcile(&all, &remaining), vec![false]);
    }

    #[test]
    fn a_document_level_fix_is_credited_without_a_per_warning_fix() {
        // MD046 rewrites the document from `Rule::fix()` and reports no per-warning
        // fix, so nothing about the warning itself says it was fixable.
        let all = vec![warning("MD046", 5, "Use fenced code blocks", None)];
        assert_eq!(reconcile(&all, &[]), vec![true]);
    }

    #[test]
    fn a_warning_another_rule_resolved_is_credited() {
        // MD013 has no fix of its own, but MD009 removing the line's trailing
        // spaces took it under the limit. The finding is gone, so the report says
        // so rather than dropping it.
        let all = vec![
            warning("MD009", 3, "Trailing spaces", some_fix()),
            warning("MD013", 3, "Line length 82 exceeds 80 characters", None),
        ];
        assert_eq!(reconcile(&all, &[]), vec![true, true]);
    }

    #[test]
    fn a_run_that_changed_nothing_credits_nothing() {
        // What `remaining_after_fixes` hands back when the fix pass rewrote no
        // bytes: the same warnings, so every rule lost nothing.
        let all = vec![
            warning("MD046", 5, "Use fenced code blocks", None),
            warning("MD013", 9, "Line length 82 exceeds 80 characters", None),
        ];
        let reconciled = reconcile_fixed_warnings(&all, &all, BEFORE, BEFORE);
        assert_eq!(reconciled.per_warning(), [false, false]);
    }

    #[test]
    fn identical_warnings_are_credited_one_per_disappearance() {
        let all = vec![
            warning("MD009", 1, "Trailing spaces", some_fix()),
            warning("MD009", 2, "Trailing spaces", some_fix()),
            warning("MD009", 3, "Trailing spaces", some_fix()),
        ];
        let remaining = vec![warning("MD009", 2, "Trailing spaces", some_fix())];
        let reconciled = reconcile(&all, &remaining);
        assert_eq!(reconciled.iter().filter(|&&f| f).count(), 2);
    }

    #[test]
    fn one_of_two_equivalent_warnings_disappearing_credits_the_one_that_was_fixable() {
        // Same rule, same message, one warning carrying a fix and one not. Exactly
        // one is gone, and only one of the two could have been fixed directly.
        let all = vec![
            warning("MD040", 1, "Code block missing language", None),
            warning("MD040", 5, "Code block missing language", some_fix()),
        ];
        let remaining = vec![warning("MD040", 1, "Code block missing language", None)];
        assert_eq!(reconcile(&all, &remaining), vec![false, true]);
    }

    #[test]
    fn crediting_does_not_depend_on_the_order_the_warnings_were_reported() {
        // The previous case with the two warnings swapped: still one disappearance,
        // still credited to the fixable one.
        let all = vec![
            warning("MD040", 1, "Code block missing language", some_fix()),
            warning("MD040", 5, "Code block missing language", None),
        ];
        let remaining = vec![warning("MD040", 5, "Code block missing language", None)];
        assert_eq!(reconcile(&all, &remaining), vec![true, false]);
    }

    #[test]
    fn no_more_warnings_are_credited_than_actually_disappeared() {
        // Three warnings, two survivors: exactly one disappearance to credit.
        let all = vec![
            warning("MD009", 1, "Trailing spaces", some_fix()),
            warning("MD009", 2, "Trailing spaces", some_fix()),
            warning("MD009", 3, "Trailing spaces", some_fix()),
        ];
        let remaining = vec![
            warning("MD009", 1, "Trailing spaces", some_fix()),
            warning("MD009", 2, "Trailing spaces", some_fix()),
        ];
        let reconciled = reconcile(&all, &remaining);
        assert_eq!(reconciled.iter().filter(|&&f| f).count(), 1);
    }

    #[test]
    fn a_reworded_survivor_is_not_counted_as_a_disappearance() {
        // Two over-long lines, one of them reflowed away. The other is 90 columns
        // before its trailing spaces are removed and 86 after: still too long, but
        // reported with a different number, so neither pre-fix message survives
        // verbatim. One finding went away, so one is credited.
        let all = vec![
            warning("MD013", 3, "Line length 131 exceeds 80 characters", some_fix()),
            warning("MD013", 5, "Line length 90 exceeds 80 characters", some_fix()),
        ];
        let remaining = vec![warning("MD013", 5, "Line length 86 exceeds 80 characters", some_fix())];
        let reconciled = reconcile(&all, &remaining);
        assert_eq!(reconciled.iter().filter(|&&f| f).count(), 1);
    }

    #[test]
    fn a_survivor_whose_message_names_a_moved_line_is_not_credited() {
        // MD053 quotes the line of the definition it found unused, so a fix above
        // it rewrites the message without resolving anything. One finding before
        // and one after means the rule lost nothing.
        let all = vec![warning("MD053", 7, "Unused link/image reference: [a] (line 7)", None)];
        let remaining = vec![warning("MD053", 9, "Unused link/image reference: [a] (line 9)", None)];
        assert_eq!(reconcile(&all, &remaining), vec![false]);
    }

    #[test]
    fn a_rule_that_gained_findings_is_credited_for_none_of_them() {
        // A fix can introduce a finding the file did not have. That is not a
        // disappearance to spend on the warning whose text no longer matches.
        let all = vec![warning("MD012", 4, "Multiple consecutive blank lines", some_fix())];
        let remaining = vec![
            warning("MD012", 6, "Multiple consecutive blank lines [expected: 1]", some_fix()),
            warning("MD012", 9, "Multiple consecutive blank lines [expected: 1]", some_fix()),
        ];
        assert_eq!(reconcile(&all, &remaining), vec![false]);
    }

    #[test]
    fn warnings_differing_only_in_severity_are_not_interchangeable() {
        // Severity is per-warning, so one rule can report the same text at two
        // severities. They are different diagnostics and one cannot stand in for
        // the other: the survivor here is the escalated one.
        let all = vec![
            warning("MD040", 1, "Code block missing language", some_fix()),
            escalated(warning("MD040", 5, "Code block missing language", some_fix())),
        ];
        let remaining = vec![escalated(warning(
            "MD040",
            5,
            "Code block missing language",
            some_fix(),
        ))];
        assert_eq!(reconcile(&all, &remaining), vec![true, false]);
    }

    #[test]
    fn warnings_differing_only_in_message_are_not_interchangeable() {
        // One rule, two findings, one resolved: what each warning says is the only
        // thing separating them, so matching on the rule name alone credits the
        // wrong one.
        let all = vec![
            warning("MD075", 5, "Orphaned table row(s)", some_fix()),
            warning("MD075", 9, "Table missing header/delimiter", some_fix()),
        ];
        let remaining = vec![warning("MD075", 8, "Table missing header/delimiter", some_fix())];
        assert_eq!(reconcile(&all, &remaining), vec![true, false]);
    }

    #[test]
    fn the_survivor_decides_which_warning_is_credited_not_the_order() {
        // The previous case with the surviving finding reported first. Counting
        // disappearances is what says one warning is credited; reading what the
        // survivor says is what says which one.
        let all = vec![
            warning("MD075", 5, "Table missing header/delimiter", some_fix()),
            warning("MD075", 9, "Orphaned table row(s)", some_fix()),
        ];
        let remaining = vec![warning("MD075", 4, "Table missing header/delimiter", some_fix())];
        assert_eq!(reconcile(&all, &remaining), vec![false, true]);
    }

    #[test]
    fn a_disappearance_is_credited_to_the_warning_the_run_could_act_on() {
        // An unbreakable line MD013 cannot rewrap, reported before a paragraph it
        // can. The paragraph is gone and the line survives one column shorter, so
        // neither message survives verbatim and the budget allows one credit: it
        // belongs to the warning that carried a fix, whichever order they came in.
        let all = vec![
            warning("MD013", 3, "Line length 90 exceeds 80 characters", None),
            warning("MD013", 5, "Line length exceeds 80 characters", some_fix()),
        ];
        let remaining = vec![warning("MD013", 3, "Line length 86 exceeds 80 characters", None)];
        assert_eq!(reconcile(&all, &remaining), vec![false, true]);
    }

    #[test]
    fn one_rule_disappearing_never_credits_another_rules_warning() {
        // The budget is per rule, so MD009 resolving both of its findings cannot
        // pay for MD052's, which is still there.
        let all = vec![
            warning("MD009", 1, "Trailing spaces", some_fix()),
            warning("MD052", 3, "Reference 'zz' not found", None),
            warning("MD009", 5, "Trailing spaces", some_fix()),
        ];
        let remaining = vec![warning("MD052", 3, "Reference 'zz' not found", None)];
        assert_eq!(reconcile(&all, &remaining), vec![true, false, true]);
    }
}
