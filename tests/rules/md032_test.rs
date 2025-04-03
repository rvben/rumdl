use rumdl::rule::Rule;
use rumdl::rules::MD032BlanksAroundLists;

#[test]
fn test_valid_lists() {
    let rule = MD032BlanksAroundLists;
    let content = "Some text\n\n* Item 1\n* Item 2\n\nMore text";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_missing_blank_line_before() {
    let rule = MD032BlanksAroundLists;
    let content = "Some text\n* Item 1\n* Item 2\n\nMore text";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_missing_blank_line_after() {
    let rule = MD032BlanksAroundLists;
    let content = "Some text\n\n* Item 1\n* Item 2\nMore text";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_fix_missing_blank_lines() {
    let rule = MD032BlanksAroundLists;
    let content = "Text\n* Item 1\n* Item 2\nMore text";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Text\n\n* Item 1\n* Item 2\n\nMore text");
}

#[test]
fn test_multiple_lists() {
    let rule = MD032BlanksAroundLists;
    let content = "Text\n* List 1\n* List 1\nText\n1. List 2\n2. List 2\nText";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "Text\n\n* List 1\n* List 1\n\nText\n\n1. List 2\n2. List 2\n\nText"
    );
}

#[test]
fn test_nested_lists() {
    let rule = MD032BlanksAroundLists;
    let content = "Text\n* Item 1\n  * Nested 1\n  * Nested 2\n* Item 2\nText";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "Text\n\n* Item 1\n  * Nested 1\n  * Nested 2\n* Item 2\n\nText"
    );
}

#[test]
fn test_mixed_list_types() {
    let rule = MD032BlanksAroundLists;
    let content = "Text\n* Unordered\n* List\nText\n1. Ordered\n2. List\nText";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "Text\n\n* Unordered\n* List\n\nText\n\n1. Ordered\n2. List\n\nText"
    );
}

#[test]
fn test_list_with_content() {
    let rule = MD032BlanksAroundLists;
    let content = "Text\n* Item 1\n  Content\n* Item 2\n  More content\nText";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "Text\n\n* Item 1\n  Content\n* Item 2\n  More content\n\nText"
    );
}

#[test]
fn test_list_at_start() {
    let rule = MD032BlanksAroundLists;
    let content = "* Item 1\n* Item 2\nText";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "* Item 1\n* Item 2\n\nText");
}

#[test]
fn test_list_at_end() {
    let rule = MD032BlanksAroundLists;
    let content = "Text\n* Item 1\n* Item 2";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Text\n\n* Item 1\n* Item 2");
}

#[test]
fn test_multiple_blank_lines() {
    let rule = MD032BlanksAroundLists;
    let content = "Text\n\n\n* Item 1\n* Item 2\n\n\nText";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_list_with_blank_lines() {
    let rule = MD032BlanksAroundLists;
    let content = "Text\n\n* Item 1\n\n* Item 2\n\nText";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}
