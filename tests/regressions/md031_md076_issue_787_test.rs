//! Regression test for issue #787: MD031 and MD076 must converge when a list
//! item starts with a fenced code block.

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{ListItemSpacingStyle, MD031BlanksAroundFences, MD076ListItemSpacing, all_rules};

const REPRO_INPUT: &str = "# Heading\n\n- foo\n- bar\n\n- ```text\n  baz\n  ```\n";

/// A fence opened on the marker line: MD031 requires the blank line above it.
const FENCED_ON_MARKER: &str = "# Heading\n\n- foo\n\n- ```text\n  baz\n  ```\n- qux\n";

/// The same shape with five spaces after the marker, which puts the fence at a
/// relative indent of 4. That is an indented code block, and MD031 requires
/// nothing around it.
const INDENTED_ON_MARKER: &str = "# Heading\n\n- foo\n\n-     ```text\n      baz\n      ```\n- qux\n";

#[test]
fn md031_md076_converge_for_fenced_list_item() {
    let config = Config::default();
    let rules = all_rules(&config);
    let warnings =
        rumdl_lib::lint(REPRO_INPUT, &rules, false, MarkdownFlavor::Standard, None, None).expect("lint must succeed");
    let mut fixed = REPRO_INPUT.to_string();

    let result = FixCoordinator::new()
        .apply_fixes_iterative(&rules, &warnings, &mut fixed, &config, 100, None)
        .expect("fix pipeline must succeed");

    assert!(result.converged, "fix pipeline reported a cycle: {result:?}");
    assert!(
        result.conflicting_rules.is_empty(),
        "MD031 and MD076 must not conflict: {result:?}"
    );
    assert!(
        fixed.contains("- bar\n\n- ```text"),
        "MD031's required blank line before the fenced list item must be preserved:\n{fixed}"
    );
}

/// MD076 keeps a blank line before a list item exactly when MD031 asks for one.
///
/// The two rules have to agree about what a fence is, or one of them undoes the
/// other. A marker followed by five spaces and a fence looks fenced but is an
/// indented code block, so MD031 requires nothing and MD076 must still treat the
/// blank as an ordinary loose separator.
#[test]
fn md076_exempts_the_gap_exactly_where_md031_requires_it() {
    let md031 = MD031BlanksAroundFences::default();
    let md076 = MD076ListItemSpacing::new(ListItemSpacingStyle::Tight);

    for (label, content, md031_requires_blank) in [
        ("fence on the marker line", FENCED_ON_MARKER, true),
        ("indented block on the marker line", INDENTED_ON_MARKER, false),
    ] {
        // Strip the blank line so MD031 is asked whether it wants one back.
        let tightened = content.replace("- foo\n\n-", "- foo\n-");
        assert_ne!(tightened, content, "the {label} fixture must contain the blank line");

        let tight_ctx = LintContext::new(&tightened, MarkdownFlavor::Standard, None);
        let md031_warnings = md031.check(&tight_ctx).expect("MD031 must succeed");
        assert_eq!(
            !md031_warnings.is_empty(),
            md031_requires_blank,
            "MD031 requirement for the {label} case was not what the exemption assumes: {md031_warnings:?}"
        );

        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let md076_warnings = md076.check(&ctx).expect("MD076 must succeed");
        assert_eq!(
            md076_warnings.is_empty(),
            md031_requires_blank,
            "MD076 must exempt the gap for the {label} case only when MD031 requires it: {md076_warnings:?}"
        );
        assert_eq!(
            md076.fix(&ctx).expect("MD076 fix must succeed"),
            if md031_requires_blank {
                content.to_string()
            } else {
                tightened
            },
            "MD076's fix disagrees with MD031 for the {label} case"
        );
    }
}
