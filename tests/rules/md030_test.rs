use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD030ListMarkerSpace;

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
    assert!(result.is_empty());
}

#[test]
fn test_invalid_spaces_ordered() {
    let rule = MD030ListMarkerSpace::default();
    let content = "1.  Too many spaces\n2.   Three spaces";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_ignore_code_blocks() {
    let rule = MD030ListMarkerSpace::default();
    let content = "* Normal item\n```\n*  Not a list\n1.  Not a list\n```\n- Back to list";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}
