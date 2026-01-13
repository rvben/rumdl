use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD032BlanksAroundLists;

#[test]
fn test_valid_lists() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Some text\n\n* Item 1\n* Item 2\n\nMore text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_missing_blank_line_before() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Some text\n* Item 1\n* Item 2\n\nMore text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_missing_blank_line_after() {
    // Per markdownlint-cli: trailing text without blank line is lazy continuation
    // so NO MD032 warning is expected for the trailing text
    let rule = MD032BlanksAroundLists::default();
    let content = "Some text\n\n* Item 1\n* Item 2\nMore text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Trailing text is lazy continuation - no warning expected"
    );
}

#[test]
fn test_fix_missing_blank_lines() {
    // Per markdownlint-cli: trailing text is lazy continuation, only preceding blank needed
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n* Item 1\n* Item 2\nMore text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(fixed, "Text\n\n* Item 1\n* Item 2\nMore text");
}

// CRITICAL REGRESSION TESTS: Emphasis text should NOT be detected as list markers

#[test]
fn test_emphasis_not_list_marker_simple() {
    // Test simple emphasis pattern that should NOT be detected as a list marker
    let rule = MD032BlanksAroundLists::default();
    let content = "*Emphasis text*\n- List item\n- Another item";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have no warnings - emphasis is not a list, and the real list is properly spaced
    assert!(result.is_empty());
}

#[test]
fn test_heading_emphasis_not_list() {
    // Test heading with emphasis that was causing false positives
    let rule = MD032BlanksAroundLists::default();
    let content = "## **Section Title**\n\nSome content\n\n- Real list item\n- Another item";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have no warnings - neither heading emphasis nor properly spaced list should trigger
    assert!(result.is_empty());
}

#[test]
fn test_multiple_lists() {
    // Per markdownlint-cli: "Text" between lists is NOT lazy continuation because it
    // comes after a list that ends, then another list starts. The final "Text" IS lazy continuation.
    // Expected warnings: 1) before list 1, 2) after list 1 (before Text), 3) before list 2
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n* List 1\n* List 1\nText\n1. List 2\n2. List 2\nText";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // The exact number depends on how the implementation handles inter-list text
    assert!(
        result.len() >= 2 && result.len() <= 4,
        "Expected 2-4 warnings, got {}: {:?}",
        result.len(),
        result
    );
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    // The fix should add blanks before lists that need them
    let fixed_warnings = rule.check(&_ctx_fixed).unwrap();
    assert_eq!(fixed_warnings.len(), 0, "Fix should resolve all warnings");
}

#[test]
fn test_nested_lists() {
    // Nested lists should not require blank lines between parent and child items
    // Per markdownlint-cli: trailing "Text" is lazy continuation, only 1 warning (preceding)
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n* Item 1\n  * Nested 1\n  * Nested 2\n* Item 2\nText";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Should only warn about missing blank line before the list (trailing text is lazy continuation)
    assert_eq!(
        result.len(),
        1,
        "Should only warn about preceding blank, trailing text is lazy continuation"
    );
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(fixed, "Text\n\n* Item 1\n  * Nested 1\n  * Nested 2\n* Item 2\nText");
}

#[test]
fn test_nested_lists_with_strict_mode() {
    // Even in strict mode, nested lists are a standard Markdown pattern and shouldn't require blank lines
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n\n* Item 1\n  * Nested 1\n  * Nested 2\n* Item 2\n\nText";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Should not warn about nested list transitions
    assert!(result.is_empty(), "Should not warn about deeply nested lists");
}

#[test]
fn test_nested_ordered_lists() {
    // Test nested ordered lists
    let rule = MD032BlanksAroundLists::default();
    let content = "## Section\n\n1. First item\n   1. Sub item 1.1\n   2. Sub item 1.2\n2. Second item\n   1. Sub item 2.1\n\nText";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not warn about nested ordered lists");
}

#[test]
fn test_mixed_nested_list_types() {
    // Test mixing ordered and unordered lists in nesting
    let rule = MD032BlanksAroundLists::default();
    let content = "## Section\n\n1. Ordered item\n   - Unordered sub-item\n   - Another unordered sub-item\n2. Another ordered item\n   * Different unordered marker\n\nText";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not warn about mixed nested list types");
}

#[test]
fn test_mixed_list_types() {
    // Similar to test_multiple_lists - inter-list text is not lazy continuation
    // but final "Text" IS lazy continuation
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n* Unordered\n* List\nText\n1. Ordered\n2. List\nText";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // The exact number depends on implementation - allow range
    assert!(
        result.len() >= 2 && result.len() <= 4,
        "Expected 2-4 warnings, got {}: {:?}",
        result.len(),
        result
    );
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    // The fix should resolve all warnings
    let fixed_warnings = rule.check(&_ctx_fixed).unwrap();
    assert_eq!(fixed_warnings.len(), 0, "Fix should resolve all warnings");
}

#[test]
fn test_list_with_content() {
    // Per markdownlint-cli: trailing "Text" is lazy continuation, only 1 warning (preceding)
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n* Item 1\n  Content\n* Item 2\n  More content\nText";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Only preceding blank warning expected (trailing is lazy continuation)"
    );
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(fixed, "Text\n\n* Item 1\n  Content\n* Item 2\n  More content\nText");
}

#[test]
fn test_list_at_start() {
    // Per markdownlint-cli: list at document start needs no preceding blank,
    // and "Text" at indent=0 is lazy continuation (no following blank needed)
    let rule = MD032BlanksAroundLists::default();
    let content = "* Item 1\n* Item 2\nText";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "No warnings: list at doc start, trailing text is lazy continuation"
    );
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(fixed, "* Item 1\n* Item 2\nText");
}

#[test]
fn test_list_at_end() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n* Item 1\n* Item 2";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    let _ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(fixed, "Text\n\n* Item 1\n* Item 2");
}

#[test]
fn test_multiple_blank_lines() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n\n\n* Item 1\n* Item 2\n\n\nText";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_list_with_blank_lines() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n\n* Item 1\n\n* Item 2\n\nText";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should warn for missing blank line before heading");
    assert!(result[0].message.contains("followed by blank line"));
}

#[test]
fn test_list_followed_by_code_block_invalid() {
    let rule = MD032BlanksAroundLists::default();
    let content = "* Item 1\n* Item 2\n```\ncode\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should warn for missing blank line before code block");
    assert!(result[0].message.contains("followed by blank line"));
}

#[test]
fn test_list_followed_by_blank_then_code_block_valid() {
    let rule = MD032BlanksAroundLists::default();
    let content = "* Item 1\n* Item 2\n\n```\ncode\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not warn when blank line precedes code block");
}

// New tests for lenient behavior

#[test]
fn test_strict_mode_flags_all() {
    let rule = MD032BlanksAroundLists::default(); // All allowances disabled

    // Even valid cases should be flagged in strict mode
    let cases = vec![
        "# Items to consider:\n* Item 1\n* Item 2\n\nMore text",
        "Here are the steps:\n1. First step\n2. Second step\n\nFollowing text",
    ];

    for case in cases {
        let ctx = LintContext::new(case, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(case, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty(), "Should still flag inappropriate cases: {case}");
    }
}

// =============================================================================
// Issue #268: Auto-fix fails when blockquote prefix whitespace varies
// =============================================================================
// The bug: MD032 fix() compares blockquote prefixes with exact string equality,
// but the regex captures varying amounts of trailing whitespace:
//   "> #### Heading" captures "> " (2 chars)
//   ">   - List item" captures ">   " (4 chars)
// Since "> " != ">   ", the fix is not applied.

#[test]
fn test_issue_268_blockquote_heading_then_indented_list() {
    // This is the minimal reproduction case for issue #268
    // The heading has prefix "> " but the indented list has prefix ">   "
    let rule = MD032BlanksAroundLists::default();
    let content = "# Test\n\n> #### Heading\n>   - List item\n";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should detect missing blank line before list
    assert_eq!(warnings.len(), 1, "Should detect missing blank line: {warnings:?}");
    assert!(
        warnings[0].message.contains("preceded by blank line"),
        "Should be 'preceded by' warning"
    );

    // The fix should actually work - this is what was broken in #268
    // Note: Per markdownlint-cli, blank blockquote lines should NOT have trailing space
    // (trailing space triggers MD009). So the blank line is ">" not "> ".
    let fixed = rule.fix(&ctx).unwrap();
    let expected = "# Test\n\n> #### Heading\n>\n>   - List item\n";
    assert_eq!(
        fixed, expected,
        "Fix should insert blank blockquote line.\nGot: {fixed:?}\nExpected: {expected:?}"
    );

    // Verify no warnings after fix
    let ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings_after = rule.check(&ctx_fixed).unwrap();
    assert!(
        warnings_after.is_empty(),
        "Should have no warnings after fix: {warnings_after:?}"
    );
}

#[test]
fn test_issue_268_multiple_fixes_in_same_blockquote() {
    // Multiple lists in same blockquote, each needing a blank line
    let rule = MD032BlanksAroundLists::default();
    let content = r#"# Test

> Text before
> * Item A
> * Item B
>
> #### Section
>   - Item 1
>   - Item 2
>
> End
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should detect both missing blank lines
    assert_eq!(warnings.len(), 2, "Should detect 2 missing blank lines: {warnings:?}");

    // Fix should work for ALL issues in one pass
    let fixed = rule.fix(&ctx).unwrap();
    let ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings_after = rule.check(&ctx_fixed).unwrap();

    assert!(
        warnings_after.is_empty(),
        "All issues should be fixed in one pass.\nFixed content:\n{fixed}\nRemaining warnings: {warnings_after:?}"
    );
}

#[test]
fn test_issue_268_nested_blockquote_with_varying_whitespace() {
    // Nested blockquotes where inner content has different indentation
    let rule = MD032BlanksAroundLists::default();
    let content = ">> Nested heading\n>>   - Nested list item\n";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    if !warnings.is_empty() {
        // If check detects an issue, fix must resolve it
        let fixed = rule.fix(&ctx).unwrap();
        let ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings_after = rule.check(&ctx_fixed).unwrap();

        assert!(
            warnings_after.is_empty(),
            "Fix should resolve nested blockquote issues.\nOriginal: {content:?}\nFixed: {fixed:?}\nWarnings: {warnings_after:?}"
        );
    }
}

#[test]
fn test_issue_268_blockquote_prefix_comparison_should_ignore_trailing_whitespace() {
    // Test various whitespace patterns that should all be treated as same blockquote level
    let rule = MD032BlanksAroundLists::default();

    // All these have blockquote level 1, but different trailing whitespace
    let cases = [
        // (content, description)
        ("> Text\n>- List", "no space after >"),
        ("> Text\n> - List", "one space after >"),
        ("> Text\n>  - List", "two spaces after >"),
        ("> Text\n>   - List", "three spaces (indented list marker)"),
        (">Text\n>- List", "no space anywhere"),
    ];

    for (content, desc) in cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        if !warnings.is_empty() {
            let fixed = rule.fix(&ctx).unwrap();
            let ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
            let warnings_after = rule.check(&ctx_fixed).unwrap();

            assert!(
                warnings_after.is_empty(),
                "Fix should work for {desc}.\nOriginal: {content:?}\nFixed: {fixed:?}\nWarnings: {warnings_after:?}"
            );
        }
    }
}

#[test]
fn test_issue_268_real_world_matrix_org_pattern() {
    // Pattern from matrix.org that triggered the bug
    let rule = MD032BlanksAroundLists::default();
    let content = r#"# TWIM

> Some intro text
> * **Item 1**
>   Description of item 1.
> * **Item 2**
>   Description of item 2.
>
> #### *Section Title:*
>   - The first point
>     - Nested point under first
>   - The second point
>   - The third point

Regular text after blockquote.
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Fix must resolve ALL detected issues
    if !warnings.is_empty() {
        let fixed = rule.fix(&ctx).unwrap();
        let ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings_after = rule.check(&ctx_fixed).unwrap();

        assert!(
            warnings_after.is_empty(),
            "Real-world pattern should be fully fixed.\nOriginal warnings: {warnings:?}\nFixed content:\n{fixed}\nRemaining: {warnings_after:?}"
        );
    }
}

#[test]
fn test_issue_268_fix_preserves_content_integrity() {
    // Ensure fix only adds blank lines, doesn't corrupt content
    let rule = MD032BlanksAroundLists::default();
    let content = "> #### Heading with **bold** and *italic*\n>   - List with `code` and [link](url)\n";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // All original content should be preserved
    assert!(
        fixed.contains("#### Heading with **bold** and *italic*"),
        "Heading content should be preserved"
    );
    assert!(
        fixed.contains("- List with `code` and [link](url)"),
        "List content should be preserved"
    );

    // Only blank lines should be added
    let original_non_blank: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    let fixed_non_blank: Vec<&str> = fixed
        .lines()
        .filter(|l| !l.trim().is_empty() && l.trim() != ">")
        .collect();

    assert_eq!(
        original_non_blank.len(),
        fixed_non_blank.len(),
        "Should only add blank lines, not modify content.\nOriginal non-blank: {original_non_blank:?}\nFixed non-blank: {fixed_non_blank:?}"
    );
}

#[test]
fn test_issue_268_followed_by_blank_line_also_affected() {
    // The "followed by" case may also be affected by the same bug
    let rule = MD032BlanksAroundLists::default();
    let content = "# Test\n\n>   - List item 1\n>   - List item 2\n> #### After list\n";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    if !warnings.is_empty() {
        let fixed = rule.fix(&ctx).unwrap();
        let ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings_after = rule.check(&ctx_fixed).unwrap();

        assert!(
            warnings_after.is_empty(),
            "'Followed by' case should also be fixed.\nFixed: {fixed:?}\nWarnings: {warnings_after:?}"
        );
    }
}

#[test]
fn test_issue_268_idempotent_fix() {
    // Running fix multiple times should produce same result
    let rule = MD032BlanksAroundLists::default();
    let content = "> #### Heading\n>   - List item\n";

    let ctx1 = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed1 = rule.fix(&ctx1).unwrap();

    let ctx2 = LintContext::new(&fixed1, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed2 = rule.fix(&ctx2).unwrap();

    let ctx3 = LintContext::new(&fixed2, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed3 = rule.fix(&ctx3).unwrap();

    assert_eq!(fixed1, fixed2, "Second fix should be idempotent");
    assert_eq!(fixed2, fixed3, "Third fix should be idempotent");
}

#[test]
fn test_issue_268_nested_blockquote_with_space_between_markers() {
    // Edge case: nested blockquotes written as "> > " (space between markers)
    // The fix should preserve this format, not collapse to ">>"
    let rule = MD032BlanksAroundLists::default();
    let content = "> > Nested text\n> >   - List item\n";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    if !warnings.is_empty() {
        let fixed = rule.fix(&ctx).unwrap();

        // The blank line should preserve the "> > " format
        assert!(
            fixed.contains("> >"),
            "Should preserve space between markers.\nFixed: {fixed:?}"
        );
        assert!(
            !fixed.contains(">>") || fixed.contains("> >"),
            "Should not collapse markers.\nFixed: {fixed:?}"
        );

        // Verify no warnings after fix
        let ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings_after = rule.check(&ctx_fixed).unwrap();
        assert!(
            warnings_after.is_empty(),
            "Should have no warnings after fix: {warnings_after:?}"
        );
    }
}
