use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD036NoEmphasisAsHeading;

#[test]
fn test_md036_for_emphasis_only_lines() {
    // Test for the proper purpose of MD036 - emphasis-only lines detection
    // MD036 no longer provides automatic fixes to prevent document corruption
    let content = "Normal text\n\n**This should be a heading**\n\nMore text";

    let ctx = LintContext::new(content);
    let md036 = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());

    // Check that MD036 detects the issue
    let warnings = md036.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1, "MD036 should detect emphasis-only line");
    assert!(warnings[0].message.contains("This should be a heading"));

    // Check that no automatic fix is provided
    assert!(warnings[0].fix.is_none(), "MD036 should not provide automatic fixes");

    // Apply fix method - should return unchanged content
    let fixed_md036 = md036.fix(&ctx).unwrap();

    // The emphasis should remain unchanged (no automatic fix)
    assert_eq!(fixed_md036, content, "Content should remain unchanged");
    assert!(fixed_md036.contains("**This should be a heading**"));
    assert!(!fixed_md036.contains("## This should be a heading"));
}
