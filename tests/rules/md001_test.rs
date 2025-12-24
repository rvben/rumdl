use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD001HeadingIncrement;

#[test]
pub fn test_md001_valid() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
pub fn test_md001_invalid() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Heading 1\n### Heading 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "Expected heading level 2, but found heading level 3");
}

#[test]
pub fn test_md001_multiple_violations() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Heading 1\n### Heading 3\n#### Heading 4\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
pub fn test_md001_fix() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Heading 1\n### Heading 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1\n## Heading 3\n");
}

#[test]
pub fn test_md001_no_headings() {
    let rule = MD001HeadingIncrement::default();
    let content = "This is a paragraph\nwith no headings.\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
pub fn test_md001_single_heading() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Single Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
pub fn test_md001_atx_and_setext() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Heading 1\nHeading 2\n---------\n### Heading 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
pub fn test_md001_ignores_headings_in_html_comments() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Real Heading 1\n\n<!--\n## This heading is in a comment\n### This one too\n-->\n\n### This should trigger MD001\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should get exactly one warning for the level 3 heading that comes after level 1
    assert_eq!(result.len(), 1, "Should have one MD001 violation, but got: {result:?}");
    assert_eq!(result[0].line, 8, "MD001 violation should be on line 8");
    assert_eq!(result[0].message, "Expected heading level 2, but found heading level 3");
}

#[test]
pub fn test_md001_html_comments_dont_affect_heading_sequence() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Heading 1\n\n<!--\n#### Random comment heading\n-->\n\n## Heading 2\n### Heading 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have no violations - the comment heading shouldn't affect the sequence
    assert!(
        result.is_empty(),
        "Should have no violations when HTML comment headings don't interfere, but got: {result:?}"
    );
}
