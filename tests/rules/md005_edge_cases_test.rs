use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD005ListIndent;

/// Test that first list item (no parent) is handled correctly
#[test]
fn test_first_list_item_no_parent() {
    let rule = MD005ListIndent::default();
    let content = "* First item ever";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "First list item should not trigger warnings");
}

/// Test jumping multiple indentation levels at once (0 → 6 spaces)
#[test]
fn test_jump_multiple_indentation_levels() {
    let rule = MD005ListIndent::default();
    let content = "\
* Level 0
      * Jumped to 6 spaces";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Dynamic detection should accept this as a valid 6-space pattern
    assert!(result.is_empty(), "Large indentation jump should be valid with dynamic detection");
}

/// Test very deep nesting (5 levels) to stress HashMap
#[test]
fn test_very_deep_nesting() {
    let rule = MD005ListIndent::default();
    let content = "\
* Level 1
  * Level 2
    * Level 3
      * Level 4
        * Level 5";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Very deep nesting should be valid");
}

/// Test sibling items at the same indentation level
#[test]
fn test_sibling_items_same_level() {
    let rule = MD005ListIndent::default();
    let content = "\
* Item 1
* Item 2
* Item 3
  * Nested under 3
* Item 4";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Sibling items at same level should be valid");
}

/// Test jumping back multiple levels (critical for testing retain() logic)
#[test]
fn test_jump_back_multiple_levels() {
    let rule = MD005ListIndent::default();
    let content = "\
* Level 1
  * Level 2
    * Level 3
      * Level 4
* Back to Level 1
  * Level 2 again";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Jumping back multiple levels should work correctly");
}

/// Test alternating back and forth between levels
#[test]
fn test_alternating_levels() {
    let rule = MD005ListIndent::default();
    let content = "\
* Level 1
  * Level 2
* Level 1
  * Level 2
    * Level 3
  * Level 2
* Level 1";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Alternating between levels should be valid");
}

/// Test different list markers (-, +, *) at different levels
#[test]
fn test_different_markers_per_level() {
    let rule = MD005ListIndent::default();
    let content = "\
* Asterisk level 1
  - Dash level 2
    + Plus level 3
  - Dash level 2 again
* Asterisk level 1 again";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Different markers at different levels should be valid");
}

/// Test that incorrect parent assignment is caught
#[test]
fn test_incorrect_parent_indentation() {
    let rule = MD005ListIndent::default();
    let content = "\
* Level 1 (0 spaces)
  * Level 2 (2 spaces)
    * Level 3 (4 spaces)
   * Wrong indent (3 spaces - not aligned with any parent)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Should flag the item with 3 spaces as it doesn't match the 2-space pattern
    assert_eq!(result.len(), 1, "Should detect incorrect indentation");
    assert_eq!(result[0].line, 4, "Should flag line 4");
}

/// Test many items at same level (stress HashMap performance)
#[test]
fn test_many_items_same_level() {
    let rule = MD005ListIndent::default();
    let mut content = String::new();
    for i in 1..=50 {
        content.push_str(&format!("* Item {}\n", i));
    }
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Many items at same level should be valid");
}

/// Test pattern that triggered the original bug: stale HashMap entries
#[test]
fn test_stale_hashmap_entries_bug() {
    let rule = MD005ListIndent::default();
    let content = "\
* Item at indent 0
 * Item at indent 1 (wrong for top-level)
  * Item at indent 2
* Back to indent 0
  * Item at indent 2 - should find indent 0 as parent, NOT indent 1";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // The critical test: line 5 should correctly identify line 4 (indent 0) as parent,
    // not line 2 (indent 1) which should have been removed from the tracking HashMap
    // This is exactly what the retain() call fixes
    assert_eq!(result.len(), 1, "Should only flag line 2 with wrong indentation");
    assert_eq!(result[0].line, 2, "Should flag line 2");
}

/// Test ordered lists with large numbers
#[test]
fn test_ordered_lists_large_numbers() {
    let rule = MD005ListIndent::default();
    let content = "\
100. Item one hundred
     1. Nested item (aligned with parent text)
101. Item one hundred one";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Large numbered items should work correctly");
}

/// Test mixed ordered and unordered at multiple levels
#[test]
fn test_mixed_ordered_unordered_complex() {
    let rule = MD005ListIndent::default();
    let content = "\
1. Ordered level 1
   * Unordered level 2
     1. Ordered level 3
        * Unordered level 4
   * Back to unordered level 2
2. Ordered level 1 again";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Complex mixing should be valid");
}

/// Test list with blank lines between items at different levels
#[test]
fn test_blank_lines_between_levels() {
    let rule = MD005ListIndent::default();
    let content = "\
* Level 1

  * Level 2

    * Level 3

* Back to level 1";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Blank lines between levels should be valid");
}

/// Test that parent is the MOST RECENT item with less indentation
#[test]
fn test_parent_is_most_recent() {
    let rule = MD005ListIndent::default();
    let content = "\
* Level 1a (indent 0)
  * Level 2a (indent 2)
* Level 1b (indent 0) - should invalidate 2a as potential parent
    * Level 2b (indent 4) - parent should be 1b (line 3), not 2a (line 2)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Dynamic detection: line 4 has 4 spaces, which becomes a new pattern
    assert!(result.is_empty(), "Should use most recent parent at each level");
}

/// Test jumping to deeper level then back, then deeper again
#[test]
fn test_zigzag_nesting() {
    let rule = MD005ListIndent::default();
    let content = "\
* Level 1
      * Deep level (6 spaces)
* Level 1
  * Level 2 (2 spaces)
      * Back to deep level (6 spaces)
  * Level 2
* Level 1";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Dynamic detection allows this pattern
    assert!(result.is_empty(), "Zigzag nesting should be valid");
}

/// Test edge case: all items at same wrong indentation level
#[test]
fn test_all_items_wrong_indent() {
    let rule = MD005ListIndent::default();
    let content = "\
 * Item 1 (1 space - wrong)
 * Item 2 (1 space - wrong)
 * Item 3 (1 space - wrong)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // All items have 1 space, which is detected as a pattern
    // Should flag all 3 lines as they should start at column 0
    assert_eq!(result.len(), 3, "Should flag all incorrectly indented items");
}

/// Test transitioning from valid to invalid indentation mid-list
#[test]
fn test_transition_valid_to_invalid() {
    let rule = MD005ListIndent::default();
    let content = "\
* Item 1
  * Item 2 (correct - 2 spaces)
    * Item 3 (correct - 4 spaces)
   * Item 4 (wrong - 3 spaces, doesn't match pattern)
  * Item 5 (correct - 2 spaces)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should flag only the item with wrong indentation");
    assert_eq!(result[0].line, 4, "Should flag line 4");
}

/// Test that empty list items are handled correctly
#[test]
fn test_empty_list_items() {
    let rule = MD005ListIndent::default();
    let content = "\
*
  *
    *
* Item";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Empty items should still follow indentation rules
    assert!(result.is_empty(), "Empty list items should follow same rules");
}

/// Test performance pattern from issue #148
#[test]
fn test_issue_148_pattern() {
    let rule = MD005ListIndent::default();
    let mut content = String::new();
    // Create nested list pattern similar to issue #148
    for _ in 0..100 {
        content.push_str("* Item\n");
        content.push_str("  * Nested\n");
        content.push_str("    * Deep\n");
        content.push_str("      * Deeper\n");
    }
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Issue #148 pattern should be valid and fast");
}

/// Regression test for issue #186: Sublist after code block in list item
/// When a parent list item is skipped as continuation content, its children
/// should also be skipped to prevent orphaned items being flagged incorrectly.
#[test]
fn test_issue_186_sublist_after_code_block() {
    let rule = MD005ListIndent::default();

    // This is the exact pattern from issue #186
    let content = "\
* Some list
  * item1

  ```sh
  echo \"code\"
  ```

  * item2
    * correctly indented subitem";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Issue #186: Sublist after code block incorrectly flagged. Got: {:?}",
        result
    );

    // Also test with ordered lists
    let content = "\
1. Main item

   ```rust
   fn foo() {}
   ```

   Sublist:

   - A
   - B";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Ordered list with code block and continuation sublist incorrectly flagged. Got: {:?}",
        result
    );
}

/// Regression test: lists inside blockquotes must use blockquote-aware indent calculation
/// Previously, raw indent (0 for blockquote lines) was compared against content_column,
/// causing continuation detection to fail for blockquote lists.
#[test]
fn test_blockquote_list_continuation_detection() {
    let rule = MD005ListIndent::default();

    // List inside blockquote with proper continuation indent
    let content = "\
> * Parent item
>   * Child item with correct indent";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Blockquote nested list should not be flagged with correct indent. Got: {:?}",
        result
    );
}

/// Test nested blockquotes with lists
#[test]
fn test_nested_blockquote_list_indent() {
    let rule = MD005ListIndent::default();

    // Nested blockquote with properly indented list
    let content = "\
> > * Outer item
> >   * Inner item";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Nested blockquote list should not be flagged. Got: {:?}",
        result
    );
}

/// Test blockquote list with continuation content (like issue #268 pattern)
#[test]
fn test_blockquote_list_with_continuation_content() {
    let rule = MD005ListIndent::default();

    // Pattern from issue #268: list items with continuation in blockquote
    let content = "\
> 1. Opening the app
>    and doing stuff
>    [**See preview here!**](https://example.com)
> 2. Second item";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Blockquote list with continuation content should not be flagged. Got: {:?}",
        result
    );
}
