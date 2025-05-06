use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD023HeadingStartLeft;

#[test]
fn test_valid_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "# Heading 1\n## Heading 2\nHeading 3\n---";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_indented_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "  # Indented\n    ## Indented\n  Heading\n  ---";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_fix_indented_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "  # Indented\n    ## Indented\n  Heading\n  ---";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("# Indented"));
    assert!(fixed.contains("## Indented"));
}

#[test]
fn test_mixed_content() {
    let rule = MD023HeadingStartLeft;
    let content = "# Good heading\n   # Bad heading\nNormal text\n  ## Another bad one";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_closed_atx_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "   # Heading 1 #\n  ## Heading 2 ##";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2 ##");
}

#[test]
fn test_preserve_heading_content() {
    let rule = MD023HeadingStartLeft;
    let content = "   # Complex *heading* with **markdown**";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Complex *heading* with **markdown**");
}

#[test]
fn test_ignore_non_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "   Not a heading\n  Also not a heading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_heading_levels() {
    let rule = MD023HeadingStartLeft;
    let content = "   # H1\n  ## H2\n ### H3\n#### H4";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3); // Only the indented ones should be flagged
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# H1\n## H2\n### H3\n#### H4");
}
