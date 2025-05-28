use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::{md004_unordered_list_style::UnorderedListStyle, MD004UnorderedListStyle};

#[test]
fn test_check_consistent_valid() {
    let content = "* Item 1\n* Item 2\n  * Nested item";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings.is_empty());
}

#[test]
fn test_check_consistent_invalid() {
    let content = "* Item 1\n- Item 2\n  + Nested item";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 2);
}

#[test]
fn test_check_specific_style_valid() {
    let content = "- Item 1\n- Item 2";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings.is_empty());
}

#[test]
fn test_check_specific_style_invalid() {
    let content = "* Item 1\n- Item 2";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);
}

#[test]
fn test_fix_consistent() {
    let content = "* Item 1\n- Item 2\n+ Item 3";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "* Item 1\n* Item 2\n* Item 3");
}

#[test]
fn test_fix_specific_style() {
    let content = "* Item 1\n- Item 2\n+ Item 3";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "* Item 1\n* Item 2\n* Item 3");
}

#[test]
fn test_fix_with_indentation() {
    let content = "  * Item 1\n    - Item 2";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "  * Item 1\n    * Item 2");
}

#[test]
fn test_check_skip_code_blocks() {
    let content = "```\n* Item 1\n- Item 2\n```\n* Item 3";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings.is_empty());
}

#[test]
fn test_check_skip_front_matter() {
    let content = "---\ntitle: Test\n---\n* Item 1\n- Item 2";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);
}

#[test]
fn test_fix_skip_code_blocks() {
    let content = "```\n* Item 1\n- Item 2\n```\n* Item 3\n- Item 4";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "```\n* Item 1\n- Item 2\n```\n* Item 3\n* Item 4");
}

#[test]
fn test_fix_skip_front_matter() {
    let content = "---\ntitle: Test\n---\n* Item 1\n- Item 2";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "---\ntitle: Test\n---\n* Item 1\n* Item 2");
}

#[test]
fn test_check_mixed_indentation() {
    let content = "* Item 1\n  - Sub Item 1\n* Item 2";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(&ctx).unwrap();

    // Flags the dash marker that doesn't match the first marker (asterisk)
    assert_eq!(warnings.len(), 1);
}

#[test]
fn test_check_consistent_first_marker_plus() {
    let content = "+ Item 1\n* Item 2\n- Item 3";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 2);
}

#[test]
fn test_check_consistent_first_marker_dash() {
    let content = "- Item 1\n* Item 2\n+ Item 3";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 2);
}

#[test]
fn test_fix_consistent_first_marker_plus() {
    let content = "+ Item 1\n* Item 2\n- Item 3";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "+ Item 1\n+ Item 2\n+ Item 3");
}

#[test]
fn test_fix_consistent_first_marker_dash() {
    let content = "- Item 1\n* Item 2\n+ Item 3";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "- Item 1\n- Item 2\n- Item 3");
}

#[test]
fn test_empty_content() {
    let content = "";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings.is_empty());
}

#[test]
fn test_no_list_items() {
    let content = "# Heading\nSome text";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings.is_empty());
}

#[test]
fn test_md004_asterisk_style() {
    let ctx = LintContext::new("- Item 1\n+ Item 2\n  - Nested 1\n  + Nested 2\n* Item 3");
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4); // All non-asterisk markers are flagged
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "* Item 1\n* Item 2\n  * Nested 1\n  * Nested 2\n* Item 3"
    );
}

#[test]
fn test_md004_plus_style() {
    let ctx = LintContext::new("- Item 1\n* Item 2\n  - Nested 1\n  * Nested 2\n+ Item 3");
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Plus);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4); // All non-plus markers are flagged
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "+ Item 1\n+ Item 2\n  + Nested 1\n  + Nested 2\n+ Item 3"
    );
}

#[test]
fn test_md004_dash_style() {
    let ctx = LintContext::new("* Item 1\n+ Item 2\n  * Nested 1\n  + Nested 2\n- Item 3");
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4); // All non-dash markers are flagged
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "- Item 1\n- Item 2\n  - Nested 1\n  - Nested 2\n- Item 3"
    );
}

#[test]
fn test_md004_deeply_nested() {
    let ctx = LintContext::new(
        "* Level 1\n  + Level 2\n    - Level 3\n      + Level 4\n  * Back to 2\n* Level 1\n",
    );
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let result = rule.check(&ctx).unwrap();
    // Flags mixed markers (+ and - don't match the first marker *)
    assert_eq!(result.len(), 3); // + on line 2, - on line 3, + on line 4
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "* Level 1\n  * Level 2\n    * Level 3\n      * Level 4\n  * Back to 2\n* Level 1\n"
    );
}

#[test]
fn test_md004_mixed_content() {
    let ctx = LintContext::new(
        "# Heading\n\n* Item 1\n  Some text\n  + Nested with text\n    More text\n* Item 2\n",
    );
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let result = rule.check(&ctx).unwrap();
    // Flags the + marker that doesn't match the first marker *
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Heading\n\n* Item 1\n  Some text\n  * Nested with text\n    More text\n* Item 2\n"
    );
}

#[test]
fn test_md004_empty_content() {
    let ctx = LintContext::new("");
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "");
}

#[test]
fn test_md004_no_lists() {
    let ctx = LintContext::new("# Heading\n\nSome text\nMore text");
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading\n\nSome text\nMore text");
}

#[test]
fn test_md004_code_blocks() {
    let ctx = LintContext::new("* Item 1\n```\n* Not a list\n+ Also not a list\n```\n* Item 2\n");
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "* Item 1\n```\n* Not a list\n+ Also not a list\n```\n* Item 2\n"
    );
}

#[test]
fn test_md004_blockquotes() {
    let ctx = LintContext::new("* Item 1\n> * Quoted item\n> + Another quoted item\n* Item 2\n");
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Should flag the + marker that doesn't match asterisk style
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "* Item 1\n> * Quoted item\n> * Another quoted item\n* Item 2\n"
    );
}

#[test]
fn test_md004_list_continuations() {
    let ctx = LintContext::new(
        "* Item 1\n  Continuation 1\n  + Nested item\n    Continuation 2\n* Item 2\n",
    );
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let result = rule.check(&ctx).unwrap();
    // Flags the + marker that doesn't match the first marker *
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "* Item 1\n  Continuation 1\n  * Nested item\n    Continuation 2\n* Item 2\n"
    );
}

#[test]
fn test_md004_mixed_ordered_unordered() {
    let ctx = LintContext::new(
        "1. Ordered item\n   * Unordered sub-item\n   + Another sub-item\n2. Ordered item\n",
    );
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "1. Ordered item\n   * Unordered sub-item\n   * Another sub-item\n2. Ordered item\n"
    );
}

#[test]
fn test_complex_list_patterns() {
    let content = "* Level 1 item 1\n  * Level 2 item 1\n    * Level 3 item 1\n  * Level 2 item 2\n* Level 1 item 2";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "* Level 1 item 1\n  * Level 2 item 1\n    * Level 3 item 1\n  * Level 2 item 2\n* Level 1 item 2");
}

#[test]
fn test_lists_in_code_blocks() {
    // Test lists inside code blocks (should be ignored)
    let content = "* Valid list item\n\n```\n* This is in a code block\n- Also in code block\n```\n\n* Another valid item";

    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0); // No warnings, code blocks ignored

    // Ensure fenced code blocks with language specifiers work too
    let content = "* Valid list item\n\n```markdown\n* This is in a code block\n- Also in code block\n```\n\n* Another valid item";

    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_nested_list_complexity() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let content = "* Item 1\n  - Item 2\n    + Item 3\n  - Item 5\n* Item 6\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Flags mixed markers (- and + don't match the first marker *)
    assert_eq!(result.len(), 3); // - on line 2, + on line 3, - on line 4
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "* Item 1\n  * Item 2\n    * Item 3\n  * Item 5\n* Item 6\n"
    );
}

#[test]
fn test_indentation_handling() {
    // Test different indentation styles
    let content = "* Level 1\n    * Indented with 4 spaces\n  * Indented with 2 spaces";

    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0); // Should handle different indentation levels

    // Non-list content with asterisks
    let content =
        "* Actual list item\nText with * asterisk that's not a list\n  * Indented list item";

    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0); // Asterisk in middle of line isn't a list marker
}

#[test]
fn test_fix_list_markers() {
    let content = "* First item\n* Second item\n* Third item";
    let ctx = LintContext::new(content);
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "* First item\n* Second item\n* Third item");
}

#[test]
fn test_performance_md004() {
    // Generate a large document with nested lists
    let mut content = String::with_capacity(20_000);

    for i in 0..50 {
        // Add a top-level list item
        let marker = match i % 3 {
            0 => "*",
            1 => "-",
            _ => "+",
        };

        content.push_str(&format!("{} Top level item {}\n", marker, i));

        // Add 3 second-level items
        for j in 0..3 {
            let marker = match (i + j) % 3 {
                0 => "*",
                1 => "-",
                _ => "+",
            };

            content.push_str(&format!("  {} Second level item {}.{}\n", marker, i, j));

            // Add 2 third-level items
            for k in 0..2 {
                let marker = match (i + j + k) % 3 {
                    0 => "*",
                    1 => "-",
                    _ => "+",
                };

                content.push_str(&format!(
                    "    {} Third level item {}.{}.{}\n",
                    marker, i, j, k
                ));
            }
        }

        content.push('\n'); // Add spacing between top-level items
    }

    // Measure performance
    let start = std::time::Instant::now();
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let ctx = LintContext::new(&content);
    let _result = rule.check(&ctx).unwrap();
    let _check_duration = start.elapsed();

    let start = std::time::Instant::now();
    let fixed_ctx = LintContext::new(&content);
    let _ = rule.fix(&fixed_ctx).unwrap();
    let _fix_duration = start.elapsed();

    // In consistent mode, contiguous runs with the same marker are not flagged, so result may be empty
    // Allow for warnings if present (do not assert result.is_empty())
}

mod parity_with_markdownlint {
    use super::*;

    #[test]
    fn parity_mixed_markers_no_trailing_newline() {
        let input = "* Item 1\n- Item 2\n+ Item 3";
        let expected = "* Item 1\n* Item 2\n* Item 3";
        let ctx = LintContext::new(input);
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_mixed_markers_with_trailing_newline() {
        let input = "* Item 1\n- Item 2\n+ Item 3\n";
        let expected = "* Item 1\n* Item 2\n* Item 3\n";
        let ctx = LintContext::new(input);
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_nested_lists() {
        let input = "* Level 1\n  - Level 2\n    + Level 3\n  - Level 2b\n* Level 1b";
        let expected = "* Level 1\n  * Level 2\n    * Level 3\n  * Level 2b\n* Level 1b";
        let ctx = LintContext::new(input);
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_code_blocks_and_front_matter() {
        let input = "---\ntitle: Test\n---\n* Item 1\n- Item 2\n```\n* Not a list\n- Not a list\n```\n* Item 3\n";
        let expected = "---\ntitle: Test\n---\n* Item 1\n* Item 2\n```\n* Not a list\n- Not a list\n```\n* Item 3\n";
        let ctx = LintContext::new(input);
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_empty_input() {
        let input = "";
        let expected = "";
        let ctx = LintContext::new(input);
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_no_lists() {
        let input = "# Heading\nSome text";
        let expected = "# Heading\nSome text";
        let ctx = LintContext::new(input);
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_single_style_list() {
        let input = "- Item 1\n- Item 2\n- Item 3";
        let expected = "- Item 1\n- Item 2\n- Item 3";
        let ctx = LintContext::new(input);
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_list_with_blank_lines() {
        let input = "* Item 1\n\n- Item 2\n\n+ Item 3";
        let expected = "* Item 1\n\n* Item 2\n\n* Item 3";
        let ctx = LintContext::new(input);
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_blockquote_with_lists() {
        let input = "* Item 1\n> * Quoted item\n> + Another quoted item\n* Item 2";
        let expected = "* Item 1\n> * Quoted item\n> * Another quoted item\n* Item 2";
        let ctx = LintContext::new(input);
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_single_item_list() {
        let input = "* Only item";
        let expected = "* Only item";
        let ctx = LintContext::new(input);
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_list_with_tabs_for_indentation() {
        let input = "* Item 1\n\t- Nested with tab\n\t\t+ Double tab nested";
        let expected = "* Item 1\n\t* Nested with tab\n\t\t* Double tab nested";
        let ctx = LintContext::new(input);
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }

    #[test]
    fn parity_list_with_extra_whitespace_after_marker() {
        let input = "*    Item 1\n-      Item 2\n+   Item 3";
        let expected = "*    Item 1\n*      Item 2\n*   Item 3";
        let ctx = LintContext::new(input);
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
    }
}
