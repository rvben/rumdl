use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::heading_utils::HeadingStyle;
use rumdl::rules::MD003HeadingStyle;

#[test]
fn test_consistent_atx() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_atx_closed() {
    let rule = MD003HeadingStyle::new(HeadingStyle::AtxClosed);
    let content = "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_styles() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1\n## Heading 2 ##\n### Heading 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_fix_mixed_styles() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1\n## Heading 2 ##\n### Heading 3";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading 1\n## Heading 2\n### Heading 3");
}

#[test]
fn test_fix_to_atx_closed() {
    let rule = MD003HeadingStyle::new(HeadingStyle::AtxClosed);
    let content = "# Heading 1\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}

#[test]
fn test_indented_headings() {
    let rule = MD003HeadingStyle::default();
    let content = "  # Heading 1\n  ## Heading 2\n  ### Heading 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_indentation() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1\n  ## Heading 2\n    ### Heading 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_preserve_content() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading with *emphasis* and **bold**\n## Another heading with [link](url)";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, content);
}

#[test]
fn test_empty_headings() {
    let rule = MD003HeadingStyle::default();
    let content = "#\n##\n###";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_heading_with_trailing_space() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1  \n## Heading 2  \n### Heading 3  ";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_setext() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Heading 1\n=========\n\nHeading 2\n---------";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_setext_atx() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Heading 1\n=========\n\n## Heading 2";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 4);
}

#[test]
fn test_fix_to_setext() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "# Heading 1\n## Heading 2";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Heading 1\n=========\nHeading 2\n---------");
}

#[test]
fn test_setext_with_formatting() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Heading with *emphasis*\n====================\n\nHeading with **bold**\n--------------------";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_mixed_setext_atx() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Heading 1\n=========\n\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "Heading 1\n=========\n\nHeading 2\n---------\n### Heading 3"
    );
}

#[test]
fn test_setext_with_indentation() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "  Heading 1\n  =========\n\n  Heading 2\n  ---------";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_with_front_matter() {
    let rule = MD003HeadingStyle::default();
    let content = "---\ntitle: \"Test Document\"\nauthor: \"Test Author\"\ndate: \"2024-04-03\"\n---\n\n# Heading 1\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Expected no warnings with front matter followed by ATX headings, but got {} warnings",
        result.len()
    );
}
