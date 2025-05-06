use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD047SingleTrailingNewline;

#[test]
fn test_valid_file_end_newline() {
    let rule = MD047SingleTrailingNewline;
    let content = "Some text\nMore text\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_missing_file_end_newline() {
    let rule = MD047SingleTrailingNewline;
    let content = "Some text\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_multiple_file_end_newlines() {
    let rule = MD047SingleTrailingNewline;
    let content = "Some text\nMore text\n\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_fix_file_end_newline() {
    let rule = MD047SingleTrailingNewline;
    let content = "Some text\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Some text\nMore text\n");
}
