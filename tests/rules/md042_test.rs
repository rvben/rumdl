use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD042NoEmptyLinks;

#[test]
fn test_valid_links() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[Link text](https://example.com)\n[Another link](./local/path)";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_link_text() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[](https://example.com)";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "");
}

#[test]
fn test_empty_link_url() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[Link text]()";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "");
}

#[test]
fn test_empty_link_both() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[]()";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "");
}

#[test]
fn test_multiple_empty_links() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[Link]() and []() and [](url)";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, " and  and ");
}

#[test]
fn test_whitespace_only_links() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[ ](  )";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "");
}

#[test]
fn test_mixed_valid_and_empty_links() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[Valid](https://example.com) and []() and [Another](./path)";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "[Valid](https://example.com) and  and [Another](./path)"
    );
}
