use rumdl::rules::MD053LinkImageReferenceDefinitions;
use rumdl::rule::Rule;

#[test]
fn test_references_in_blockquote() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "> [link][id]\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_references_in_code_blocks() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "```markdown\n[link][id]\n```\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0); // Should be ignored since it's in a code block
}

#[test]
fn test_references_with_special_chars() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][special-id!@#]\n\n[special-id!@#]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_references_with_unicode() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][üñîçødé]\n\n[üñîçødé]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_with_trailing_whitespace() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][id  ]\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_with_leading_whitespace() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][  id]\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_reference_with_unusual_formatting() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][id]\n\n[id]:    http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_consecutive_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[link][id1]\n\n[id1]: http://example1.com\n[id2]: http://example2.com\n[id3]: http://example3.com";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "[link][id1]\n\n[id1]: http://example1.com\n");
}

#[test]
fn test_fix_references_at_document_start() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[unused]: http://example.com\n\n# Heading\nSome content";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Heading\nSome content");
}

#[test]
fn test_fix_references_at_document_end() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "# Heading\nSome content\n\n[unused]: http://example.com";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Heading\nSome content\n");
}

#[test]
fn test_ignored_definitions_case_insensitivity() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec!["IGNORED".to_string()]);
    let content = "[ignored]: http://example.com\nNo references here";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_ignored_definitions() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec!["ignore1".to_string(), "ignore2".to_string()]);
    let content = "[ignore1]: http://example1.com\n[ignore2]: http://example2.com\n[used]: http://example3.com\n[used] is a link";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_reference_link() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "[text][]\n\n[text]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_image_reference() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let content = "![alt][]\n\n[alt]: http://example.com/image.png";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_performance_with_many_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
    let mut content = String::new();
    
    // Add 100 reference definitions
    for i in 1..101 {
        content.push_str(&format!("[ref{}]: http://example.com/{}\n", i, i));
    }
    
    // Use only 50 of them
    content.push_str("\n");
    for i in 1..51 {
        content.push_str(&format!("[link{}][ref{}]\n", i, i));
    }
    
    let result = rule.check(&content).unwrap();
    assert_eq!(result.len(), 50); // 50 unused references
}
