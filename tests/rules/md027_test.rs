use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD027MultipleSpacesBlockquote;

#[test]
fn test_md027_valid() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = "> Quote\n> Another line\n> Third line\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md027_invalid() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = ">  Quote\n>   Another line\n>    Third line\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 3);
}

#[test]
fn test_md027_mixed() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = "> Quote\n>  Another line\n> Third line\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_md027_fix() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = ">  Quote\n>   Another line\n>    Third line\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "> Quote\n> Another line\n> Third line\n");
}
