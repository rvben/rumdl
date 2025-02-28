use rumdl::rules::MD029OrderedListPrefix;
use rumdl::rule::Rule;

#[test]
fn test_md029_valid() {
    let rule = MD029OrderedListPrefix::default();
    let content = "1. First item\n2. Second item\n3. Third item\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md029_invalid() {
    let rule = MD029OrderedListPrefix::default();
    let content = "1. First item\n3. Second item\n5. Third item\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[1].line, 3);
}

#[test]
fn test_md029_nested() {
    let rule = MD029OrderedListPrefix::default();
    let content = "1. First item\n   1. Nested first\n   2. Nested second\n2. Second item\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md029_fix() {
    let rule = MD029OrderedListPrefix::default();
    let content = "1. First item\n3. Second item\n5. Third item\n";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "1. First item\n2. Second item\n3. Third item\n");
} 