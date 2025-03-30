use rumdl::rule::Rule;
use rumdl::MD015NoMissingSpaceAfterListMarker;
use rumdl::MD053LinkImageReferenceDefinitions;
use rumdl::rules::{MD003HeadingStyle, MD017NoEmphasisAsHeading, MD036NoEmphasisOnlyFirst};

#[test]
fn cross_rule_md015_md053() {
    let content = "- [Link][ref]\n* [Another][ref2]";

    // Apply MD015 fix
    let fixed = MD015NoMissingSpaceAfterListMarker::new()
        .fix(content)
        .unwrap();

    // Check MD053 results
    let result = MD053LinkImageReferenceDefinitions::new(vec![])
        .check(&fixed)
        .unwrap();

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
    
    // Apply MD036 (NoEmphasisOnlyFirst) fix
    let md036 = MD036NoEmphasisOnlyFirst {};
    let fixed_md036 = md036.fix(content).unwrap();
    
    // The emphasis should be converted to a proper heading
    assert!(fixed_md036.contains("## This should be a heading"));
    assert!(!fixed_md036.contains("**This should be a heading**"));
}
