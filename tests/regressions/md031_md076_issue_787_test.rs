//! Regression test for issue #787: MD031 and MD076 must converge when a list
//! item starts with a fenced code block.

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::rules::all_rules;

const REPRO_INPUT: &str = "# Heading\n\n- foo\n- bar\n\n- ```text\n  baz\n  ```\n";

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
