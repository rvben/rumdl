use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD001HeadingIncrement;

#[test]
pub fn test_md001_valid() {
    let rule = MD001HeadingIncrement;
    let content = "# Heading 1\n## Heading 2\n### Heading 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
pub fn test_md001_invalid() {
    let rule = MD001HeadingIncrement;
    let content = "# Heading 1\n### Heading 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "Expected heading level 2, but found heading level 3");
}

#[test]
pub fn test_md001_multiple_violations() {
    let rule = MD001HeadingIncrement;
    let content = "# Heading 1\n### Heading 3\n#### Heading 4\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
pub fn test_md001_fix() {
    let rule = MD001HeadingIncrement;
    let content = "# Heading 1\n### Heading 3\n";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1\n## Heading 3\n");
}

#[test]
pub fn test_md001_no_headings() {
    let rule = MD001HeadingIncrement;
    let content = "This is a paragraph\nwith no headings.\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
pub fn test_md001_single_heading() {
    let rule = MD001HeadingIncrement;
    let content = "# Single Heading\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
pub fn test_md001_atx_and_setext() {
    let rule = MD001HeadingIncrement;
    let content = "# Heading 1\nHeading 2\n---------\n### Heading 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}
