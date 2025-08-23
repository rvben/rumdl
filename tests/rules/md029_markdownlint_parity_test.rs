use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD029OrderedListPrefix;

#[test]
fn test_md029_2_space_code_blocks_break_lists() {
    // Test that 2-space indented code blocks break list continuity
    // This should match markdownlint's behavior
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

    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    // Should report 4 MD029 errors matching markdownlint
    assert_eq!(
        warnings.len(),
        4,
        "Should report 4 MD029 errors for list items after 2-space indented code blocks"
    );

    // Check specific errors
    assert_eq!(warnings[0].line, 9); // Line 9: "2. Test 2" should be "1."
    assert!(warnings[0].message.contains("expected 1"));

    assert_eq!(warnings[1].line, 10); // Line 10: "3. Test 3" should be "2."
    assert!(warnings[1].message.contains("expected 2"));

    assert_eq!(warnings[2].line, 16); // Line 16: "4. Test 4" should be "1."
    assert!(warnings[2].message.contains("expected 1"));

    assert_eq!(warnings[3].line, 22); // Line 22: "5. Test 5" should be "1."
    assert!(warnings[3].message.contains("expected 1"));
}

#[test]
fn test_md029_4_space_code_blocks_continue_lists() {
    // Test that 4-space indented code blocks do NOT break list continuity
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

    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    // Should not report any MD029 errors for properly indented code blocks
    assert_eq!(
        warnings.len(),
        0,
        "Should not report MD029 errors when code blocks are properly indented (4 spaces)"
    );
}

#[test]
fn test_md029_3_space_code_blocks_continue_lists() {
    // Test that 3-space indented code blocks do NOT break list continuity
    // (3 spaces is the minimum for a list item like "1. ")
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

    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    // Should not report any MD029 errors for 3-space indented code blocks
    assert_eq!(
        warnings.len(),
        0,
        "Should not report MD029 errors when code blocks have 3 spaces (minimum continuation indent)"
    );
}

#[test]
fn test_md029_unindented_code_blocks_break_lists() {
    // Test that unindented code blocks definitely break list continuity
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

    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    // Should report MD029 errors for all items after unindented code blocks
    assert_eq!(
        warnings.len(),
        3,
        "Should report MD029 errors for list items after unindented code blocks"
    );

    assert_eq!(warnings[0].line, 9); // "2. Test 2" should be "1."
    assert_eq!(warnings[1].line, 10); // "3. Test 3" should be "2."
    assert_eq!(warnings[2].line, 16); // "4. Test 4" should be "1."
}

#[test]
fn test_md029_detection_with_2_space_code_blocks() {
    // Test that MD029 correctly detects issues with 2-space indented code blocks
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. Test 1

  ```sh
  code
  ```

2. Test 2
3. Test 3

  ```sh
  code
  ```

4. Test 4"#;

    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    // Should detect 3 MD029 issues
    assert_eq!(warnings.len(), 3, "Should detect MD029 issues for items 2, 3, and 4");

    // Verify specific issues
    assert_eq!(warnings[0].line, 7); // Line 7: "2. Test 2" should be "1."
    assert!(warnings[0].message.contains("expected 1"));

    assert_eq!(warnings[1].line, 8); // Line 8: "3. Test 3" should be "2."
    assert!(warnings[1].message.contains("expected 2"));

    assert_eq!(warnings[2].line, 14); // Line 14: "4. Test 4" should be "1."
    assert!(warnings[2].message.contains("expected 1"));
}

#[test]
fn test_md029_wider_markers() {
    // Test with wider list markers like "10." which affect min_continuation_indent
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item
10. Test item with wide marker

   ```sh
   three spaces - not enough for "10. " (needs 4)
   ```

11. This should be flagged"#;

    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    // With "10. " (3 chars + 1 space = 4), need 4 spaces for continuation
    // 3 spaces is insufficient, so the list should break
    assert_eq!(
        warnings.len(),
        2,
        "Should report MD029 errors: one for wrong initial numbering, one for break after code block"
    );

    // First error: "10." should be "2." (continues from "1.")
    assert_eq!(warnings[0].line, 2);
    assert!(warnings[0].message.contains("expected 2"));

    // Second error: "11." should be "1." (new list after insufficiently indented code block)
    assert_eq!(warnings[1].line, 8);
    assert!(warnings[1].message.contains("expected 1"));
}
