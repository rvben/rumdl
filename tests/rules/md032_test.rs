use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD032BlanksAroundLists;

#[test]
fn test_valid_lists() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Some text\n\n* Item 1\n* Item 2\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_missing_blank_line_before() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Some text\n* Item 1\n* Item 2\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_missing_blank_line_after() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Some text\n\n* Item 1\n* Item 2\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_fix_missing_blank_lines() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n* Item 1\n* Item 2\nMore text";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed);
    assert_eq!(fixed, "Text\n\n* Item 1\n* Item 2\n\nMore text");
}

// CRITICAL REGRESSION TESTS: Emphasis text should NOT be detected as list markers

#[test]
fn test_emphasis_not_list_marker_simple() {
    // Test simple emphasis pattern that should NOT be detected as a list marker
    let rule = MD032BlanksAroundLists::default();
    let content = "*Emphasis text*\n- List item\n- Another item";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the list items (lines 2-3) for missing blank line before,
    // NOT the emphasis text on line 1
    assert_eq!(
        result.len(),
        1,
        "Should only detect one warning for the list missing blank line before"
    );
    assert_eq!(result[0].line, 2, "Warning should be on line 2 (first list item)");
    assert!(result[0].message.contains("preceded by blank line"));
}

#[test]
fn test_emphasis_not_list_marker_multiple_stars() {
    // Test various emphasis patterns that should NOT be detected as lists
    let rule = MD032BlanksAroundLists::default();
    let content =
        "**Bold text here**\n*Italic text*\n***Bold italic***\n\n- Actual list item\n- Another item\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have no warnings - the list is properly surrounded by blank lines
    assert!(
        result.is_empty(),
        "Emphasis text should not be detected as list markers"
    );
}

#[test]
fn test_emphasis_followed_by_list_needs_blank() {
    // This is the exact case from the parity corpus that was failing
    let rule = MD032BlanksAroundLists::default();
    let content = "**Problem: Permission errors**\n- On Windows: Run as administrator";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should flag that the list needs a blank line before it
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);

    // Test the fix
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "**Problem: Permission errors**\n\n- On Windows: Run as administrator"
    );
}

#[test]
fn test_nested_lists_issue_33() {
    // Test for GitHub issue #33 - Nested lists should not require blank lines between levels
    let rule = MD032BlanksAroundLists::default();
    let content = "## Heading\n\n1. List item 1\n   - sub list 1.1\n   - sub list 1.2\n1. List item 2\n   - sub list 2.1\n\nThat was a nice list.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have no warnings - the nested list is properly surrounded by blank lines
    assert!(
        result.is_empty(),
        "Nested lists should not require blank lines between parent and child items. Got {} warnings: {:?}",
        result.len(),
        result
    );
}

#[test]
fn test_blockquote_numbers_issue_32() {
    // Test for GitHub issue #32 - Lines starting with numbers in blockquotes should not be detected as lists
    let rule = MD032BlanksAroundLists::default();
    let content = "> The following versions are vulnerable:\n>   all versions 9 and before\n>   10.5 - 10.6\n>   11.1 - 11.2\n> Other information";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have no MD032 warnings - these are not list items
    assert!(
        result.is_empty(),
        "Version numbers like '10.5 - 10.6' should not be detected as list items. Got {} warnings: {:?}",
        result.len(),
        result
    );
}

#[test]
fn test_emphasis_patterns_not_lists() {
    // Test various emphasis patterns that contain * or + characters
    let rule = MD032BlanksAroundLists::default();
    let content = "**API Parameters**\n*userId (string) - The user ID*\n\n+ This is a real list item\n+ Another real item\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have no warnings - emphasis is not a list, and the real list is properly spaced
    assert!(result.is_empty());
}

#[test]
fn test_heading_emphasis_not_list() {
    // Test heading with emphasis that was causing false positives
    let rule = MD032BlanksAroundLists::default();
    let content = "## **Section Title**\n\nSome content\n\n- Real list item\n- Another item";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have no warnings - neither heading emphasis nor properly spaced list should trigger
    assert!(result.is_empty());
}

#[test]
fn test_multiple_lists() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n* List 1\n* List 1\nText\n1. List 2\n2. List 2\nText";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed);
    assert_eq!(
        fixed,
        "Text\n\n* List 1\n* List 1\n\nText\n\n1. List 2\n2. List 2\n\nText"
    );
}

#[test]
fn test_nested_lists() {
    // Nested lists should not require blank lines between parent and child items
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n* Item 1\n  * Nested 1\n  * Nested 2\n* Item 2\nText";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Should only warn about missing blank lines around the outer list, not the nested items
    assert_eq!(
        result.len(),
        2,
        "Should only warn about outer list boundaries, not nested items"
    );
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed);
    assert_eq!(fixed, "Text\n\n* Item 1\n  * Nested 1\n  * Nested 2\n* Item 2\n\nText");
}

#[test]
fn test_nested_lists_with_strict_mode() {
    // Even in strict mode, nested lists are a standard Markdown pattern and shouldn't require blank lines
    let rule = MD032BlanksAroundLists::strict();
    let content = "Text\n\n* Item 1\n  * Nested 1\n  * Nested 2\n* Item 2\n\nText";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Even strict mode should not warn about nested lists - this is standard Markdown
    assert!(
        result.is_empty(),
        "Nested lists are standard Markdown and shouldn't trigger warnings even in strict mode"
    );
}

#[test]
fn test_deeply_nested_lists() {
    // Test multiple levels of nesting
    let rule = MD032BlanksAroundLists::default();
    let content = "## Section\n\n* Level 1\n  * Level 2\n    * Level 3\n      * Level 4\n  * Back to Level 2\n* Back to Level 1\n\nText";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Should not warn about nested list transitions
    assert!(result.is_empty(), "Should not warn about deeply nested lists");
}

#[test]
fn test_nested_ordered_lists() {
    // Test nested ordered lists
    let rule = MD032BlanksAroundLists::default();
    let content = "## Section\n\n1. First item\n   1. Sub item 1.1\n   2. Sub item 1.2\n2. Second item\n   1. Sub item 2.1\n\nText";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not warn about nested ordered lists");
}

#[test]
fn test_mixed_nested_list_types() {
    // Test mixing ordered and unordered lists in nesting
    let rule = MD032BlanksAroundLists::default();
    let content = "## Section\n\n1. Ordered item\n   - Unordered sub-item\n   - Another unordered sub-item\n2. Another ordered item\n   * Different unordered marker\n\nText";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not warn about mixed nested list types");
}

#[test]
fn test_mixed_list_types() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n* Unordered\n* List\nText\n1. Ordered\n2. List\nText";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed);
    assert_eq!(
        fixed,
        "Text\n\n* Unordered\n* List\n\nText\n\n1. Ordered\n2. List\n\nText"
    );
}

#[test]
fn test_list_with_content() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n* Item 1\n  Content\n* Item 2\n  More content\nText";
    let ctx = LintContext::new(content);

    // --- Temporary Debugging ---
    let temp_structure = rumdl_lib::utils::document_structure::document_structure_from_str(ctx.content);
    println!(
        "DEBUG MD032 - test_list_with_content - structure.list_lines: {:?}",
        temp_structure.list_lines
    );

    let lines_vec: Vec<&str> = ctx.content.lines().collect();
    let num_lines_vec = lines_vec.len();
    let mut calculated_blocks: Vec<(usize, usize)> = Vec::new();
    let mut current_block_start_debug: Option<usize> = None;
    for i_debug in 0..num_lines_vec {
        let current_line_idx_1_debug = i_debug + 1;
        let is_list_related_debug = temp_structure.list_lines.contains(&current_line_idx_1_debug);
        let is_excluded_debug = temp_structure.is_in_code_block(current_line_idx_1_debug)
            || temp_structure.is_in_front_matter(current_line_idx_1_debug);
        if is_list_related_debug && !is_excluded_debug {
            if current_block_start_debug.is_none() {
                current_block_start_debug = Some(current_line_idx_1_debug);
            }
            if i_debug == num_lines_vec - 1
                && let Some(start) = current_block_start_debug
            {
                calculated_blocks.push((start, current_line_idx_1_debug));
            }
        } else if let Some(start) = current_block_start_debug {
            calculated_blocks.push((start, i_debug));
            current_block_start_debug = None;
        }
    }
    println!("DEBUG MD032 - test_list_with_content - calculated_blocks: {calculated_blocks:?}");
    // --- End Temporary Debugging ---

    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed);
    assert_eq!(fixed, "Text\n\n* Item 1\n  Content\n* Item 2\n  More content\n\nText");
}

#[test]
fn test_list_at_start() {
    let rule = MD032BlanksAroundLists::default();
    let content = "* Item 1\n* Item 2\nText";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed);
    assert_eq!(fixed, "* Item 1\n* Item 2\n\nText");
}

#[test]
fn test_list_at_end() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n* Item 1\n* Item 2";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed);
    assert_eq!(fixed, "Text\n\n* Item 1\n* Item 2");
}

#[test]
fn test_multiple_blank_lines() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n\n\n* Item 1\n* Item 2\n\n\nText";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_list_with_blank_lines() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n\n* Item 1\n\n* Item 2\n\nText";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md032_toc_false_positive() {
    let rule = MD032BlanksAroundLists::default();
    let content = r#"
## Table of Contents

- [Item 1](#item-1)
  - [Sub Item 1a](#sub-item-1a)
  - [Sub Item 1b](#sub-item-1b)
- [Item 2](#item-2)
  - [Sub Item 2a](#sub-item-2a)
- [Item 3](#item-3)

## Next Section
"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "MD032 should not trigger inside a list, but got warnings: {result:?}"
    );
}

#[test]
fn test_list_followed_by_heading_invalid() {
    let rule = MD032BlanksAroundLists::default();
    let content = "* Item 1\n* Item 2\n## Next Section";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should warn for missing blank line before heading");
    assert!(result[0].message.contains("followed by blank line"));
}

#[test]
fn test_list_followed_by_code_block_invalid() {
    let rule = MD032BlanksAroundLists::default();
    let content = "* Item 1\n* Item 2\n```\ncode\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should warn for missing blank line before code block");
    assert!(result[0].message.contains("followed by blank line"));
}

#[test]
fn test_list_followed_by_blank_then_code_block_valid() {
    let rule = MD032BlanksAroundLists::default();
    let content = "* Item 1\n* Item 2\n\n```\ncode\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not warn when blank line precedes code block");
}

// New tests for lenient behavior

#[test]
fn test_allow_list_after_heading() {
    let rule = MD032BlanksAroundLists::default(); // Has allow_after_headings = true

    // Test cases that should NOT be flagged with lenient settings
    let valid_cases = vec![
        "# Items to consider:\n* Item 1\n* Item 2\n\nMore text",
        "## Steps:\n1. First step\n2. Second step\n\nFollowing text",
        "### Features\n- Feature A\n- Feature B\n\nDescription",
    ];

    for case in valid_cases {
        let ctx = LintContext::new(case);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lists after headings in: {case}");
    }
}

#[test]
fn test_allow_list_after_colon() {
    let rule = MD032BlanksAroundLists::default(); // Has allow_after_colons = true

    // Test cases that should NOT be flagged with lenient settings
    let valid_cases = vec![
        "Here are the steps:\n1. First step\n2. Second step\n\nFollowing text",
        "Items to consider:\n* Item 1\n* Item 2\n\nMore text",
        "The following options are available:\n- Option A\n- Option B\n\nDescription",
    ];

    for case in valid_cases {
        let ctx = LintContext::new(case);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag lists after colons in: {case}");
    }
}

#[test]
fn test_strict_mode_flags_all() {
    let rule = MD032BlanksAroundLists::strict(); // All allowances disabled

    // Even valid cases should be flagged in strict mode
    let cases = vec![
        "# Items to consider:\n* Item 1\n* Item 2\n\nMore text",
        "Here are the steps:\n1. First step\n2. Second step\n\nFollowing text",
    ];

    for case in cases {
        let ctx = LintContext::new(case);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty(), "Strict mode should flag lists in: {case}");
    }
}

#[test]
fn test_still_flags_inappropriate_cases() {
    let rule = MD032BlanksAroundLists::default();

    // These should still be flagged even with lenient settings
    let invalid_cases = vec![
        "Some random text\n* Item 1\n* Item 2\n\nMore text", // No colon, not a heading
        "This is a paragraph.\n- Item A\n- Item B\n\nText",  // Period, not colon
    ];

    for case in invalid_cases {
        let ctx = LintContext::new(case);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty(), "Should still flag inappropriate cases: {case}");
    }
}
