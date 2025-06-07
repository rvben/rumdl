use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD010NoHardTabs;

#[test]
fn test_no_hard_tabs() {
    let rule = MD010NoHardTabs::default();
    let content = "This line is fine\n    Indented with spaces";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_leading_hard_tabs() {
    let rule = MD010NoHardTabs::default();
    let content = "\tIndented line\n\t\tDouble indented";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // One warning per line (grouped consecutive tabs)
    assert_eq!(result[0].line, 1);
    assert_eq!(
        result[0].message,
        "Found leading tab, use 4 spaces instead"
    );
    assert_eq!(result[1].line, 2);
    assert_eq!(
        result[1].message,
        "Found 2 leading tabs, use 8 spaces instead"
    );
}

#[test]
fn test_alignment_tabs() {
    let rule = MD010NoHardTabs::default();
    let content = "Text with\ttab for alignment";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    assert_eq!(
        result[0].message,
        "Found tab for alignment, use spaces instead"
    );
}

#[test]
fn test_empty_line_tabs() {
    let rule = MD010NoHardTabs::default();
    let content = "Normal line\n\t\t\n\tMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // One warning per line (grouped consecutive tabs)
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "Empty line contains 2 tabs");
}

#[test]
fn test_code_blocks_allowed() {
    let rule = MD010NoHardTabs::new(4, false);
    let content = "Normal line\n```\n\tCode with tab\n\tMore code\n```\nNormal\tline";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the tab outside code block is flagged
    assert_eq!(result[0].line, 6);
}

#[test]
fn test_code_blocks_not_allowed() {
    let rule = MD010NoHardTabs::default(); // code_blocks = true
    let content = "Normal line\n```\n\tCode with tab\n\tMore code\n```\nNormal\tline";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3); // All tabs are flagged
}

#[test]
fn test_fix_with_code_blocks() {
    let rule = MD010NoHardTabs::new(2, false); // 2 spaces per tab, preserve code blocks
    let content = "\tIndented line\n```\n\tCode\n```\n\t\tDouble indented";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "  Indented line\n```\n\tCode\n```\n    Double indented"
    );
}

#[test]
fn test_fix_without_code_blocks() {
    let rule = MD010NoHardTabs::new(2, true); // 2 spaces per tab, fix code blocks
    let content = "\tIndented line\n```\n\tCode\n```\n\t\tDouble indented";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "  Indented line\n```\n  Code\n```\n    Double indented"
    );
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
    assert_eq!(
        result.len(),
        0,
        "Should ignore tabs in single-line HTML comments"
    );

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
    let rule = MD010NoHardTabs::new(4, false); // Don't check code blocks
    
    // Note: The last line has a blank line before it and starts with tab, so it's an indented code block
    let content = "No\ttabs\there\n\n```\n\tTabs\tin\tcode\n```\n\nRegular\ttext\twith\ttabs";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    
    assert!(fixed.contains("No    tabs    here"), "Tabs outside code should be replaced");
    assert!(fixed.contains("\tTabs\tin\tcode"), "Tabs in fenced code should be preserved");
    assert!(fixed.contains("Regular    text    with    tabs"), "Tabs in regular text should be replaced");
}

#[test]
fn test_md010_tabs_in_indented_code() {
    let rule = MD010NoHardTabs::new(4, false);
    
    let content = "Text\n\n\t\tCode with tabs\n\t\tMore code\n\nText\twith\ttab";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    
    assert!(fixed.contains("\t\tCode with tabs"), "Tabs in indented code should be preserved");
    assert!(fixed.contains("Text    with    tab"), "Tabs outside code should be replaced");
}

#[test]
fn test_md010_mixed_indentation_in_code() {
    let rule = MD010NoHardTabs::new(2, false);
    
    let content = "```python\n  spaces\n\ttab\n  \tmixed\n```\n\nOutside\ttab";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    
    // Code block content should be preserved exactly
    assert!(fixed.contains("  spaces\n\ttab\n  \tmixed"), "Mixed indentation in code preserved");
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
    let rule_tabs = MD010NoHardTabs::new(4, false);
    let ctx = LintContext::new(content);
    let fixed_tabs = rule_tabs.fix(&ctx).unwrap();
    
    // Expected: tabs in list items are replaced with spaces, tabs in code blocks preserved
    let expected = r#"1. List    with    tab
   
   ```
   	Code with tab
   ```

2. Wrong    number    here"#;
    
    assert_eq!(fixed_tabs, expected, "Tabs in list items should be replaced, code block tabs preserved");
}
