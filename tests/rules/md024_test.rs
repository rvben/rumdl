use rumdl::rule::Rule;
use rumdl::rules::MD024MultipleHeadings;

#[test]
fn test_md024_valid() {
    let rule = MD024MultipleHeadings::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md024_invalid() {
    let rule = MD024MultipleHeadings::default();
    let content = "# Heading\n## Subheading\n# Heading\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md024_different_levels() {
    let rule = MD024MultipleHeadings::default();
    let content = "# Heading\n## Heading\n### Heading\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_md024_different_levels_with_allow_different_nesting() {
    let rule = MD024MultipleHeadings::new(true);
    let content = "# Heading\n## Heading\n### Heading\n";
    let result = rule.check(content).unwrap();

    // Since we're converting all headings to lowercase with the same content,
    // we should expect 2 warnings (one for each duplicate heading)
    assert_eq!(
        result.len(),
        2,
        "Expected 2 warnings for duplicated headings with allow_different_nesting=true"
    );
    assert_eq!(result[0].line, 2);
    assert_eq!(result[1].line, 3);
}

#[test]
fn test_md024_different_case() {
    let rule = MD024MultipleHeadings::default();
    let content = "# Heading\n## Subheading\n# heading\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_md024_with_setext_headings() {
    let rule = MD024MultipleHeadings::default();
    let content = "Heading 1\n=========\nSome text\n\nHeading 1\n=========\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 5);
}

#[test]
fn test_md024_mixed_heading_styles() {
    let rule = MD024MultipleHeadings::default();
    let content = "# Heading\n\nHeading\n=======\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md024_with_empty_headings() {
    let rule = MD024MultipleHeadings::default();
    // Empty headings should be ignored by the rule
    let content = "#\n## \n###  \n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md024_in_code_blocks() {
    let rule = MD024MultipleHeadings::default();
    let content = "# Heading\n\n```markdown\n# Heading\n```\n# Heading\n";
    let result = rule.check(content).unwrap();
    // The heading in the code block should not be counted
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 6);
}

#[test]
fn test_md024_with_front_matter() {
    let rule = MD024MultipleHeadings::default();
    let content = "---\ntitle: My Document\n---\n# Heading\n\nSome text\n\n# Heading\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 8);
}

#[test]
fn test_md024_with_closed_atx_headings() {
    let rule = MD024MultipleHeadings::default();
    let content = "# Heading #\n\n## Subheading ##\n\n# Heading #\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 5);
}

#[test]
fn test_md024_with_multiple_duplicates() {
    let rule = MD024MultipleHeadings::default();
    let content = "# Heading\n\n## Subheading\n\n# Heading\n\n## Subheading\n\n# Heading\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 5);
    assert_eq!(result[1].line, 7);
    assert_eq!(result[2].line, 9);
}

#[test]
fn test_md024_with_trailing_whitespace() {
    let rule = MD024MultipleHeadings::default();
    let content = "# Heading \n\n# Heading\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md024_performance_with_many_headings() {
    let rule = MD024MultipleHeadings::default();

    // Create a document with 100 unique headings
    let mut content = String::new();
    for i in 1..=100 {
        content.push_str(&format!("# Heading {}\n\n", i));
    }

    let start = std::time::Instant::now();
    let result = rule.check(&content).unwrap();
    let duration = start.elapsed();

    assert!(result.is_empty());
    assert!(
        duration.as_millis() < 100,
        "Checking 100 unique headings should take less than 100ms"
    );
}

#[test]
fn test_md024_fix() {
    let rule = MD024MultipleHeadings::default();
    let content = "# Heading\n## Subheading\n# Heading\n";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, content, "Fix method should not modify content");
}
