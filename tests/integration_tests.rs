use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD036NoEmphasisAsHeading;

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
