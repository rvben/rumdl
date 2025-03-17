use rumdl::rule::Rule;
use rumdl::rules::MD023HeadingStartLeft;

#[test]
fn test_valid_heading_positions() {
    let rule = MD023HeadingStartLeft;
    let content = "# Heading 1\n## Heading 2\n### Heading 3";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_indented_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "   # Indented heading\n  ## Also indented";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
}

#[test]
fn test_fix_indented_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "   # Heading 1\n  ## Heading 2";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Heading 1\n## Heading 2");
}

#[test]
fn test_mixed_content() {
    let rule = MD023HeadingStartLeft;
    let content = "# Good heading\n   # Bad heading\nNormal text\n  ## Another bad one";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_closed_atx_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "   # Heading 1 #\n  ## Heading 2 ##";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2 ##");
}

#[test]
fn test_preserve_heading_content() {
    let rule = MD023HeadingStartLeft;
    let content = "   # Complex *heading* with **markdown**";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Complex *heading* with **markdown**");
}

#[test]
fn test_ignore_non_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "   Not a heading\n  Also not a heading";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_heading_levels() {
    let rule = MD023HeadingStartLeft;
    let content = "   # H1\n  ## H2\n ### H3\n#### H4";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3); // Only the indented ones should be flagged
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# H1\n## H2\n### H3\n#### H4");
}
