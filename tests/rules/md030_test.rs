mod tests {
    use rumdl::lint_context::LintContext;
    use rumdl::rule::Rule;
    use rumdl::rules::MD030ListMarkerSpace;

    #[test]
    fn test_valid_single_line_lists() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* Item\n- Another item\n+ Third item\n1. Ordered item";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_valid_multi_line_lists() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* First line\n  continued\n- Second item\n  also continued";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_invalid_spaces_unordered() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Too many spaces\n-   Three spaces";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        for warning in result {
            assert!(
                warning.message.starts_with("Spaces after list markers (Expected:") && warning.message.contains("Actual:"),
                "Warning message should include expected and actual values, got: '{}'",
                warning.message
            );
        }
    }

    #[test]
    fn test_invalid_spaces_ordered() {
        let rule = MD030ListMarkerSpace::default();
        let content = "1.  Too many spaces\n2.   Three spaces";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        for warning in result {
            assert!(
                warning.message.starts_with("Spaces after list markers (Expected:") && warning.message.contains("Actual:"),
                "Warning message should include expected and actual values, got: '{}'",
                warning.message
            );
        }
    }

    #[test]
    fn test_ignore_code_blocks() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* Normal item\n```\n*  Not a list\n1.  Not a list\n```\n- Back to list";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_missing_space_after_list_marker_unordered() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*Item 1\n-Item 2\n+Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Per CommonMark and markdownlint, these are not valid list items, so no warnings expected
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_missing_space_after_list_marker_ordered() {
        let rule = MD030ListMarkerSpace::default();
        let content = "1.First\n2.Second\n3.Third";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Per CommonMark and markdownlint, these are not valid list items, so no warnings expected
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_mixed_list_types_missing_space() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*Item 1\n1.First\n-Item 2\n2.Second";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Per CommonMark and markdownlint, these are not valid list items, so no warnings expected
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_nested_lists_missing_space() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* Item 1\n  *Nested 1\n  *Nested 2\n* Item 2";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Per CommonMark and markdownlint, these are not valid list items, so no warnings expected
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_code_block_ignored() {
        let rule = MD030ListMarkerSpace::default();
        let content = "```markdown\n*Item 1\n*Item 2\n```\n* Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Only the valid item outside the code block should be checked
        assert!(result.is_empty());
    }

    #[test]
    fn test_horizontal_rule_not_flagged() {
        let rule = MD030ListMarkerSpace::default();
        let content = "***\n---\n___";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_preserve_indentation() {
        let rule = MD030ListMarkerSpace::default();
        let content = "  *Item 1\n    *Item 2\n      *Item 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Per CommonMark and markdownlint, these are not valid list items, so no warnings expected
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_real_world_single_space() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* [danbev](https://github.com/danbev) -\n  **Daniel Bevenius** <<daniel.bevenius@gmail.com>> (he/him)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_real_world_multiple_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*   [bengl](https://github.com/bengl) -\n    **Bryan English** <<bryan@bryanenglish.com>> (he/him)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.starts_with("Spaces after list markers (Expected:") && result[0].message.contains("Actual:"),
            "Warning message should include expected and actual values, got: '{}'",
            result[0].message
        );
    }

    #[test]
    fn test_real_world_tab_after_marker() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*	[benjamingr](https://github.com/benjamingr) -\n    **Benjamin Gruenbaum** <<benjamingr@gmail.com>>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.starts_with("Spaces after list markers (Expected:") && result[0].message.contains("Actual:"),
            "Warning message should include expected and actual values, got: '{}'",
            result[0].message
        );
    }

    #[test]
    fn test_real_world_nested_extra_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content = "  *   [nested](https://github.com/nested) -\n      **Nested User** <<nested@example.com>>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.starts_with("Spaces after list markers (Expected:") && result[0].message.contains("Actual:"),
            "Warning message should include expected and actual values, got: '{}'",
            result[0].message
        );
    }

    #[test]
    fn test_real_world_multiline_extra_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*   [multi](https://github.com/multi) -\n    **Multi Line**\n    <<multi@example.com>>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.starts_with("Spaces after list markers (Expected:") && result[0].message.contains("Actual:"),
            "Warning message should include expected and actual values, got: '{}'",
            result[0].message
        );
    }

    #[test]
    fn test_real_world_three_spaces_after_marker() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*   [geeksilva97](https://github.com/geeksilva97) -";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.starts_with("Spaces after list markers (Expected:") && result[0].message.contains("Actual:"),
            "Warning message should include expected and actual values, got: '{}'",
            result[0].message
        );
    }

    #[test]
    fn test_indented_list_item_with_extra_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content = "    *   [indented](https://github.com/indented) -";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Indented lines are treated as code blocks and should not be flagged
        assert_eq!(result.len(), 0);
    }

    // ===== COMPREHENSIVE EDGE CASE TESTS FOR FIX METHOD =====

    #[test]
    fn test_fix_basic_unordered_list_extra_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content =
            "*  Item with two spaces\n-   Item with three spaces\n+    Item with four spaces";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Item with two spaces\n- Item with three spaces\n+ Item with four spaces";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_basic_ordered_list_extra_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content =
            "1.  Item with two spaces\n2.   Item with three spaces\n10.    Item with four spaces";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected =
            "1. Item with two spaces\n2. Item with three spaces\n10. Item with four spaces";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_tabs_after_markers() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*\tItem with tab\n-\t\tItem with two tabs\n1.\tOrdered with tab";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Item with tab\n- Item with two tabs\n1. Ordered with tab";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_mixed_spaces_and_tabs() {
        let rule = MD030ListMarkerSpace::default();
        let content =
            "* \tMixed space and tab\n- \t Item with space-tab-space\n1. \t\tOrdered mixed";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Mixed space and tab\n- Item with space-tab-space\n1. Ordered mixed";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_preserves_indentation() {
        let rule = MD030ListMarkerSpace::default();
        let content = "  *  Indented item\n    -   Deeply indented\n      +    Very deep";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "  * Indented item\n    - Deeply indented\n      + Very deep";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_preserves_code_blocks() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Normal item\n```\n*  Code block item (should not be fixed)\n1.   Code block ordered\n```\n-   Another normal item";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Normal item\n```\n*  Code block item (should not be fixed)\n1.   Code block ordered\n```\n- Another normal item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_preserves_fenced_code_with_tildes() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Normal item\n~~~\n*  Code block item\n1.   Code block ordered\n~~~\n-   Another normal item";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Normal item\n~~~\n*  Code block item\n1.   Code block ordered\n~~~\n- Another normal item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_preserves_indented_code_blocks() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Normal item\n\n    *  Indented code block\n    1.   Should not be fixed\n\n-   Another normal item";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Normal item\n\n    *  Indented code block\n    1.   Should not be fixed\n\n- Another normal item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_preserves_blockquotes() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Normal item\n> *  Blockquote item\n> 1.   Blockquote ordered\n-   Another normal item";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected =
            "* Normal item\n> *  Blockquote item\n> 1.   Blockquote ordered\n- Another normal item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_preserves_front_matter() {
        let rule = MD030ListMarkerSpace::default();
        let content =
            "---\ntitle: Test\n*  This is in front matter\n---\n*  This is a real list item";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected =
            "---\ntitle: Test\n*  This is in front matter\n---\n* This is a real list item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_empty_content() {
        let rule = MD030ListMarkerSpace::default();
        let content = "";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "");
    }

    #[test]
    fn test_fix_no_list_items() {
        let rule = MD030ListMarkerSpace::default();
        let content = "# Heading\n\nSome paragraph text.\n\nAnother paragraph.";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_only_fixes_clear_violations() {
        let rule = MD030ListMarkerSpace::default();
        let content =
            "* Correct spacing\n*  Two spaces (fixed)\n* Another correct\n*   Three spaces (fixed)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected =
            "* Correct spacing\n* Two spaces (fixed)\n* Another correct\n* Three spaces (fixed)";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_does_not_break_empty_list_items() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  \n-   \n+    ";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        // Empty list items should not be fixed to avoid breaking structure
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_handles_large_ordered_numbers() {
        let rule = MD030ListMarkerSpace::default();
        let content = "999.  Large number\n1000.   Very large number\n12345.    Huge number";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "999. Large number\n1000. Very large number\n12345. Huge number";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_handles_zero_padded_numbers() {
        let rule = MD030ListMarkerSpace::default();
        let content = "01.  Zero padded\n001.   More zeros\n0001.    Many zeros";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "01. Zero padded\n001. More zeros\n0001. Many zeros";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_complex_nested_structure() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Top level\n  *  Nested level\n    *   Deep nested\n      1.  Ordered nested\n        2.   Very deep ordered";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Top level\n  * Nested level\n    * Deep nested\n      1. Ordered nested\n        2. Very deep ordered";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_mixed_content_with_lists() {
        let rule = MD030ListMarkerSpace::default();
        let content = "# Heading\n\n*  List item\n\nParagraph text.\n\n1.  Ordered item\n\n```\ncode block\n```\n\n-   Another item";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "# Heading\n\n* List item\n\nParagraph text.\n\n1. Ordered item\n\n```\ncode block\n```\n\n- Another item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_with_custom_configuration() {
        let rule = MD030ListMarkerSpace::new(2, 2, 3, 3); // Custom spacing
        let content = "*  Item (should become 2 spaces)\n1.   Item (should become 3 spaces)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "*  Item (should become 2 spaces)\n1.   Item (should become 3 spaces)";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Item with extra spaces\n";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item with extra spaces\n");
    }

    #[test]
    fn test_fix_preserves_no_trailing_newline() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Item with extra spaces";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item with extra spaces");
    }

    #[test]
    fn test_fix_handles_unicode_content() {
        let rule = MD030ListMarkerSpace::default();
        let content =
            "*  Unicode content: 你好世界\n-   Emoji content: 🚀🎉\n+    Mixed: café naïve résumé";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected =
            "* Unicode content: 你好世界\n- Emoji content: 🚀🎉\n+ Mixed: café naïve résumé";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_handles_special_characters() {
        let rule = MD030ListMarkerSpace::default();
        let content =
            "*  Content with `code`\n-   Content with **bold**\n+    Content with [link](url)";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Content with `code`\n- Content with **bold**\n+ Content with [link](url)";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_handles_very_long_lines() {
        let rule = MD030ListMarkerSpace::default();
        let long_content = "a".repeat(1000);
        let content = format!("*  {}", long_content);
        let ctx = LintContext::new(&content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = format!("* {}", long_content);
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_handles_edge_case_markers() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* Normal\n*  Two spaces\n*   Three spaces\n- Normal\n-  Two spaces\n+ Normal\n+   Three spaces";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Normal\n* Two spaces\n* Three spaces\n- Normal\n- Two spaces\n+ Normal\n+ Three spaces";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_performance_with_large_content() {
        let rule = MD030ListMarkerSpace::default();
        let mut lines = Vec::new();
        for i in 0..1000 {
            lines.push(format!("*  Item {}", i));
        }
        let content = lines.join("\n");
        let ctx = LintContext::new(&content);

        let start = std::time::Instant::now();
        let fixed = rule.fix(&ctx).unwrap();
        let duration = start.elapsed();

        // Should complete in reasonable time (less than 1 second)
        assert!(duration.as_secs() < 1, "Fix took too long: {:?}", duration);

        // Verify all items were fixed
        let fixed_lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(fixed_lines.len(), 1000);
        for (i, line) in fixed_lines.iter().enumerate() {
            assert_eq!(*line, format!("* Item {}", i));
        }
    }
}
