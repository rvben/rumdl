use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{ListStyle, MD029OrderedListPrefix};

#[test]
fn test_issue_42_lists_separated_by_heading() {
    // Test case from issue #42 - two lists separated by a heading should be treated as separate lists
    let content = r#"# The 4 ways to import a module

There are four different ways to import:

1. Import the whole module using its original name:

    ```python
    import random
    ```

2. Import specific things from the module:

    ```python
    from random import choice, randint
    ```

3. Import the whole module and rename it, usually using a shorter variable name:

    ```python
    import pandas as pd
    ```

4. Import specific things from the module and rename them as you're importing them:

    ```python
    from os.path import join as join_path
    ```

That last one is usually done to avoid a name collision or *sometimes* to make a more descriptive name (though that's not very common).

## My recommendations

1. Use `from` for **short and descriptive variable names**
    I tend to use the `from` syntax most (number 2 above) because I prefer **short and descriptive variable names**.

2. Import the whole module if needed **to avoid ambiguity**.
    If there's a name like `choice` that isn't as clear as `random.choice`, then I prefer to import the whole module for a more descriptive name

3. **Avoid renaming imports**.
    I very rarely use the `as` syntax (unless I'm in the `pandas` or `numpy` worlds, where it's common convention).
    And I almost never use the `as` syntax with `from` unless I'm avoiding a name collision.
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should not report MD029 errors for the second list starting at 1
    // The two lists are separated by a heading and should be treated as separate lists
    for issue in &result {
        // Check that we're not incorrectly flagging the second list's numbering
        assert!(
            !issue.message.contains("expected 5")
                && !issue.message.contains("expected 6")
                && !issue.message.contains("expected 7"),
            "Lists separated by headings should not be treated as continuous. Found: {}",
            issue.message
        );
    }
}

#[test]
fn test_lists_separated_by_paragraph() {
    // Two lists separated by a paragraph should also be treated as separate lists
    let content = r#"# Test

1. First list item
2. Second list item

This is a paragraph between the lists.

1. First item of second list
2. Second item of second list
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should not report MD029 errors for the second list
    for issue in &result {
        assert!(
            !issue.message.contains("expected 3") && !issue.message.contains("expected 4"),
            "Lists separated by paragraphs should not be treated as continuous. Found: {}",
            issue.message
        );
    }
}

#[test]
fn test_lists_separated_by_horizontal_rule() {
    // Two lists separated by a horizontal rule should be treated as separate lists
    let content = r#"# Test

1. First list item
2. Second list item

---

1. First item of second list
2. Second item of second list
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should not report MD029 errors for the second list
    for issue in &result {
        assert!(
            !issue.message.contains("expected 3") && !issue.message.contains("expected 4"),
            "Lists separated by horizontal rules should not be treated as continuous. Found: {}",
            issue.message
        );
    }
}

#[test]
fn test_lists_separated_by_blockquote() {
    // Two lists separated by a blockquote should be treated as separate lists
    let content = r#"# Test

1. First list item
2. Second list item

> This is a blockquote between the lists.

1. First item of second list
2. Second item of second list
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should not report MD029 errors for the second list
    for issue in &result {
        assert!(
            !issue.message.contains("expected 3") && !issue.message.contains("expected 4"),
            "Lists separated by blockquotes should not be treated as continuous. Found: {}",
            issue.message
        );
    }
}

#[test]
fn test_deeply_nested_lists_with_separator() {
    // Edge case: Deeply nested lists with separator at different levels
    let content = r#"# Test

1. First item
   1. Nested item
      1. Deeply nested item
      2. Another deeply nested item

## Section break

1. New list starts at 1
   1. Nested in new list
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should not report MD029 errors for the second list
    for issue in &result {
        assert!(
            !issue.message.contains("expected 3") && !issue.message.contains("expected 5"),
            "Deeply nested lists separated by headings should reset numbering. Found: {}",
            issue.message
        );
    }
}

#[test]
fn test_multiple_blank_lines_between_list_items() {
    // Edge case: Multiple blank lines without other content should NOT separate lists
    let content = r#"# Test

1. First item
2. Second item


3. Third item after multiple blank lines
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // This SHOULD report an MD029 error since blank lines alone don't separate lists
    // The third item should be "3." not something else
    assert_eq!(
        result.len(),
        0,
        "Multiple blank lines alone should not cause MD029 errors"
    );
}

#[test]
fn test_code_block_between_lists() {
    // Edge case: Code block between lists should separate them
    let content = r#"# Test

1. First list item
2. Second list item

```python
# Some code here
print("Hello")
```

1. First item of new list
2. Second item of new list
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should not report MD029 errors for the second list
    for issue in &result {
        assert!(
            !issue.message.contains("expected 3") && !issue.message.contains("expected 4"),
            "Lists separated by code blocks should not be treated as continuous. Found: {}",
            issue.message
        );
    }
}

#[test]
fn test_table_between_lists() {
    // Edge case: Table between lists should separate them
    let content = r#"# Test

1. First list item
2. Second list item

| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |

1. First item of new list
2. Second item of new list
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should not report MD029 errors for the second list
    for issue in &result {
        assert!(
            !issue.message.contains("expected 3") && !issue.message.contains("expected 4"),
            "Lists separated by tables should not be treated as continuous. Found: {}",
            issue.message
        );
    }
}

#[test]
fn test_html_comment_between_lists() {
    // Edge case: HTML comment alone might not separate lists (implementation specific)
    let content = r#"# Test

1. First list item
2. Second list item

<!-- This is an HTML comment -->

3. Third list item
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // HTML comments at top level separate lists, so we expect an MD029 error
    assert_eq!(
        result.len(),
        1,
        "HTML comments at top level should separate lists and cause MD029 error for incorrect numbering"
    );
}

#[test]
fn test_mixed_list_types() {
    // Edge case: Ordered list followed by unordered list followed by ordered list
    let content = r#"# Test

1. First ordered item
2. Second ordered item

- Unordered item
- Another unordered item

1. New ordered list starts at 1
2. Second item of new ordered list
"#;

    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should not report MD029 errors - unordered list separates the ordered lists
    for issue in &result {
        assert!(
            !issue.message.contains("expected 3") && !issue.message.contains("expected 4"),
            "Ordered lists separated by unordered lists should reset numbering. Found: {}",
            issue.message
        );
    }
}
