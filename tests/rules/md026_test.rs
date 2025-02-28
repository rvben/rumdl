use rumdl::rules::MD026NoTrailingPunctuation;
use rumdl::rule::Rule;

#[test]
fn test_md026_valid() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md026_invalid() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Heading 1!\n## Heading 2?\n### Heading 3.\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 3);
}

#[test]
fn test_md026_mixed() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Heading 1\n## Heading 2!\n### Heading 3\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_md026_fix() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Heading 1!\n## Heading 2?\n### Heading 3.\n";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Heading 1\n## Heading 2\n### Heading 3\n");
}

#[test]
fn test_md026_custom_punctuation() {
    let rule = MD026NoTrailingPunctuation::new("!?".to_string());
    let content = "# Heading 1!\n## Heading 2?\n### Heading 3.\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2); // Only ! and ? should be detected, not .
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
}

#[test]
fn test_md026_setext_headings() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "Heading 1!\n=======\nHeading 2?\n-------\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_md026_closed_atx() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Heading 1! #\n## Heading 2? ##\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2 ##\n");
} 