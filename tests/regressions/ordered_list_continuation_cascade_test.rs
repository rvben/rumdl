/// Regression tests for ordered-list continuation cascade
///
/// An ordered list item with a tight (no-blank-line) continuation followed by a
/// nested sub-list must round-trip through the fix pipeline without corruption:
/// MD032 must not insert a blank line between the item and its continuation,
/// MD005 must not de-nest the sub-list to column 0, and MD029 must not renumber
/// the inner sub-list as if it were part of the outer list.
use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::rules;

/// The canonical repro input from the issue.
const REPRO_INPUT: &str = "\
# aabbcc

## aabbcc

1. aabbcc
  aabbcc aabbcc aabbcc aabbcc.aabbcc aabbcc

  ```
  aabbcc (aabbcc) {
      'aabbcc' {
          aabbcc
      },
  }
  ```

2. aabbcc

  aabbcc aabbcc aabbcc

3. aabbcc
  aabbcc

  1. aabbcc
  2. aabbcc

---
### abc


";

/// Run the full fix pipeline on `input` (same code path as `rumdl fmt`).
fn run_fmt_pipeline(input: &str) -> String {
    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let coordinator = FixCoordinator::new();
    let mut content = input.to_string();
    let warnings = rumdl_lib::lint(input, &all_rules, false, MarkdownFlavor::Standard, None, None).unwrap();
    coordinator
        .apply_fixes_iterative(&all_rules, &warnings, &mut content, &config, 100, None)
        .expect("fix pipeline should not error");
    content
}

/// MD032 must not insert a blank line between `3. aabbcc` and its tight continuation `  aabbcc`.
#[test]
fn test_no_blank_inserted_between_item_and_tight_continuation() {
    let fixed = run_fmt_pipeline(REPRO_INPUT);

    // The pattern "3. aabbcc\n\n" means a blank was wrongly inserted after item 3.
    // Instead item 3 and its continuation should be adjacent (possibly with updated indent).
    assert!(
        !fixed.contains("3. aabbcc\n\n"),
        "MD032 must not insert a blank line between item 3 and its tight continuation.\nActual output:\n{fixed}"
    );
}

/// The nested sub-list (`1. aabbcc`, `2. aabbcc`) must remain indented — it must NOT
/// appear at column 0 as if it were a continuation of the outer list.
#[test]
fn test_nested_sublist_stays_indented() {
    let fixed = run_fmt_pipeline(REPRO_INPUT);

    // The sub-list items must NOT appear at col 0 (de-nested by MD005).
    assert!(
        !fixed.contains("\n1. aabbcc\n2. aabbcc\n"),
        "MD005 must not de-nest the sub-list items to column 0.\nActual output:\n{fixed}"
    );

    // The sub-list items must be indented (start with at least one space).
    // The exact indent may vary depending on MD077/MD005 corrections, but they must be nested.
    let has_indented_sublist = fixed
        .lines()
        .any(|l| l.starts_with("   1. aabbcc") || l.starts_with("  1. aabbcc"));
    assert!(
        has_indented_sublist,
        "The nested sub-list item '1. aabbcc' must remain indented in the output.\nActual output:\n{fixed}"
    );
}

/// MD029 must not renumber the inner sub-list items as 4 and 5.
/// The sub-list is its own ordered context and should stay 1, 2.
#[test]
fn test_inner_sublist_not_renumbered() {
    let fixed = run_fmt_pipeline(REPRO_INPUT);

    assert!(
        !fixed.contains("4. aabbcc"),
        "MD029 must not renumber the sub-list's first item to 4.\nActual output:\n{fixed}"
    );
    assert!(
        !fixed.contains("5. aabbcc"),
        "MD029 must not renumber the sub-list's second item to 5.\nActual output:\n{fixed}"
    );
}

/// After fixing, a second pass of `check` on the fixed output must produce zero MD005
/// and MD032 warnings (the fix is stable / check-clean).
#[test]
fn test_fix_output_is_check_clean_for_md005_and_md032() {
    let fixed = run_fmt_pipeline(REPRO_INPUT);

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let warnings = rumdl_lib::lint(&fixed, &all_rules, false, MarkdownFlavor::Standard, None, None).unwrap();

    let md005_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD005"))
        .collect();
    assert!(
        md005_warnings.is_empty(),
        "MD005 should report no warnings on the fixed output.\nFixed:\n{fixed}\nWarnings: {md005_warnings:?}"
    );

    let md032_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD032"))
        .collect();
    assert!(
        md032_warnings.is_empty(),
        "MD032 should report no warnings on the fixed output.\nFixed:\n{fixed}\nWarnings: {md032_warnings:?}"
    );
}

/// MD029 must report no renumber warnings on the fixed output.
#[test]
fn test_fix_output_has_no_md029_warnings() {
    let fixed = run_fmt_pipeline(REPRO_INPUT);

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let warnings = rumdl_lib::lint(&fixed, &all_rules, false, MarkdownFlavor::Standard, None, None).unwrap();

    let md029_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD029"))
        .collect();
    assert!(
        md029_warnings.is_empty(),
        "MD029 should report no renumber warnings on the fixed output.\nFixed:\n{fixed}\nWarnings: {md029_warnings:?}"
    );
}

/// Idempotency: applying the fix pipeline twice must yield the same result as once.
#[test]
fn test_fix_pipeline_is_idempotent() {
    let fixed_once = run_fmt_pipeline(REPRO_INPUT);
    let fixed_twice = run_fmt_pipeline(&fixed_once);

    assert_eq!(
        fixed_once, fixed_twice,
        "Fix pipeline is not idempotent: the second pass changed the output.\nAfter first pass:\n{fixed_once}\nAfter second pass:\n{fixed_twice}"
    );
}

/// MD032 must not fire on an ordered list item whose immediately following line is a
/// tight continuation (indented > marker column, not itself a list item).
#[test]
fn test_md032_no_warning_on_tight_continuation_after_ordered_item() {
    let content = "1. First item\n  tight continuation\n\n2. Second item\n";
    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None, None).unwrap();
    let list_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("followed by blank line"))
        .collect();
    assert!(
        list_warnings.is_empty(),
        "MD032 must not warn about missing blank line when the next line is tight continuation.\nWarnings: {list_warnings:?}"
    );
}

/// MD005 must not de-nest a sub-list that follows a tight continuation of its parent item.
/// The tight continuation (indented > parent marker_col but < content_col) must be
/// recognized as evidence that the child items are continuation content.
#[test]
fn test_md005_recognizes_tight_continuation_before_nested_list() {
    // `3. item` has marker_col=0, content_col=3.
    // `  tight` at col 2 is tight continuation (0 < 2 < 3).
    // `  1. sub` at col 2 is a nested sub-list — must NOT be de-nested.
    let content = "\
1. outer A

2. outer B

3. outer C
  tight

  1. sub one
  2. sub two
";
    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md005_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD005").collect();

    let warnings = rumdl_lib::lint(content, &md005_rules, false, MarkdownFlavor::Standard, None, None).unwrap();
    assert!(
        warnings.is_empty(),
        "MD005 must not warn about the nested sub-list indented after a tight continuation.\nWarnings: {warnings:?}"
    );
}

/// Direct check of MD005 on the exact repro input — no warnings should be emitted for
/// the nested sub-list items since they follow a tight continuation.
#[test]
fn test_md005_no_warning_on_repro_input() {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD005ListIndent;

    let rule = MD005ListIndent::default();
    let ctx = LintContext::new(REPRO_INPUT, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    let sublist_warnings: Vec<_> = warnings.iter().filter(|w| w.line == 23 || w.line == 24).collect();
    assert!(
        sublist_warnings.is_empty(),
        "MD005 must not warn about the nested sub-list items on lines 23-24.\nAll MD005 warnings: {warnings:?}"
    );
}

/// Loose continuation (after a blank line) below the parent's content column must
/// still be classified as ending the list item — otherwise an under-indented
/// "child" list would be silently accepted as nested. This guards the threshold
/// relaxation in MD005 from over-applying to loose continuation.
#[test]
fn test_md005_loose_under_indented_text_terminates_list() {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD005ListIndent;

    // `* parent` has marker_col=0, content_col=2.
    // After a blank line, ` Text` at col 1 is below the loose threshold (2),
    // so the parent list ends. `  * child` at col 2 is therefore NOT a nested
    // sub-list of `* parent`; it is a fresh top-level item that is mis-indented
    // relative to its own list's first item, which MD005 should report.
    let content = "* parent\n\n Text\n  * child\n";
    let rule = MD005ListIndent::default();
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(
        !warnings.is_empty(),
        "MD005 must report the under-indented child as a top-level violation, not silently treat it as nested.\nWarnings: {warnings:?}"
    );
}
