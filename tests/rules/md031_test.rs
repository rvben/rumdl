use rumdl::rule::Rule;
use rumdl::rules::MD031BlanksAroundFences;

#[test]
fn test_valid_fenced_blocks() {
    let rule = MD031BlanksAroundFences;
    let content = "Text before\n\n```\ncode block\n```\n\nText after";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_blank_before() {
    let rule = MD031BlanksAroundFences;
    let content = "Text before\n```\ncode block\n```\n\nText after";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].column, 1);
}

#[test]
fn test_no_blank_after() {
    let rule = MD031BlanksAroundFences;
    let content = "Text before\n\n```\ncode block\n```\nText after";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 5);
    assert_eq!(result[0].column, 1);
}

#[test]
fn test_fix_missing_blanks() {
    let rule = MD031BlanksAroundFences;
    let content = "Text before\n```\ncode block\n```\nText after";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "Text before\n\n```\ncode block\n```\n\nText after");
}
