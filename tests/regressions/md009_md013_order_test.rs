/// Test for issue #76 - trailing whitespace handling with reflow
/// https://github.com/rvben/rumdl/issues/76
///
/// The issue: When content has trailing whitespace at end of lines in a list,
/// MD013 reflow would combine lines with their trailing whitespace, creating
/// mid-line whitespace that wouldn't be detected.
///
/// The fix: MD013 now trims trailing whitespace when extracting list item content
/// before joining lines. MD013 runs before MD009 so that any trailing spaces
/// created by reflow operations can be cleaned up by MD009.
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD009TrailingSpaces, MD013LineLength};

#[test]
fn test_fix_coordinator_runs_md013_before_md009() {
    // This test verifies the fix coordinator orders MD013 before MD009
    // MD013 now trims trailing whitespace internally during reflow,
    // and MD009 cleans up any trailing spaces that MD013's reflow might create
    let coordinator = FixCoordinator::new();

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD013LineLength::default()),
        Box::new(MD009TrailingSpaces::default()),
    ];

    let ordered = coordinator.get_optimal_order(&rules);
    let ordered_names: Vec<&str> = ordered.iter().map(|r| r.name()).collect();

    let md009_idx = ordered_names.iter().position(|&n| n == "MD009").unwrap();
    let md013_idx = ordered_names.iter().position(|&n| n == "MD013").unwrap();

    assert!(
        md013_idx < md009_idx,
        "MD013 (reflow) should run BEFORE MD009 (trailing spaces) so that \
         any trailing spaces created by reflow operations can be cleaned up. \
         Current order: {ordered_names:?}"
    );
}
