use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD036NoEmphasisAsHeading;
use rumdl::MD015NoMissingSpaceAfterListMarker;
use rumdl::MD053LinkImageReferenceDefinitions;

#[test]
fn cross_rule_md015_md053() {
    let content = "- [Link][ref]\n* [Another][ref2]";

    let ctx = LintContext::new(content);
    // Apply MD015 fix
    let fixed = MD015NoMissingSpaceAfterListMarker::new().fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed);
    // Check MD053 results
    let rule = MD053LinkImageReferenceDefinitions::default();
    let result = rule.check(&fixed_ctx).unwrap();

    // The rule should not generate any warnings because all references are used
    assert!(
        result.is_empty(),
        "Should not detect unused refs after MD015 fix: {:?}",
        result
    );
}

#[test]
fn test_md036_for_emphasis_only_lines() {
    // Test for the proper purpose of MD036 - emphasis-only lines
    let content = "Normal text\n\n**This should be a heading**\n\nMore text";

    let ctx = LintContext::new(content);
    // Apply MD036 (NoEmphasisOnlyFirst) fix
    let md036 = MD036NoEmphasisAsHeading {};
    let fixed_md036 = md036.fix(&ctx).unwrap();

    // The emphasis should be converted to a proper heading
    assert!(fixed_md036.contains("## This should be a heading"));
    assert!(!fixed_md036.contains("**This should be a heading**"));
}
