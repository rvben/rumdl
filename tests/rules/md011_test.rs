use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD011NoReversedLinks;

#[test]
fn test_md011_valid() {
    let rule = MD011NoReversedLinks {};
    let content = "[text](link)\n[more text](another/link)\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md011_invalid() {
    let rule = MD011NoReversedLinks {};
    let content = "(link)[text]\n(another/link)[more text]\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
}

#[test]
fn test_md011_mixed() {
    let rule = MD011NoReversedLinks {};
    let content = "[text](link)\n(link)[reversed]\n[text](link)\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_md011_fix() {
    let rule = MD011NoReversedLinks {};
    let content = "(link)[text]\n(another/link)[more text]\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "[text](link)\n[more text](another/link)\n");
}
