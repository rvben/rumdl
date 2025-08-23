use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{ListStyle, MD029OrderedListPrefix};

/// Tests for code block separation behavior in MD029
/// Based on CommonMark specification and markdownlint compatibility testing

#[test]
fn test_root_level_code_block_separates_lists() {
    // Test case verified against markdownlint and pandoc
    // Root-level code blocks should separate lists, causing numbering to restart
    let content = r#"1. First item
2. Second item

```
code block at root level
```

1. This should be item 1 (new list)
2. This should be item 2 (new list)
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should NOT report MD029 errors because the second list correctly starts at 1
    // If rumdl incorrectly treats this as one continuous list, it would expect items 3 and 4
    for issue in &result {
        assert!(
            !issue.message.contains("expected 3") && !issue.message.contains("expected 4"),
            "Root-level code blocks should separate lists. Found: {}",
            issue.message
        );
    }

    // There should be no MD029 errors at all
    assert_eq!(
        result.len(),
        0,
        "No MD029 errors expected when lists are properly separated by code blocks"
    );
}

#[test]
fn test_indented_code_block_does_not_separate_lists() {
    // Test case verified against markdownlint and pandoc
    // Indented code blocks (4+ spaces) should be part of the list item, not separate lists
    let content = r#"1. First item
2. Second item

    ```
    code block indented as list content
    ```

3. This should be item 3 (same list)
4. This should be item 4 (same list)
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should NOT report MD029 errors because items 3 and 4 are correct
    assert_eq!(
        result.len(),
        0,
        "No MD029 errors expected when code block is properly indented as list content"
    );
}

#[test]
fn test_tilde_fenced_code_block_separates_lists() {
    // Test that ~~~ fenced blocks also separate lists
    let content = r#"1. First item
2. Second item

~~~python
print("tilde-fenced code block")
~~~

1. This should be item 1 (new list)
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should NOT report MD029 errors
    for issue in &result {
        assert!(
            !issue.message.contains("expected 3"),
            "Tilde-fenced code blocks should separate lists. Found: {}",
            issue.message
        );
    }

    assert_eq!(
        result.len(),
        0,
        "No MD029 errors expected when lists are separated by tilde-fenced code blocks"
    );
}

#[test]
fn test_indented_code_block_3_spaces_continues_list() {
    // Test that 3 spaces IS sufficient for list continuation with "2. " (width=3)
    // According to CommonMark, content indented by marker width continues the list
    let content = r#"1. First item
2. Second item

   ```
   3 spaces - sufficient for list continuation
   ```

3. This should be item 3 (same list continues)
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should NOT report MD029 errors because the third item correctly follows as item 3
    assert_eq!(
        result.len(),
        0,
        "No MD029 errors expected when code block is properly indented as list content"
    );
}

#[test]
fn test_indented_code_block_2_spaces_insufficient() {
    // Test that 2 spaces is insufficient indentation for list continuation with "2. " (width=3)
    // This should separate the lists since it's not properly indented
    let content = r#"1. First item
2. Second item

  ```
  only 2 spaces - insufficient for list continuation
  ```

1. This should be item 1 (new list)
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should NOT report MD029 errors because lists should be separate
    assert_eq!(
        result.len(),
        0,
        "No MD029 errors expected when code block has insufficient indentation"
    );
}

#[test]
fn test_mixed_code_block_scenarios() {
    // Complex test with multiple code blocks at different indentation levels
    let content = r#"1. First item
2. Second item

```
root level code block
```

1. New list item 1
2. New list item 2

    ```
    properly indented code block
    ```

3. Continues same list (item 3)

```
another root level code block
```

1. Another new list starts at 1
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should NOT report any MD029 errors
    assert_eq!(
        result.len(),
        0,
        "No MD029 errors expected in mixed code block scenario with proper separation"
    );
}

#[test]
fn test_code_block_with_language_specification() {
    // Test that code blocks with language specifications also separate lists
    let content = r#"1. Item one
2. Item two

```python
# Python code with language specification
def hello():
    print("Hello, world!")
```

1. Should be item 1 of new list
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should NOT report MD029 errors
    for issue in &result {
        assert!(
            !issue.message.contains("expected 3"),
            "Code blocks with language specs should separate lists. Found: {}",
            issue.message
        );
    }

    assert_eq!(
        result.len(),
        0,
        "No MD029 errors expected when lists are separated by language-specified code blocks"
    );
}

#[test]
fn test_code_block_separates_lists_basic() {
    // Basic test: code blocks should separate lists
    let content = r#"1. First item
2. Second item

```
code block
```

1. This should be item 1 (new list starts)
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should NOT report MD029 errors because the second list correctly starts at 1
    assert_eq!(
        result.len(),
        0,
        "Code blocks should separate lists - no MD029 errors expected"
    );
}

#[test]
fn test_edge_case_empty_code_block() {
    // Test with empty code blocks
    let content = r#"1. First item

```
```

1. Should be item 1 of new list
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should NOT report MD029 errors
    for issue in &result {
        assert!(
            !issue.message.contains("expected 2"),
            "Empty code blocks should separate lists. Found: {}",
            issue.message
        );
    }
}

#[test]
fn test_code_block_immediately_after_list_no_blank_line() {
    // Test edge case where code block immediately follows list without blank line
    let content = r#"1. First item
2. Second item
```
code block right after list
```

1. Should be item 1 of new list
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should NOT report MD029 errors - code block should still separate
    for issue in &result {
        assert!(
            !issue.message.contains("expected 3"),
            "Code blocks immediately after lists should separate them. Found: {}",
            issue.message
        );
    }
}
