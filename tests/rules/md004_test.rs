use rumdl::rules::{MD004UnorderedListStyle, md004_unordered_list_style::UnorderedListStyle};
use rumdl::rule::Rule;

#[test]
fn test_md004_consistent_valid() {
    let rule = MD004UnorderedListStyle::default();
    let content = "* Item 1\n* Item 2\n  * Nested 1\n  * Nested 2\n* Item 3\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md004_consistent_invalid() {
    let rule = MD004UnorderedListStyle::default();
    let content = "* Item 1\n+ Item 2\n  - Nested 1\n  * Nested 2\n- Item 3\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[1].line, 3);
    assert_eq!(result[2].line, 5);
}

#[test]
fn test_md004_asterisk_style() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let content = "- Item 1\n+ Item 2\n  - Nested 1\n  + Nested 2\n* Item 3\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "* Item 1\n* Item 2\n  * Nested 1\n  * Nested 2\n* Item 3\n");
}

#[test]
fn test_md004_plus_style() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Plus);
    let content = "- Item 1\n* Item 2\n  - Nested 1\n  * Nested 2\n+ Item 3\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "+ Item 1\n+ Item 2\n  + Nested 1\n  + Nested 2\n+ Item 3\n");
}

#[test]
fn test_md004_dash_style() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
    let content = "* Item 1\n+ Item 2\n  * Nested 1\n  + Nested 2\n- Item 3\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "- Item 1\n- Item 2\n  - Nested 1\n  - Nested 2\n- Item 3\n");
}

#[test]
fn test_md004_deeply_nested() {
    let rule = MD004UnorderedListStyle::default();
    let content = "* Level 1\n  + Level 2\n    - Level 3\n      * Level 4\n  * Back to 2\n* Level 1\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "* Level 1\n  * Level 2\n    * Level 3\n      * Level 4\n  * Back to 2\n* Level 1\n");
}

#[test]
fn test_md004_mixed_content() {
    let rule = MD004UnorderedListStyle::default();
    let content = "# Heading\n\n* Item 1\n  Some text\n  + Nested with text\n    More text\n* Item 2\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Heading\n\n* Item 1\n  Some text\n  * Nested with text\n    More text\n* Item 2\n");
}

#[test]
fn test_md004_empty_content() {
    let rule = MD004UnorderedListStyle::default();
    let content = "";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "");
}

#[test]
fn test_md004_no_lists() {
    let rule = MD004UnorderedListStyle::default();
    let content = "# Heading\n\nSome text\nMore text\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Heading\n\nSome text\nMore text\n");
}

#[test]
fn test_md004_code_blocks() {
    let rule = MD004UnorderedListStyle::default();
    let content = "* Item 1\n```\n* Not a list\n+ Also not a list\n```\n* Item 2\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "* Item 1\n```\n* Not a list\n+ Also not a list\n```\n* Item 2\n");
}

#[test]
fn test_md004_blockquotes() {
    let rule = MD004UnorderedListStyle::default();
    let content = "* Item 1\n> * Quoted item\n> + Another quoted item\n* Item 2\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "* Item 1\n> * Quoted item\n> + Another quoted item\n* Item 2\n");
}

#[test]
fn test_md004_list_continuations() {
    let rule = MD004UnorderedListStyle::default();
    let content = "* Item 1\n  Continuation 1\n  + Nested item\n    Continuation 2\n* Item 2\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "* Item 1\n  Continuation 1\n  * Nested item\n    Continuation 2\n* Item 2\n");
}

#[test]
fn test_md004_mixed_ordered_unordered() {
    let rule = MD004UnorderedListStyle::default();
    let content = "1. Ordered item\n   * Unordered sub-item\n   + Another sub-item\n2. Ordered item\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "1. Ordered item\n   * Unordered sub-item\n   * Another sub-item\n2. Ordered item\n");
} 