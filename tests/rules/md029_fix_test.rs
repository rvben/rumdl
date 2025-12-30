use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD029OrderedListPrefix;

#[test]
fn test_md029_fix_with_2_space_code_blocks() {
    // Test that fix correctly handles 2-space indented code blocks breaking lists.
    // With 2-space indent, code blocks don't belong to list items, causing lists to break.
    // CommonMark sees multiple separate lists, each correctly numbered from their start value.
    let rule = MD029OrderedListPrefix::default();
    let content = r#"# Title

1. Test 1

  ```sh
  sudo dnf install ...
  ```

2. Test 2
3. Test 3

  ```sh
  cargo install ...
  ```

4. Test 4

  ```sh
  sudo dnf install ...
  ```

5. Test 5

  ```sh
  sudo dnf install ...
  ```"#;

    // No changes expected - each list is correctly numbered from its start value:
    // List 1: [1], List 2: [2,3], List 3: [4], List 4: [5]
    let expected = content;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    assert_eq!(
        fixed, expected,
        "No changes - each list is correctly numbered from its CommonMark start value"
    );
}

#[test]
fn test_md029_fix_with_4_space_code_blocks() {
    // Test that fix correctly handles 4-space indented code blocks (don't break lists)
    let rule = MD029OrderedListPrefix::default();
    let content = r#"# Title

1. Test 1

    ```sh
    sudo dnf install ...
    ```

2. Test 2
3. Test 3

    ```sh
    cargo install ...
    ```

4. Test 4"#;

    // No changes expected since 4-space indented code blocks maintain list continuity
    let expected = content;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    assert_eq!(
        fixed, expected,
        "MD029 fix should not change numbering when code blocks are properly indented"
    );
}

#[test]
fn test_md029_fix_matches_check() {
    // Test that fix and check are consistent
    // With 2 space indent, code block doesn't belong to item 1, causing list to break.
    // CommonMark sees two lists: [1] and [2, 3], both correctly numbered.
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item

  ```
  code block with 2 spaces
  ```

2. Second item
3. Third item"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // With default style (one_or_ordered) and respecting CommonMark start values:
    // - List 1 starts at 1: item 1 is correct
    // - List 2 starts at 2: items 2, 3 are correct
    // No warnings expected.
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "No warnings - both lists are correctly numbered from their start values"
    );

    // Apply fix (should be no-op since no warnings)
    let fixed = rule.fix(&ctx).unwrap();

    // Content should be unchanged
    assert_eq!(
        fixed, content,
        "Fix should not change content when there are no warnings"
    );
}

#[test]
fn test_md029_fix_preserves_content() {
    // Test that fix only changes the numbers, not the content
    // With 2 space indent, code block doesn't belong to item 1, causing list to break.
    // CommonMark sees two lists: [1] and [2, 3], both correctly numbered.
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item with some text

  ```python
  def hello():
      print("world")
  ```

2. Second item with more text
3. Third item with even more text"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Content should be unchanged since there are no warnings
    assert_eq!(
        fixed, content,
        "Fix should not change content when there are no warnings"
    );
}
