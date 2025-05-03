use rumdl::rule::Rule;
use rumdl::rules::code_fence_utils::CodeFenceStyle;
use rumdl::MD048CodeFenceStyle;

#[test]
fn test_consistent_backticks() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
    let content = "# Code blocks\n\n```\ncode here\n```\n\n```rust\nmore code\n```";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_tildes() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
    let content = "# Code blocks\n\n~~~\ncode here\n~~~\n\n~~~rust\nmore code\n~~~";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_fences_prefer_backticks() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
    let content = "# Mixed blocks\n\n```\nbacktick block\n```\n\n~~~\ntilde block\n~~~";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Mixed blocks\n\n```\nbacktick block\n```\n\n```\ntilde block\n```\n"
    );
}

#[test]
fn test_mixed_fences_prefer_tildes() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
    let content = "# Mixed blocks\n\n```\nbacktick block\n```\n\n~~~\ntilde block\n~~~";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Mixed blocks\n\n~~~\nbacktick block\n~~~\n\n~~~\ntilde block\n~~~\n"
    );
}

#[test]
fn test_consistent_style_first_backtick() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let content = "# Mixed blocks\n\n```\nbacktick block\n```\n\n~~~\ntilde block\n~~~";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Mixed blocks\n\n```\nbacktick block\n```\n\n```\ntilde block\n```\n"
    );
}

#[test]
fn test_consistent_style_first_tilde() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let content = "# Mixed blocks\n\n~~~\ntilde block\n~~~\n\n```\nbacktick block\n```";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Mixed blocks\n\n~~~\ntilde block\n~~~\n\n~~~\nbacktick block\n~~~\n"
    );
}

#[test]
fn test_empty_content() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let content = "";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_code_blocks() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}
