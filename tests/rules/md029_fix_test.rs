use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD029OrderedListPrefix;

#[test]
fn test_md029_fix_with_2_space_code_blocks() {
    // Test that fix correctly handles 2-space indented code blocks breaking lists
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

    let expected = r#"# Title

1. Test 1

  ```sh
  sudo dnf install ...
  ```

1. Test 2
2. Test 3

  ```sh
  cargo install ...
  ```

1. Test 4

  ```sh
  sudo dnf install ...
  ```

1. Test 5

  ```sh
  sudo dnf install ...
  ```"#;

    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();

    assert_eq!(
        fixed, expected,
        "MD029 fix should correctly handle 2-space indented code blocks"
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

    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();

    assert_eq!(
        fixed, expected,
        "MD029 fix should not change numbering when code blocks are properly indented"
    );
}

#[test]
fn test_md029_fix_matches_check() {
    // Test that fix and check are consistent
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item

  ```
  code block with 2 spaces
  ```

2. Second item
3. Third item"#;

    let ctx = LintContext::new(content);

    // Get warnings from check
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 2, "Should detect 2 MD029 issues");

    // Apply fix
    let fixed = rule.fix(&ctx).unwrap();

    // Check the fixed content - should have no warnings
    let fixed_ctx = LintContext::new(&fixed);
    let fixed_warnings = rule.check(&fixed_ctx).unwrap();

    assert_eq!(fixed_warnings.len(), 0, "Fixed content should have no MD029 warnings");

    // Verify the fix
    assert!(
        fixed.contains("1. Second item"),
        "Second item should be renumbered to 1"
    );
    assert!(fixed.contains("2. Third item"), "Third item should be renumbered to 2");
}

#[test]
fn test_md029_fix_preserves_content() {
    // Test that fix only changes the numbers, not the content
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item with some text

  ```python
  def hello():
      print("world")
  ```

2. Second item with more text
3. Third item with even more text"#;

    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();

    // Check that content is preserved
    assert!(fixed.contains("First item with some text"));
    assert!(fixed.contains("Second item with more text"));
    assert!(fixed.contains("Third item with even more text"));
    assert!(fixed.contains(
        r#"def hello():
      print("world")"#
    ));

    // Check that numbering is fixed
    assert!(fixed.contains("1. Second item"));
    assert!(fixed.contains("2. Third item"));
}
