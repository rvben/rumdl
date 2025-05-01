use rumdl::rule::Rule;
use rumdl::rules::{md004_unordered_list_style::UnorderedListStyle, MD004UnorderedListStyle};

#[test]
fn test_check_consistent_valid() {
    let content = "* Item 1\n* Item 2\n  * Nested item";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(content).unwrap();
    assert!(warnings.is_empty());
}

#[test]
fn test_check_consistent_invalid() {
    let content = "* Item 1\n- Item 2\n  + Nested item";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(content).unwrap();
    assert_eq!(warnings.len(), 2);
}

#[test]
fn test_check_specific_style_valid() {
    let content = "- Item 1\n- Item 2";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
    let warnings = rule.check(content).unwrap();
    assert!(warnings.is_empty());
}

#[test]
fn test_check_specific_style_invalid() {
    let content = "* Item 1\n- Item 2";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
    let warnings = rule.check(content).unwrap();
    assert_eq!(warnings.len(), 1);
}

#[test]
fn test_fix_consistent() {
    let content = "* Item 1\n- Item 2\n+ Item 3";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "* Item 1\n* Item 2\n* Item 3\n");
}

#[test]
fn test_fix_specific_style() {
    let content = "* Item 1\n- Item 2\n+ Item 3";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "* Item 1\n* Item 2\n* Item 3\n");
}

#[test]
fn test_fix_with_indentation() {
    let content = "  * Item 1\n    - Item 2";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "  * Item 1\n    * Item 2\n");
}

#[test]
fn test_check_skip_code_blocks() {
    let content = "```\n* Item 1\n- Item 2\n```\n* Item 3";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(content).unwrap();
    assert!(warnings.is_empty());
}

#[test]
fn test_check_skip_front_matter() {
    let content = "---\ntitle: Test\n---\n* Item 1\n- Item 2";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(content).unwrap();
    assert_eq!(warnings.len(), 1);
}

#[test]
fn test_fix_skip_code_blocks() {
    let content = "```\n* Item 1\n- Item 2\n```\n* Item 3\n- Item 4";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "```\n* Item 1\n- Item 2\n```\n* Item 3\n* Item 4\n");
}

#[test]
fn test_fix_skip_front_matter() {
    let content = "---\ntitle: Test\n---\n* Item 1\n- Item 2";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "---\ntitle: Test\n---\n* Item 1\n* Item 2\n");
}

#[test]
fn test_check_mixed_indentation() {
    let content = "* Item 1\n  - Sub Item 1\n* Item 2";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(content).unwrap();
    // Expect 1 warning because the simple consistent logic flags the nested item
    assert_eq!(warnings.len(), 1, "Should flag nested inconsistent marker with simple consistent logic");
    assert_eq!(warnings[0].line, 2);
    assert!(warnings[0].message.contains("marker '-' does not match expected style '*'"));
}

#[test]
fn test_check_consistent_first_marker_plus() {
    let content = "+ Item 1\n* Item 2\n- Item 3";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(content).unwrap();
    assert_eq!(warnings.len(), 2, "Should flag * and - when + is first");
}

#[test]
fn test_check_consistent_first_marker_dash() {
    let content = "- Item 1\n* Item 2\n+ Item 3";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(content).unwrap();
    assert_eq!(warnings.len(), 2, "Should flag * and + when - is first");
}

#[test]
fn test_fix_consistent_first_marker_plus() {
    let content = "+ Item 1\n* Item 2\n- Item 3";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "+ Item 1\n+ Item 2\n+ Item 3\n", "Should fix to + style");
}

#[test]
fn test_fix_consistent_first_marker_dash() {
    let content = "- Item 1\n* Item 2\n+ Item 3";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "- Item 1\n- Item 2\n- Item 3\n", "Should fix to - style");
}

#[test]
fn test_empty_content() {
    let content = "";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(content).unwrap();
    assert!(warnings.is_empty());
}

#[test]
fn test_no_list_items() {
    let content = "# Heading\nSome text";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let warnings = rule.check(content).unwrap();
    assert!(warnings.is_empty());
}

#[test]
fn test_md004_asterisk_style() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let content = "- Item 1\n+ Item 2\n  - Nested 1\n  + Nested 2\n* Item 3\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4); // All non-asterisk markers are flagged
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "* Item 1\n* Item 2\n  * Nested 1\n  * Nested 2\n* Item 3\n"
    );
}

#[test]
fn test_md004_plus_style() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Plus);
    let content = "- Item 1\n* Item 2\n  - Nested 1\n  * Nested 2\n+ Item 3\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4); // All non-plus markers are flagged
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "+ Item 1\n+ Item 2\n  + Nested 1\n  + Nested 2\n+ Item 3\n"
    );
}

#[test]
fn test_md004_dash_style() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
    let content = "* Item 1\n+ Item 2\n  * Nested 1\n  + Nested 2\n- Item 3\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4); // All non-dash markers are flagged
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "- Item 1\n- Item 2\n  - Nested 1\n  - Nested 2\n- Item 3\n"
    );
}

#[test]
fn test_md004_deeply_nested() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let content =
        "* Level 1\n  + Level 2\n    - Level 3\n      + Level 4\n  * Back to 2\n* Level 1\n";
    let mut result = rule.check(content).unwrap();
    result.sort_by_key(|w| w.line);
    // The most common marker is '*', so all others are flagged
    assert_eq!(result.len(), 3); // + Level 2, - Level 3, + Level 4
    assert_eq!(
        result.iter().map(|w| w.line).collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "* Level 1\n  * Level 2\n    * Level 3\n      * Level 4\n  * Back to 2\n* Level 1\n"
    );
    // Now test with a specific style: all non-matching markers should be flagged
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3); // All non-matching markers are flagged
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "* Level 1\n  * Level 2\n    * Level 3\n      * Level 4\n  * Back to 2\n* Level 1\n"
    );
}

#[test]
fn test_md004_mixed_content() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let content =
        "# Heading\n\n* Item 1\n  Some text\n  + Nested with text\n    More text\n* Item 2\n";
    let result = rule.check(content).unwrap();
    // The most common marker is '*', so only the '+' is flagged
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "# Heading\n\n* Item 1\n  Some text\n  * Nested with text\n    More text\n* Item 2\n"
    );
}

#[test]
fn test_md004_empty_content() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let content = "";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "\n");
}

#[test]
fn test_md004_no_lists() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let content = "# Heading\n\nSome text\nMore text";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Heading\n\nSome text\nMore text\n");
}

#[test]
fn test_md004_code_blocks() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let content = "* Item 1\n```\n* Not a list\n+ Also not a list\n```\n* Item 2\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "* Item 1\n```\n* Not a list\n+ Also not a list\n```\n* Item 2\n"
    );
}

#[test]
fn test_md004_blockquotes() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let content = "* Item 1\n> * Quoted item\n> + Another quoted item\n* Item 2\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "* Item 1\n> * Quoted item\n> + Another quoted item\n* Item 2\n"
    );
}

#[test]
fn test_md004_list_continuations() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let content = "* Item 1\n  Continuation 1\n  + Nested item\n    Continuation 2\n* Item 2\n";
    let result = rule.check(content).unwrap();
    // All unordered list items must match the first marker ('*')
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    // No markers are changed
    assert_eq!(
        fixed,
        "* Item 1\n  Continuation 1\n  * Nested item\n    Continuation 2\n* Item 2\n"
    );
}

#[test]
fn test_md004_mixed_ordered_unordered() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let content =
        "1. Ordered item\n   * Unordered sub-item\n   + Another sub-item\n2. Ordered item\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "1. Ordered item\n   * Unordered sub-item\n   * Another sub-item\n2. Ordered item\n"
    );
}

#[test]
fn test_complex_list_patterns() {
    // Test with different list marker styles in different levels
    let content = "* Level 1 item 1\n  - Level 2 item 1\n    + Level 3 item 1\n  - Level 2 item 2\n* Level 1 item 2";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let result = rule.check(content).unwrap();
    // All unordered list items must match the first marker ('*')
    assert_eq!(result.len(), 3); // - Level 2 item 1, + Level 3 item 1, - Level 2 item 2
    let flagged_lines: Vec<_> = result.iter().map(|w| w.line).collect();
    assert_eq!(flagged_lines, vec![2, 3, 4]);
    let fixed = rule.fix(content).unwrap();
    // All flagged markers are changed
    assert_eq!(
        fixed,
        "* Level 1 item 1\n  * Level 2 item 1\n    * Level 3 item 1\n  * Level 2 item 2\n* Level 1 item 2\n"
    );
}

#[test]
fn test_lists_in_code_blocks() {
    // Test lists inside code blocks (should be ignored)
    let content = "* Valid list item\n\n```\n* This is in a code block\n- Also in code block\n```\n\n* Another valid item";

    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0); // No warnings, code blocks ignored

    // Ensure fenced code blocks with language specifiers work too
    let content = "* Valid list item\n\n```markdown\n* This is in a code block\n- Also in code block\n```\n\n* Another valid item";

    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_nested_list_complexity() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let content = "* Item 1\n  - Item 2\n    + Item 3\n  - Item 5\n* Item 6\n";
    let result = rule.check(content).unwrap();
    // All unordered list items must match the first marker ('*')
    assert_eq!(result.len(), 3); // - Item 2, + Item 3, - Item 5
    let flagged_lines: Vec<_> = result.iter().map(|w| w.line).collect();
    assert_eq!(flagged_lines, vec![2, 3, 4]);
    let fixed = rule.fix(content).unwrap();
    // All flagged markers are changed
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
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0); // Should handle different indentation levels

    // Non-list content with asterisks
    let content =
        "* Actual list item\nText with * asterisk that's not a list\n  * Indented list item";

    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0); // Asterisk in middle of line isn't a list marker
}

#[test]
fn test_fix_list_markers() {
    // Test that fix correctly standardizes list markers
    let content = "* First item\n- Second item\n+ Third item";

    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "* First item\n* Second item\n* Third item\n");

    // Test fix with specified style
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "- First item\n- Second item\n- Third item\n");

    // Test with trailing newline preserved
    let content = "* First item\n- Second item\n+ Third item\n";
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Plus);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "+ First item\n+ Second item\n+ Third item\n");
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
    let result = rule.check(&content).unwrap();
    let _check_duration = start.elapsed();

    let start = std::time::Instant::now();
    let _ = rule.fix(&content).unwrap();
    let _fix_duration = start.elapsed();

    // We expect many warnings due to mixed markers
    assert!(!result.is_empty());
}
