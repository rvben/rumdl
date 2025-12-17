use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD038NoSpaceInCode;

#[test]
fn test_valid_code_spans() {
    let rule = MD038NoSpaceInCode::new();
    let content = "`code` and `another code` here";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_spaces_both_ends() {
    // CommonMark: single space at start AND end is valid (spaces are stripped)
    // See https://spec.commonmark.org/0.31.2/#code-spans
    let rule = MD038NoSpaceInCode::new();
    let content = "` code ` and ` another code ` here";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Single space at both ends is valid CommonMark");
}

#[test]
fn test_space_at_start() {
    let rule = MD038NoSpaceInCode::new();
    let content = "` code` and ` another code` here";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "`code` and `another code` here");
}

#[test]
fn test_space_at_end() {
    let rule = MD038NoSpaceInCode::new();
    let content = "`code ` and `another code ` here";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "`code` and `another code` here");
}

#[test]
fn test_code_in_code_block() {
    // CommonMark: single space at start AND end is valid (spaces are stripped)
    let rule = MD038NoSpaceInCode::new();
    let content = "```\n` code `\n```\n` code `";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Both code spans have single space at both ends - valid CommonMark
    assert!(result.is_empty(), "Single space at both ends is valid CommonMark");
}

#[test]
fn test_multiple_code_spans() {
    // CommonMark: single space at start AND end is valid (spaces are stripped)
    let rule = MD038NoSpaceInCode::new();
    let content = "` code ` and ` another ` in one line";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Single space at both ends is valid CommonMark");
}

#[test]
fn test_code_with_internal_spaces() {
    // CommonMark: single space at start AND end is valid (spaces are stripped)
    let rule = MD038NoSpaceInCode::new();
    let content = "`this is code` and ` this is also code `";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Second code span has single space at both ends - valid CommonMark
    assert!(result.is_empty(), "Single space at both ends is valid CommonMark");
}

#[test]
fn test_code_with_punctuation() {
    // CommonMark: single space at start AND end is valid (spaces are stripped)
    let rule = MD038NoSpaceInCode::new();
    let content = "` code! ` and ` code? ` here";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Single space at both ends is valid CommonMark");
}
