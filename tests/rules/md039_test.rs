use rumdl::rule::Rule;
use rumdl::rules::MD039NoSpaceInLinks;
use rumdl::lint_context::LintContext;

#[test]
fn test_valid_links() {
    let rule = MD039NoSpaceInLinks;
    let content = "[link](url) and [another link](url) here";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_spaces_both_ends() {
    let rule = MD039NoSpaceInLinks;
    let content = "[ link ](url) and [ another link ](url) here";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx = LintContext::new(&fixed);
    assert_eq!(fixed, "[link](url) and [another link](url) here");
}

#[test]
fn test_space_at_start() {
    let rule = MD039NoSpaceInLinks;
    let content = "[ link](url) and [ another link](url) here";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx = LintContext::new(&fixed);
    assert_eq!(fixed, "[link](url) and [another link](url) here");
}

#[test]
fn test_space_at_end() {
    let rule = MD039NoSpaceInLinks;
    let content = "[link ](url) and [another link ](url) here";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx = LintContext::new(&fixed);
    assert_eq!(fixed, "[link](url) and [another link](url) here");
}

#[test]
fn test_link_in_code_block() {
    let rule = MD039NoSpaceInLinks;
    let content = "```\n[ link ](url)\n```\n[ link ](url)";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx = LintContext::new(&fixed);
    assert_eq!(fixed, "```\n[ link ](url)\n```\n[link](url)");
}

#[test]
fn test_multiple_links() {
    let rule = MD039NoSpaceInLinks;
    let content = "[ link ](url) and [ another ](url) in one line";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx = LintContext::new(&fixed);
    assert_eq!(fixed, "[link](url) and [another](url) in one line");
}

#[test]
fn test_link_with_internal_spaces() {
    let rule = MD039NoSpaceInLinks;
    let content = "[this is link](url) and [ this is also link ](url)";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx = LintContext::new(&fixed);
    assert_eq!(fixed, "[this is link](url) and [this is also link](url)");
}

#[test]
fn test_link_with_punctuation() {
    let rule = MD039NoSpaceInLinks;
    let content = "[ link! ](url) and [ link? ](url) here";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx = LintContext::new(&fixed);
    assert_eq!(fixed, "[link!](url) and [link?](url) here");
}
