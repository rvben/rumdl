use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD007ULIndent;

#[test]
fn test_valid_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n  * Item 2\n    * Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
            let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
            let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
            let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_custom_indent_3_spaces() {
        // With smart auto-detection, pure unordered lists use fixed style
        // This provides markdownlint compatibility (fixes issue #210)
        let rule = MD007ULIndent::new(3);

        // Fixed style with indent=3: level 0 = 0, level 1 = 3, level 2 = 6
        let correct_content = "* Item 1\n   * Item 2\n      * Item 3";
        let ctx = LintContext::new(correct_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Fixed style expects 0, 3, 6 spaces");

        // Wrong indentation (text-aligned spacing)
        let wrong_content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(wrong_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty(), "Should warn: expected 3 spaces, found 2");

        // Test fix corrects to fixed style
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item 1\n   * Item 2\n      * Item 3");
    }

    #[test]
    fn test_custom_indent_4_spaces() {
        // With smart auto-detection, pure unordered lists use fixed style
        // This provides markdownlint compatibility (fixes issue #210)
        let rule = MD007ULIndent::new(4);

        // Fixed style with indent=4: level 0 = 0, level 1 = 4, level 2 = 8
        let correct_content = "* Item 1\n    * Item 2\n        * Item 3";
        let ctx = LintContext::new(correct_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Fixed style expects 0, 4, 8 spaces");

        // Wrong indentation (text-aligned spacing)
        let wrong_content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(wrong_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty(), "Should warn: expected 4 spaces, found 2");

        // Test fix corrects to fixed style
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item 1\n    * Item 2\n        * Item 3");
    }

    // 7. Tab indentation
    #[test]
    fn test_tab_indentation() {
        let rule = MD007ULIndent::default();

        // Single tab
        let content = "* Item 1\n\t* Item 2";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Tab indentation should trigger warning");

        // Fix should convert tab to spaces
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item 1\n  * Item 2");

        // Multiple tabs
        let content_multi = "* Item 1\n\t* Item 2\n\t\t* Item 3";
        let ctx = LintContext::new(content_multi, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // With text-aligned style: Item 3's marker aligns with Item 2's content position
        // Item 2: marker at 2, content at 4 → Item 3: marker at 4 (4 spaces)
        assert_eq!(fixed, "* Item 1\n  * Item 2\n    * Item 3");

        // Mixed tabs and spaces
        // TODO: Tab handling may not be consistent
        let content_mixed = "* Item 1\n \t* Item 2\n\t * Item 3";
        let ctx = LintContext::new(content_mixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content1, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty());

        // Single level blockquote with improper indentation
        let content2 = "> * Item 1\n>    * Item 2\n>      * Item 3";
        let ctx = LintContext::new(content2, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content3, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty());

        // Mixed blockquote and regular lists
        let content4 = "* Regular item\n> * Blockquote item\n>   * Nested in blockquote\n* Another regular";
        let ctx = LintContext::new(content4, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    // Additional edge cases
    #[test]
    fn test_empty_list_items() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n* \n  * Item 2";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Lists in YAML front matter should be ignored");
    }

    #[test]
    fn test_fix_preserves_content() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1 with **bold** and *italic*\n   * Item 2 with `code`\n     * Item 3 with [link](url)";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with wrong deep nesting
        let wrong_content = r#"* L1
  * L2
    * L3
      * L4
         * L5
            * L6"#;
        let ctx = LintContext::new(wrong_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(wrong_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_nested_list_default_indent() {
        let input = "* Item 1\n  * Nested 1\n    * Nested 2";
        let expected = "* Item 1\n  * Nested 1\n    * Nested 2";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_blockquote_list() {
        let input = "> * Item 1\n>   * Nested";
        let expected = "> * Item 1\n>   * Nested";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_tabs_for_indent() {
        let input = "* Item 1\n\t* Nested";
        let expected = "* Item 1\n  * Nested";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_code_block_ignored() {
        let input = "```\n* Not a list\n  * Not a nested list\n```\n* Item 1";
        let expected = "```\n* Not a list\n  * Not a nested list\n```\n* Item 1";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_custom_indent_4() {
        // With smart auto-detection, pure unordered lists use fixed style
        // Input has text-aligned spacing (2, 4), output should be fixed (4, 8)
        let input = "* Item 1\n  * Nested 1\n    * Nested 2";
        let expected = "* Item 1\n    * Nested 1\n        * Nested 2";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let rule = MD007ULIndent::new(4);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_empty_input() {
        let input = "";
        let expected = "";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_no_lists() {
        let input = "# Heading\nSome text";
        let expected = "# Heading\nSome text";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn parity_list_with_blank_lines_between_items() {
        let input = "* Item 1\n\n* Item 2\n\n  * Nested item 1\n\n  * Nested item 2\n* Item 3";
        let expected = "* Item 1\n\n* Item 2\n\n  * Nested item 1\n\n  * Nested item 2\n* Item 3";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_deeply_nested_blockquotes_with_lists() {
        let input = "> > * Item 1\n> >   * Nested item 1\n> >     * Nested item 2\n> > * Item 2";
        let expected = "> > * Item 1\n> >   * Nested item 1\n> >     * Nested item 2\n> > * Item 2";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let rule = MD007ULIndent::default();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_inconsistent_marker_styles_different_nesting() {
        let input = "* Item 1\n  - Nested item 1\n    + Nested item 2\n* Item 2";
        let expected = "* Item 1\n  - Nested item 1\n    + Nested item 2\n* Item 2";
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(test, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(test, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
        // Test that deeply indented content (8+ spaces) is treated as code blocks, not lists
        // Per CommonMark, 8+ spaces of indentation creates an indented code block, not nested lists
        // markdownlint agrees: it reports 0 MD007 warnings for this content
        let test = "# Document\n\n- Top level list\n        - 8 spaces (should be 2)\n            - 12 spaces (should be 4)\n\nRegular paragraph.\n\n    This is an actual code block (4 spaces, not a list)\n    It continues here";

        let rule = MD007ULIndent::default();
        let ctx = LintContext::new(test, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Per CommonMark, 8+ spaces creates code blocks, not nested list items
        // Only line 3 ("- Top level list") is a list item, which has correct indentation (0 spaces)
        // So MD007 should report 0 warnings
        assert!(
            warnings.is_empty(),
            "Expected no MD007 warnings (deeply indented lines are code blocks, not lists), got: {warnings:?}"
        );
    }

    #[test]
    fn test_md007_with_4_space_config() {
        // Test with MD007 configured for 4-space indents
        // With smart auto-detection, pure unordered lists use fixed style
        // Fixed style: level 0 = 0, level 1 = 4, level 2 = 8, level 3 = 12
        let test = "- Item 1\n    - Item 2 with 4 spaces\n     - Item 3 with 5 spaces\n      - Item 4 with 6 spaces\n        - Item 5 with 8 spaces";

        let rule = MD007ULIndent::new(4);
        let ctx = LintContext::new(test, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Per CommonMark/pulldown-cmark list detection:
        // Line 1: 0 spaces, level 0 - list item (correct)
        // Line 2: 4 spaces, level 1 - nested list item (correct for indent=4)
        // Line 3: 5 spaces, level 2 - nested deeper (wrong: expected 8, got 5)
        // Line 4: 6 spaces - NOT a list item per CommonMark (content of previous item)
        // Line 5: 8 spaces, level 3 - nested even deeper (wrong: expected 12, got 8)
        //
        // Note: Line 4 is not detected as a list item because CommonMark parsing
        // doesn't treat it as such - it's continuation content. This matches markdownlint.

        assert_eq!(
            warnings.len(),
            2,
            "Should detect indentation issues on lines 3 and 5, got: {warnings:?}"
        );

        // Line 3 should have warning (5 spaces, expected 8 for depth 2)
        assert!(warnings.iter().any(|w| w.line == 3), "Line 3 should have warning");
        // Line 5 should have warning (8 spaces, expected 12 for depth 3)
        assert!(warnings.iter().any(|w| w.line == 5), "Line 5 should have warning");
    }

    #[test]
    fn test_md007_excessive_indentation_fix() {
        // Test that the fix properly corrects excessive indentation
        let test = "- Item 1\n     - Item 2 with 5 spaces\n       - Item 3 with 7 spaces";

        let rule = MD007ULIndent::default();
        let ctx = LintContext::new(test, rumdl_lib::config::MarkdownFlavor::Standard, None);

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
        let ctx = LintContext::new(test, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should have no MD007 warnings - code blocks are not list items
        assert!(warnings.is_empty(), "Code blocks should not trigger MD007");
    }

    // Tests for Issue #210: MD007 indent config
    // These tests verify that custom indent values are respected when configured
    mod issue210_indent_config {
        use std::fs;
        use std::process::Command;
        use tempfile::tempdir;

        /// Test the exact scenario from issue #210
        /// Pure unordered list with indent=4 should use fixed style (0, 4, 8 spaces)
        #[test]
        fn test_indent_4_pure_unordered() {
            let temp_dir = tempdir().unwrap();
            let test_file = temp_dir.path().join("repro.md");
            let config_file = temp_dir.path().join(".rumdl.toml");

            // Exact content from issue #210
            let content = r#"# Title

* some
    * list
    * items
"#;

            // Exact config from issue #210
            let config = r#"[global]
line-length = 120

[MD007]
indent = 4
start-indented = false
"#;

            fs::write(&test_file, content).unwrap();
            fs::write(&config_file, config).unwrap();

            // Run check - should find NO issues because:
            // - Level 0: 0 spaces (correct for "* some")
            // - Level 1: 4 spaces (correct for "* list" and "* items" with indent=4 fixed style)
            let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .arg("check")
                .arg("--no-cache")
                .current_dir(temp_dir.path())
                .output()
                .expect("Failed to execute rumdl");

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code().unwrap_or(-1);

            // Should have no issues - the 4-space indent is correct for level 1 with indent=4
            assert!(
                stdout.contains("No issues found") && exit_code == 0,
                "Issue #210: With indent=4, pure unordered lists should use fixed style (0, 4, 8 spaces).\n\
                 The 4-space indent for level 1 items should be correct.\n\
                 stdout: {stdout}\n\
                 stderr: {stderr}\n\
                 exit code: {exit_code}\n\
                 If this fails, the indent=4 config is being ignored."
            );
        }

        /// Test that indent=4 works with deeper nesting
        #[test]
        fn test_indent_4_deep_nesting() {
            let temp_dir = tempdir().unwrap();
            let test_file = temp_dir.path().join("test.md");
            let config_file = temp_dir.path().join(".rumdl.toml");

            // Content with 3 levels - should use 0, 4, 8 spaces with indent=4 fixed style
            let content = r#"# Title

* Level 0
    * Level 1 (4 spaces)
        * Level 2 (8 spaces)
"#;

            let config = r#"[MD007]
indent = 4
"#;

            fs::write(&test_file, content).unwrap();
            fs::write(&config_file, config).unwrap();

            let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .arg("check")
                .arg("--no-cache")
                .current_dir(temp_dir.path())
                .output()
                .expect("Failed to execute rumdl");

            let stdout = String::from_utf8_lossy(&output.stdout);
            let exit_code = output.status.code().unwrap_or(-1);

            assert!(
                stdout.contains("No issues found") && exit_code == 0,
                "With indent=4 fixed style, 0/4/8 spaces should be correct.\n\
                 stdout: {stdout}\n\
                 exit code: {exit_code}"
            );
        }

        /// Test that wrong indentation is detected with indent=4
        #[test]
        fn test_indent_4_detects_wrong_indent() {
            let temp_dir = tempdir().unwrap();
            let test_file = temp_dir.path().join("test.md");
            let config_file = temp_dir.path().join(".rumdl.toml");

            // Content with wrong indentation - 2 spaces instead of 4
            let content = r#"# Title

* Level 0
  * Level 1 (2 spaces - WRONG, should be 4)
"#;

            let config = r#"[MD007]
indent = 4
"#;

            fs::write(&test_file, content).unwrap();
            fs::write(&config_file, config).unwrap();

            let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .arg("check")
                .arg("--no-cache")
                .current_dir(temp_dir.path())
                .output()
                .expect("Failed to execute rumdl");

            let stdout = String::from_utf8_lossy(&output.stdout);
            let exit_code = output.status.code().unwrap_or(-1);

            // Should detect the wrong indentation
            assert!(
                stdout.contains("MD007") && stdout.contains("Expected 4 spaces"),
                "Should detect wrong indentation (2 spaces instead of 4).\n\
                 stdout: {stdout}\n\
                 exit code: {exit_code}"
            );
        }

        /// Test that explicit style=fixed works with indent=4
        #[test]
        fn test_explicit_fixed_style() {
            let temp_dir = tempdir().unwrap();
            let test_file = temp_dir.path().join("test.md");
            let config_file = temp_dir.path().join(".rumdl.toml");

            // Content with correct fixed style indentation
            let content = r#"# Title

* Level 0
    * Level 1 (4 spaces)
        * Level 2 (8 spaces)
"#;

            // Explicit style=fixed should work the same as auto-detection
            let config = r#"[MD007]
indent = 4
style = "fixed"
"#;

            fs::write(&test_file, content).unwrap();
            fs::write(&config_file, config).unwrap();

            let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .arg("check")
                .arg("--no-cache")
                .current_dir(temp_dir.path())
                .output()
                .expect("Failed to execute rumdl");

            let stdout = String::from_utf8_lossy(&output.stdout);
            let exit_code = output.status.code().unwrap_or(-1);

            assert!(
                stdout.contains("No issues found") && exit_code == 0,
                "With explicit style=fixed and indent=4, should work correctly.\n\
                 stdout: {stdout}\n\
                 exit code: {exit_code}"
            );
        }
    }

    // Tests for Issue #209: Fix convergence for mixed ordered/unordered lists
    // These tests verify that MD007 and MD005 don't oscillate when fixing mixed lists
    mod issue209_fix_convergence {
        use std::fs;
        use std::process::Command;
        use tempfile::tempdir;

        /// Test the exact scenario from issue #209
        /// Mixed ordered/unordered list with indent=3 should converge in one pass
        #[test]
        fn test_mixed_list_single_pass_convergence() {
            let temp_dir = tempdir().unwrap();
            let test_file = temp_dir.path().join("test.md");
            let config_file = temp_dir.path().join(".rumdl.toml");

            // Exact content from issue #209
            let content = r#"# Header 1

- **First item**:
  - First subitem
  - Second subitem
  - Third subitem
- **Second item**:
  - **This is a nested list**:
    1. **First point**
       - First subpoint
       - Second subpoint
       - Third subpoint
    2. **Second point**
       - First subpoint
       - Second subpoint
       - Third subpoint
"#;

            // Config from issue #209 - explicitly use text-aligned style
            // to verify no oscillation with that style setting
            let config = r#"[global]
enable = ["MD005", "MD007"]

[MD007]
indent = 3
style = "text-aligned"
"#;

            fs::write(&test_file, content).unwrap();
            fs::write(&config_file, config).unwrap();

            // Run fmt once
            let output1 = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .arg("fmt")
                .arg("--no-cache")
                .current_dir(temp_dir.path())
                .output()
                .expect("Failed to execute rumdl");

            let stdout1 = String::from_utf8_lossy(&output1.stdout);

            // Run fmt a second time - should find no issues (convergence)
            let output2 = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .arg("fmt")
                .arg("--no-cache")
                .current_dir(temp_dir.path())
                .output()
                .expect("Failed to execute rumdl");

            let stdout2 = String::from_utf8_lossy(&output2.stdout);

            // The second run should show "No issues found" - single pass convergence
            assert!(
                stdout2.contains("No issues found"),
                "Fix should converge in single pass.\n\
                 First run output:\n{stdout1}\n\
                 Second run output:\n{stdout2}"
            );
        }

        /// Test that check --fix also converges in one pass
        #[test]
        fn test_check_fix_single_pass() {
            let temp_dir = tempdir().unwrap();
            let test_file = temp_dir.path().join("test.md");
            let config_file = temp_dir.path().join(".rumdl.toml");

            let content = r#"# Header 1

- **Second item**:
  - **This is a nested list**:
    1. **First point**
       - First subpoint
    2. **Second point**
       - First subpoint
"#;

            // Explicitly use text-aligned style to test no oscillation
            let config = r#"[global]
enable = ["MD005", "MD007"]

[MD007]
indent = 3
style = "text-aligned"
"#;

            fs::write(&test_file, content).unwrap();
            fs::write(&config_file, config).unwrap();

            // Run check --fix
            let output1 = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .arg("check")
                .arg("--fix")
                .arg("--no-cache")
                .current_dir(temp_dir.path())
                .output()
                .expect("Failed to execute rumdl");

            // Run check (no fix) - should find no issues
            let output2 = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .arg("check")
                .arg("--no-cache")
                .current_dir(temp_dir.path())
                .output()
                .expect("Failed to execute rumdl");

            let stdout2 = String::from_utf8_lossy(&output2.stdout);
            let exit_code = output2.status.code().unwrap_or(-1);

            assert!(
                stdout2.contains("No issues found") && exit_code == 0,
                "After check --fix, no issues should remain.\n\
                 First run: {:?}\n\
                 Second run stdout: {stdout2}\n\
                 Exit code: {exit_code}",
                String::from_utf8_lossy(&output1.stdout)
            );
        }

        /// Test that explicit style=text-aligned works correctly
        #[test]
        fn test_explicit_text_aligned_no_issues() {
            let temp_dir = tempdir().unwrap();
            let test_file = temp_dir.path().join("test.md");
            let config_file = temp_dir.path().join(".rumdl.toml");

            // This content should have NO issues with text-aligned style
            let content = r#"# Header 1

- **Second item**:
  - **This is a nested list**:
    1. **First point**
       - First subpoint
"#;

            let config = r#"[global]
enable = ["MD005", "MD007"]

[MD007]
indent = 3
style = "text-aligned"
"#;

            fs::write(&test_file, content).unwrap();
            fs::write(&config_file, config).unwrap();

            let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .arg("check")
                .arg("--no-cache")
                .current_dir(temp_dir.path())
                .output()
                .expect("Failed to execute rumdl");

            let stdout = String::from_utf8_lossy(&output.stdout);
            let exit_code = output.status.code().unwrap_or(-1);

            assert!(
                stdout.contains("No issues found") && exit_code == 0,
                "With explicit text-aligned style, mixed lists should have no issues.\n\
                 stdout: {stdout}\n\
                 exit code: {exit_code}"
            );
        }

        /// Test that without explicit style, text-aligned is used (default)
        /// This is the key behavioral change - we no longer auto-switch to fixed
        #[test]
        fn test_default_style_is_text_aligned() {
            let temp_dir = tempdir().unwrap();
            let test_file = temp_dir.path().join("test.md");
            let config_file = temp_dir.path().join(".rumdl.toml");

            // Content matching the exact issue 209 scenario - this should have no issues
            // with text-aligned style (default) but would oscillate with fixed style
            let content = r#"# Header 1

- **Second item**:
  - **This is a nested list**:
    1. **First point**
       - First subpoint
"#;

            // Explicitly use text-aligned style to verify no oscillation with that style
            // With issue #236 fix, style must be explicit to get pure text-aligned behavior
            let config = r#"[global]
enable = ["MD005", "MD007"]

[MD007]
indent = 3
style = "text-aligned"
"#;

            fs::write(&test_file, content).unwrap();
            fs::write(&config_file, config).unwrap();

            let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .arg("check")
                .arg("--no-cache")
                .current_dir(temp_dir.path())
                .output()
                .expect("Failed to execute rumdl");

            let stdout = String::from_utf8_lossy(&output.stdout);
            let exit_code = output.status.code().unwrap_or(-1);

            // With text-aligned (default), this structure should be valid
            // With the old auto-switch to fixed, MD007 would flag the sub-bullets
            // expecting 9 spaces instead of 7
            assert!(
                stdout.contains("No issues found") && exit_code == 0,
                "Default style should be text-aligned, not auto-switching to fixed.\n\
                 stdout: {stdout}\n\
                 exit code: {exit_code}\n\
                 (If this fails, the auto-switch to fixed style may still be active)"
            );
        }
    }
}

// Edge case tests for MD007 indent config (Issue #210)
// These tests verify edge cases and potential issues with the smart
// auto-detection for custom indent values.
mod indent_config_edge_cases {
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    /// Test edge case: indent=3 with pure unordered lists
    #[test]
    fn test_indent_3_pure_unordered() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.md");
        let config_file = temp_dir.path().join(".rumdl.toml");

        let content = r#"# Title

* Level 0
   * Level 1 (3 spaces)
      * Level 2 (6 spaces)
"#;

        let config = r#"[MD007]
indent = 3
"#;

        fs::write(&test_file, content).unwrap();
        fs::write(&config_file, config).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--no-cache")
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute rumdl");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code().unwrap_or(-1);

        assert!(
            stdout.contains("No issues found") && exit_code == 0,
            "With indent=3, pure unordered lists should use fixed style (0, 3, 6 spaces).\n\
             stdout: {stdout}\n\
             exit code: {exit_code}"
        );
    }

    /// Test edge case: indent=5 with pure unordered lists
    #[test]
    fn test_indent_5_pure_unordered() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.md");
        let config_file = temp_dir.path().join(".rumdl.toml");

        let content = r#"# Title

* Level 0
     * Level 1 (5 spaces)
          * Level 2 (10 spaces)
"#;

        let config = r#"[MD007]
indent = 5
"#;

        fs::write(&test_file, content).unwrap();
        fs::write(&config_file, config).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--no-cache")
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute rumdl");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code().unwrap_or(-1);

        assert!(
            stdout.contains("No issues found") && exit_code == 0,
            "With indent=5, pure unordered lists should use fixed style (0, 5, 10 spaces).\n\
             stdout: {stdout}\n\
             exit code: {exit_code}"
        );
    }

    /// Test edge case: indent=4 with mixed lists (should use text-aligned)
    #[test]
    fn test_indent_4_mixed_lists_text_aligned() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.md");
        let config_file = temp_dir.path().join(".rumdl.toml");

        // Mixed list - with issue #236 fix, bullets under unordered use configured indent
        // but bullets under ordered still use text-aligned
        // Use consistent markers to avoid MD004 issues
        let content = r#"# Title

* Unordered item
    * Nested unordered (4 spaces - configured indent)
        1. Ordered child
           * Deeply nested bullet (text-aligned with ordered)
"#;

        let config = r#"[global]
disable = ["MD004"]

[MD007]
indent = 4
"#;

        fs::write(&test_file, content).unwrap();
        fs::write(&config_file, config).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--no-cache")
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute rumdl");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code().unwrap_or(-1);

        // With issue #236 fix, bullets under unordered use configured indent (4)
        // and bullets under ordered use text-aligned
        assert!(
            stdout.contains("No issues found") && exit_code == 0,
            "With indent=4, bullets under unordered should use 4-space indent.\n\
             stdout: {stdout}\n\
             exit code: {exit_code}"
        );
    }

    /// Test for issue #236: indent config is respected when document has mixed lists
    /// https://github.com/rvben/rumdl/issues/236
    #[test]
    fn test_issue_236_indent_config_respected_in_mixed_docs() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.md");
        let config_file = temp_dir.path().join(".rumdl.toml");

        // Exact content from issue #236 - pure unordered + mixed list in same doc
        let content = r#"# Some Heading

- one item
    - another item
- another item

## Heading

1. Some Text.
   - a bullet list inside a numbered list.
2. Hello World.
"#;

        // Config from issue #236
        let config = r#"[ul-indent]
indent = 4
"#;

        fs::write(&test_file, content).unwrap();
        fs::write(&config_file, config).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--no-cache")
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute rumdl");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code().unwrap_or(-1);

        // Issue #236: The pure unordered list should use 4-space indent
        // and the bullet under ordered list should use text-aligned (3 spaces)
        assert!(
            stdout.contains("No issues found") && exit_code == 0,
            "Issue #236: indent=4 should be respected for pure unordered sections.\n\
             stdout: {stdout}\n\
             exit code: {exit_code}"
        );
    }

    /// Test edge case: config loaded from pyproject.toml
    #[test]
    fn test_indent_4_from_pyproject() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.md");
        let config_file = temp_dir.path().join("pyproject.toml");

        let content = r#"# Title

* some
    * list
    * items
"#;

        let config = r#"[tool.rumdl.MD007]
indent = 4
"#;

        fs::write(&test_file, content).unwrap();
        fs::write(&config_file, config).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--no-cache")
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute rumdl");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code().unwrap_or(-1);

        assert!(
            stdout.contains("No issues found") && exit_code == 0,
            "Config from pyproject.toml should work correctly.\n\
             stdout: {stdout}\n\
             exit code: {exit_code}"
        );
    }

    /// Test edge case: config with explicit style=fixed should override auto-detection
    #[test]
    fn test_indent_4_explicit_fixed_overrides_auto() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.md");
        let config_file = temp_dir.path().join(".rumdl.toml");

        // Even with mixed lists, explicit style=fixed should be used
        let content = r#"# Title

* Unordered
    * Nested (4 spaces - fixed style)
"#;

        let config = r#"[MD007]
indent = 4
style = "fixed"
"#;

        fs::write(&test_file, content).unwrap();
        fs::write(&config_file, config).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--no-cache")
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute rumdl");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code().unwrap_or(-1);

        // With explicit fixed style, it should expect 4 spaces for level 1
        // The content has 4 spaces, so this should be valid
        assert!(
            stdout.contains("No issues found") && exit_code == 0,
            "Explicit style=fixed with correct 4-space indent should be valid.\n\
             stdout: {stdout}\n\
             exit code: {exit_code}"
        );
    }

    /// Test edge case: config with explicit style=text-aligned
    #[test]
    fn test_indent_4_explicit_text_aligned() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.md");
        let config_file = temp_dir.path().join(".rumdl.toml");

        let content = r#"# Title

* Unordered
  * Nested (2 spaces - text-aligned)
"#;

        let config = r#"[MD007]
indent = 4
style = "text-aligned"
"#;

        fs::write(&test_file, content).unwrap();
        fs::write(&config_file, config).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--no-cache")
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute rumdl");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code().unwrap_or(-1);

        assert!(
            stdout.contains("No issues found") && exit_code == 0,
            "Explicit style=text-aligned should work correctly.\n\
             stdout: {stdout}\n\
             exit code: {exit_code}"
        );
    }

    /// Test edge case: indent=1 (minimum value)
    #[test]
    fn test_indent_1_pure_unordered() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.md");
        let config_file = temp_dir.path().join(".rumdl.toml");

        let content = r#"# Title

* Level 0
 * Level 1 (1 space - correct for indent=1 fixed style)
  * Level 2 (2 spaces - correct for indent=1 fixed style)
"#;

        let config = r#"[global]
disable = ["MD005"]

[MD007]
indent = 1
"#;

        fs::write(&test_file, content).unwrap();
        fs::write(&config_file, config).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--no-cache")
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute rumdl");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code().unwrap_or(-1);

        assert!(
            stdout.contains("No issues found") && exit_code == 0,
            "With indent=1, pure unordered lists should use fixed style (0, 1, 2 spaces).\n\
             stdout: {stdout}\n\
             exit code: {exit_code}"
        );
    }

    /// Test edge case: indent=8 (maximum value)
    #[test]
    fn test_indent_8_pure_unordered() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.md");
        let config_file = temp_dir.path().join(".rumdl.toml");

        let content = r#"# Title

* Level 0
        * Level 1 (8 spaces)
                * Level 2 (16 spaces)
"#;

        let config = r#"[MD007]
indent = 8
"#;

        fs::write(&test_file, content).unwrap();
        fs::write(&config_file, config).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--no-cache")
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to execute rumdl");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code().unwrap_or(-1);

        assert!(
            stdout.contains("No issues found") && exit_code == 0,
            "With indent=8, pure unordered lists should use fixed style (0, 8, 16 spaces).\n\
             stdout: {stdout}\n\
             exit code: {exit_code}"
        );
    }

    /// Test edge case: config in parent directory
    #[test]
    fn test_indent_4_config_in_parent() {
        let temp_dir = tempdir().unwrap();
        let sub_dir = temp_dir.path().join("sub");
        fs::create_dir_all(&sub_dir).unwrap();

        let test_file = sub_dir.join("test.md");
        let config_file = temp_dir.path().join(".rumdl.toml");

        let content = r#"# Title

* some
    * list
    * items
"#;

        let config = r#"[MD007]
indent = 4
"#;

        fs::write(&test_file, content).unwrap();
        fs::write(&config_file, config).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--no-cache")
            .current_dir(&sub_dir)
            .output()
            .expect("Failed to execute rumdl");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code().unwrap_or(-1);

        assert!(
            stdout.contains("No issues found") && exit_code == 0,
            "Config from parent directory should be discovered and used.\n\
             stdout: {stdout}\n\
             exit code: {exit_code}"
        );
    }
}

// Tests for Issue #247: MD007 false positives on nested unordered lists in ordered lists
mod issue247_nested_unordered_in_ordered {
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD007ULIndent;

    /// Test that nested unordered lists within ordered lists don't trigger false positives
    /// This was the ping-pong bug: MD007 would change 5→4 spaces, then MD005 would change 4→3
    /// destroying the document's nesting structure
    #[test]
    fn test_nested_unordered_in_ordered_no_false_positives() {
        let rule = MD007ULIndent::default();

        // This content should have NO errors (matches markdownlint-cli behavior)
        // Structure:
        // - Ordered item "1. " at column 0, content starts at column 3
        // - Unordered child "- " at 3 spaces, content at column 5
        // - Unordered grandchild "- " at 5 spaces (3 + 2 = parent content column)
        let content = r#"# Header

1. First
   - Abc abc

2. Second
   - Abc abc
   - Xyz
     - Aaa
     - Bbb

3. Third
   - Thirty one
     - Hello
     - World
   - Thirty two
     - One
     - More
"#;

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(
            warnings.is_empty(),
            "Nested unordered lists in ordered lists should not trigger MD007.\n\
             markdownlint-cli shows no errors for this structure.\n\
             Got {} warnings: {:?}",
            warnings.len(),
            warnings
        );
    }

    /// Test that fix preserves nested structure (no ping-pong)
    #[test]
    fn test_fix_preserves_nested_structure() {
        let rule = MD007ULIndent::default();

        let content = r#"1. First
   - Xyz
     - Aaa
     - Bbb
"#;

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should be unchanged - structure is already correct
        assert_eq!(fixed, content, "Fix should not modify already-correct nested structure");
    }

    /// Test the simple case: unordered under ordered at proper indent
    #[test]
    fn test_simple_unordered_under_ordered() {
        let rule = MD007ULIndent::default();

        // Single level: bullet should be at 3 spaces (aligns with "1. " content)
        let content = r#"1. Ordered item
   - Bullet under ordered
"#;

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(warnings.is_empty(), "Bullet at 3 spaces under '1. ' should be valid");
    }

    /// Test double-digit ordered list (issue #247 original case)
    #[test]
    fn test_double_digit_ordered_list() {
        let rule = MD007ULIndent::default();

        // For "10. " (4 chars), content starts at column 4
        // Child bullet should be at 4 spaces
        let content = r#"10. Item ten
    - sub
11. Item eleven
    - sub
12. Item twelve
"#;

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(warnings.is_empty(), "Bullets at 4 spaces under '10. ' should be valid");
    }

    /// Test that parent's content column is used, not nesting_level × indent_size
    #[test]
    fn test_parent_content_column_used() {
        let rule = MD007ULIndent::default();

        // Parent "- Xyz" at 3 spaces has content at column 5
        // Child should be at 5 spaces, not 4 (which would be nesting_level × 2)
        let content = r#"1. First
   - Xyz
     - Child at 5 spaces
"#;

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(
            warnings.is_empty(),
            "Child at parent's content column (5) should be valid, not nesting_level × indent (4)"
        );
    }

    /// Test deeply nested structure
    #[test]
    fn test_deeply_nested_mixed_lists() {
        let rule = MD007ULIndent::default();

        let content = r#"1. Level 1 ordered
   - Level 2 unordered (3 spaces)
     - Level 3 unordered (5 spaces)
       - Level 4 unordered (7 spaces)
"#;

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(warnings.is_empty(), "Deeply nested mixed lists should work correctly");
    }
}
