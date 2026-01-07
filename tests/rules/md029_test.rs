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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md029_ordered_any_valid() {
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::Ordered);

    let content = r#"1. Item 1
2. Item 2
3. Item 3"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md029_ordered_any_invalid() {
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::Ordered);

    let content = r#"1. Item 1
1. Item 2
1. Item 3"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md029_fix() {
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::Ordered);
    let content = r#"1. First item
3. Second item
5. Third item"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "1. First item\n2. Second item\n3. Third item");
}

#[test]
fn test_line_index() {
    let content = r#"1. First item
2. Second item
3. Third item"#;
    let index = LineIndex::new(content);

    // The byte range should be calculated based on the actual content
    // Line 2, Column 1 corresponds to the beginning of "2. Second item" which is at index 14
    assert_eq!(index.line_col_to_byte_range(2, 1), 14..14);
}

#[test]
fn test_md029_with_code_blocks() {
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::Ordered);

    // Non-indented code blocks break the list per CommonMark. Each list item
    // becomes its own list. With CommonMark start value support, each list is
    // correctly numbered from its own start value (1, 2, 3 respectively).
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Each list is correctly numbered from its CommonMark start value.
    assert!(
        result.is_empty(),
        "No warnings - each list is correctly numbered from its start value"
    );
}

#[test]
fn test_md029_nested_with_code_blocks() {
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::Ordered);

    // NOTE: The code block after "1. First substep" has insufficient indent.
    // This breaks the nested list per CommonMark. "2. Second substep" becomes
    // a new list starting with "2." - which is correctly numbered from its start value.
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // With CommonMark start value support, each list is correctly numbered:
    // - Outer list: 1, 2, 3 (correct)
    // - Nested list: 1 (correct)
    // - New list starting at 2: 2 (correct)
    assert!(
        result.is_empty(),
        "No warnings - each list is correctly numbered from its start value"
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    // Add blank line after HTML block so "4." becomes a new list item
    // (without blank line, "4." is consumed by the HTML block per CommonMark)
    let content = "\
1. First item with <strong>bold</strong> text
2. Second item
<div>Some HTML block</div>

4. Wrong number after HTML";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // HTML block separates the lists, creating [1, 2] and [4].
    // With CommonMark start value support, both lists are correctly numbered.
    assert!(
        result.is_empty(),
        "No warnings - HTML block separates lists, each correctly numbered: {result:?}"
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // HTML comments at column 0 break the list per CommonMark, creating:
    // [1], [2], [4] - three separate lists, each correctly numbered.
    // With CommonMark start value support, no warnings.
    assert!(
        result.is_empty(),
        "No warnings - HTML comments break list, each list correctly numbered: {result:?}"
    );
}

#[test]
fn test_lists_with_mathematical_expressions() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. Calculate 3.14 * 2.5 = 7.85
2. The result of 1.5 + 2.3 is 3.8
4. Wrong number with math: 10.5 / 2.1";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    // CommonMark requires 3+ space indent per level for proper nesting after "1. " markers
    let content = "\
1. Level 1 item
   1. Level 2 item
      1. Level 3 item
         1. Level 4 item
            1. Level 5 item
               1. Level 6 item
               3. Wrong number at deep level";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    // No indentation - lazy continuation per CommonMark
    let content = r#"1. First item first line
second line of first item
1. Second item first line
second line of second item"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // pulldown-cmark sees this as one list with two items via lazy continuation.
    // With Ordered style, the second "1." should be "2.".
    assert_eq!(result.len(), 1, "Should have 1 numbering warning");
    assert_eq!(result[0].line, 3);
    assert!(result[0].message.contains("expected 2"));
}

#[test]
fn test_md029_multiline_3_space_indent() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // 3-space indentation - should be treated as continuation
    let content = r#"1. First item first line
   second line of first item
1. Second item first line
   second line of second item"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have warning for second "1." since it should be "2."
    assert_eq!(result.len(), 1, "3-space indentation should be treated as continuation");
    assert!(result[0].message.contains("1") && result[0].message.contains("expected 2"));
}

#[test]
fn test_md029_multiline_4_space_indent() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // 4-space indentation - should be treated as continuation
    let content = r#"1. First item first line
    second line of first item
1. Second item first line
    second line of second item"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have warning for second "1." since it should be "2."
    assert_eq!(result.len(), 1);
    assert!(result[0].message.contains("1") && result[0].message.contains("expected 2"));
}

#[test]
fn test_md029_multiline_2_space_indent() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // 2-space indentation with lazy continuation
    let content = r#"1. First item first line
  second line of first item
1. Second item first line
  second line of second item"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // According to CommonMark (verified with pulldown-cmark), lazy continuation makes this
    // one list with two items. With ListStyle::Ordered, the second "1." should be "2.".
    assert_eq!(result.len(), 1, "Second item should be numbered 2");
    assert_eq!(result[0].line, 3);
    assert!(result[0].message.contains("expected 2"));
}

#[test]
fn test_md029_multiline_mixed_content() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Code blocks between items break the list per CommonMark
    let content = r#"1. First item
   continuation line
```
code block
```
2. Second item
   continuation line"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // pulldown-cmark sees two separate lists (code block breaks list)
    // With CommonMark start value support, each list is correctly numbered.
    assert!(
        result.is_empty(),
        "No warnings - each list correctly numbered from its start value"
    );
}

#[test]
fn test_md029_fix_multiline_3_space() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    let content = r#"1. First item first line
   second line of first item
1. Second item first line
   second line of second item"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    // CommonMark sees this as one list starting at 9, with items 9, 10, 11
    // Since the numbering is sequential from the start value, no warnings
    let content = r#"9. Ninth item
   continuation with 3 spaces
10. Tenth item
    continuation with 4 spaces
11. Eleventh item
     continuation with 5 spaces"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // All items form one list starting at 9 with correct sequential numbering
    assert!(
        result.is_empty(),
        "No warnings - list 9, 10, 11 is correctly numbered from start value 9"
    );
}

#[test]
fn test_md029_double_digit_insufficient_indent() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Test list with double-digit markers and continuation lines.
    // CommonMark sees this as one list starting at 9, with items 9, 10, 11.
    // Since the numbering is sequential from the start value, no warnings.
    let content = r#"9. Ninth item
   continuation
10. Tenth item
   text
11. Eleventh item
    text"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // CommonMark says list starts at 9, and items 9, 10, 11 are correctly numbered
    assert!(
        result.is_empty(),
        "No warnings - list 9, 10, 11 is correctly numbered from start value 9"
    );
}

#[test]
fn test_md029_triple_digit_marker_width() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Test that continuation indentation works for triple-digit markers
    // CommonMark sees this as one list starting at 99, with items 99, 100, 101
    let content = r#"99. Ninety-ninth item
    continuation with 4 spaces
100. One hundredth item
     continuation with 5 spaces
101. One hundred first item
     continuation with 5 spaces"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // All items form one list starting at 99 with correct sequential numbering
    assert!(
        result.is_empty(),
        "No warnings - list 99, 100, 101 is correctly numbered from start value 99"
    );
}

#[test]
fn test_md029_quadruple_digit_marker_width() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Test that continuation indentation works for quadruple-digit markers
    // CommonMark sees one list starting at 999
    // Items 999, 1000, 1111 - 1111 is wrong (should be 1001)
    let content = r#"999. Nine hundred ninety-ninth item
     continuation with 5 spaces
1000. One thousandth item
      continuation with 6 spaces
1111. Eleven eleven item
      continuation with 6 spaces"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Items 999, 1000 are correct, but 1111 should be 1001
    assert_eq!(result.len(), 1, "Only item 1111 should be flagged");
    assert!(
        result[0].message.contains("1111") && result[0].message.contains("expected 1001"),
        "Item 1111 should expect 1001: {}",
        result[0].message
    );
}

#[test]
fn test_md029_large_digit_insufficient_indent() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // With CommonMark lazy continuation, all items stay in one list regardless of
    // insufficient indentation for continuation lines. pulldown-cmark correctly
    // parses these as a single ordered list starting at 99.
    let content = r#"99. Item ninety-nine
    continuation with 4 spaces
100. Item one hundred
    only 4 spaces (not enough for "100. " which needs 5)
1000. Item one thousand
     only 5 spaces (not enough for "1000. " which needs 6)"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // CommonMark sees list starting at 99. Items 99, 100 are correct.
    // Item 1000 is wrong (should be 101).
    assert_eq!(result.len(), 1, "Only item 1000 should be flagged");
    assert!(
        result[0].message.contains("1000") && result[0].message.contains("expected 101"),
        "Item 1000 should expect 101: {}",
        result[0].message
    );
}

#[test]
fn test_md029_simple_insufficient_indent() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Simple test case with lazy continuation per CommonMark
    let content = r#"10. Item ten
   not enough spaces
10. Item ten again"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // pulldown-cmark sees this as 1 list with 2 items via lazy continuation.
    // CommonMark says the list starts at 10, so:
    // - First item "10." is correct (start value)
    // - Second item "10." should be 11
    assert_eq!(result.len(), 1, "Only second '10.' should be flagged (expected 11)");
    assert_eq!(result[0].line, 3);
    assert!(
        result[0].message.contains("10") && result[0].message.contains("expected 11"),
        "Expected message about 10 should be 11: {}",
        result[0].message
    );
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        line_5_error.unwrap().message.contains("1") && line_5_error.unwrap().message.contains("expected 2"),
        "Line 5 should expect 2, got: {}",
        line_5_error.unwrap().message
    );
    assert!(
        line_8_error.unwrap().message.contains("1") && line_8_error.unwrap().message.contains("expected 2"),
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

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
                line_info.content(ctx.content).trim(),
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
            .any(|w| w.line == 5 && w.message.contains("1") && w.message.contains("expected 2")),
        "Line 5 (1. Sub 2) should be flagged as needing to be 2"
    );
    assert!(
        result
            .iter()
            .any(|w| w.line == 8 && w.message.contains("1") && w.message.contains("expected 2")),
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should detect 2 errors: lines 7 and 11 (second items at level 3 under different parents)
    assert_eq!(result.len(), 2, "Should detect 2 errors in triple-nested structure");

    // Verify specific errors
    let line_7_error = result.iter().find(|w| w.line == 7);
    let line_11_error = result.iter().find(|w| w.line == 11);

    assert!(line_7_error.is_some(), "Should have error on line 7");
    assert!(line_11_error.is_some(), "Should have error on line 11");

    assert!(line_7_error.unwrap().message.contains("1") && line_7_error.unwrap().message.contains("expected 2"));
    assert!(line_11_error.unwrap().message.contains("3") && line_11_error.unwrap().message.contains("expected 2"));
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // pulldown-cmark sees this as 3 separate lists, each correctly numbered:
    // - List 1: items 1 and 2 at lines 1 and 5 (correct sequence)
    // - List 2: single item "1. Nested item" at line 6 (correct - starts at 1)
    // - List 3: single item "1. Should be 2..." at line 10 (correct - starts at 1)
    // Verified with markdownlint-cli: no MD029 errors with ordered style.
    assert_eq!(result.len(), 0, "All lists are correctly numbered");
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx);

        // Just verify it doesn't crash and produces some result
        assert!(result.is_ok(), "Style {style:?} should not crash on nested lists");

        // Test that fix works too
        let fixed = rule.fix(&ctx);
        assert!(fixed.is_ok(), "Style {style:?} fix should not crash on nested lists");
    }
}
// ==================== COMPREHENSIVE TEST SUITE ====================

/// Tests for lists starting at different numbers with various styles
mod starting_numbers {
    use super::*;

    #[test]
    fn test_ordered_style_list_starting_at_5() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "5. First item\n6. Second item\n7. Third item";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // With CommonMark start value support, list starting at 5 is correctly numbered
        assert!(
            result.is_empty(),
            "No warnings - list is correctly numbered from its start value 5"
        );
    }

    #[test]
    fn test_ordered0_style_accepts_zero_based() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered0);
        let content = "0. First item\n1. Second item\n2. Third item";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Ordered0 style accepts 0-based numbering
        assert!(result.is_empty());
    }

    #[test]
    fn test_ordered0_style_rejects_one_based() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered0);
        let content = "1. First item\n2. Second item\n3. Third item";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Expects 0, 1, 2
        assert_eq!(result.len(), 3);
        assert!(result[0].message.contains("1") && result[0].message.contains("expected 0"));
    }

    #[test]
    fn test_ordered_style_rejects_zero_based() {
        // NOTE: Ordered style with CommonMark start value support respects start at 0
        // This list 0, 1, 2 is correctly numbered from its start value 0
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "0. First item\n1. Second item\n2. Third item";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // With CommonMark start value support, list is correctly numbered from start 0
        assert!(
            result.is_empty(),
            "No warnings - list is correctly numbered from its start value 0"
        );
    }

    #[test]
    fn test_very_large_starting_number() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "9999. Item at 9999\n10000. Item at 10000\n10001. Item at 10001";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // With CommonMark start value support, list is correctly numbered from start 9999
        assert!(
            result.is_empty(),
            "No warnings - list is correctly numbered from its start value 9999"
        );
    }
}

/// Comprehensive tests for each ListStyle variant
mod list_style_behaviors {
    use super::*;

    #[test]
    fn test_one_style_all_ones_valid() {
        let rule = MD029OrderedListPrefix::new(ListStyle::One);
        let content = "1. First\n1. Second\n1. Third";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_one_style_rejects_incrementing() {
        let rule = MD029OrderedListPrefix::new(ListStyle::One);
        let content = "1. First\n2. Second\n3. Third";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Expects all 1s
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("2") && result[0].message.contains("expected 1"));
        assert!(result[1].message.contains("3") && result[1].message.contains("expected 1"));
    }

    #[test]
    fn test_oneone_style_all_ones_valid() {
        let rule = MD029OrderedListPrefix::new(ListStyle::OneOne);
        let content = "1. First\n1. Second\n1. Third";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_oneone_style_rejects_incrementing() {
        let rule = MD029OrderedListPrefix::new(ListStyle::OneOne);
        let content = "1. First\n2. Second\n3. Third";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Expects all 1s
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_ordered_style_incrementing_valid() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "1. First\n2. Second\n3. Third";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_ordered_style_rejects_all_ones() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "1. First\n1. Second\n1. Third";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Expects 1, 2, 3
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("1") && result[0].message.contains("expected 2"));
        assert!(result[1].message.contains("1") && result[1].message.contains("expected 3"));
    }

    #[test]
    fn test_ordered0_style_zero_based_valid() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered0);
        let content = "0. First\n1. Second\n2. Third\n3. Fourth";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }
}

/// Tests for fix functionality across all styles
mod fix_functionality {
    use super::*;

    #[test]
    fn test_fix_ordered_style() {
        // With CommonMark start value support, 5, 6, 7 is correctly numbered
        // The fix should not change it
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "5. First\n6. Second\n7. Third";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // No fix needed - list is correctly numbered from start value 5
        assert_eq!(fixed, "5. First\n6. Second\n7. Third");
    }

    #[test]
    fn test_fix_one_style() {
        let rule = MD029OrderedListPrefix::new(ListStyle::One);
        let content = "1. First\n2. Second\n3. Third";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "1. First\n1. Second\n1. Third");
    }

    #[test]
    fn test_fix_ordered0_style() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered0);
        let content = "1. First\n2. Second\n3. Third";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "0. First\n1. Second\n2. Third");
    }

    #[test]
    fn test_fix_preserves_content() {
        // With CommonMark start value support, 5, 6, 7 is correctly numbered
        // The fix should not change it
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "5. **Bold** text\n6. *Italic* text\n7. `Code` text";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // No fix needed - list is correctly numbered from start value 5
        assert_eq!(fixed, "5. **Bold** text\n6. *Italic* text\n7. `Code` text");
    }

    #[test]
    fn test_fix_with_indented_content() {
        // With CommonMark start value support, 5, 6 is correctly numbered
        // The fix should not change it
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "5. First item\n   with continuation\n6. Second item";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // No fix needed - list is correctly numbered from start value 5
        assert_eq!(fixed, "5. First item\n   with continuation\n6. Second item");
    }
}

/// Tests for list grouping and separation
mod list_grouping {
    use super::*;

    #[test]
    fn test_lists_separated_by_heading_are_independent() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "1. First list\n\n## Heading\n\n1. Second list";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Both lists are valid independently
        assert!(result.is_empty());
    }
}

/// Tests for nested and mixed lists
mod nested_lists {
    use super::*;

    #[test]
    fn test_nested_ordered_in_ordered() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "1. Top level\n   1. Nested level\n   2. Second nested\n2. Second top";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Both levels should be valid with Ordered style
        assert!(result.is_empty());
    }

    #[test]
    fn test_ordered_nested_in_unordered() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "- Unordered item\n  1. Nested ordered\n  2. Second ordered\n- Another unordered";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Nested ordered list should be valid
        assert!(result.is_empty());
    }

    #[test]
    fn test_mixed_list_markers() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "1. Ordered\n- Unordered\n2. Ordered again";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Unordered list separates the two ordered lists.
        // With CommonMark start value support, both lists are correctly numbered:
        // First list starts at 1, second list starts at 2.
        assert!(
            result.is_empty(),
            "No warnings - both lists are correctly numbered from their start values"
        );
    }
}

/// Tests for lazy continuation detection
mod lazy_continuation {
    use super::*;

    #[test]
    fn test_lazy_continuation_detected() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "1. First item\ncontinuation without indent\n2. Second item";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // With pulldown-cmark, lazy continuation makes this one list with two properly
        // numbered items (1 and 2). No warnings expected - verified with markdownlint-cli.
        assert_eq!(
            result.len(),
            0,
            "Lazy continuation is valid CommonMark, no numbering error"
        );
    }

    #[test]
    fn test_proper_indent_not_flagged() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "1. First item\n   properly indented continuation\n2. Second item";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Properly indented continuation should not trigger lazy continuation warning
        let lazy_warnings = result
            .iter()
            .filter(|w| w.message.contains("lazy continuation"))
            .count();
        assert_eq!(lazy_warnings, 0);
    }
}

/// Edge cases and boundary conditions
mod edge_cases {
    use super::*;

    #[test]
    fn test_single_item_list() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "1. Only one item";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Single item starting at 1 is valid
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_item_list_starting_at_zero() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered0);
        let content = "0. Only one item";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Single item starting at 0 is valid for Ordered0
        assert!(result.is_empty());
    }

    #[test]
    fn test_empty_document() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_no_lists_in_document() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "# Heading\n\nParagraph text.\n\nMore text.";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_unordered_lists_only() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "- Item 1\n- Item 2\n- Item 3";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // No ordered lists to check
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_with_formatting() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "1. **Bold** item\n2. *Italic* item\n3. `Code` item\n4. [Link](url) item";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Formatting should not affect validation
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_with_blank_lines_between_items() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "1. First item\n\n2. Second item\n\n3. Third item";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Blank lines between items should not affect numbering
        assert!(result.is_empty());
    }

    #[test]
    fn test_skipped_numbers() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "1. First\n3. Third\n5. Fifth";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should expect continuous numbering
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("3") && result[0].message.contains("expected 2"));
        assert!(result[1].message.contains("5") && result[1].message.contains("expected 3"));
    }

    #[test]
    fn test_descending_numbers() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "3. Third\n2. Second\n1. First";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // With CommonMark start value support, list starts at 3.
        // Items should be 3, 4, 5, so 2 and 1 are wrong.
        assert_eq!(result.len(), 2);
        assert!(
            result[0].message.contains("expected 4"),
            "2 should expect 4: {}",
            result[0].message
        );
        assert!(
            result[1].message.contains("expected 5"),
            "1 should expect 5: {}",
            result[1].message
        );
    }
}

/// Tests for different number of items
mod item_counts {
    use super::*;

    #[test]
    fn test_two_item_list() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let content = "1. First\n2. Second";

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_long_list() {
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
        let mut items = Vec::new();
        for i in 1..=20 {
            items.push(format!("{i}. Item {i}"));
        }
        let content = items.join("\n");

        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_long_list_all_ones() {
        let rule = MD029OrderedListPrefix::new(ListStyle::One);
        let mut items = Vec::new();
        for i in 1..=20 {
            items.push(format!("1. Item {i}"));
        }
        let content = items.join("\n");

        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }
}
