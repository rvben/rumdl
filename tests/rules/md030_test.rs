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
            assert_eq!(warning.message, "Spaces after list markers");
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
            assert_eq!(warning.message, "Spaces after list markers");
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
        assert_eq!(result[0].message, "Spaces after list markers");
    }

    #[test]
    fn test_real_world_tab_after_marker() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*	[benjamingr](https://github.com/benjamingr) -\n    **Benjamin Gruenbaum** <<benjamingr@gmail.com>>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Spaces after list markers");
    }

    #[test]
    fn test_real_world_nested_extra_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content = "  *   [nested](https://github.com/nested) -\n      **Nested User** <<nested@example.com>>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Spaces after list markers");
    }

    #[test]
    fn test_real_world_multiline_extra_spaces() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*   [multi](https://github.com/multi) -\n    **Multi Line**\n    <<multi@example.com>>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Spaces after list markers");
    }

    #[test]
    fn test_real_world_three_spaces_after_marker() {
        let rule = MD030ListMarkerSpace::default();
        let content = "*   [geeksilva97](https://github.com/geeksilva97) -";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Spaces after list markers");
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
}
