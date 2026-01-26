mod tests {
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD030ListMarkerSpace;

    #[test]
    fn test_valid_single_line_lists() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* Item\n- Another item\n+ Third item\n1. Ordered item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_valid_multi_line_lists() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* First line\n  continued\n- Second item\n  also continued";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_invalid_spaces_unordered() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Too many spaces\n-   Three spaces";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        for warning in result {
            assert!(
                warning.message.starts_with("Spaces after list markers (Expected:")
                    && warning.message.contains("Actual:"),
                "Warning message should include expected and actual values, got: '{}'",
                warning.message
            );
        }
    }

    #[test]
    fn test_invalid_spaces_ordered() {
        let rule = MD030ListMarkerSpace::default();
        let content = "1.  Too many spaces\n2.   Three spaces";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        for warning in result {
            assert!(
                warning.message.starts_with("Spaces after list markers (Expected:")
                    && warning.message.contains("Actual:"),
                "Warning message should include expected and actual values, got: '{}'",
                warning.message
            );
        }
    }

    #[test]
    fn test_ignore_code_blocks() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* Normal item\n```\n*  Not a list\n1.  Not a list\n```\n- Back to list";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_missing_space_after_list_marker_unordered() {
        // Unordered markers (*, -, +) without spaces are NOT flagged because:
        // 1. They have too many non-list uses (emphasis, globs, diffs, etc.)
        // 2. CommonMark requires space after marker for valid list items
        // 3. The parser correctly doesn't recognize these as list items
        let rule = MD030ListMarkerSpace::default();
        let content = "*Item 1\n-Item 2\n+Item 3";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Not flagged - parser doesn't recognize as lists, no heuristic detection for unordered
        assert_eq!(result.len(), 0, "Unordered markers without space are not flagged");
    }

    #[test]
    fn test_missing_space_after_list_marker_ordered() {
        // User intention: these look like list items missing spaces, so flag them
        let rule = MD030ListMarkerSpace::default();
        let content = "1.First\n2.Second\n3.Third";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // User-intention-based detection: flag all lines that look like list items
        assert_eq!(result.len(), 3, "Should detect 3 ordered list items missing spaces");
    }

    #[test]
    fn test_mixed_list_types_missing_space() {
        // Only ordered markers (1., 2.) are flagged via heuristics
        // Unordered markers (*, -) are not flagged
        let rule = MD030ListMarkerSpace::default();
        let content = "*Item 1\n1.First\n-Item 2\n2.Second";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Only 2 warnings for ordered markers (1.First and 2.Second)
        assert_eq!(result.len(), 2, "Should detect 2 ordered list items missing spaces");
    }

    #[test]
    fn test_nested_lists_missing_space() {
        // Unordered markers without spaces are not flagged
        // This matches markdownlint-cli behavior (0 warnings)
        let rule = MD030ListMarkerSpace::default();
        let content = "* Item 1\n  *Nested 1\n  *Nested 2\n* Item 2";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // No warnings - unordered markers without space are not flagged
        assert_eq!(
            result.len(),
            0,
            "Unordered markers without space are not flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_code_block_ignored() {
        let rule = MD030ListMarkerSpace::default();
        let content = "```markdown\n*Item 1\n*Item 2\n```\n* Item 3";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Only the valid item outside the code block should be checked
        assert!(result.is_empty());
    }

    #[test]
    fn test_horizontal_rule_not_flagged() {
        let rule = MD030ListMarkerSpace::default();
        let content = "***\n---\n___";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_preserve_indentation() {
        // Unordered markers without spaces are not flagged
        let rule = MD030ListMarkerSpace::default();
        let content = "  *Item 1\n    *Item 2\n      *Item 3";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // No warnings - unordered markers without space are not flagged
        assert_eq!(
            result.len(),
            0,
            "Unordered markers without space are not flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_real_world_single_space() {
        let rule = MD030ListMarkerSpace::default();
        let content =
            "* [danbev](https://github.com/danbev) -\n  **Daniel Bevenius** <<daniel.bevenius@gmail.com>> (he/him)";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_real_world_multiple_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content =
            "*   [bengl](https://github.com/bengl) -\n    **Bryan English** <<bryan@bryanenglish.com>> (he/him)";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.starts_with("Spaces after list markers (Expected:")
                && result[0].message.contains("Actual:"),
            "Warning message should include expected and actual values, got: '{}'",
            result[0].message
        );
    }

    #[test]
    fn test_real_world_tab_after_marker() {
        // MD030 only checks for multiple spaces, not tabs
        // Tabs are handled by MD010 (no-hard-tabs), matching markdownlint behavior
        let rule = MD030ListMarkerSpace::default();
        let content =
            "*\t[benjamingr](https://github.com/benjamingr) -\n    **Benjamin Gruenbaum** <<benjamingr@gmail.com>>";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Tabs should NOT be flagged by MD030 - that's MD010's job
        assert_eq!(result.len(), 0, "MD030 should not flag tabs");
    }

    #[test]
    fn test_real_world_nested_extra_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content = "  *   [nested](https://github.com/nested) -\n      **Nested User** <<nested@example.com>>";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.starts_with("Spaces after list markers (Expected:")
                && result[0].message.contains("Actual:"),
            "Warning message should include expected and actual values, got: '{}'",
            result[0].message
        );
    }

    #[test]
    fn test_real_world_multiline_extra_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*   [multi](https://github.com/multi) -\n    **Multi Line**\n    <<multi@example.com>>";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.starts_with("Spaces after list markers (Expected:")
                && result[0].message.contains("Actual:"),
            "Warning message should include expected and actual values, got: '{}'",
            result[0].message
        );
    }

    #[test]
    fn test_real_world_three_spaces_after_marker() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*   [geeksilva97](https://github.com/geeksilva97) -";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.starts_with("Spaces after list markers (Expected:")
                && result[0].message.contains("Actual:"),
            "Warning message should include expected and actual values, got: '{}'",
            result[0].message
        );
    }

    #[test]
    fn test_indented_list_item_with_extra_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content = "    *   [indented](https://github.com/indented) -";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Indented lines are treated as code blocks and should not be flagged
        assert_eq!(result.len(), 0);
    }

    // ===== COMPREHENSIVE EDGE CASE TESTS FOR FIX METHOD =====

    #[test]
    fn test_fix_basic_unordered_list_extra_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Item with two spaces\n-   Item with three spaces\n+    Item with four spaces";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Item with two spaces\n- Item with three spaces\n+ Item with four spaces";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_basic_ordered_list_extra_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content = "1.  Item with two spaces\n2.   Item with three spaces\n10.    Item with four spaces";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "1. Item with two spaces\n2. Item with three spaces\n10. Item with four spaces";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_tabs_after_markers_not_modified() {
        // MD030 only handles multiple SPACES after list markers, not tabs
        // Tabs are handled by MD010 (no-hard-tabs)
        // This matches markdownlint reference behavior
        let rule = MD030ListMarkerSpace::default();

        // Content with tabs should not be modified by MD030
        let content = "*\tItem with tab\n-\t\tItem with two tabs\n1.\tOrdered with tab";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Tabs should remain unchanged - MD010 handles those
        assert_eq!(fixed, content, "MD030 should not modify tabs");
    }

    #[test]
    fn test_multiple_spaces_after_markers() {
        // MD030 flags multiple SPACES, not tabs
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Two spaces\n-   Three spaces\n1.  Ordered two spaces";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Two spaces\n- Three spaces\n1. Ordered two spaces";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_preserves_indentation() {
        let rule = MD030ListMarkerSpace::default();
        let content = "  *  Indented item\n    -   Deeply indented\n      +    Very deep";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "  * Indented item\n    - Deeply indented\n      + Very deep";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_preserves_code_blocks() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Normal item\n```\n*  Code block item (should not be fixed)\n1.   Code block ordered\n```\n-   Another normal item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Normal item\n```\n*  Code block item (should not be fixed)\n1.   Code block ordered\n```\n- Another normal item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_preserves_fenced_code_with_tildes() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Normal item\n~~~\n*  Code block item\n1.   Code block ordered\n~~~\n-   Another normal item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Normal item\n~~~\n*  Code block item\n1.   Code block ordered\n~~~\n- Another normal item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_preserves_indented_code_blocks() {
        let rule = MD030ListMarkerSpace::default();
        let content =
            "*  Normal item\n\n    *  Indented code block\n    1.   Should not be fixed\n\n-   Another normal item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected =
            "* Normal item\n\n    *  Indented code block\n    1.   Should not be fixed\n\n- Another normal item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_blockquotes() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Normal item\n> *  Blockquote item\n> 1.   Blockquote ordered\n-   Another normal item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Blockquoted list items should also be fixed to correct spacing
        let expected = "* Normal item\n> * Blockquote item\n> 1. Blockquote ordered\n- Another normal item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_preserves_front_matter() {
        let rule = MD030ListMarkerSpace::default();
        let content = "---\ntitle: Test\n*  This is in front matter\n---\n*  This is a real list item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "---\ntitle: Test\n*  This is in front matter\n---\n* This is a real list item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_empty_content() {
        let rule = MD030ListMarkerSpace::default();
        let content = "";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "");
    }

    #[test]
    fn test_fix_no_list_items() {
        let rule = MD030ListMarkerSpace::default();
        let content = "# Heading\n\nSome paragraph text.\n\nAnother paragraph.";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_only_fixes_clear_violations() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* Correct spacing\n*  Two spaces (fixed)\n* Another correct\n*   Three spaces (fixed)";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Correct spacing\n* Two spaces (fixed)\n* Another correct\n* Three spaces (fixed)";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_does_not_break_empty_list_items() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  \n-   \n+    ";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Empty list items should not be fixed to avoid breaking structure
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_handles_large_ordered_numbers() {
        let rule = MD030ListMarkerSpace::default();
        let content = "999.  Large number\n1000.   Very large number\n12345.    Huge number";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "999. Large number\n1000. Very large number\n12345. Huge number";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_handles_zero_padded_numbers() {
        let rule = MD030ListMarkerSpace::default();
        let content = "01.  Zero padded\n001.   More zeros\n0001.    Many zeros";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "01. Zero padded\n001. More zeros\n0001. Many zeros";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_complex_nested_structure() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Top level\n  *  Nested level\n    *   Deep nested\n      1.  Ordered nested\n        2.   Very deep ordered";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected =
            "* Top level\n  * Nested level\n    * Deep nested\n      1. Ordered nested\n        2. Very deep ordered";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_mixed_content_with_lists() {
        let rule = MD030ListMarkerSpace::default();
        let content = "# Heading\n\n*  List item\n\nParagraph text.\n\n1.  Ordered item\n\n```\ncode block\n```\n\n-   Another item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected =
            "# Heading\n\n* List item\n\nParagraph text.\n\n1. Ordered item\n\n```\ncode block\n```\n\n- Another item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_with_custom_configuration() {
        let rule = MD030ListMarkerSpace::new(2, 2, 3, 3); // Custom spacing
        let content = "*  Item (should become 2 spaces)\n1.   Item (should become 3 spaces)";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // With custom config ul_single=2 and ol_single=3, spacing should be adjusted to match
        let expected = "*  Item (should become 2 spaces)\n1.   Item (should become 3 spaces)";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_custom_config_single_space_to_multi() {
        // Issue #318: Fix should work when configured spacing differs from default
        // When ul_single=3, a line with 1 space should be fixed to 3 spaces
        let rule = MD030ListMarkerSpace::new(3, 3, 2, 2); // Custom: ul=3 spaces, ol=2 spaces
        let content = "* Item with one space\n1. Ordered with one space";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Single space should be expanded to match configuration
        let expected = "*   Item with one space\n1.  Ordered with one space";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_check_custom_config_single_space_violation() {
        // Issue #318: Check should detect when single space doesn't match config
        let rule = MD030ListMarkerSpace::new(3, 3, 2, 2); // Custom: ul=3 spaces, ol=2 spaces
        let content = "* Item with one space\n1. Ordered with one space";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should detect violations for both lines
        assert_eq!(result.len(), 2, "Should detect 2 spacing violations. Got: {result:?}");
        assert!(result[0].message.contains("Expected: 3"));
        assert!(result[0].message.contains("Actual: 1"));
        assert!(result[1].message.contains("Expected: 2"));
        assert!(result[1].message.contains("Actual: 1"));
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Item with extra spaces\n";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item with extra spaces\n");
    }

    #[test]
    fn test_fix_preserves_no_trailing_newline() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Item with extra spaces";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item with extra spaces");
    }

    #[test]
    fn test_fix_handles_unicode_content() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Unicode content: 你好世界\n-   Emoji content: 🚀🎉\n+    Mixed: café naïve résumé";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Unicode content: 你好世界\n- Emoji content: 🚀🎉\n+ Mixed: café naïve résumé";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_handles_special_characters() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*  Content with `code`\n-   Content with **bold**\n+    Content with [link](url)";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Content with `code`\n- Content with **bold**\n+ Content with [link](url)";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_handles_very_long_lines() {
        let rule = MD030ListMarkerSpace::default();
        let long_content = "a".repeat(1000);
        let content = format!("*  {long_content}");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = format!("* {long_content}");
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_handles_edge_case_markers() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* Normal\n*  Two spaces\n*   Three spaces\n- Normal\n-  Two spaces\n+ Normal\n+   Three spaces";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Normal\n* Two spaces\n* Three spaces\n- Normal\n- Two spaces\n+ Normal\n+ Three spaces";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_fix_performance_with_large_content() {
        let rule = MD030ListMarkerSpace::default();
        let mut lines = Vec::new();
        for i in 0..1000 {
            lines.push(format!("*  Item {i}"));
        }
        let content = lines.join("\n");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        let start = std::time::Instant::now();
        let fixed = rule.fix(&ctx).unwrap();
        let duration = start.elapsed();

        // Should complete in reasonable time (less than 1 second)
        assert!(duration.as_secs() < 1, "Fix took too long: {duration:?}");

        // Verify all items were fixed
        let fixed_lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(fixed_lines.len(), 1000);
        for (i, line) in fixed_lines.iter().enumerate() {
            assert_eq!(*line, format!("* Item {i}"));
        }
    }

    #[test]
    fn test_multi_line_configuration_support() {
        // Test that ul_multi and ol_multi configuration options are actually used
        let rule = MD030ListMarkerSpace::new(
            1, // ul_single
            3, // ul_multi  - key test: multi-line should use this
            1, // ol_single
            4, // ol_multi  - key test: multi-line should use this
        );

        let content = "* Single line\n*  Multi-line item\n   with continuation\n1. Single ordered\n1.   Multi-line ordered\n     with continuation";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should find 2 violations:
        // - Line 2: multi-line unordered list (expects 3 spaces, has 2)
        // - Line 5: multi-line ordered list (expects 4 spaces, has 3)
        assert_eq!(
            result.len(),
            2,
            "Should detect multi-line spacing violations, got: {result:?}"
        );

        // Check the specific violations
        assert_eq!(result[0].line, 2);
        assert!(result[0].message.contains("Expected: 3"));
        assert!(result[0].message.contains("Actual: 2"));

        assert_eq!(result[1].line, 5);
        assert!(result[1].message.contains("Expected: 4"));
        assert!(result[1].message.contains("Actual: 3"));

        // Test the fix
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "* Single line\n*   Multi-line item\n   with continuation\n1. Single ordered\n1.    Multi-line ordered\n     with continuation";
        assert_eq!(fixed, expected, "Multi-line spacing should be fixed correctly");
    }

    #[test]
    fn test_multi_line_blockquote_list_regression() {
        // Regression test: multi-line detection must work for lists inside blockquotes
        // Previously, the raw indent (0 for blockquote lines) was compared against
        // content_column, causing all blockquote list items to appear as single-line.
        let rule = MD030ListMarkerSpace::new(
            1, // ul_single
            3, // ul_multi - multi-line items should require 3 spaces
            1, // ol_single
            1, // ol_multi
        );

        // A blockquote list where the item has continuation content
        // The continuation ">   more text" has 2 spaces of indent after the marker
        let content = "> - First item\n>   more text\n> - Second item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // First item is multi-line (has continuation), so should expect 3 spaces (ul_multi)
        // It only has 1 space, so should be flagged
        assert_eq!(
            result.len(),
            1,
            "Multi-line blockquote list item should be detected. Got: {result:?}"
        );
        assert_eq!(result[0].line, 1, "Warning should be on line 1");
        assert!(
            result[0].message.contains("Expected: 3"),
            "Should expect ul_multi (3) spaces. Got: {}",
            result[0].message
        );
    }

    // Tests for issue #253: MD030 false positive on hard-wrapped brackets
    // https://github.com/rvben/rumdl/issues/253

    #[test]
    fn test_issue_253_citation_continuation() {
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- foobar foobar foobar foobar foobar foobar foobar foobar foobar (Doe 2003, p.
  1234)"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should NOT trigger MD030 on the continuation line "  1234)"
        assert_eq!(
            result.len(),
            0,
            "Should not trigger MD030 on continuation lines. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_multiple_citations() {
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- Citation example (Author 2023, p.
  1234)

- Reference with number (see item
  99)

* Multiple digits (total:
  123456)"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should NOT trigger MD030 on any continuation lines
        assert_eq!(
            result.len(),
            0,
            "Should not trigger MD030 on continuation lines. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_valid_nested_lists() {
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- Unordered item
  1) Nested ordered item with parenthesis
  2) Another nested item

* Another unordered
  1. Nested with period
  2. More nested"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should NOT trigger MD030 on nested lists
        assert_eq!(
            result.len(),
            0,
            "Should not trigger MD030 on properly formatted nested lists. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_lsp_formatting_loop_prevention() {
        // This test ensures that continuation lines like "  1234)" don't trigger
        // MD030, which was causing an LSP formatting loop:
        // 1. MD030 would add a space → triggers MD009 (trailing space)
        // 2. MD009 fix removes trailing space → triggers MD030 again
        // 3. Infinite loop

        let rule = MD030ListMarkerSpace::default();
        let content = r#"- Text with citation (Author 2003, p.
  1234)"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0, "Should not trigger MD030 on continuation line");
    }

    // Comprehensive edge case tests for issue #253

    #[test]
    fn test_issue_253_blockquoted_citation_continuation() {
        // Blockquoted lists with citation continuations
        let rule = MD030ListMarkerSpace::default();
        let content = r#"> - Item with citation (Smith 2020, p.
>   456) in blockquote"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag continuation in blockquoted list. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_ordered_list_continuation() {
        // Ordered lists with citation continuations
        let rule = MD030ListMarkerSpace::default();
        let content = r#"1. First item with reference (Jones et al. 2019,
   pp. 123-125)
2. Second item"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag continuation in ordered list. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_mixed_nested_lists_with_continuation() {
        // Mixed ordered/unordered nested lists with continuations
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- Unordered item
  1. Nested ordered with citation (Author 2021,
     p. 789)
  2. Another nested item"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag continuation in nested mixed lists. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_multiple_continuations_same_item() {
        // Multiple continuation patterns in same item
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- Item with multiple citations (Ref1 2020,
  p. 100) and (Ref2 2021,
  p. 200) and (Ref3 2022,
  p. 300)"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag multiple continuations. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_deeply_nested_continuation() {
        // Continuation at various nesting levels
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- Level 1
  - Level 2 with citation (Author,
    p. 456)
    - Level 3 with citation (Another,
      p. 789)"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag continuations at different nesting levels. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_wrapped_url_continuation() {
        // Real-world: Wrapped URLs that look like list markers
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- See documentation at https://example.com/path/
  123456789/more/path
- Another item"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag wrapped URL continuations. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_enumerated_continuation() {
        // Wrapped enumerated lists within prose
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- The document lists three items: (1) first item, (2) second item, (3)
  345) which should not be treated as a list marker"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag enumerated prose continuations. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_mathematical_expression_continuation() {
        // Mathematical expressions with numbers and parentheses
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- Calculate using the formula (x + y) * (a + b) where x = 123 and y =
  456) to get the result"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag mathematical expression continuations. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_single_digit_continuation() {
        // Boundary case: Single-digit continuation
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- Text with reference (Page
  1)"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag single-digit continuations. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_very_long_number_continuation() {
        // Boundary case: Very long number sequence
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- ISBN reference (ISBN-13:
  9781234567890)"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag long number continuations. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_period_delimiter_continuation() {
        // Test period delimiter (1.) in continuations
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- Reference to section (Chapter 3, Section
  1. Introduction) for details"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag period delimiter in continuations. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_mixed_delimiters_continuation() {
        // Both ) and . delimiters in same continuation context
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- References: (1) Smith 2020, (2) Jones 2021, and section
  3. Additional notes"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag mixed delimiters in continuations. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_continuation_after_code_span() {
        // Continuation after inline code
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- Use `function(param1,
  param2)` to call"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag continuations after code spans. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_continuation_with_emphasis() {
        // Continuation with emphasis markers
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- See *important note (page
  123)* for details"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag continuations with emphasis. Got: {result:?}"
        );
    }

    #[test]
    fn test_issue_253_actual_nested_list_still_detected() {
        // NEGATIVE TEST: Actual nested list items should still be detected for spacing issues
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- Parent item
  1.Child without space"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            1,
            "Should still detect actual list items without proper spacing. Got: {result:?}"
        );
        assert_eq!(result[0].line, 2, "Error should be on the actual list item line");
    }

    #[test]
    fn test_issue_253_actual_list_after_continuation() {
        // Ensure actual list items after continuations are still checked
        let rule = MD030ListMarkerSpace::default();
        let content = r#"- Item with citation (Author,
  p. 123)
  1.Actual nested item without space"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            1,
            "Should detect actual list items after continuations. Got: {result:?}"
        );
        assert_eq!(result[0].line, 3, "Error should be on the nested list item");
    }

    // ========================================================================
    // User-intention-based detection: edge cases
    // ========================================================================

    #[test]
    fn test_emphasis_not_flagged_as_list() {
        // **bold** and similar patterns should NOT be flagged as list items
        let rule = MD030ListMarkerSpace::default();
        let content = "**bold text**\n--not a list--\n++also not++";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Emphasis patterns should not be flagged as list items. Got: {result:?}"
        );
    }

    #[test]
    fn test_fix_missing_space_unordered() {
        // Unordered markers without spaces are NOT fixed
        // They are not recognized as list items, too many non-list uses
        let rule = MD030ListMarkerSpace::default();
        let content = "*Item without space";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Unordered markers without space are not modified");
    }

    #[test]
    fn test_fix_missing_space_ordered() {
        // Verify fix adds missing space for ordered list items
        let rule = MD030ListMarkerSpace::default();
        let content = "1.First item\n2.Second item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "1. First item\n2. Second item",
            "Fix should add space after ordered markers"
        );
    }

    #[test]
    fn test_fix_preserves_valid_spacing() {
        // Valid list items should not be modified
        let rule = MD030ListMarkerSpace::default();
        let content = "* Valid item\n- Also valid\n1. Ordered valid";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Valid spacing should be preserved");
    }

    #[test]
    fn test_mixed_valid_and_invalid_spacing() {
        // Unordered markers without spaces are not flagged or fixed
        let rule = MD030ListMarkerSpace::default();
        let content = "* Valid\n*Invalid\n- Also valid\n-Also invalid";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            0,
            "Unordered markers without space are not flagged. Got: {result:?}"
        );

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Unordered markers without space are not modified");
    }

    #[test]
    fn test_special_characters_after_marker() {
        // Special characters that don't look like list content should not be flagged
        let rule = MD030ListMarkerSpace::default();
        // These don't look like intentional list items
        let content = "*#heading\n-=separator\n+!exclaim";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Special characters after marker should not be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_bracket_content_flagged() {
        // Unordered markers without spaces are not flagged, even if content looks like links
        let rule = MD030ListMarkerSpace::default();
        let content = "*[link](url)\n-[another](url2)";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            0,
            "Unordered markers without space are not flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_parentheses_content_flagged() {
        // Unordered markers without spaces are not flagged
        // Only ordered markers (1.) are flagged via heuristics
        let rule = MD030ListMarkerSpace::default();
        let content = "*(parenthetical)\n1.(also parenthetical)";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Only ordered markers without space are flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_blockquote_list_missing_space() {
        // Only ordered markers are flagged via heuristics
        // Unordered markers without spaces are not flagged
        let rule = MD030ListMarkerSpace::default();
        let content = "> *Item in blockquote\n> 1.Ordered in blockquote";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Only ordered markers in blockquotes are flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_large_ordered_number_missing_space() {
        // Large ordered list numbers with missing space
        let rule = MD030ListMarkerSpace::default();
        let content = "100.Hundredth item\n999.Nine ninety nine";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "Large ordered numbers missing space should be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_decimal_not_flagged() {
        // Decimal numbers should not be flagged (e.g., "3.14 is pi")
        let rule = MD030ListMarkerSpace::default();
        let content = "3.14 is pi\n2.5 is half of 5";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // These have space after the dot, so they're valid (if detected as lists) or not lists at all
        assert!(
            result.is_empty(),
            "Decimal numbers with space should not be flagged. Got: {result:?}"
        );
    }

    // ===== ROBUSTNESS EDGE CASE TESTS =====

    #[test]
    fn test_html_comments_skipped() {
        // List-like content inside HTML comments should not be flagged
        let rule = MD030ListMarkerSpace::default();
        let content = "<!-- *Item in comment -->\n<!-- -Another -->\n* Real item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Content inside HTML comments should not be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_multiline_html_comments_skipped() {
        // Multi-line HTML comments with list-like content
        let rule = MD030ListMarkerSpace::default();
        let content = "<!--\n*Item in comment\n-Another item\n-->\n* Real item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multi-line HTML comment content should not be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_signed_numbers_not_flagged() {
        // Signed numbers like -1, +1, -123 should not be flagged as list items
        let rule = MD030ListMarkerSpace::default();
        let content = "-1 is negative one\n+1 is positive one\n-123 is also negative";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Signed numbers should not be flagged as list items. Got: {result:?}"
        );
    }

    #[test]
    fn test_signed_numbers_fix_not_modified() {
        // Fix should not modify signed numbers
        let rule = MD030ListMarkerSpace::default();
        let content = "-1 is negative\n+1 is positive";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Signed numbers should not be modified by fix");
    }

    #[test]
    fn test_glob_patterns_not_flagged() {
        // Glob/filename patterns like *.txt should not be flagged
        let rule = MD030ListMarkerSpace::default();
        let content = "*.txt\n*.md\n*.rs";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Glob patterns should not be flagged as list items. Got: {result:?}"
        );
    }

    #[test]
    fn test_glob_patterns_fix_not_modified() {
        // Fix should not modify glob patterns
        let rule = MD030ListMarkerSpace::default();
        let content = "*.txt\n*.md";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Glob patterns should not be modified by fix");
    }

    #[test]
    fn test_mixed_valid_content_with_edge_cases() {
        // Mix of actual list items and edge cases
        let rule = MD030ListMarkerSpace::default();
        let content = "* Valid list item\n-1 is a number\n*.txt is a pattern\n- Another valid item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag valid items or edge cases. Got: {result:?}"
        );
    }

    #[test]
    fn test_html_comment_fix_preserves_content() {
        // Fix should preserve HTML comment content unchanged
        let rule = MD030ListMarkerSpace::default();
        let content = "<!--\n*  Extra spaces in comment\n-->\n*  Real item with extra spaces";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        let expected = "<!--\n*  Extra spaces in comment\n-->\n* Real item with extra spaces";
        assert_eq!(
            fixed, expected,
            "HTML comment content should be preserved, real items fixed"
        );
    }

    #[test]
    fn test_decimal_numbers_fix_not_modified() {
        // Decimal numbers like 3.14, 2.5 should not be modified by fix
        let rule = MD030ListMarkerSpace::default();
        let content = "3.14 is pi\n2.5 is half of 5\n10.25 is also a decimal";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Decimal numbers should not be modified by fix");
    }

    // ===== EMPHASIS DETECTION TESTS =====
    // These test proper detection of emphasis patterns to avoid false positives

    #[test]
    fn test_single_emphasis_not_flagged_as_list() {
        // Single emphasis like *italic* should NOT be flagged as list items
        let rule = MD030ListMarkerSpace::default();
        let content = "*Reading view* is the default\n*Another italic* phrase";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Single emphasis patterns should not be flagged as list items. Got: {result:?}"
        );
    }

    #[test]
    fn test_emphasis_in_blockquote_not_flagged() {
        // Emphasis inside blockquotes should NOT be flagged as list items
        // This was a real-world false positive from obsidian-help repo
        let rule = MD030ListMarkerSpace::default();
        let content = "> *Q1. How do I activate my license?*\n> *Q2. Can I try before paying?*";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Emphasis in blockquotes should not be flagged as list items. Got: {result:?}"
        );
    }

    #[test]
    fn test_emphasis_in_nested_blockquote_not_flagged() {
        // Nested blockquotes with emphasis
        let rule = MD030ListMarkerSpace::default();
        let content = "> > *Nested emphasis*\n> > > *Deeply nested*";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Emphasis in nested blockquotes should not be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_emphasis_fix_not_modified() {
        // Fix should not modify emphasis patterns
        let rule = MD030ListMarkerSpace::default();
        let content = "*Italic text*\n*Another italic*";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Emphasis patterns should not be modified by fix");
    }

    #[test]
    fn test_emphasis_in_blockquote_fix_not_modified() {
        // Fix should not modify emphasis in blockquotes
        let rule = MD030ListMarkerSpace::default();
        let content = "> *Italic in quote*\n> *Another italic*";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Emphasis in blockquotes should not be modified by fix");
    }

    #[test]
    fn test_actual_list_in_blockquote_still_flagged() {
        // Unordered markers without spaces are NOT flagged
        // They have too many non-list uses (emphasis, globs, diffs)
        let rule = MD030ListMarkerSpace::default();
        let content = "> *Item without space";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Not flagged - unordered markers without space are not flagged
        assert_eq!(
            result.len(),
            0,
            "Unordered markers without space are not flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_emphasis_vs_list_disambiguation() {
        // Mix of emphasis and actual list items
        let rule = MD030ListMarkerSpace::default();
        let content = "*italic text*\n* Valid list item\n*Another italic*";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should correctly distinguish emphasis from list items. Got: {result:?}"
        );
    }

    #[test]
    fn test_underscore_emphasis_not_flagged() {
        // Underscore emphasis patterns - these use _ not * so shouldn't interact
        // with list detection, but good to verify
        let rule = MD030ListMarkerSpace::default();
        let content = "_Italic text_\n_Another italic_";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Underscore emphasis should not trigger any issues. Got: {result:?}"
        );
    }

    #[test]
    fn test_mixed_emphasis_and_lists_in_blockquote() {
        // Real-world scenario: blockquote with both emphasis and actual lists
        let rule = MD030ListMarkerSpace::default();
        let content = "> *This is emphasis*\n> \n> * This is a list item\n> * Another item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should handle mixed emphasis and lists in blockquotes. Got: {result:?}"
        );
    }

    #[test]
    fn test_faq_callout_pattern_not_flagged() {
        // Real-world Obsidian FAQ pattern: `> [!FAQ]- Q1. Question`
        // The `[` after marker should be flagged as list needing space
        // But this tests the bracketed callout which has valid spacing
        let rule = MD030ListMarkerSpace::default();
        let content = "> [!FAQ]- Q1. How do I do this?\n> Answer here.\n>\n> [!FAQ]- Q2. Another question?";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "FAQ callout patterns should not be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_math_block_content_not_flagged() {
        // Issue #275: Lines starting with - inside math blocks should not be flagged
        // The -D in the LaTeX array is not a list item
        let rule = MD030ListMarkerSpace::default();
        let content = r#"# Heading

$$
A = \left[
\begin{array}{c}
1 \\
-D
\end{array}
\right]
$$"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Lines inside math blocks should not be flagged as list items. Got: {result:?}"
        );
    }

    #[test]
    fn test_math_block_with_multiple_dashes() {
        // More complex math block with multiple lines that could look like list items
        let rule = MD030ListMarkerSpace::default();
        let content = r#"# Math Example

$$
-x + y = z
-a - b = c
* not a list
+ also not a list
$$

Regular text after."#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Math block content with -, *, + should not be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_list_after_math_block_still_checked() {
        // Ensure lists AFTER math blocks are still properly checked
        let rule = MD030ListMarkerSpace::default();
        let content = r#"# Heading

$$
-x = y
$$

*  Too many spaces"#;
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "List after math block should still be checked. Got: {result:?}"
        );
        assert!(result[0].message.contains("Spaces after list markers"));
    }

    // ===== ORDERED LIST MARKER EDGE CASES =====
    // Tests for patterns that could be confused with ordered list markers

    #[test]
    fn test_double_digit_marker_without_space() {
        // Double-digit ordered markers without space should be flagged
        let rule = MD030ListMarkerSpace::default();
        let content = "10.First item\n11.Second item\n99.Last item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            3,
            "Double-digit markers without space should be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_version_numbers_not_flagged() {
        // Version numbers like 1.0.0 should NOT be flagged as list items
        // because they have multiple dots, not the single-dot list marker pattern
        let rule = MD030ListMarkerSpace::default();
        let content = "1.0.0 is a version\nv2.1.3 is another version\n10.20.30 is also a version";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Version numbers should not be flagged as list items. Got: {result:?}"
        );
    }

    #[test]
    fn test_ip_addresses_not_flagged() {
        // IP addresses like 192.168.1.1 should NOT be flagged
        let rule = MD030ListMarkerSpace::default();
        let content = "192.168.1.1 is localhost\n10.0.0.1 is gateway\n127.0.0.1 is loopback";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "IP addresses should not be flagged as list items. Got: {result:?}"
        );
    }

    #[test]
    fn test_zero_based_marker_without_space() {
        // 0. is a valid ordered list marker in CommonMark
        // so 0.text should be flagged as missing space
        let rule = MD030ListMarkerSpace::default();
        let content = "0.Zero based item";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Zero-based marker without space should be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_zero_padded_marker_without_space() {
        // Zero-padded markers like 00., 01. should be flagged
        let rule = MD030ListMarkerSpace::default();
        let content = "00.Zero padded\n01.Also padded\n007.James Bond";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            3,
            "Zero-padded markers without space should be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_date_format_not_flagged() {
        // Date-like patterns should NOT be flagged
        let rule = MD030ListMarkerSpace::default();
        let content = "2024.01.15 is a date\n2023.12.25 is Christmas";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Date-like patterns should not be flagged. Got: {result:?}"
        );
    }

    #[test]
    fn test_file_extensions_not_flagged() {
        // Patterns that look like file extensions should not be flagged
        let rule = MD030ListMarkerSpace::default();
        let content = ".md files are markdown\n.rs files are Rust\n.py files are Python";
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "File extension patterns should not be flagged. Got: {result:?}"
        );
    }
}
