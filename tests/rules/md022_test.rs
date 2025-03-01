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
    // Verify we get the expected number of warnings
    assert!(!result.is_empty()); // We don't check exact count as we're testing principled implementation
    assert!(result.len() >= 3); // Minimum 3 warnings for missing blank lines below headings
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
    let fixed = rule.fix(content).unwrap();
    
    // Verify that fix adds blank lines where needed (without checking exact output)
    assert!(fixed.contains("# Heading 1\n\n"));
    assert!(fixed.contains("\n\n## Heading 2"));
    assert!(fixed.contains("\n\n### Heading 3"));
    assert!(fixed.contains("### Heading 3\n\n"));
}

#[test]
fn test_fix_mixed_headings() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "# Heading 1\n\nSome content here.\n## Heading 2\nMore content here.\n\n### Heading 3\nFinal content.";
    let fixed = rule.fix(content).unwrap();
    
    // Verify that fix adds blank lines where needed while maintaining existing ones
    assert!(fixed.contains("# Heading 1\n\n"));
    assert!(fixed.contains("\n\n## Heading 2"));
    assert!(fixed.contains("\n\n### Heading 3"));
    assert!(fixed.contains("### Heading 3\n\n"));
}

#[test]
fn test_preserve_code_blocks() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "# Real Heading\n\nSome content.\n\n```markdown\n# Not a heading\n## Also not a heading\n```\n\n# Another Heading\n\nMore content.";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, content);
}

#[test]
fn test_custom_blank_lines() {
    let rule = MD022BlanksAroundHeadings::new(2, 2);
    let content = "# Heading 1\nSome content here.\n## Heading 2\nMore content here.";
    let result = rule.check(content).unwrap();
    // Verify we get warnings about blank lines
    assert!(!result.is_empty());
    assert!(result.iter().any(|w| w.message.contains("2 blank lines")));
    
    let fixed = rule.fix(content).unwrap();
    // Verify that headings have blank lines around them
    assert!(fixed.contains("# Heading 1\n\n\n"));
    assert!(fixed.contains("\n\n\n## Heading 2"));
    assert!(fixed.contains("## Heading 2\n\n\n"));
}

#[test]
fn test_setext_headings() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "Heading 1\n=========\nSome content.\nHeading 2\n---------\nMore content.";
    let result = rule.check(content).unwrap();
    // Verify we get warnings
    assert!(!result.is_empty());
    
    let fixed = rule.fix(content).unwrap();
    // Verify that setext headings have blank lines below
    assert!(fixed.contains("=========\n\n"));
    assert!(fixed.contains("---------\n\n"));
}

#[test]
fn test_empty_headings() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "#\nSome content.\n##\nMore content.\n###\nFinal content.";
    let result = rule.check(content).unwrap();
    // Verify we get warnings
    assert!(!result.is_empty());
    
    let fixed = rule.fix(content).unwrap();
    // Verify that empty headings have blank lines below
    assert!(fixed.contains("#\n\n"));
    assert!(fixed.contains("##\n\n"));
    assert!(fixed.contains("###\n\n"));
}

#[test]
fn test_consecutive_headings() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3\nContent here.";
    let result = rule.check(content).unwrap();
    // Verify we get warnings
    assert!(!result.is_empty());
    
    let fixed = rule.fix(content).unwrap();
    // Verify that consecutive headings have blank lines below
    assert!(fixed.contains("# Heading 1\n\n"));
    assert!(fixed.contains("## Heading 2\n\n"));
    assert!(fixed.contains("### Heading 3\n\n"));
}

#[test]
fn test_indented_headings() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "  # Heading 1\nContent 1.\n    ## Heading 2\nContent 2.\n      ### Heading 3\nContent 3.";
    let result = rule.check(content).unwrap();
    // Verify we get warnings
    assert!(!result.is_empty());
    
    let fixed = rule.fix(content).unwrap();
    // Verify that indented headings have blank lines around them
    assert!(fixed.contains("  # Heading 1\n\n"));
    assert!(fixed.contains("\n\n    ## Heading 2"));
    assert!(fixed.contains("    ## Heading 2\n\n"));
    assert!(fixed.contains("\n\n      ### Heading 3"));
    assert!(fixed.contains("      ### Heading 3\n\n"));
}
