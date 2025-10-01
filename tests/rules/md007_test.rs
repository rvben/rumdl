use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD007ULIndent;

#[test]
fn test_valid_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n  * Item 2\n    * Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Expected no warnings for valid indentation, but got {} warnings",
        result.len()
    );
}

#[test]
fn test_invalid_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n   * Item 2\n      * Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();
    println!("test_invalid_list_indent: result.len() = {}", result.len());
    for (i, w) in result.iter().enumerate() {
        println!("  warning {}: line={}, column={}", i, w.line, w.column);
    }
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].column, 1);
    assert_eq!(result[1].line, 3);
    assert_eq!(result[1].column, 1);
}

#[test]
fn test_mixed_indentation() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n  * Item 2\n   * Item 3\n  * Item 4";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();
    println!("test_mixed_indentation: result.len() = {}", result.len());
    for (i, w) in result.iter().enumerate() {
        println!("  warning {}: line={}, column={}", i, w.line, w.column);
    }
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[0].column, 1);
}

#[test]
fn test_fix_indentation() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n   * Item 2\n      * Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.fix(&ctx).unwrap();
    let expected = "* Item 1\n  * Item 2\n    * Item 3";
    assert_eq!(result, expected);
}

#[test]
fn test_md007_in_yaml_code_block() {
    let rule = MD007ULIndent::default();
    let content = r#"```yaml
repos:
-   repo: https://github.com/rvben/rumdl
    rev: v0.5.0
    hooks:
    -   id: rumdl-check
```"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "MD007 should not trigger inside a code block, but got warnings: {result:?}"
    );
}

#[test]
fn test_blockquoted_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "> * Item 1\n>   * Item 2\n>     * Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Expected no warnings for valid blockquoted list indentation, but got {result:?}"
    );
}

#[test]
fn test_blockquoted_list_invalid_indent() {
    let rule = MD007ULIndent::default();
    let content = "> * Item 1\n>    * Item 2\n>       * Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        2,
        "Expected 2 warnings for invalid blockquoted list indentation, got {result:?}"
    );
    assert_eq!(result[0].line, 2);
    assert_eq!(result[1].line, 3);
}

#[test]
fn test_nested_blockquote_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "> > * Item 1\n> >   * Item 2\n> >     * Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Expected no warnings for valid nested blockquoted list indentation, but got {result:?}"
    );
}

#[test]
fn test_blockquote_list_with_code_block() {
    let rule = MD007ULIndent::default();
    let content = "> * Item 1\n>   * Item 2\n>   ```\n>   code\n>   ```\n>   * Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "MD007 should not trigger inside a code block within a blockquote, but got warnings: {result:?}"
    );
}

// Additional comprehensive tests for MD007
mod comprehensive_tests {
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD007ULIndent;

    // 1. Properly indented lists (should pass)
    #[test]
    fn test_properly_indented_lists() {
        let rule = MD007ULIndent::default();

        // Test various properly indented lists
        let test_cases = vec![
            "* Item 1\n* Item 2",
            "* Item 1\n  * Item 1.1\n    * Item 1.1.1",
            "- Item 1\n  - Item 1.1",
            "+ Item 1\n  + Item 1.1",
            "* Item 1\n  * Item 1.1\n* Item 2\n  * Item 2.1",
        ];

        for content in test_cases {
            let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Expected no warnings for properly indented list:\n{}\nGot {} warnings",
                content,
                result.len()
            );
        }
    }

    // 2. Under-indented lists (should fail)
    #[test]
    fn test_under_indented_lists() {
        let rule = MD007ULIndent::default();

        let test_cases = vec![
            ("* Item 1\n * Item 1.1", 1, 2),                   // Expected 2 spaces, got 1
            ("* Item 1\n  * Item 1.1\n   * Item 1.1.1", 1, 3), // Expected 4 spaces, got 3
            // Note: MD007 doesn't enforce semantic nesting based on item content
            ("- Item 1\n- Item 1.1\n  - Item 1.1.1", 0, 0), // All items properly indented
        ];

        for (content, expected_warnings, line) in test_cases {
            let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(
                result.len(),
                expected_warnings,
                "Expected {expected_warnings} warnings for under-indented list:\n{content}"
            );
            if expected_warnings > 0 {
                assert_eq!(result[0].line, line);
            }
        }
    }

    // 3. Over-indented lists (should fail)
    #[test]
    fn test_over_indented_lists() {
        let rule = MD007ULIndent::default();

        let test_cases = vec![
            ("* Item 1\n   * Item 1.1", 1, 2),                   // Expected 2 spaces, got 3
            ("* Item 1\n    * Item 1.1", 1, 2),                  // Expected 2 spaces, got 4
            ("* Item 1\n  * Item 1.1\n     * Item 1.1.1", 1, 3), // Expected 4 spaces, got 5
        ];

        for (content, expected_warnings, line) in test_cases {
            let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(
                result.len(),
                expected_warnings,
                "Expected {expected_warnings} warnings for over-indented list:\n{content}"
            );
            if expected_warnings > 0 {
                assert_eq!(result[0].line, line);
            }
        }
    }

    // 4. Nested lists with correct indentation
    #[test]
    fn test_nested_lists_correct_indentation() {
        let rule = MD007ULIndent::default();

        let content = r#"* Level 1
  * Level 2
    * Level 3
      * Level 4
    * Level 3 again
  * Level 2 again
* Level 1 again"#;

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for correctly nested list");
    }

    // 5. Nested lists with incorrect indentation
    #[test]
    fn test_nested_lists_incorrect_indentation() {
        let rule = MD007ULIndent::default();

        let content = r#"* Level 1
   * Level 2 (wrong)
     * Level 3 (wrong)
  * Level 2 (correct)
      * Level 3 (wrong)"#;

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3, "Expected 3 warnings for incorrectly nested list");

        // Check that fix works correctly
        let fixed = rule.fix(&ctx).unwrap();
        let expected = r#"* Level 1
  * Level 2 (wrong)
    * Level 3 (wrong)
  * Level 2 (correct)
    * Level 3 (wrong)"#;
        assert_eq!(fixed, expected);
    }

    // 6. Configuration for indent parameter (2, 3, 4 spaces)
    #[test]
    fn test_custom_indent_2_spaces() {
        let rule = MD007ULIndent::new(2); // Default
        let content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_custom_indent_3_spaces() {
        // Test dynamic alignment behavior (default start_indented=false)
        let rule = MD007ULIndent::new(3);

        let content = "* Item 1\n   * Item 2\n      * Item 3";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // With dynamic alignment, Item 2 should align with Item 1's text (2 spaces)
        // and Item 3 should align with Item 2's text (4 spaces), not fixed increments
        assert!(!result.is_empty()); // Should have warnings due to alignment

        // Test that dynamic alignment works correctly
        // Item 3 should align with Item 2's text content (4 spaces)
        let correct_content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(correct_content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test that 2-space indentation fails with 3-space config
        let wrong_content = "* Item 1\n  * Item 2";
        let ctx = LintContext::new(wrong_content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // With dynamic alignment, this is actually correct (2 spaces aligns with text)
        assert_eq!(result.len(), 0);

        // Test fix - no fix needed since it's correct
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item 1\n  * Item 2");
    }

    #[test]
    fn test_custom_indent_4_spaces() {
        // Test dynamic alignment behavior (default start_indented=false)
        let rule = MD007ULIndent::new(4);
        let content = "* Item 1\n    * Item 2\n        * Item 3";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // With dynamic alignment, should expect 2 spaces and 6 spaces, not 4 and 8
        assert!(!result.is_empty()); // Should have warnings due to alignment

        // Test correct dynamic alignment
        // Item 3 should align with Item 2's text content (4 spaces)
        let correct_content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(correct_content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test fix with wrong indentation - dynamic alignment means no fix needed
        let wrong_content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(wrong_content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Dynamic alignment makes this correct
        assert!(result.is_empty());
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item 1\n  * Item 2\n    * Item 3");
    }

    // 7. Tab indentation
    #[test]
    fn test_tab_indentation() {
        let rule = MD007ULIndent::default();

        // Single tab
        let content = "* Item 1\n\t* Item 2";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Tab indentation should trigger warning");

        // Fix should convert tab to spaces
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item 1\n  * Item 2");

        // Multiple tabs
        let content_multi = "* Item 1\n\t* Item 2\n\t\t* Item 3";
        let ctx = LintContext::new(content_multi, rumdl_lib::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        // With text-aligned style: Item 3's marker aligns with Item 2's content position
        // Item 2: marker at 2, content at 4 → Item 3: marker at 4 (4 spaces)
        assert_eq!(fixed, "* Item 1\n  * Item 2\n    * Item 3");

        // Mixed tabs and spaces
        // TODO: Tab handling may not be consistent
        let content_mixed = "* Item 1\n \t* Item 2\n\t * Item 3";
        let ctx = LintContext::new(content_mixed, rumdl_lib::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        // With cascade behavior: Item 3 aligns with Item 2's actual content position
        assert_eq!(fixed, "* Item 1\n  * Item 2\n    * Item 3");
    }

    // 8. Mixed ordered and unordered lists
    #[test]
    fn test_mixed_ordered_unordered_lists() {
        let rule = MD007ULIndent::default();

        // MD007 only checks unordered lists, so ordered lists should be ignored
        // Note: 3 spaces is now correct for bullets under numbered items
        let content = r#"1. Ordered item
  * Unordered sub-item (wrong indent - only 2 spaces)
   2. Ordered sub-item
* Unordered item
  1. Ordered sub-item
  * Unordered sub-item"#;

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Only unordered list indentation should be checked");
        assert_eq!(result[0].line, 2, "Error should be on line 2");

        // Fix should only correct unordered lists
        let fixed = rule.fix(&ctx).unwrap();
        let expected = r#"1. Ordered item
   * Unordered sub-item (wrong indent - only 2 spaces)
   2. Ordered sub-item
* Unordered item
  1. Ordered sub-item
  * Unordered sub-item"#;
        assert_eq!(fixed, expected);
    }

    // 9. Lists in blockquotes
    #[test]
    fn test_lists_in_blockquotes_comprehensive() {
        let rule = MD007ULIndent::default();

        // Single level blockquote with proper indentation
        let content1 = "> * Item 1\n>   * Item 2\n>     * Item 3";
        let ctx = LintContext::new(content1, rumdl_lib::config::MarkdownFlavor::Standard);
        assert!(rule.check(&ctx).unwrap().is_empty());

        // Single level blockquote with improper indentation
        let content2 = "> * Item 1\n>    * Item 2\n>      * Item 3";
        let ctx = LintContext::new(content2, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Detects two issues: Item 2 and Item 3

        let fixed = rule.fix(&ctx).unwrap();
        // With text-aligned style and non-cascade, proper blockquote list indentation is:
        // Level 1: > * (no extra spaces)
        // Level 2: >   * (2 spaces)
        // Level 3: >     * (4 spaces)
        assert_eq!(fixed, "> * Item 1\n>   * Item 2\n>     * Item 3");

        // Nested blockquotes
        let content3 = "> > * Item 1\n> >   * Item 2\n> >     * Item 3";
        let ctx = LintContext::new(content3, rumdl_lib::config::MarkdownFlavor::Standard);
        assert!(rule.check(&ctx).unwrap().is_empty());

        // Mixed blockquote and regular lists
        let content4 = "* Regular item\n> * Blockquote item\n>   * Nested in blockquote\n* Another regular";
        let ctx = LintContext::new(content4, rumdl_lib::config::MarkdownFlavor::Standard);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    // 10. Start_indented configuration option
    // Note: Based on the code analysis, start_indented is not currently implemented
    // This test documents the expected behavior if it were implemented
    #[test]
    #[ignore = "start_indented configuration not implemented"]
    fn test_start_indented_configuration() {
        // This would test the behavior where top-level lists can start with indentation
        // Currently not supported by the implementation
    }

    // Additional edge cases
    #[test]
    fn test_empty_list_items() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n* \n  * Item 2";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Empty list items should not affect indentation checks"
        );
    }

    #[test]
    fn test_list_with_code_blocks() {
        let rule = MD007ULIndent::default();
        let content = r#"* Item 1
  ```
  code
  ```
  * Item 2
    * Item 3"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_in_front_matter() {
        let rule = MD007ULIndent::default();
        let content = r#"---
tags:
  - tag1
  - tag2
---
* Item 1
  * Item 2"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Lists in YAML front matter should be ignored");
    }

    #[test]
    fn test_fix_preserves_content() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1 with **bold** and *italic*\n   * Item 2 with `code`\n     * Item 3 with [link](url)";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        // With non-cascade: Item 3 aligns with Item 2's expected text position (4 spaces)
        let expected = "* Item 1 with **bold** and *italic*\n  * Item 2 with `code`\n    * Item 3 with [link](url)";
        assert_eq!(fixed, expected, "Fix should only change indentation, not content");
    }

    #[test]
    fn test_deeply_nested_lists() {
        let rule = MD007ULIndent::default();
        let content = r#"* L1
  * L2
    * L3
      * L4
        * L5
          * L6"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with wrong deep nesting
        let wrong_content = r#"* L1
  * L2
    * L3
      * L4
         * L5
            * L6"#;
        let ctx = LintContext::new(wrong_content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Deep nesting errors should be detected");
    }

    #[test]
    fn test_list_markers_variety() {
        let rule = MD007ULIndent::default();

        // Test all three unordered list markers
        let content = r#"* Asterisk
  * Nested asterisk
- Hyphen
  - Nested hyphen
+ Plus
  + Nested plus"#;

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "All unordered list markers should work with proper indentation"
        );

        // Test with wrong indentation for each marker type
        let wrong_content = r#"* Asterisk
   * Wrong asterisk
- Hyphen
 - Wrong hyphen
+ Plus
    + Wrong plus"#;

        let ctx = LintContext::new(wrong_content, rumdl_lib::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3, "All marker types should be checked for indentation");
    }
}

mod parity_with_markdownlint {
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD007ULIndent;

    #[test]
    fn parity_flat_list_default_indent() {
        let input = "* Item 1\n* Item 2\n* Item 3";
        let expected = "* Item 1\n* Item 2\n* Item 3";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_nested_list_default_indent() {
        let input = "* Item 1\n  * Nested 1\n    * Nested 2";
        let expected = "* Item 1\n  * Nested 1\n    * Nested 2";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_nested_list_incorrect_indent() {
        let input = "* Item 1\n * Nested 1\n   * Nested 2";
        // With 1 space, Nested 1 is insufficient for nesting, so it becomes a sibling at 0
        // With 3 spaces, Nested 2 is a child of Nested 1, so it should be at 2 spaces
        let expected = "* Item 1\n* Nested 1\n  * Nested 2";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2); // Two errors: Nested 1 and Nested 2
    }

    #[test]
    fn parity_mixed_markers() {
        let input = "* Item 1\n  - Nested 1\n    + Nested 2";
        let expected = "* Item 1\n  - Nested 1\n    + Nested 2";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_blockquote_list() {
        let input = "> * Item 1\n>   * Nested";
        let expected = "> * Item 1\n>   * Nested";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_tabs_for_indent() {
        let input = "* Item 1\n\t* Nested";
        let expected = "* Item 1\n  * Nested";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_code_block_ignored() {
        let input = "```\n* Not a list\n  * Not a nested list\n```\n* Item 1";
        let expected = "```\n* Not a list\n  * Not a nested list\n```\n* Item 1";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_custom_indent_4() {
        let input = "* Item 1\n  * Nested 1\n    * Nested 2";
        let expected = "* Item 1\n  * Nested 1\n    * Nested 2";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::new(4);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_empty_input() {
        let input = "";
        let expected = "";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_no_lists() {
        let input = "# Heading\nSome text";
        let expected = "# Heading\nSome text";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_list_with_blank_lines_between_items() {
        let input = "* Item 1\n\n* Item 2\n\n  * Nested item 1\n\n  * Nested item 2\n* Item 3";
        let expected = "* Item 1\n\n* Item 2\n\n  * Nested item 1\n\n  * Nested item 2\n* Item 3";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, expected,
            "Nested items should maintain proper indentation even after blank lines"
        );
    }

    #[test]
    fn parity_list_items_with_trailing_whitespace() {
        let input = "* Item 1   \n  * Nested item 1   \n* Item 2   ";
        let expected = "* Item 1   \n  * Nested item 1   \n* Item 2   ";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_deeply_nested_blockquotes_with_lists() {
        let input = "> > * Item 1\n> >   * Nested item 1\n> >     * Nested item 2\n> > * Item 2";
        let expected = "> > * Item 1\n> >   * Nested item 1\n> >     * Nested item 2\n> > * Item 2";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_inconsistent_marker_styles_different_nesting() {
        let input = "* Item 1\n  - Nested item 1\n    + Nested item 2\n* Item 2";
        let expected = "* Item 1\n  - Nested item 1\n    + Nested item 2\n* Item 2";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_mixed_tabs_and_spaces_in_indentation() {
        let input = "* Item 1\n\t* Nested item 1\n  \t* Nested item 2\n* Item 2";
        // Both nested items are at level 1, so both should have 2 spaces of indentation
        // Note: markdownlint produces hybrid space+tab indentation, but we convert to pure spaces
        // which is cleaner and more consistent
        let expected = "* Item 1\n  * Nested item 1\n  * Nested item 2\n* Item 2";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }
}

mod excessive_indentation_bug_fix {
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD007ULIndent;

    /// Test MD007 for excessive indentation detection (bug fix for issue #77)
    /// This was a bug where list items with 5+ spaces were incorrectly detected as code blocks
    #[test]
    fn test_md007_excessive_indentation_detection() {
        // Test case from issue #77 - excessive indentation should be detected
        let test =
            "- Formatter:\n     - The stable style changed\n- Language server:\n  - An existing capability is removed";

        let rule = MD007ULIndent::default();
        let ctx = LintContext::new(test, rumdl_lib::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();

        // Should have exactly one MD007 warning for line 2 (5 spaces instead of 2)
        assert_eq!(warnings.len(), 1, "Should detect excessive indentation on line 2");
        assert_eq!(warnings[0].line, 2);
        assert!(warnings[0].message.contains("Expected 2 spaces"));
        assert!(warnings[0].message.contains("found 5"));
    }

    #[test]
    fn test_md007_list_items_not_code_blocks() {
        // Test that list items with 4+ spaces are not incorrectly detected as code blocks
        // This was the root cause of the bug - DocumentStructure was treating indented list items as code blocks
        let test = "# Test\n\n- Item 1\n    - Item 2 with 4 spaces\n     - Item 3 with 5 spaces\n      - Item 4 with 6 spaces\n        - Item 5 with 8 spaces";

        let rule = MD007ULIndent::default();
        let ctx = LintContext::new(test, rumdl_lib::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();

        // Line 4: 4 spaces instead of 2 → warning
        // Line 5: 5 spaces instead of 4 (child of line 4) → warning
        // Lines 6-7: Correct indentation for their respective nesting levels
        // Note: markdownlint may handle excessive nesting differently
        assert!(warnings.len() >= 2, "Should detect indentation issues on lines 4 and 5");

        // These should NOT be treated as code blocks - ensure they're detected as list items
        // (The bug was that they were being treated as code blocks)
        for warning in &warnings {
            assert!(
                warning.message.contains("spaces"),
                "Should be list indentation warnings"
            );
        }
    }

    #[test]
    fn test_md007_deeply_nested_lists_vs_code_blocks() {
        // Test that deeply indented list items are correctly distinguished from actual code blocks
        let test = "# Document\n\n- Top level list\n        - 8 spaces (should be 2)\n            - 12 spaces (should be 4)\n\nRegular paragraph.\n\n    This is an actual code block (4 spaces, not a list)\n    It continues here";

        let rule = MD007ULIndent::default();
        let ctx = LintContext::new(test, rumdl_lib::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();

        // Should detect excessive indentation in list items (lines 4 and 5)
        assert!(warnings.len() >= 2, "Should detect excessive list indentation");

        // The actual code block (lines 9-10) should NOT trigger MD007
        assert!(
            !warnings.iter().any(|w| w.line >= 9),
            "Actual code blocks should not trigger MD007"
        );
    }

    #[test]
    fn test_md007_with_4_space_config() {
        // Test with MD007 configured for 4-space indents
        // Note: MD007ULIndent::new(4) uses TextAligned style with dynamic alignment
        let test = "- Item 1\n    - Item 2 with 4 spaces\n     - Item 3 with 5 spaces\n      - Item 4 with 6 spaces\n        - Item 5 with 8 spaces";

        let rule = MD007ULIndent::new(4);
        let ctx = LintContext::new(test, rumdl_lib::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();

        // With TextAligned style (indent param not used for text-aligned):
        // Line 2: 4 spaces - wrong, should align with Item 1's text (2 spaces)
        // Line 3: 5 spaces - wrong, should align with Item 2's expected text (4 spaces)
        // Lines 4-5: Correct for their respective nesting levels
        // Note: markdownlint may handle excessive nesting differently

        assert!(warnings.len() >= 2, "Should detect indentation issues on lines 2 and 3");

        // At least lines 2 and 3 should have warnings
        assert!(warnings.iter().any(|w| w.line == 2), "Line 2 should have warning");
        assert!(warnings.iter().any(|w| w.line == 3), "Line 3 should have warning");
    }

    #[test]
    fn test_md007_excessive_indentation_fix() {
        // Test that the fix properly corrects excessive indentation
        let test = "- Item 1\n     - Item 2 with 5 spaces\n       - Item 3 with 7 spaces";

        let rule = MD007ULIndent::default();
        let ctx = LintContext::new(test, rumdl_lib::config::MarkdownFlavor::Standard);

        // Check warnings are detected
        let warnings = rule.check(&ctx).unwrap();
        // Line 2: 5 spaces instead of 2 (depth 1)
        // Line 3: 7 spaces instead of 4 (depth 2) - with non-cascade
        assert_eq!(
            warnings.len(),
            2,
            "Should detect excessive indentation on lines 2 and 3"
        );
        assert_eq!(warnings[0].line, 2);
        assert_eq!(warnings[1].line, 3);

        // Check fix works correctly
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "- Item 1\n  - Item 2 with 5 spaces\n    - Item 3 with 7 spaces";
        assert_eq!(fixed, expected, "Should fix excessive indentation to correct levels");
    }

    #[test]
    fn test_md007_not_triggered_by_actual_code_blocks() {
        // Ensure that actual indented code blocks (not list items) don't trigger MD007
        let test = "Regular paragraph.\n\n    This is a code block\n    with multiple lines\n    all indented with 4 spaces\n\n- List after code block\n  - Properly indented";

        let rule = MD007ULIndent::default();
        let ctx = LintContext::new(test, rumdl_lib::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();

        // Should have no MD007 warnings - code blocks are not list items
        assert!(warnings.is_empty(), "Code blocks should not trigger MD007");
    }
}
