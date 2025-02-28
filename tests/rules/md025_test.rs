use rumdl::rules::MD025SingleTitle;
use rumdl::rule::Rule;

#[test]
fn test_md025_valid() {
    let rule = MD025SingleTitle::default();
    let content = "# Title\n## Heading 2\n### Heading 3\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md025_invalid() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n# Title 2\n## Heading\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_md025_no_title() {
    let rule = MD025SingleTitle::default();
    let content = "## Heading 2\n### Heading 3\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md025_fix() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n# Title 2\n## Heading\n";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Title 1\n## Title 2\n## Heading\n");
} 