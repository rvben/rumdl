use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD010NoHardTabs;

#[test]
fn test_no_hard_tabs() {
    let rule = MD010NoHardTabs::default();
    let content = "This line is fine\n    Indented with spaces";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_content_with_no_tabs_various_contexts() {
    let rule = MD010NoHardTabs::default();

    // Test various content without tabs
    let content = "# Heading without tabs\n\n    Indented with spaces\n\n- List item\n  - Nested with spaces\n\n```\nCode without tabs\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Content with only spaces should pass");
}

#[test]
fn test_leading_hard_tabs() {
    let rule = MD010NoHardTabs::default();
    let content = "\tIndented line\n\t\tDouble indented";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // One warning per line (grouped consecutive tabs)
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].message, "Found leading tab, use 4 spaces instead");
    assert_eq!(result[1].line, 2);
    assert_eq!(result[1].message, "Found 2 leading tabs, use 8 spaces instead");
}

#[test]
fn test_alignment_tabs() {
    let rule = MD010NoHardTabs::default();
    let content = "Text with\ttab for alignment";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].message, "Found tab for alignment, use spaces instead");
}

#[test]
fn test_empty_line_tabs() {
    let rule = MD010NoHardTabs::default();
    let content = "Normal line\n\t\t\n\tMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Line 3 starts with tab after blank line, so it's an indented code block and is skipped
    assert_eq!(result.len(), 1); // Only the empty line with tabs
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "Empty line contains 2 tabs");
}

#[test]
fn test_code_blocks_allowed() {
    let rule = MD010NoHardTabs::new(4);
    let content = "Normal line\n```\n\tCode with tab\n\tMore code\n```\nNormal\tline";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the tab outside code block is flagged
    assert_eq!(result[0].line, 6);
}

#[test]
fn test_code_blocks_not_allowed() {
    let rule = MD010NoHardTabs::default(); // code blocks are always skipped now
    let content = "Normal line\n```\n\tCode with tab\n\tMore code\n```\nNormal\tline";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only tab outside code block is flagged
    assert_eq!(result[0].line, 6);
}

#[test]
fn test_fix_with_code_blocks() {
    let rule = MD010NoHardTabs::new(2); // 2 spaces per tab, preserve code blocks
    let content = "\tIndented line\n```\n\tCode\n```\n\t\tDouble indented";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "  Indented line\n```\n\tCode\n```\n    Double indented");
}

#[test]
fn test_fix_without_code_blocks() {
    let rule = MD010NoHardTabs::new(2); // 2 spaces per tab, code blocks always preserved
    let content = "\tIndented line\n```\n\tCode\n```\n\t\tDouble indented";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "  Indented line\n```\n\tCode\n```\n    Double indented");
}

#[test]
fn test_mixed_indentation() {
    let rule = MD010NoHardTabs::default();
    let content = "    Spaces\n\tTab\n  \tMixed";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[1].line, 3);
}

#[test]
fn test_html_comments_with_tabs() {
    let rule = MD010NoHardTabs::default();

    // Single line HTML comment with tabs
    let content = "<!-- This comment has a \t tab -->\nNormal line";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Should ignore tabs in single-line HTML comments");

    // Multi-line HTML comment with tabs
    let content = "<!-- Start of comment\nUser: \t\tuser\nPassword:\tpass\n-->\nNormal\tline";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should only flag tab in normal line, not in multi-line comment"
    );
    assert_eq!(result[0].line, 5);

    // Test fix for content with HTML comments
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, "<!-- Start of comment\nUser: \t\tuser\nPassword:\tpass\n-->\nNormal    line",
        "Should preserve tabs in HTML comments but fix tabs in normal text"
    );
}

#[test]
fn test_md010_tabs_in_nested_code_blocks() {
    // Test tabs in various code block contexts
    let rule = MD010NoHardTabs::new(4); // Don't check code blocks

    // Note: The last line has a blank line before it and starts with tab, so it's an indented code block
    let content = "No\ttabs\there\n\n```\n\tTabs\tin\tcode\n```\n\nRegular\ttext\twith\ttabs";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();

    assert!(
        fixed.contains("No    tabs    here"),
        "Tabs outside code should be replaced"
    );
    assert!(
        fixed.contains("\tTabs\tin\tcode"),
        "Tabs in fenced code should be preserved"
    );
    assert!(
        fixed.contains("Regular    text    with    tabs"),
        "Tabs in regular text should be replaced"
    );
}

#[test]
fn test_md010_tabs_in_indented_code() {
    let rule = MD010NoHardTabs::new(4);

    let content = "Text\n\n\t\tCode with tabs\n\t\tMore code\n\nText\twith\ttab";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();

    assert!(
        fixed.contains("\t\tCode with tabs"),
        "Tabs in indented code should be preserved"
    );
    assert!(
        fixed.contains("Text    with    tab"),
        "Tabs outside code should be replaced"
    );
}

#[test]
fn test_md010_mixed_indentation_in_code() {
    let rule = MD010NoHardTabs::new(2);

    let content = "```python\n  spaces\n\ttab\n  \tmixed\n```\n\nOutside\ttab";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();

    // Code block content should be preserved exactly
    assert!(
        fixed.contains("  spaces\n\ttab\n  \tmixed"),
        "Mixed indentation in code preserved"
    );
    assert!(fixed.contains("Outside  tab"), "Tab outside converted to 2 spaces");
}

#[test]
fn test_interaction_list_code_tabs() {
    // Test tabs in list items and code blocks
    let content = r#"1. List	with	tab

   ```
   	Code with tab
   ```

2. Wrong	number	here"#;

    // Test MD010 - tabs in list items are replaced, tabs in code blocks are preserved
    let rule_tabs = MD010NoHardTabs::new(4);
    let ctx = LintContext::new(content);
    let fixed_tabs = rule_tabs.fix(&ctx).unwrap();

    // Expected: tabs in list items are replaced with spaces, tabs in code blocks preserved
    let expected = r#"1. List    with    tab

   ```
   	Code with tab
   ```

2. Wrong    number    here"#;

    assert_eq!(
        fixed_tabs, expected,
        "Tabs in list items should be replaced, code block tabs preserved"
    );
}

#[test]
fn test_multiple_tabs_on_same_line() {
    let rule = MD010NoHardTabs::default();

    // Test with multiple separate tabs on same line
    let content = "Start\there\tand\there\twith\ttabs";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 5, "Should detect each tab separately");

    // Verify each warning
    for warning in result.iter() {
        assert_eq!(warning.line, 1);
        assert_eq!(warning.message, "Found tab for alignment, use spaces instead");
    }
}

#[test]
fn test_tab_character_in_different_positions() {
    let rule = MD010NoHardTabs::default();

    // Test tabs at start, middle, and end
    let content = "\tStart tab\nMiddle\ttab\nEnd tab\t\n\t\tDouble start\nMixed \t \t spaces";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 6, "Should detect all tabs");
    assert_eq!(result[0].message, "Found leading tab, use 4 spaces instead");
    assert_eq!(result[1].message, "Found tab for alignment, use spaces instead");
    assert_eq!(result[2].message, "Found tab for alignment, use spaces instead");
    assert_eq!(result[3].message, "Found 2 leading tabs, use 8 spaces instead");
    assert_eq!(result[4].message, "Found tab for alignment, use spaces instead");
    assert_eq!(result[5].message, "Found tab for alignment, use spaces instead");
}

#[test]
fn test_mixed_tabs_and_spaces_detailed() {
    let rule = MD010NoHardTabs::default();

    // Various mixed indentation patterns
    let content =
        "  \tTwo spaces then tab\n\t  Tab then two spaces\n \t \t Space tab space tab\n\t\t  Two tabs then spaces";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 5, "Should detect all tabs");

    // Fix test
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "      Two spaces then tab\n      Tab then two spaces\n           Space tab space tab\n          Two tabs then spaces"
    );
}

#[test]
fn test_empty_lines_with_only_tabs_variations() {
    let rule = MD010NoHardTabs::default();

    // Various empty line patterns
    let content = "\t\n\t\t\n\t\t\t\n\t \t\n \t \t \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 7, "Should detect all tab groups");
    assert_eq!(result[0].message, "Empty line contains tab");
    assert_eq!(result[1].message, "Empty line contains 2 tabs");
    assert_eq!(result[2].message, "Empty line contains 3 tabs");

    // Mixed spaces and tabs on empty lines
    assert_eq!(result[3].message, "Empty line contains tab");
    assert_eq!(result[4].message, "Empty line contains tab");
    assert_eq!(result[5].message, "Empty line contains tab");
    assert_eq!(result[6].message, "Empty line contains tab");
}

#[test]
fn test_configuration_spaces_per_tab() {
    // Test different spaces_per_tab configurations
    let content = "\tOne tab\n\t\tTwo tabs\n\t\t\tThree tabs";

    // Test with 2 spaces per tab
    let rule2 = MD010NoHardTabs::new(2);
    let ctx = LintContext::new(content);
    let fixed2 = rule2.fix(&ctx).unwrap();
    assert_eq!(fixed2, "  One tab\n    Two tabs\n      Three tabs");

    // Test with 8 spaces per tab
    let rule8 = MD010NoHardTabs::new(8);
    let fixed8 = rule8.fix(&ctx).unwrap();
    assert_eq!(
        fixed8,
        "        One tab\n                Two tabs\n                        Three tabs"
    );
}

#[test]
fn test_configuration_code_blocks_parameter() {
    let content = "Normal\ttab\n\n```javascript\nfunction\tfoo() {\n\treturn\ttrue;\n}\n```\n\nAnother\ttab";

    // Code blocks are always skipped now
    let rule = MD010NoHardTabs::new(4);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Should always skip tabs in code blocks");
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 9);

    // Verify fix behavior
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("function\tfoo()"), "Should preserve tabs in code blocks");
    assert!(fixed.contains("Normal    tab"), "Should fix tabs outside code blocks");
}

#[test]
fn test_consecutive_vs_separate_tabs() {
    let rule = MD010NoHardTabs::default();

    // Test grouping of consecutive tabs
    let content = "\t\t\tThree consecutive\nOne\tthen\tanother\t";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 4, "Should have 1 group for consecutive, 3 separate");
    assert_eq!(result[0].message, "Found 3 leading tabs, use 12 spaces instead");
    assert_eq!(result[1].message, "Found tab for alignment, use spaces instead");
    assert_eq!(result[2].message, "Found tab for alignment, use spaces instead");
    assert_eq!(result[3].message, "Found tab for alignment, use spaces instead");
}

#[test]
fn test_fix_preserves_content_structure() {
    let rule = MD010NoHardTabs::default();

    // Complex content with various elements
    let content = "# Header\n\n\tIndented paragraph\n\n- List\n\t- Nested\n\t\t- Double nested\n\n```\n\tCode block\n```\n\n> Quote\n> \tWith tab\n\n| Col1\t| Col2\t|\n|---\t|---\t|\n| Data\t| Data\t|";

    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();

    // Verify structure is preserved
    assert!(fixed.contains("# Header"), "Headers preserved");
    // After blank line, tab-indented line is treated as indented code block and preserved
    assert!(fixed.contains("\tIndented paragraph"), "Indented code block preserved");
    assert!(fixed.contains("    - Nested"), "List indentation converted");
    assert!(
        fixed.contains("        - Double nested"),
        "Double indentation converted"
    );
    // Code blocks are always preserved now
    assert!(fixed.contains("\tCode block"), "Code block tabs preserved");
    assert!(fixed.contains(">     With tab"), "Quote tab converted");
    assert!(fixed.contains("| Col1    | Col2    |"), "Table tabs converted");
}

#[test]
fn test_edge_cases() {
    let rule = MD010NoHardTabs::default();

    // Test edge cases
    let content = "\t"; // Single tab only
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].message, "Empty line contains tab");

    // Test file ending with tab
    let content2 = "Text\t";
    let ctx2 = LintContext::new(content2);
    let result2 = rule.check(&ctx2).unwrap();
    assert_eq!(result2.len(), 1);

    // Test fix preserves lack of final newline
    let fixed2 = rule.fix(&ctx2).unwrap();
    assert_eq!(fixed2, "Text    ");
    assert!(!fixed2.ends_with('\n'), "Should preserve lack of final newline");
}

#[test]
fn test_inline_code_spans() {
    let rule = MD010NoHardTabs::new(4);

    // Test tabs in inline code spans
    let content = "Text with `inline\tcode` and\ttab outside";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should detect both tabs (inline code spans are not excluded like code blocks)
    assert_eq!(result.len(), 2, "Should detect tabs in inline code and outside");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Text with `inline    code` and    tab outside");
}
