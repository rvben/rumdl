use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD029OrderedListPrefix;

#[test]
fn test_md029_2_space_code_blocks_break_lists() {
    // Test that 2-space indented code blocks break list continuity.
    // CommonMark respects the start value of each new list, so items
    // that are correctly numbered within their list are not flagged.
    let rule = MD029OrderedListPrefix::default(); // default is one_or_ordered
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // CommonMark parses this as 4 lists, each correctly numbered from its start value:
    // List 1: [1] - starts at 1, item 1 is correct
    // List 2: [2, 3] - starts at 2, items 2,3 are correct (2+0=2, 2+1=3)
    // List 3: [4] - starts at 4, item 4 is correct
    // List 4: [5] - starts at 5, item 5 is correct
    // With one_or_ordered style: each list is checked independently, all correct.
    assert!(
        warnings.is_empty(),
        "No warnings - each list is correctly numbered from its start value"
    );
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    // Test that unindented code blocks definitely break list continuity.
    // Each new list starts at the value of its first item.
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // CommonMark parses this as 3 lists, each correctly numbered:
    // List 1: [1] - starts at 1, item 1 is correct
    // List 2: [2, 3] - starts at 2, items 2,3 are correct (2+0=2, 2+1=3)
    // List 3: [4] - starts at 4, item 4 is correct
    assert!(
        warnings.is_empty(),
        "No warnings - each list is correctly numbered from its start value"
    );
}

#[test]
fn test_md029_detection_with_2_space_code_blocks() {
    // Test that MD029 correctly respects CommonMark start values
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // CommonMark parses as 3 lists, each correctly numbered:
    // List 1: [1] - starts at 1, item 1 is correct
    // List 2: [2, 3] - starts at 2, items 2,3 are correct
    // List 3: [4] - starts at 4, item 4 is correct
    assert!(
        warnings.is_empty(),
        "No warnings - each list is correctly numbered from its start value"
    );
}

#[test]
fn test_md029_wider_markers() {
    // Test with wider list markers like "10." which affect min_continuation_indent
    // CommonMark respects the start value of each list
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item
10. Test item with wide marker

   ```sh
   three spaces - not enough for "10. " (needs 4)
   ```

11. This should be flagged"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // With "10. " (3 chars + 1 space = 4), need 4 spaces for continuation
    // 3 spaces is insufficient, so the list breaks after 10.
    // CommonMark parses as:
    // List 1: [1, 10] - starts at 1, item 10 should be 2 → WARNING
    // List 2: [11] - starts at 11, correctly numbered
    assert_eq!(
        warnings.len(),
        1,
        "Should report 1 MD029 error for item 10 (expected 2)"
    );

    // "10." should be "2." (continues from "1.")
    assert_eq!(warnings[0].line, 2);
    assert!(warnings[0].message.contains("expected 2"));
    // Auto-fix is available because the list starts at 1
    assert!(warnings[0].fix.is_some(), "Should have auto-fix");
}
