use rumdl::rules::MD024MultipleHeadings;
use rumdl::rule::Rule;

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
    assert!(result.is_empty());
}

#[test]
fn test_md024_fix() {
    let rule = MD024MultipleHeadings::default();
    let content = "# Heading\n## Subheading\n# Heading\n";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Heading\n## Subheading\n# Heading 2\n");
} 