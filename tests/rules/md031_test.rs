use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD031BlanksAroundFences;

#[test]
fn test_valid_fenced_blocks() {
    let rule = MD031BlanksAroundFences;
    let content = "Text before\n\n```\ncode block\n```\n\nText after";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_blank_before() {
    let rule = MD031BlanksAroundFences;
    let content = "Text before\n```\ncode block\n```\n\nText after";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].column, 1);
}

#[test]
fn test_no_blank_after() {
    let rule = MD031BlanksAroundFences;
    let content = "Text before\n\n```\ncode block\n```\nText after";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 5);
    assert_eq!(result[0].column, 1);
}

#[test]
fn test_fix_missing_blanks() {
    let rule = MD031BlanksAroundFences;
    let content = "Text before\n```\ncode block\n```\nText after";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&result);
    let fixed_result = rule.check(&fixed_ctx).unwrap();
    assert_eq!(fixed_result, Vec::new());
}
