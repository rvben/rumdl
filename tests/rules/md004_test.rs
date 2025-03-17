use rumdl::rule::Rule;
use rumdl::rules::{md004_unordered_list_style::UnorderedListStyle, MD004UnorderedListStyle};

#[test]
fn test_md004_consistent_valid() {
    let rule = MD004UnorderedListStyle::default();
    let content = "* Item 1\n* Item 2\n  * Nested 1\n  * Nested 2\n* Item 3\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md004_consistent_invalid() {
    let rule = MD004UnorderedListStyle::default();
    let content = "* Item 1\n+ Item 2\n  - Nested 1\n  * Nested 2\n- Item 3\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[1].line, 3);
    assert_eq!(result[2].line, 5);
}

#[test]
fn test_md004_asterisk_style() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let content = "- Item 1\n+ Item 2\n  - Nested 1\n  + Nested 2\n* Item 3\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
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
    assert_eq!(result.len(), 4);
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
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "- Item 1\n- Item 2\n  - Nested 1\n  - Nested 2\n- Item 3\n"
    );
}

#[test]
fn test_md004_deeply_nested() {
    let rule = MD004UnorderedListStyle::default();
    let content =
        "* Level 1\n  + Level 2\n    - Level 3\n      * Level 4\n  * Back to 2\n* Level 1\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "* Level 1\n  * Level 2\n    * Level 3\n      * Level 4\n  * Back to 2\n* Level 1\n"
    );
}

#[test]
fn test_md004_mixed_content() {
    let rule = MD004UnorderedListStyle::default();
    let content =
        "# Heading\n\n* Item 1\n  Some text\n  + Nested with text\n    More text\n* Item 2\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "# Heading\n\n* Item 1\n  Some text\n  * Nested with text\n    More text\n* Item 2\n"
    );
}

#[test]
fn test_md004_empty_content() {
    let rule = MD004UnorderedListStyle::default();
    let content = "";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "");
}

#[test]
fn test_md004_no_lists() {
    let rule = MD004UnorderedListStyle::default();
    let content = "# Heading\n\nSome text\nMore text\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Heading\n\nSome text\nMore text\n");
}

#[test]
fn test_md004_code_blocks() {
    let rule = MD004UnorderedListStyle::default();
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
    let rule = MD004UnorderedListStyle::default();
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
    let rule = MD004UnorderedListStyle::default();
    let content = "* Item 1\n  Continuation 1\n  + Nested item\n    Continuation 2\n* Item 2\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "* Item 1\n  Continuation 1\n  * Nested item\n    Continuation 2\n* Item 2\n"
    );
}

#[test]
fn test_md004_mixed_ordered_unordered() {
    let rule = MD004UnorderedListStyle::default();
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

    // With Consistent style (default), we follow the first marker
    let rule = MD004UnorderedListStyle::default();
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3); // The - and + markers should be flagged

    // With Asterisk style, only the asterisks are valid
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3); // The - and + markers should be flagged

    // With Dash style, only the dashes are valid
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3); // The * and + markers should be flagged

    // With Plus style, only the plus signs are valid
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Plus);
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4); // The * and - markers should be flagged
}

#[test]
fn test_lists_in_code_blocks() {
    // Test lists inside code blocks (should be ignored)
    let content = "* Valid list item\n\n```\n* This is in a code block\n- Also in code block\n```\n\n* Another valid item";

    let rule = MD004UnorderedListStyle::default();
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0); // No warnings, code blocks ignored

    // Ensure fenced code blocks with language specifiers work too
    let content = "* Valid list item\n\n```markdown\n* This is in a code block\n- Also in code block\n```\n\n* Another valid item";

    let rule = MD004UnorderedListStyle::default();
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_nested_list_complexity() {
    // Test complex nested lists with mixed content
    let content = "* Level 1 item 1\n  * Level 2 item 1\n    * Level 3 item 1\n* Level 1 item 2\n  * Level 2 item 2\n    * Level 3 in **bold**\n      * Level 4 with `code`";

    // All consistent, should be valid
    let rule = MD004UnorderedListStyle::default();
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);

    // With complex mixed content and multiple nesting levels
    let content = "* Top level\n  - Mixed marker\n    + Another mixed marker\n  * Back to asterisk\n* Final item";

    // With Consistent style, first marker (*) should be used
    let rule = MD004UnorderedListStyle::default();
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2); // The - and + should be flagged

    // Complex case with code spans, bold text, etc.
    let content = "* Item with `code`\n* Item with **bold**\n- Mixed marker with _emphasis_";

    let rule = MD004UnorderedListStyle::default();
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1); // The - should be flagged
}

#[test]
fn test_indentation_handling() {
    // Test different indentation styles
    let content = "* Level 1\n    * Indented with 4 spaces\n  * Indented with 2 spaces";

    let rule = MD004UnorderedListStyle::default();
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0); // Should handle different indentation levels

    // Non-list content with asterisks
    let content =
        "* Actual list item\nText with * asterisk that's not a list\n  * Indented list item";

    let rule = MD004UnorderedListStyle::default();
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0); // Asterisk in middle of line isn't a list marker
}

#[test]
fn test_fix_list_markers() {
    // Test that fix correctly standardizes list markers
    let content = "* First item\n- Second item\n+ Third item";

    let rule = MD004UnorderedListStyle::default();
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
    let rule = MD004UnorderedListStyle::default();
    let result = rule.check(&content).unwrap();
    let check_duration = start.elapsed();

    let start = std::time::Instant::now();
    let _ = rule.fix(&content).unwrap();
    let fix_duration = start.elapsed();

    println!(
        "MD004 check duration: {:?} for content length {}",
        check_duration,
        content.len()
    );
    println!("MD004 fix duration: {:?}", fix_duration);
    println!("Found {} warnings", result.len());

    // We expect many warnings due to mixed markers
    assert!(!result.is_empty());
}
