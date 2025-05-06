use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD015NoMissingSpaceAfterListMarker;

#[test]
fn test_valid_unordered_list() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "* Item 1\n* Item 2\n* Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_valid_ordered_list() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "1. First\n2. Second\n3. Third";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_unordered_list() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "*Item 1\n*Item 2\n*Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 1);
    assert_eq!(
        result[0].message,
        "Missing space after unordered list marker"
    );
}

#[test]
fn test_invalid_ordered_list() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "1.First\n2.Second\n3.Third";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 1);
    assert_eq!(result[0].message, "Missing space after ordered list marker");
}

#[test]
fn test_mixed_list_types() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "*Item 1\n1.First\n-Item 2\n2.Second";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4);
}

#[test]
fn test_nested_lists() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "* Item 1\n  *Nested 1\n  *Nested 2\n* Item 2";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_code_block() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "```markdown\n*Item 1\n*Item 2\n```\n* Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_unordered_list() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "*Item 1\n*Item 2\n*Item 3";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "* Item 1\n* Item 2\n* Item 3");
}

#[test]
fn test_fix_ordered_list() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "1.First\n2.Second\n3.Third";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "1. First\n2. Second\n3. Third");
}

#[test]
fn test_fix_mixed_list_types() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "*Item 1\n1.First\n-Item 2\n2.Second";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "* Item 1\n1. First\n- Item 2\n2. Second");
}

#[test]
fn test_fix_nested_lists() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "* Item 1\n  *Nested 1\n  *Nested 2\n* Item 2";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "* Item 1\n  * Nested 1\n  * Nested 2\n* Item 2");
}

#[test]
fn test_disabled_rule() {
    let rule = MD015NoMissingSpaceAfterListMarker::with_require_space(false);
    let content = "*Item 1\n*Item 2\n*Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_list_marker_variations() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "*Item\n-Item\n+Item";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "* Item\n- Item\n+ Item");
}

#[test]
fn test_preserve_indentation() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();

    // Test with a single line to verify indentation preservation
    let single_line = "  *Item 1";
    let ctx = LintContext::new(single_line);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "  * Item 1");

    // Test with multiple lines to verify each line is fixed properly
    let multi_line = "  *Item 1\n  *Item 2\n  *Item 3";
    let ctx = LintContext::new(multi_line);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "  * Item 1\n  * Item 2\n  * Item 3");

    // Test with increasing indentation
    let indented = "  *Item 1\n    *Item 2\n      *Item 3";
    let ctx = LintContext::new(indented);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "  * Item 1\n    * Item 2\n      * Item 3");
}

#[test]
fn test_preserve_code_blocks() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "* Item 1\n```\n*Not a list\n```\n* Item 2";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "* Item 1\n```\n*Not a list\n```\n* Item 2");
}

#[test]
fn test_horizontal_rule() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();

    // Test with asterisk horizontal rule
    let content = "***";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Horizontal rule with asterisks should not trigger warnings"
    );
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "***", "Horizontal rule should not be modified");

    // Test with dash horizontal rule
    let content = "---";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Horizontal rule with dashes should not trigger warnings"
    );

    // Test with underscore horizontal rule
    let content = "___";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Horizontal rule with underscores should not trigger warnings"
    );

    // Test with longer horizontal rules
    let content = "*****";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Longer horizontal rule should not trigger warnings"
    );

    // Test with spaced horizontal rules
    let content = "* * *";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Spaced horizontal rule should not trigger warnings"
    );

    // Test with horizontal rule in context
    let content = "# Heading\n\n***\n\nParagraph after rule.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Horizontal rule in content should not trigger warnings"
    );
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading\n\n***\n\nParagraph after rule.");
}

#[test]
fn test_fixes_missing_space() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "-Item 1\n*Item 2\n+ Item 3";
    let expected = "- Item 1\n* Item 2\n+ Item 3";
    let ctx = LintContext::new(content);
    assert_eq!(rule.fix(&ctx).unwrap(), expected);
}

#[test]
fn test_preserves_valid_items() {
    let rule = MD015NoMissingSpaceAfterListMarker::new();
    let content = "- Valid item\n*  Properly spaced\n+   Correct";
    let ctx = LintContext::new(content);
    assert_eq!(rule.fix(&ctx).unwrap(), content);
}
