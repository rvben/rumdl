use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{ListStyle, MD029OrderedListPrefix};
use rumdl_lib::utils::range_utils::LineIndex;

#[test]
fn test_md029_valid() {
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::OneOne);

    let content = r#"1. Item 1
1. Item 2
1. Item 3"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md029_ordered_any_valid() {
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::Ordered);

    let content = r#"1. Item 1
2. Item 2
3. Item 3"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md029_ordered_any_invalid() {
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::Ordered);

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
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::OneOne);
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
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::Ordered);
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
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::Ordered);

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
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::Ordered);

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
    println!("Warnings: {result:?}");
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

#[test]
fn test_md029_code_blocks_in_nested_lists() {
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::One);

    let content = r#"1. First item

   ```rust
   fn code() {}
   ```

   More content

2. Second item

   1. Nested item

      ```python
      def nested_code():
          pass
      ```

   2. Another nested"#;

    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    // Should handle numbering correctly despite code blocks
    assert_eq!(warnings.len(), 2, "Should flag both issues with ListStyle::One");
    // With ListStyle::One, both "2. Second item" and "2. Another nested" should be flagged
}

#[test]
fn test_md029_fenced_vs_indented_in_list() {
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::Ordered);

    let content = r#"1. Item with fenced code:
   ```js
   console.log(1);
   ```

2. Item with indented code:

       indented code
       more code

3. Final item"#;

    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    assert!(warnings.is_empty(), "Ordered numbering should be accepted");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("1. Item"), "First item preserved");
    assert!(fixed.contains("2. Item"), "Second item preserved");
    assert!(fixed.contains("3. Final"), "Third item preserved");
}

/// Edge case tests for improved robustness

#[test]
fn test_zero_padded_numbers() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
01. First item with leading zero
02. Second item
05. Wrong number with padding";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should detect that 05 should be 03
    assert!(!result.is_empty(), "Should detect wrong zero-padded number");

    let fixed = rule.fix(&ctx).unwrap();
    // The fix should correct the sequence (removes leading zeros and fixes numbering)
    assert!(fixed.contains("3. Wrong number with padding"));
}

#[test]
fn test_lists_with_inline_html() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. First item with <strong>bold</strong> text
2. Second item
<div>Some HTML block</div>
4. Wrong number after HTML";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // HTML should not interfere with numbering detection
    assert!(!result.is_empty(), "Should detect wrong number despite HTML");
    assert!(
        result.iter().any(|w| w.message.contains("4")),
        "Should detect that 4 should be 3"
    );
}

#[test]
fn test_lists_with_html_comments() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. First item
<!-- This is a comment -->
2. Second item
<!-- Another comment -->
4. Wrong number after comments";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Comments should not break list sequences
    assert!(!result.is_empty(), "Should detect wrong number despite comments");
    assert!(
        result.iter().any(|w| w.message.contains("4")),
        "Should detect that 4 should be 3"
    );
}

#[test]
fn test_lists_with_mathematical_expressions() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. Calculate 3.14 * 2.5 = 7.85
2. The result of 1.5 + 2.3 is 3.8
4. Wrong number with math: 10.5 / 2.1";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Mathematical expressions shouldn't interfere with list numbering
    assert!(
        !result.is_empty(),
        "Should detect wrong number despite math expressions"
    );
    assert!(
        result.iter().any(|w| w.message.contains("4")),
        "Should detect that 4 should be 3"
    );
}

#[test]
fn test_deeply_nested_lists() {
    let rule = MD029OrderedListPrefix::default();

    // Generate a deeply nested list (6 levels)
    let content = "\
1. Level 1 item
  1. Level 2 item
    1. Level 3 item
      1. Level 4 item
        1. Level 5 item
          1. Level 6 item
          3. Wrong number at deep level";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should handle deep nesting and detect the wrong number
    assert!(!result.is_empty(), "Should detect wrong number at deep level");
    assert!(
        result.iter().any(|w| w.message.contains("3")),
        "Should detect that 3 should be 2 at deep level"
    );
}

#[test]
fn test_analyze_performance_errors() {
    let rule = MD029OrderedListPrefix::default();

    // Generate a small sample to analyze what's happening
    let mut content = String::new();
    for i in 1..=3 {
        content.push_str(&format!("1. List {i} item 1\n"));
        content.push_str(&format!("3. List {i} item 2 (wrong)\n")); // Should be 2
        content.push_str(&format!("3. List {i} item 3 (wrong)\n\n")); // Should be 3
    }

    println!("Content:\n{content}");

    let ctx = LintContext::new(&content);
    let result = rule.check(&ctx).unwrap();

    println!("Found {} warnings:", result.len());
    for (i, warning) in result.iter().enumerate() {
        println!("  {}: Line {} - {}", i + 1, warning.line, warning.message);
    }

    // Expected: 2 errors per list * 3 lists = 6 errors total
    // Let's see if we get exactly 6 or more due to cross-list detection
    println!("Expected 6 errors, got {}", result.len());
}

#[test]
fn test_performance_with_many_small_lists() {
    let rule = MD029OrderedListPrefix::default();

    // Generate many small lists with errors to test performance
    let mut content = String::new();
    for i in 1..=50 {
        content.push_str(&format!("1. List {i} item 1\n"));
        content.push_str(&format!("3. List {i} item 2 (wrong)\n")); // Wrong number
        content.push_str(&format!("3. List {i} item 3 (wrong)\n\n")); // Wrong number
    }

    let ctx = LintContext::new(&content);

    let start = std::time::Instant::now();
    let result = rule.check(&ctx).unwrap();
    let duration = start.elapsed();

    // Should complete quickly even with many errors
    assert!(duration.as_millis() < 500, "Should complete within 500ms");

    // Should find at least 100 errors (2 per intended list)
    assert!(
        result.len() >= 100,
        "Should find at least 100 errors (2 per intended list)"
    );
}

#[test]
fn test_lists_with_continuation_paragraphs() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. First item

   This is a continuation paragraph for the first item.
   It should be part of the same list item.

2. Second item

   Another continuation paragraph.

4. Wrong number with continuation

   This item has wrong numbering.";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Continuation paragraphs should not break sequences
    assert!(!result.is_empty(), "Should detect wrong number despite continuations");
    assert!(
        result.iter().any(|w| w.message.contains("4")),
        "Should detect that 4 should be 3"
    );
}

#[test]
fn test_mixed_indentation_patterns() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. Root item
   1. Indented 3 spaces (non-standard)
     1. Indented 5 spaces total
  3. Back to 2 spaces - wrong number";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should handle non-standard indentation robustly
    // The "3." should likely be "2." since it's at a different level
    println!("Mixed indentation warnings: {}", result.len());
    // Note: exact behavior depends on how indentation levels are calculated
}

#[test]
fn test_single_item_edge_cases() {
    // Test various single-item scenarios
    let test_cases = vec![
        (ListStyle::One, "5. Single item", true),
        (ListStyle::OneOne, "2. Single item", true),
        (ListStyle::Ordered, "1. Single item", false), // Should be correct
        (ListStyle::Ordered0, "1. Single item", true), // Should be 0
    ];

    for (style, content, should_have_error) in test_cases {
        let rule = MD029OrderedListPrefix::new(style.clone());
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        if should_have_error {
            assert!(!result.is_empty(), "Should detect error for style {style:?}");
        } else {
            assert!(result.is_empty(), "Should not detect error for style {style:?}");
        }
    }
}

// Additional tests for multiline list item issue (Issue #16)

#[test]
fn test_md029_multiline_no_indent() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // No indentation - should be treated as lazy continuation
    let content = r#"1. First item first line
second line of first item
1. Second item first line
second line of second item"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have warnings for:
    // 1. Lazy continuation on line 2
    // 2. Wrong number on line 3
    // 3. Lazy continuation on line 4
    assert!(
        result.len() >= 3,
        "Should detect lazy continuations and wrong numbering"
    );

    // Check for lazy continuation warnings
    let lazy_warnings = result.iter().filter(|w| w.rule_name == Some("MD029-style")).count();
    assert_eq!(lazy_warnings, 2, "Should have 2 lazy continuation warnings");

    // Check for numbering warning
    let numbering_warnings = result.iter().filter(|w| w.rule_name == Some("MD029")).count();
    assert_eq!(numbering_warnings, 1, "Should have 1 numbering warning");
}

#[test]
fn test_md029_multiline_3_space_indent() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // 3-space indentation - should be treated as continuation
    let content = r#"1. First item first line
   second line of first item
1. Second item first line
   second line of second item"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have warning for second "1." since it should be "2."
    assert_eq!(result.len(), 1, "3-space indentation should be treated as continuation");
    assert!(result[0].message.contains("1 does not match style (expected 2)"));
}

#[test]
fn test_md029_multiline_4_space_indent() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // 4-space indentation - should be treated as continuation
    let content = r#"1. First item first line
    second line of first item
1. Second item first line
    second line of second item"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have warning for second "1." since it should be "2."
    assert_eq!(result.len(), 1);
    assert!(result[0].message.contains("1 does not match style (expected 2)"));
}

#[test]
fn test_md029_multiline_2_space_indent() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // 2-space indentation - edge case
    let content = r#"1. First item first line
  second line of first item
1. Second item first line
  second line of second item"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // 2 spaces is not enough for ordered list continuation (need 3)
    // So these should be treated as separate lists
    assert!(result.is_empty(), "2-space indentation breaks the list");
}

#[test]
fn test_md029_multiline_mixed_content() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Test with code blocks between items
    let content = r#"1. First item
   continuation line
```
code block
```
2. Second item
   continuation line"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Code blocks should not break list numbering");
}

#[test]
fn test_md029_fix_multiline_3_space() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    let content = r#"1. First item first line
   second line of first item
1. Second item first line
   second line of second item"#;

    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();

    let expected = r#"1. First item first line
   second line of first item
2. Second item first line
   second line of second item"#;

    assert_eq!(fixed, expected, "Fix should preserve indentation and update numbering");
}

#[test]
fn test_md029_double_digit_marker_width() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Test that continuation indentation respects actual marker width
    let content = r#"9. Ninth item
   continuation with 3 spaces
10. Tenth item
    continuation with 4 spaces
11. Eleventh item
     continuation with 5 spaces"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // All items should be part of the same list
    assert_eq!(result.len(), 3, "All items should be flagged for renumbering");
    assert!(result[0].message.contains("9 does not match style (expected 1)"));
    assert!(result[1].message.contains("10 does not match style (expected 2)"));
    assert!(result[2].message.contains("11 does not match style (expected 3)"));
}

#[test]
fn test_md029_double_digit_insufficient_indent() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Test that insufficient indentation breaks the list
    let content = r#"9. Ninth item
   continuation
10. Tenth item
   text
11. Eleventh item
    text"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Line 2 has 3 spaces (OK for "9. ")
    // Line 4 has 3 spaces (NOT OK for "10. " which needs 4)
    // Line 6 has 4 spaces (NOT OK for "11. " which needs 5)
    // So item 10 and 11 should be separate lists

    // Actually, we should have 3 warnings:
    // - Item 9 should be 1 (first item in first list)
    // - Item 10 should be 2 (continues first list because line 4 has 3 spaces which is OK for item 9)
    // - Item 11 should be 1 (starts new list because line 6 has only 4 spaces which is not enough for item 10)
    assert_eq!(result.len(), 3, "Should have 3 warnings");
    assert!(result[0].message.contains("9 does not match style (expected 1)"));
    assert!(result[1].message.contains("10 does not match style (expected 2)"));
    assert!(result[2].message.contains("11 does not match style (expected 1)"));
}

#[test]
fn test_md029_triple_digit_marker_width() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Test that continuation indentation works for triple-digit markers
    let content = r#"99. Ninety-ninth item
    continuation with 4 spaces
100. One hundredth item
     continuation with 5 spaces
101. One hundred first item
     continuation with 5 spaces"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // All items should be part of the same list
    assert_eq!(result.len(), 3, "All items should be flagged for renumbering");
    assert!(result[0].message.contains("99 does not match style (expected 1)"));
    assert!(result[1].message.contains("100 does not match style (expected 2)"));
    assert!(result[2].message.contains("101 does not match style (expected 3)"));
}

#[test]
fn test_md029_quadruple_digit_marker_width() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Test that continuation indentation works for quadruple-digit markers
    let content = r#"999. Nine hundred ninety-ninth item
     continuation with 5 spaces
1000. One thousandth item
      continuation with 6 spaces
1111. Eleven eleven item
      continuation with 6 spaces"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // All items should be part of the same list
    assert_eq!(result.len(), 3, "All items should be flagged for renumbering");
    assert!(result[0].message.contains("999 does not match style (expected 1)"));
    assert!(result[1].message.contains("1000 does not match style (expected 2)"));
    assert!(result[2].message.contains("1111 does not match style (expected 3)"));
}

#[test]
fn test_md029_large_digit_insufficient_indent() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Test that insufficient indentation breaks the list for large numbers
    let content = r#"99. Item ninety-nine
    continuation with 4 spaces
100. Item one hundred
    only 4 spaces (not enough for "100. " which needs 5)
1000. Item one thousand
     only 5 spaces (not enough for "1000. " which needs 6)"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // We expect the list to be broken into multiple blocks
    // Item 99 and 100 should be in one list (since 4 spaces is enough for "99. ")
    // Item 1000 should start a new list (since 5 spaces is not enough for "100. ")
    assert_eq!(result.len(), 3, "Should have 3 warnings");
    assert!(result[0].message.contains("99 does not match style (expected 1)"));
    assert!(result[1].message.contains("100 does not match style (expected 2)"));
    assert!(result[2].message.contains("1000 does not match style (expected 1)")); // New list
}

#[test]
fn test_md029_lazy_continuation_fix() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Test that MD029 fixes numbering (not indentation)
    // Note: lazy continuation breaks the list, so "1. Second item" starts a new list
    let content = r#"1. First item
lazy continuation
1. Second item
another lazy line"#;

    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();

    // MD029 only fixes numbering, not indentation
    // The lazy continuation doesn't actually break the list in our implementation,
    // so "1. Second item" should become "2. Second item"
    let expected = r#"1. First item
lazy continuation
2. Second item
another lazy line"#;

    assert_eq!(fixed, expected, "MD029 should only fix list numbering");
}

#[test]
fn test_md029_mixed_lazy_and_proper_continuation() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    let content = r#"1. First item
lazy line
   proper continuation
1. Second item
  two space indent
    four space indent"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should detect lazy continuations (0 and 2 space lines)
    let lazy_warnings = result.iter().filter(|w| w.rule_name == Some("MD029-style")).count();
    // Note: Only line 2 is detected as lazy continuation because it's within the list block.
    // Line 5 has 2 spaces but it's after line 4 which starts a new list, so it's not
    // considered part of the list block and thus not checked for lazy continuation.
    assert_eq!(lazy_warnings, 1, "Should detect 0-space line as lazy continuation");

    // MD029 fix only fixes list numbering, not indentation
    let fixed = rule.fix(&ctx).unwrap();
    // Lazy lines remain unchanged
    assert!(fixed.contains("\nlazy line"));
    // The list item numbering is fixed
    assert!(fixed.contains("1. First item"));
    assert!(fixed.contains("2. Second item")); // Second item becomes "2." not "1."
    // Two-space indent line remains unchanged (MD029 doesn't fix indentation)
    assert!(fixed.contains("  two space indent"));
}

#[test]
fn test_md029_simple_insufficient_indent() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Simple test case - second item has insufficient indentation
    let content = r#"10. Item ten
   not enough spaces
10. Item ten again"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Line 2 has 3 spaces but needs 4 for "10. "
    // So item on line 3 should start a new list

    // The list should be split into 2 blocks because line 2 doesn't have enough indentation
    assert_eq!(ctx.list_blocks.len(), 2, "Should have 2 separate list blocks");

    // And MD029 should flag both "10." items as starting with the wrong number
    assert_eq!(result.len(), 2, "Both '10.' items should be flagged");
    assert!(result[0].message.contains("10 does not match style (expected 1)"));
    assert!(result[1].message.contains("10 does not match style (expected 1)"));
}

#[test]
fn test_md029_nested_ordered_lists_issue_52() {
    // Test for GitHub issue #52: nested ordered lists incorrectly treated as continuous sequence
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Exact test case from issue #52
    let content = r#"# Title

1. Top 1
   1. Sub 1
   1. Sub 2
2. Top 2
   1. Sub 3
   1. Sub 4"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should only flag 2 errors (not 3 as the bug produced):
    // - Line 5: "1. Sub 2" should be "2. Sub 2"
    // - Line 8: "1. Sub 4" should be "2. Sub 4"
    // Line 7: "1. Sub 3" should NOT be flagged (correctly starts new nested sequence)
    assert_eq!(result.len(), 2, "Should only flag 2 errors after fix");

    // Verify the specific errors - they should be on lines 5 and 8
    let line_5_error = result.iter().find(|w| w.line == 5);
    let line_8_error = result.iter().find(|w| w.line == 8);

    assert!(line_5_error.is_some(), "Should have error on line 5");
    assert!(line_8_error.is_some(), "Should have error on line 8");

    assert!(
        line_5_error
            .unwrap()
            .message
            .contains("1 does not match style (expected 2)"),
        "Line 5 should expect 2, got: {}",
        line_5_error.unwrap().message
    );
    assert!(
        line_8_error
            .unwrap()
            .message
            .contains("1 does not match style (expected 2)"),
        "Line 8 should expect 2, got: {}",
        line_8_error.unwrap().message
    );

    // Test the fix function produces correct output
    let fixed = rule.fix(&ctx).unwrap();
    let expected_fixed = r#"# Title

1. Top 1
   1. Sub 1
   2. Sub 2
2. Top 2
   1. Sub 3
   2. Sub 4"#;

    assert_eq!(fixed, expected_fixed, "Fix should produce correct nested numbering");
}

#[test]
fn test_md029_nested_ordered_lists_bug() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Test case from issue #52 - nested ordered lists should restart numbering
    let content = r#"# Title

1. Top 1
   1. Sub 1
   1. Sub 2
2. Top 2
   1. Sub 3
   1. Sub 4"#;

    let ctx = LintContext::new(content);

    // Debug info
    println!("List blocks found:");
    for (i, block) in ctx.list_blocks.iter().enumerate() {
        println!(
            "  Block {}: lines {}-{}, item_lines: {:?}, is_ordered: {}",
            i, block.start_line, block.end_line, block.item_lines, block.is_ordered
        );
    }

    println!("Line info for list items:");
    for line_num in 1..=ctx.content.lines().count() {
        if let Some(line_info) = ctx.line_info(line_num)
            && let Some(list_item) = &line_info.list_item
        {
            println!(
                "  Line {}: '{}' - marker: '{}', column: {}, ordered: {}",
                line_num,
                line_info.content.trim(),
                list_item.marker,
                list_item.marker_column,
                list_item.is_ordered
            );
        }
    }

    let result = rule.check(&ctx).unwrap();

    println!("Found {} warnings:", result.len());
    for warning in &result {
        println!("  Line {}: {}", warning.line, warning.message);
    }

    // Expected behavior: nested ordered lists should restart numbering at each level
    // So we should NOT get warnings for the nested "1." items
    // But the current implementation incorrectly treats them as one continuous sequence

    // This test currently fails - documenting the bug
    // The nested "1. Sub 1", "1. Sub 3" should NOT be flagged as errors
    // Only the second "1. Sub 2" and "1. Sub 4" should be flagged (should be "2.")

    // Expected warnings:
    // - Line 5: "1. Sub 2" should be "2. Sub 2"
    // - Line 8: "1. Sub 4" should be "2. Sub 4"

    // Now the fix is working correctly!
    // We should get exactly 2 warnings for the second items in each nested sequence
    assert_eq!(
        result.len(),
        2,
        "Should have exactly 2 warnings for nested sequence numbering"
    );

    // Check that the correct lines are flagged with the correct expected numbers
    assert!(
        result
            .iter()
            .any(|w| w.line == 5 && w.message.contains("1 does not match style (expected 2)")),
        "Line 5 (1. Sub 2) should be flagged as needing to be 2"
    );
    assert!(
        result
            .iter()
            .any(|w| w.line == 8 && w.message.contains("1 does not match style (expected 2)")),
        "Line 8 (1. Sub 4) should be flagged as needing to be 2"
    );

    // Test the fix function too
    let fixed = rule.fix(&ctx).unwrap();
    let expected_fixed = r#"# Title

1. Top 1
   1. Sub 1
   2. Sub 2
2. Top 2
   1. Sub 3
   2. Sub 4"#;

    assert_eq!(
        fixed, expected_fixed,
        "Fix should correct the nested sequence numbering"
    );
}

// Additional edge cases for nested lists to ensure robustness

#[test]
fn test_md029_triple_nested_ordered_lists() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    let content = r#"1. Level 1 item 1
   1. Level 2 item 1
      1. Level 3 item 1
      2. Level 3 item 2
   2. Level 2 item 2
      1. Level 3 item 3
      1. Level 3 item 4 - wrong
2. Level 1 item 2
   1. Level 2 item 3
      1. Level 3 item 5
      3. Level 3 item 6 - wrong"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should detect 2 errors: lines 7 and 11 (second items at level 3 under different parents)
    assert_eq!(result.len(), 2, "Should detect 2 errors in triple-nested structure");

    // Verify specific errors
    let line_7_error = result.iter().find(|w| w.line == 7);
    let line_11_error = result.iter().find(|w| w.line == 11);

    assert!(line_7_error.is_some(), "Should have error on line 7");
    assert!(line_11_error.is_some(), "Should have error on line 11");

    assert!(
        line_7_error
            .unwrap()
            .message
            .contains("1 does not match style (expected 2)")
    );
    assert!(
        line_11_error
            .unwrap()
            .message
            .contains("3 does not match style (expected 2)")
    );
}

#[test]
fn test_md029_ordered_under_unordered_parents() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    let content = r#"- Unordered parent 1
  1. Nested ordered 1
  1. Nested ordered 2 (should be 2)
- Unordered parent 2
  1. New sequence starts at 1
  3. Should be 2
- Unordered parent 3
  1. Another sequence starts at 1"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should find 2 errors: second items under each unordered parent
    assert_eq!(
        result.len(),
        2,
        "Should find 2 errors in ordered lists under unordered parents"
    );

    // Verify the errors are on the expected lines
    let has_line_3_error = result.iter().any(|w| w.line == 3 && w.message.contains("expected 2"));
    let has_line_6_error = result.iter().any(|w| w.line == 6 && w.message.contains("expected 2"));

    assert!(has_line_3_error, "Should have error on line 3");
    assert!(has_line_6_error, "Should have error on line 6");
}

#[test]
fn test_md029_lists_with_code_block_interruptions() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    let content = r#"1. First item
   ```python
   code block
   ```
2. Second item (should maintain sequence)
   1. Nested item
   ```rust
   more code
   ```
   1. Should be 2 after code block"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should find 1 error: the nested "1." that should be "2."
    assert_eq!(result.len(), 1, "Should find 1 error for nested item after code block");
    assert!(result[0].message.contains("1 does not match style (expected 2)"));
}

#[test]
fn test_md029_mixed_indentation_robustness() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Mix of 2, 3, and 4 space indentation
    let content = r#"1. Normal item
  1. 2-space nested (non-standard)
    1. 4-space nested
    3. Should be 2
   1. 3-space nested (back to standard)
   5. Should be 2"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should handle mixed indentation gracefully and detect numbering errors
    assert!(
        !result.is_empty(),
        "Should detect numbering errors despite mixed indentation"
    );
}

#[test]
fn test_md029_all_styles_with_nesting() {
    // Test that all ListStyle variants work with nested scenarios without crashing
    let styles = vec![
        ListStyle::One,
        ListStyle::OneOne,
        ListStyle::Ordered,
        ListStyle::Ordered0,
    ];

    let content = r#"1. Top level
   1. Nested level
   2. Second nested
2. Second top
   1. Another nested"#;

    for style in styles {
        let rule = MD029OrderedListPrefix::new(style.clone());
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx);

        // Just verify it doesn't crash and produces some result
        assert!(result.is_ok(), "Style {style:?} should not crash on nested lists");

        // Test that fix works too
        let fixed = rule.fix(&ctx);
        assert!(fixed.is_ok(), "Style {style:?} fix should not crash on nested lists");
    }
}
