mod core_utils_test;
mod front_matter_utils_test;
mod line_index_test;
mod text_reflow_test;

/// Assert that running `fix()` on content with violations produces output that
/// passes `check()` with zero remaining violations.
///
/// This catches check/fix asymmetry bugs where `check()` detects violations that
/// `fix()` silently skips, leaving warnings present after `--fix`.
///
/// # Panics
///
/// Panics if `check()` finds no violations in the original content (the caller
/// should only pass content known to have violations), or if `check()` finds any
/// remaining violations after `fix()`.
pub fn assert_fix_resolves_all_violations(
    rule: &dyn rumdl_lib::rule::Rule,
    content: &str,
    flavor: rumdl_lib::config::MarkdownFlavor,
) {
    let ctx = rumdl_lib::lint_context::LintContext::new(content, flavor, None);
    let before = rule.check(&ctx).unwrap();
    assert!(
        !before.is_empty(),
        "Expected violations but check() found none in:\n{content}"
    );

    let fixed = rule.fix(&ctx).unwrap();
    let ctx_fixed = rumdl_lib::lint_context::LintContext::new(&fixed, flavor, None);
    let after = rule.check(&ctx_fixed).unwrap();
    assert!(
        after.is_empty(),
        "fix() left {} violation(s) unresolved:\n{:?}\nOriginal:\n{content}\nFixed:\n{fixed}",
        after.len(),
        after
    );
}
