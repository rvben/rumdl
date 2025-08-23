use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD043RequiredHeadings;

#[test]
fn test_matching_headings() {
    let required = vec![
        "# Introduction".to_string(),
        "# Methods".to_string(),
        "# Results".to_string(),
    ];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\n\n# Methods\n\n# Results";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_missing_heading() {
    let required = vec![
        "# Introduction".to_string(),
        "# Methods".to_string(),
        "# Results".to_string(),
    ];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\n\n# Results";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Introduction\n\n# Methods\n\n# Results");
}

#[test]
fn test_extra_heading() {
    let required = vec!["# Introduction".to_string(), "# Results".to_string()];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\n\n# Methods\n\n# Results";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Introduction\n\n# Results");
}

#[test]
fn test_wrong_order() {
    let required = vec![
        "# Introduction".to_string(),
        "# Methods".to_string(),
        "# Results".to_string(),
    ];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\n\n# Results\n\n# Methods";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Introduction\n\n# Methods\n\n# Results");
}

#[test]
fn test_empty_required_headings() {
    let required = vec![];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Any heading\n\n# Another heading";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_case_sensitive() {
    let required = vec!["# Introduction".to_string()];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# INTRODUCTION";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Should match because match_case is false by default
}

#[test]
fn test_mixed_heading_styles() {
    let required = vec!["# Introduction".to_string(), "======= Methods".to_string()];
    let rule = MD043RequiredHeadings::new(required);
    let content = "# Introduction\nContent\nMethods\n=======";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}
