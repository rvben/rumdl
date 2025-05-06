use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD029OrderedListPrefix;
use rumdl::utils::range_utils::LineIndex;

#[test]
fn test_md029_valid() {
    let rule = MD029OrderedListPrefix::new(rumdl::rules::ListStyle::OneOne);

    let content = r#"1. Item 1
1. Item 2
1. Item 3"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md029_ordered_any_valid() {
    let rule = MD029OrderedListPrefix::new(rumdl::rules::ListStyle::Ordered);

    let content = r#"1. Item 1
2. Item 2
3. Item 3"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md029_ordered_any_invalid() {
    let rule = MD029OrderedListPrefix::new(rumdl::rules::ListStyle::Ordered);

    let content = r#"1. Item 1
1. Item 2
1. Item 3"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());

    // Check that it fixes to 1, 2, 3
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "1. Item 1\n2. Item 2\n3. Item 3");
}

#[test]
fn test_md029_nested() {
    let rule = MD029OrderedListPrefix::new(rumdl::rules::ListStyle::OneOne);
    let content = r#"1. First item
   1. Nested first
   1. Nested second
1. Second item"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md029_fix() {
    let rule = MD029OrderedListPrefix::new(rumdl::rules::ListStyle::Ordered);
    let content = r#"1. First item
3. Second item
5. Third item"#;
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "1. First item\n2. Second item\n3. Third item");
}

#[test]
fn test_line_index() {
    let content = r#"1. First item
2. Second item
3. Third item"#;
    let index = LineIndex::new(content.to_string());

    // The byte range should be calculated based on the actual content
    // Line 2, Column 1 corresponds to the beginning of "2. Second item" which is at index 14
    assert_eq!(index.line_col_to_byte_range(2, 1), 14..14);
}

#[test]
fn test_md029_with_code_blocks() {
    let rule = MD029OrderedListPrefix::new(rumdl::rules::ListStyle::Ordered);

    let content = r#"1. First step
```bash
some code
```
2. Second step
```bash
more code
```
3. Third step
```bash
final code
```"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "List items with code blocks between them should maintain sequence"
    );

    // Test that it doesn't generate false positives
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, content,
        "Content should remain unchanged as it's already correct"
    );
}

#[test]
fn test_md029_nested_with_code_blocks() {
    let rule = MD029OrderedListPrefix::new(rumdl::rules::ListStyle::Ordered);

    let content = r#"1. First step
   ```bash
   some code
   ```
   1. First substep
   ```bash
   nested code
   ```
   2. Second substep
2. Second step
   ```bash
   more code
   ```
3. Third step"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    println!("Warnings: {:?}", result);
    assert!(
        result.is_empty(),
        "Nested lists with code blocks should maintain correct sequence"
    );

    // Test that it doesn't generate false positives
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, content,
        "Content should remain unchanged as it's already correct"
    );
}
