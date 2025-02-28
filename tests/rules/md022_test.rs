use rumdl::rules::MD022BlanksAroundHeadings;
use rumdl::rule::Rule;

#[test]
fn test_valid_headings() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "# Heading 1\n\nSome content here.\n\n## Heading 2\n\nMore content here.\n\n### Heading 3\n\nFinal content.";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_headings() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "# Heading 1\nSome content here.\n## Heading 2\nMore content here.\n### Heading 3\nFinal content.";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 6);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].message, "Heading should have 1 blank line below");
}

#[test]
fn test_first_heading() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "# First Heading\n\nSome content.\n\n## Second Heading\n\nMore content.";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_code_block() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "# Real Heading\n\nSome content.\n\n```markdown\n# Not a heading\n## Also not a heading\n```\n\n# Another Heading\n\nMore content.";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_front_matter() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "---\ntitle: Test\n---\n\n# First Heading\n\nContent here.\n\n## Second Heading\n\nMore content.";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_headings() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "# Heading 1\nSome content here.\n## Heading 2\nMore content here.\n### Heading 3\nFinal content.";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Heading 1\n\nSome content here.\n\n## Heading 2\n\nMore content here.\n\n### Heading 3\n\nFinal content.");
}

#[test]
fn test_fix_mixed_headings() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "# Heading 1\n\nSome content here.\n## Heading 2\nMore content here.\n\n### Heading 3\nFinal content.";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Heading 1\n\nSome content here.\n\n## Heading 2\n\nMore content here.\n\n### Heading 3\n\nFinal content.");
}

#[test]
fn test_preserve_code_blocks() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "# Real Heading\n\nSome content.\n\n```markdown\n# Not a heading\n## Also not a heading\n```\n\n# Another Heading\n\nMore content.";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Real Heading\n\nSome content.\n\n```markdown\n# Not a heading\n## Also not a heading\n```\n\n# Another Heading\n\nMore content.");
}

#[test]
fn test_custom_blank_lines() {
    let rule = MD022BlanksAroundHeadings::new(2, 2);
    let content = "# Heading 1\nSome content here.\n## Heading 2\nMore content here.";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
    assert_eq!(result[0].message, "Heading should have 2 blank lines below");
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Heading 1\n\n\nSome content here.\n\n\n## Heading 2\n\n\nMore content here.");
}

#[test]
fn test_setext_headings() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "Heading 1\n=========\nSome content.\nHeading 2\n---------\nMore content.";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Heading 1\n=========\n\nSome content.\n\nHeading 2\n---------\n\nMore content.");
}

#[test]
fn test_empty_headings() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "#\nSome content.\n##\nMore content.\n###\nFinal content.";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 6);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "#\n\nSome content.\n\n##\n\nMore content.\n\n###\n\nFinal content.");
}

#[test]
fn test_consecutive_headings() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3\nContent here.";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Heading 1\n\n## Heading 2\n\n### Heading 3\n\nContent here.");
}

#[test]
fn test_indented_headings() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "  # Heading 1\nContent 1.\n    ## Heading 2\nContent 2.\n      ### Heading 3\nContent 3.";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 6);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "  # Heading 1\n\nContent 1.\n\n    ## Heading 2\n\nContent 2.\n\n      ### Heading 3\n\nContent 3.");
}
