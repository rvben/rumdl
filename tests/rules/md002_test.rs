use rustmark::rules::MD002FirstHeadingH1;
use rustmark::rule::Rule;

#[test]
fn test_custom_level() {
    let rule = MD002FirstHeadingH1::new(2);
    let content = "## Heading\n### Subheading";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_first_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading\n### Subheading";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
}

#[test]
fn test_no_headings() {
    let rule = MD002FirstHeadingH1::default();
    let content = "This is a paragraph\nAnother paragraph";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_only_one_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "# Heading";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_closed_atx_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading ##\n### Subheading ###";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Heading #\n### Subheading ###");
}

#[test]
fn test_fix_first_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading\n### Subheading";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Heading\n### Subheading");
}

#[test]
fn test_fix_closed_atx_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading ##\n### Subheading ###";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Heading #\n### Subheading ###");
}

#[test]
fn test_mixed_heading_styles() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading\n### Subheading ###\n#### Another heading";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Heading\n### Subheading ###\n#### Another heading");
}

#[test]
fn test_indented_first_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "  ## Heading\n### Subheading";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "  # Heading\n### Subheading");
}

#[test]
fn test_setext_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "Heading\n-------\n### Subheading";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Heading\n### Subheading");
}

#[test]
fn test_with_front_matter() {
    let rule = MD002FirstHeadingH1::default();
    let content = "---\ntitle: Test\n---\n## Heading\n### Subheading";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 4);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "---\ntitle: Test\n---\n# Heading\n### Subheading");
}

#[test]
fn test_setext_with_front_matter() {
    let rule = MD002FirstHeadingH1::default();
    let content = "---\ntitle: Test\n---\nHeading\n-------\n### Subheading";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 4);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "---\ntitle: Test\n---\n# Heading\n### Subheading");
} 