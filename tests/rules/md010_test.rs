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
        "Found leading hard tab, use 4 spaces instead"
    );
    assert_eq!(result[1].line, 2);
    assert_eq!(
        result[1].message,
        "Found 2 leading hard tabs, use 8 spaces instead"
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
        "Found hard tab for alignment, use spaces instead"
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
    assert_eq!(result[0].message, "Empty line contains 2 hard tabs");
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
