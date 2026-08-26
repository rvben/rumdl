//! Deciding which of a document's warnings the fix pass actually resolved.
//!
//! The fix pass rewrites a whole document and the result is re-linted, so nothing
//! links a pre-fix warning to its post-fix self. Three properties of that gap
//! shape the reconciliation:
//!
//! - A rule can resolve a violation without attaching a `Fix` to the warning.
//!   MD046 rewrites the document from `Rule::fix()` and reports `fix: None`, so
//!   "carries a fix" answers whether an editor can offer a code action, not
//!   whether the CLI resolved anything. One rule's fix also resolves other rules'
//!   findings - removing trailing spaces shortens the line MD013 was reporting -
//!   which no record of which rules fixed something can predict.
//! - A fix that changes the line count moves every warning below it, so a
//!   survivor sits somewhere else afterwards. Matching on position calls a
//!   warning that merely moved "fixed".
//! - What a warning says is not stable either. A message quoting a length or a
//!   line number is rewritten when a fix changes either, so a survivor can read
//!   as a disappearance.
//!
//! So the re-lint decides, and it is read twice. How many findings a rule lost
//! bounds how many of its warnings can be credited, which is what an unstable
//! message cannot inflate; what each warning says picks which ones, which is what
//! keeps a warning that merely moved out of the count. Between them, a rule's
//! share of the report is always `max(before, after)` entries: whatever a fix run
//! did, every finding the file had is either reported as fixed or still there.

use std::collections::HashMap;

use rumdl_lib::rule::{LintWarning, Severity};

/// What a warning says, independent of where it sits.
///
/// Message text is the closest thing a rule offers to a stable identity, and it
/// is not a contract: a rule that names a position in its message (MD053 reports
/// the line of the definition it conflicts with) reads as a different warning
/// once that line moves. That is why identity only ever picks *which* warnings to
/// credit, never how many.
#[derive(PartialEq, Eq, Hash)]
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
}

/// Reconcile a document's pre-fix warnings against the ones that survived.
///
/// `remaining_warnings` has to come from linting the fixed document with the same
/// rules that produced `all_warnings`, or a rule missing from one side reads as a
/// document whose findings all disappeared.
pub fn reconcile_fixed_warnings(all_warnings: &[LintWarning], remaining_warnings: &[LintWarning]) -> FixReconciliation {
    // How many findings each rule lost. A rule that reported four and still
    // reports one resolved three of them, whichever three they were, and a rule
    // that gained findings resolved none.
    let mut net_resolved: HashMap<Option<&str>, usize> = HashMap::new();
    for warning in all_warnings {
        *net_resolved.entry(warning.rule_name.as_deref()).or_insert(0) += 1;
    }

    let mut survivors: HashMap<WarningIdentity<'_>, usize> = HashMap::new();
    for warning in remaining_warnings {
        *survivors.entry(WarningIdentity::of(warning)).or_insert(0) += 1;
        let remaining_for_rule = net_resolved.entry(warning.rule_name.as_deref()).or_insert(0);
        *remaining_for_rule = remaining_for_rule.saturating_sub(1);
    }

    // Pair each pre-fix warning with a survivor saying the same thing. Every
    // warning claims at most one, so when two of them are indistinguishable the
    // order they were reported in would otherwise decide which one is called the
    // survivor. Letting the ones the CLI could not have acted on claim first
    // leaves each disappearance to a warning a fix can account for.
    let mut claim_order: Vec<usize> = (0..all_warnings.len()).collect();
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

    fn reconcile(all: &[LintWarning], remaining: &[LintWarning]) -> Vec<bool> {
        reconcile_fixed_warnings(all, remaining).per_warning().to_vec()
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
        assert_eq!(reconcile(&all, &all), vec![false, false]);
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
