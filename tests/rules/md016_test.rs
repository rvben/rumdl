use rumdl::rule::Rule;
use rumdl::rules::MD016NoMultipleSpaceAfterListMarker;
use rumdl::lint_context::LintContext;

#[test]
fn test_valid_list_items() {
    let rule = MD016NoMultipleSpaceAfterListMarker::default();
    let content = "- Item 1\n* Item 2\n+ Item 3\n1. Ordered item";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_list_items() {
    let rule = MD016NoMultipleSpaceAfterListMarker::default();
    let content = "-  Item 1\n*   Item 2\n+    Item 3\n1.  Ordered item";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4);
}

#[test]
fn test_fix_list_items() {
    let rule = MD016NoMultipleSpaceAfterListMarker::default();
    let content = "-  Item 1\n*   Item 2\n+    Item 3\n1.  Ordered item";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "- Item 1\n* Item 2\n+ Item 3\n1. Ordered item");
}

#[test]
fn test_allow_multiple_spaces() {
    let rule = MD016NoMultipleSpaceAfterListMarker::with_allow_multiple_spaces(true);
    let content = "-  Item 1\n*   Item 2\n+    Item 3\n1.  Ordered item";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_valid_unordered_list() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "* Item 1\n* Item 2\n* Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_valid_ordered_list() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "1. First\n2. Second\n3. Third";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_unordered_list() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "*  Item 1\n*   Item 2\n*    Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 1);
    assert_eq!(
        result[0].message,
        "Multiple spaces after unordered list marker"
    );
}

#[test]
fn test_invalid_ordered_list() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "1.  First\n2.   Second\n3.    Third";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 1);
    assert_eq!(
        result[0].message,
        "Multiple spaces after ordered list marker"
    );
}

#[test]
fn test_mixed_list_types() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "*  Item 1\n1.  First\n-  Item 2\n2.  Second";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4);
}

#[test]
fn test_nested_lists() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "* Item 1\n  *  Nested 1\n  *   Nested 2\n* Item 2";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_code_block() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "```markdown\n*  Item 1\n*   Item 2\n```\n* Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_unordered_list() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "*  Item 1\n*   Item 2\n*    Item 3";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "* Item 1\n* Item 2\n* Item 3");
}

#[test]
fn test_fix_ordered_list() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "1.  First\n2.   Second\n3.    Third";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "1. First\n2. Second\n3. Third");
}

#[test]
fn test_fix_mixed_list_types() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "*  Item 1\n1.  First\n-  Item 2\n2.  Second";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "* Item 1\n1. First\n- Item 2\n2. Second");
}

#[test]
fn test_fix_nested_lists() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "* Item 1\n  *  Nested 1\n  *   Nested 2\n* Item 2";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "* Item 1\n  * Nested 1\n  * Nested 2\n* Item 2");
}

#[test]
fn test_list_marker_variations() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "*  Item\n-   Item\n+    Item";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "* Item\n- Item\n+ Item");
}

#[test]
fn test_preserve_indentation() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "  *  Item 1\n    *   Item 2\n      *    Item 3";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    if result != "  * Item 1\n    * Item 2\n      * Item 3" {
        println!("[DEBUG] Actual result: {:?}", result);
    }
    assert_eq!(result, "  * Item 1\n    * Item 2\n      * Item 3");
}

#[test]
fn test_preserve_code_blocks() {
    let rule = MD016NoMultipleSpaceAfterListMarker::new();
    let content = "* Item 1\n```\n*  Not a list\n```\n* Item 2";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "* Item 1\n```\n*  Not a list\n```\n* Item 2");
}

#[test]
fn test_readme_md016() {
    let rule = MD016NoMultipleSpaceAfterListMarker::default();
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
    // We expect warnings for lines with more than one space after the marker
    assert!(!result.is_empty(), "Should flag lines with more than one space after list marker");
    // Check that the fix produces the expected output (all list markers have only one space)
    let fixed = rule.fix(&ctx).unwrap();
    // Spot check a few lines
    assert!(fixed.contains("- one\n    wrapped"), "Unordered list should have only one space");
    assert!(fixed.contains("1. one\n    wrapped"), "Ordered list should have only one space");
}
