use rumdl::rule::Rule;
use rumdl::rules::MD030ListMarkerSpace;
use rumdl::lint_context::LintContext;

#[test]
fn test_valid_single_line_lists() {
    let rule = MD030ListMarkerSpace::default();
    let content = "* Item\n- Another item\n+ Third item\n1. Ordered item";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_valid_multi_line_lists() {
    let rule = MD030ListMarkerSpace::default();
    let content = "* First line\n  continued\n- Second item\n  also continued";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_spaces_unordered() {
    let rule = MD030ListMarkerSpace::default();
    let content = "*  Too many spaces\n-   Three spaces";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_invalid_spaces_ordered() {
    let rule = MD030ListMarkerSpace::default();
    let content = "1.  Too many spaces\n2.   Three spaces";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_fix_unordered_list() {
    let rule = MD030ListMarkerSpace::default();
    let content = "*  Item\n-   Another\n+    Third";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "* Item\n- Another\n+ Third");
}

#[test]
fn test_fix_ordered_list() {
    let rule = MD030ListMarkerSpace::default();
    let content = "1.  First\n2.   Second\n3.    Third";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "1. First\n2. Second\n3. Third");
}

#[test]
fn test_custom_spacing() {
    let rule = MD030ListMarkerSpace::new(2, 3, 2, 3);
    let content = "* One space\n- One space\n1. One space";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "*  One space\n-  One space\n1.  One space");
}

#[test]
fn test_mixed_list_types() {
    let rule = MD030ListMarkerSpace::default();
    let content = "*  Unordered\n1.  Ordered\n-   Mixed\n2.   Types";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "* Unordered\n1. Ordered\n- Mixed\n2. Types");
}

#[test]
fn test_nested_lists() {
    let rule = MD030ListMarkerSpace::default();
    let content = "* First\n  *  Nested\n    *   More nested";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "* First\n  * Nested\n    * More nested");
}

#[test]
fn test_ignore_code_blocks() {
    let rule = MD030ListMarkerSpace::default();
    let content = "* Normal item\n```\n*  Not a list\n1.  Not a list\n```\n- Back to list";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multi_line_items() {
    let rule = MD030ListMarkerSpace::new(1, 2, 1, 2);
    let content = "* Single line\n* Multi line\n  continued here\n* Another single";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the multi-line item should be flagged
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "* Single line\n*  Multi line\n  continued here\n* Another single"
    );
}

#[test]
fn test_preserve_indentation() {
    let rule = MD030ListMarkerSpace::default();
    let content = "  *  Item\n    -   Another\n      +    Third";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "  * Item\n    - Another\n      + Third");
}

#[test]
fn test_readme_md030_config() {
    let rule = MD030ListMarkerSpace::new(1, 3, 1, 2);
    let content = r#"# A title

Single ol:

1. one
1. two
1. three

Single ul:

- one
- two
- three

Unordered nested list:

-   one
    wrapped
-   two
    -   three
        wrapped
    -   four
-   five
    - six
    - seven

Ordered nested list:

1.  one
    wrapped
1.  two
    1.  three
        wrapped
    1.  four
1.  five
    1. six
    1. seven

Mixed nested lists A:

1.  one
    wrapped
1.  two
    -   three
        wrapped
    -   four
1.  five

Mixed nested lists A:

-   one
    wrapped
-   two
    1.  three
        wrapped
    1.  four
-   five
"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // We expect warnings for lines with more than the configured spaces after the marker
    assert!(!result.is_empty(), "Should flag lines with too many spaces after list marker");
    // Check that the fix produces the expected output (all list markers have correct spaces)
    let fixed = rule.fix(&ctx).unwrap();
    // All unordered multi-line items should have 3 spaces, single-line 1; ordered multi-line 2, single-line 1
    // Spot check a few lines
    assert!(fixed.contains("-   one\n    wrapped"), "Unordered multi-line should have 3 spaces");
    assert!(fixed.contains("1.  one\n    wrapped"), "Ordered multi-line should have 2 spaces");
    assert!(fixed.contains("- one"), "Unordered single-line should have 1 space");
    assert!(fixed.contains("1. one"), "Ordered single-line should have 1 space");
}
