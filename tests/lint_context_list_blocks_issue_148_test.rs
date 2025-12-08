// Regression tests for issue #148 - list block parsing O(n²) optimization
// These tests ensure that the O(n) forward-scanning optimization produces
// identical results to the original nested loop implementation

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;

#[test]
fn test_consecutive_list_items() {
    // Two consecutive list items should be in the same block
    let content = "- Item 1\n- Item 2\n- Item 3";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should have exactly 1 list block containing all 3 items
    assert_eq!(
        ctx.list_blocks.len(),
        1,
        "Consecutive list items should form a single block"
    );
    assert_eq!(ctx.list_blocks[0].item_lines.len(), 3);
}

#[test]
fn test_list_items_with_blank_line() {
    // List items with one blank line between should remain in same block
    let content = "- Item 1\n\n- Item 2\n\n- Item 3";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.list_blocks.len(),
        1,
        "List items with blank lines should stay in same block (reasonable distance)"
    );
}

#[test]
fn test_list_broken_by_heading() {
    // A heading should break the list into separate blocks
    let content = "- Item 1\n\n# Heading\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.list_blocks.len(),
        2,
        "Heading should separate lists into distinct blocks"
    );
}

#[test]
fn test_list_broken_by_setext_heading() {
    // A setext heading should also break lists
    let content = "- Item 1\n\nHeading\n=======\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.list_blocks.len(), 2, "Setext heading should separate lists");
}

#[test]
fn test_list_broken_by_horizontal_rule() {
    // Horizontal rules should break lists
    let content = "- Item 1\n\n---\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.list_blocks.len(), 2, "Horizontal rule should separate lists");
}

#[test]
fn test_list_broken_by_table() {
    // Tables should break lists
    let content = "- Item 1\n\n| Col1 | Col2 |\n|------|------|\n| A    | B    |\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.list_blocks.len(), 2, "Table should separate lists");
}

#[test]
fn test_list_with_properly_indented_continuation() {
    // Properly indented content should continue the list
    let content = "- Item 1\n\n  Continuation paragraph for item 1\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.list_blocks.len(),
        1,
        "Properly indented continuation should not break list"
    );
}

#[test]
fn test_list_broken_by_insufficiently_indented_content() {
    // Content that's not indented enough should break the list
    let content = "- Item 1\n\nNot indented enough\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.list_blocks.len(),
        2,
        "Insufficiently indented content should break list"
    );
}

#[test]
fn test_ordered_list_continuation_indent() {
    // Ordered lists require different indentation for continuation
    let content = "1. First item\n\n   Continuation paragraph\n\n2. Second item";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.list_blocks.len(),
        1,
        "Properly indented ordered list continuation should not break list"
    );
}

#[test]
fn test_nested_lists_same_block() {
    // Nested lists should be part of the parent block
    let content = "- Item 1\n  - Nested A\n  - Nested B\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.list_blocks.len(), 1, "Nested lists should be part of parent block");
}

#[test]
fn test_mixed_list_types_separate_blocks() {
    // Switching between ordered and unordered at same level breaks list
    let content = "- Item 1\n\n1. Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.list_blocks.len(),
        2,
        "Switching list types should create separate blocks"
    );
}

#[test]
fn test_list_with_standalone_code_block() {
    // Standalone code blocks (not indented for list continuation) should break lists
    let content = "- Item 1\n\n```\ncode\n```\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.list_blocks.len(), 2, "Standalone code block should separate lists");
}

#[test]
fn test_list_with_indented_code_block_continuation() {
    // Code blocks indented as list continuation - current behavior creates 2 blocks
    // This is because 6-space indented code after blank line is treated as standalone
    let content = "- Item 1\n\n      code line 1\n      code line 2\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Current implementation treats this as 2 separate blocks
    assert_eq!(
        ctx.list_blocks.len(),
        2,
        "Indented code block currently breaks list (existing behavior)"
    );
}

#[test]
fn test_list_broken_by_blockquote() {
    // Blockquotes should break lists
    let content = "- Item 1\n\n> Quote\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.list_blocks.len(), 2, "Blockquote should separate lists");
}

#[test]
fn test_performance_many_consecutive_items() {
    // Performance test: many consecutive items should be fast (O(n))
    let mut content = String::new();
    for i in 0..1000 {
        content.push_str(&format!("- Item {i}\n"));
    }

    let start = std::time::Instant::now();
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
    let elapsed = start.elapsed();

    // Should complete in < 50ms for 1000 items (O(n) behavior)
    assert!(
        elapsed.as_millis() < 50,
        "1000 consecutive items took {elapsed:?} (should be < 50ms for O(n))"
    );

    assert_eq!(ctx.list_blocks.len(), 1, "All consecutive items should be in one block");
}

#[test]
fn test_performance_issue_148_pattern() {
    // Reproduce the exact pattern from issue #148: nested lists with brackets
    let mut content = String::new();
    for i in 0..300 {
        content.push_str(&format!("- item{i}\n"));
        content.push_str(&format!("  - nested{i}\n"));
        content.push_str(&format!("    - fix: [\"record_{i}\"]\n"));
    }

    let start = std::time::Instant::now();
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
    let elapsed = start.elapsed();

    // Should complete in < 100ms for 900 lines (O(n) behavior)
    // The old O(n²) code took 50+ seconds for this pattern
    assert!(
        elapsed.as_millis() < 100,
        "Issue #148 pattern (900 lines) took {elapsed:?} (should be < 100ms for O(n))"
    );

    assert_eq!(ctx.list_blocks.len(), 1, "Nested list pattern should form one block");
}

#[test]
fn test_reasonable_distance_limit() {
    // Items separated by multiple blank lines - current behavior keeps them together
    // The reasonable_distance check is: line_num <= last_list_item_line + 2
    // With 4 blank lines: lines are 1 and 6, distance is 5, but they're still in same block
    let content = "- Item 1\n\n\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Current implementation keeps these in same block
    assert_eq!(
        ctx.list_blocks.len(),
        1,
        "Items with blank lines currently stay in same block (existing behavior)"
    );
}

#[test]
fn test_unordered_marker_consistency() {
    // Different markers (*,-,+) at same level should create separate blocks
    let content = "- Item 1\n* Item 2\n+ Item 3";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // This is marker-inconsistent, may create separate blocks
    // The behavior depends on marker_compatible logic
    assert!(!ctx.list_blocks.is_empty(), "Should parse list blocks");
}

#[test]
fn test_triple_dash_horizontal_rule() {
    // Ensure --- is detected as horizontal rule, not part of list
    let content = "- Item 1\n\n---\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.list_blocks.len(),
        2,
        "Triple dash horizontal rule should separate lists"
    );
}

#[test]
fn test_triple_underscore_horizontal_rule() {
    // Ensure ___ is detected as horizontal rule
    let content = "- Item 1\n\n___\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.list_blocks.len(),
        2,
        "Triple underscore horizontal rule should separate lists"
    );
}

#[test]
fn test_triple_asterisk_horizontal_rule() {
    // Ensure *** is detected as horizontal rule
    let content = "- Item 1\n\n***\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.list_blocks.len(),
        2,
        "Triple asterisk horizontal rule should separate lists"
    );
}

#[test]
fn test_table_pipe_detection() {
    // Tables with pipes should break lists
    let content = "- Item 1\n\n| A | B |\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.list_blocks.len(), 2, "Table with pipes should separate lists");
}

#[test]
fn test_table_pipe_in_link_not_breaking() {
    // Pipes inside links like [text](url|param) shouldn't break lists
    let content = "- Item 1\n\n  See [link](http://example.com?a|b)\n\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.list_blocks.len(),
        1,
        "Pipe in link URL should not break list continuation"
    );
}

#[test]
fn test_ordered_list_double_digit_continuation() {
    // Ordered lists with double-digit numbers need more continuation indent
    let content = "10. First item\n\n    Continuation paragraph\n\n11. Second item";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.list_blocks.len(),
        1,
        "Double-digit ordered list with proper continuation should not break"
    );
}

#[test]
fn test_deeply_nested_lists() {
    // Deeply nested lists should all be part of the same top-level block
    let content = "- L1\n  - L2\n    - L3\n      - L4\n- L1 Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.list_blocks.len(), 1, "Deeply nested lists should be in same block");
}

#[test]
fn test_empty_list_items() {
    // Empty lines between list items
    let content = "- Item 1\n-\n- Item 2";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(!ctx.list_blocks.is_empty(), "Should handle empty list items");
}

#[test]
fn test_list_in_blockquote() {
    // Lists inside blockquotes should be separate from lists outside
    let content = "> - Quote item\n\n- Normal item";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.list_blocks.len(),
        2,
        "Lists in different blockquote contexts should be separate"
    );
}
