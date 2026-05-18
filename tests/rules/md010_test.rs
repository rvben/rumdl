use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD010Config, MD010NoHardTabs};
use rumdl_lib::types::PositiveUsize;

#[test]
fn test_no_hard_tabs() {
    let rule = MD010NoHardTabs::default();
    let content = "This line is fine\n    Indented with spaces";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_content_with_no_tabs_various_contexts() {
    let rule = MD010NoHardTabs::default();

    // Test various content without tabs
    let content = "# Heading without tabs\n\n    Indented with spaces\n\n- List item\n  - Nested with spaces\n\n```\nCode without tabs\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Content with only spaces should pass");
}

#[test]
fn test_leading_hard_tabs() {
    // Both lines start with a tab at column 0 -> indented code block.
    // Default code_blocks=false skips tabs in indented code blocks.
    let content = "\tIndented line\n\t\tDouble indented";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let rule_off = MD010NoHardTabs::default();
    let result_off = rule_off.check(&ctx).unwrap();
    assert!(
        result_off.is_empty(),
        "indented code block skipped by default, got {result_off:?}"
    );
    assert_eq!(
        rule_off.fix(&ctx).unwrap(),
        "\tIndented line\n\t\tDouble indented",
        "content preserved unchanged"
    );

    // code_blocks=true: tabs in indented code blocks are flagged.
    let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
        spaces_per_tab: PositiveUsize::from_const(4),
        code_blocks: true,
    });
    let result_on = rule_on.check(&ctx).unwrap();
    assert_eq!(result_on.len(), 2, "got {result_on:?}");
    assert_eq!(result_on[0].line, 1);
    assert_eq!(result_on[0].message, "Found leading tab, use 4 spaces instead");
    assert_eq!(result_on[1].line, 2);
    assert_eq!(result_on[1].message, "Found 2 leading tabs, use 8 spaces instead");
    assert_eq!(rule_on.fix(&ctx).unwrap(), "    Indented line\n        Double indented");
}

#[test]
fn test_alignment_tabs() {
    let rule = MD010NoHardTabs::default();
    let content = "Text with\ttab for alignment";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].message, "Found tab for alignment, use spaces instead");
}

#[test]
fn test_empty_line_tabs() {
    // Line 2 "\t\t" is an empty line with tabs -> always flagged (not an indented code block).
    // Line 3 "\tMore text" starts with tab at column 0 -> indented code block, skipped by default.
    let content = "Normal line\n\t\t\n\tMore text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let rule_off = MD010NoHardTabs::default();
    let result_off = rule_off.check(&ctx).unwrap();
    assert_eq!(result_off.len(), 1, "only empty-line tabs flagged, got {result_off:?}");
    assert_eq!(result_off[0].line, 2);
    assert_eq!(result_off[0].message, "Empty line contains 2 tabs");
    // Empty-line tabs replaced; indented code block line preserved.
    assert_eq!(rule_off.fix(&ctx).unwrap(), "Normal line\n        \n\tMore text");

    // code_blocks=true: line 3 indented code block tab also flagged.
    let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
        spaces_per_tab: PositiveUsize::from_const(4),
        code_blocks: true,
    });
    let result_on = rule_on.check(&ctx).unwrap();
    assert_eq!(result_on.len(), 2, "got {result_on:?}");
    assert_eq!(result_on[0].line, 2);
    assert_eq!(result_on[0].message, "Empty line contains 2 tabs");
    assert_eq!(result_on[1].line, 3);
    assert_eq!(result_on[1].message, "Found leading tab, use 4 spaces instead");
    assert_eq!(rule_on.fix(&ctx).unwrap(), "Normal line\n        \n    More text");
}

#[test]
fn test_code_blocks_allowed() {
    // Intentionally mirrors test_code_blocks_not_allowed; do not delete the redundancy.
    let rule = MD010NoHardTabs::new(4);
    let content = "Normal line\n```\n\tCode with tab\n\tMore code\n```\nNormal\tline";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the tab outside code block is flagged
    assert_eq!(result[0].line, 6);
}

#[test]
fn test_code_blocks_not_allowed() {
    // Fenced code block tabs are skipped by default (flagged when code_blocks=true).
    let content = "Normal line\n```\n\tCode with tab\n\tMore code\n```\nNormal\tline";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let rule_off = MD010NoHardTabs::default();
    let result_off = rule_off.check(&ctx).unwrap();
    assert_eq!(result_off.len(), 1); // Only tab outside code block is flagged
    assert_eq!(result_off[0].line, 6);

    // code_blocks=true: tabs inside the fenced block are also flagged.
    let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
        spaces_per_tab: PositiveUsize::from_const(4),
        code_blocks: true,
    });
    let result_on = rule_on.check(&ctx).unwrap();
    assert_eq!(result_on.len(), 3, "got {result_on:?}");
    assert_eq!(result_on[0].line, 3);
    assert_eq!(result_on[0].message, "Found leading tab, use 4 spaces instead");
    assert_eq!(result_on[1].line, 4);
    assert_eq!(result_on[1].message, "Found leading tab, use 4 spaces instead");
    assert_eq!(result_on[2].line, 6);
    assert_eq!(result_on[2].message, "Found tab for alignment, use spaces instead");
    assert_eq!(
        rule_on.fix(&ctx).unwrap(),
        "Normal line\n```\n    Code with tab\n    More code\n```\nNormal    line"
    );
}

#[test]
fn test_fix_with_code_blocks() {
    // Default code_blocks=false: lines 1 and 5 are indented code blocks (tab at column 0);
    // line 3 is inside a fenced code block. All tabs skipped -> content preserved as-is.
    let content = "\tIndented line\n```\n\tCode\n```\n\t\tDouble indented";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let rule_off = MD010NoHardTabs::new(2);
    assert!(
        rule_off.check(&ctx).unwrap().is_empty(),
        "all tabs in code blocks skipped"
    );
    assert_eq!(
        rule_off.fix(&ctx).unwrap(),
        "\tIndented line\n```\n\tCode\n```\n\t\tDouble indented",
        "content preserved unchanged"
    );
}

#[test]
fn test_fix_with_code_blocks_true_variant() {
    // code_blocks=true: tabs in both fenced and indented code blocks are replaced.
    let content = "\tIndented line\n```\n\tCode\n```\n\t\tDouble indented";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
        spaces_per_tab: PositiveUsize::from_const(2),
        code_blocks: true,
    });
    let result_on = rule_on.check(&ctx).unwrap();
    assert_eq!(result_on.len(), 3, "got {result_on:?}");
    assert_eq!(result_on[0].line, 1);
    assert_eq!(result_on[1].line, 3);
    assert_eq!(result_on[2].line, 5);
    assert_eq!(
        rule_on.fix(&ctx).unwrap(),
        "  Indented line\n```\n  Code\n```\n    Double indented"
    );
}

#[test]
fn test_fix_without_code_blocks() {
    // Intentionally duplicates test_fix_with_code_blocks content as a historical regression
    // counterpart; do not merge or delete either. The code_blocks=true behavior for this
    // content lives in test_fix_with_code_blocks_true_variant.
    let content = "\tIndented line\n```\n\tCode\n```\n\t\tDouble indented";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let rule_off = MD010NoHardTabs::new(2);
    assert!(
        rule_off.check(&ctx).unwrap().is_empty(),
        "all tabs in code blocks skipped"
    );
    assert_eq!(
        rule_off.fix(&ctx).unwrap(),
        "\tIndented line\n```\n\tCode\n```\n\t\tDouble indented",
        "content preserved unchanged"
    );
}

#[test]
fn test_mixed_indentation() {
    // "    Spaces" is space-indented (not a tab). "\tTab" and "  \tMixed" start with
    // tab/space-tab and are classified as indented code blocks.
    // Default code_blocks=false skips indented code blocks.
    let content = "    Spaces\n\tTab\n  \tMixed";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let rule_off = MD010NoHardTabs::default();
    let result_off = rule_off.check(&ctx).unwrap();
    assert!(
        result_off.is_empty(),
        "indented code block lines skipped, got {result_off:?}"
    );
    assert_eq!(
        rule_off.fix(&ctx).unwrap(),
        "    Spaces\n\tTab\n  \tMixed",
        "content preserved unchanged"
    );

    // code_blocks=true: tabs on lines 2 and 3 are flagged.
    let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
        spaces_per_tab: PositiveUsize::from_const(4),
        code_blocks: true,
    });
    let result_on = rule_on.check(&ctx).unwrap();
    assert_eq!(result_on.len(), 2, "got {result_on:?}");
    assert_eq!(result_on[0].line, 2);
    assert_eq!(result_on[1].line, 3);
    assert_eq!(rule_on.fix(&ctx).unwrap(), "    Spaces\n    Tab\n      Mixed");
}

#[test]
fn test_html_comments_with_tabs() {
    let rule = MD010NoHardTabs::default();

    // Single line HTML comment with tabs
    let content = "<!-- This comment has a \t tab -->\nNormal line";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Should ignore tabs in single-line HTML comments");

    // Multi-line HTML comment with tabs
    let content = "<!-- Start of comment\nUser: \t\tuser\nPassword:\tpass\n-->\nNormal\tline";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    // Lines 3-4 start with double-tabs -> indented code block.
    // Default code_blocks=false skips indented code blocks; line 6 alignment tabs fixed.
    let content = "Text\n\n\t\tCode with tabs\n\t\tMore code\n\nText\twith\ttab";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let rule_off = MD010NoHardTabs::new(4);
    let result_off = rule_off.check(&ctx).unwrap();
    assert_eq!(result_off.len(), 2, "got {result_off:?}");
    assert_eq!(result_off[0].line, 6);
    assert_eq!(result_off[1].line, 6);
    let fixed_off = rule_off.fix(&ctx).unwrap();
    assert!(
        fixed_off.contains("\t\tCode with tabs"),
        "indented code block preserved, got: {fixed_off:?}"
    );
    assert!(
        fixed_off.contains("Text    with    tab"),
        "alignment tabs fixed, got: {fixed_off:?}"
    );

    // code_blocks=true: indented code block tabs also flagged and replaced.
    let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
        spaces_per_tab: PositiveUsize::from_const(4),
        code_blocks: true,
    });
    let result_on = rule_on.check(&ctx).unwrap();
    assert_eq!(result_on.len(), 4, "got {result_on:?}");
    assert_eq!(result_on[0].line, 3);
    assert_eq!(result_on[1].line, 4);
    assert_eq!(result_on[2].line, 6);
    assert_eq!(result_on[3].line, 6);
    let fixed_on = rule_on.fix(&ctx).unwrap();
    assert!(
        fixed_on.contains("        Code with tabs"),
        "indented code tabs replaced (2 tabs * 4 spaces), got: {fixed_on:?}"
    );
    assert!(
        fixed_on.contains("Text    with    tab"),
        "alignment tabs fixed, got: {fixed_on:?}"
    );
}

#[test]
fn test_md010_mixed_indentation_in_code() {
    let rule = MD010NoHardTabs::new(2);

    let content = "```python\n  spaces\n\ttab\n  \tmixed\n```\n\nOutside\ttab";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    // Line 1 "\tStart tab" starts with a tab at column 0 -> indented code block, skipped by default.
    // Lines 2-5 have non-leading or leading tabs on non-code-block lines.
    let content = "\tStart tab\nMiddle\ttab\nEnd tab\t\n\t\tDouble start\nMixed \t \t spaces";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let rule_off = MD010NoHardTabs::default();
    let result_off = rule_off.check(&ctx).unwrap();
    assert_eq!(result_off.len(), 5, "got {result_off:?}");
    assert_eq!(result_off[0].line, 2);
    assert_eq!(result_off[0].message, "Found tab for alignment, use spaces instead");
    assert_eq!(result_off[1].line, 3);
    assert_eq!(result_off[1].message, "Found tab for alignment, use spaces instead");
    assert_eq!(result_off[2].line, 4);
    assert_eq!(result_off[2].message, "Found 2 leading tabs, use 8 spaces instead");
    assert_eq!(result_off[3].line, 5);
    assert_eq!(result_off[3].message, "Found tab for alignment, use spaces instead");
    assert_eq!(result_off[4].line, 5);
    assert_eq!(result_off[4].message, "Found tab for alignment, use spaces instead");
    assert_eq!(
        rule_off.fix(&ctx).unwrap(),
        "\tStart tab\nMiddle    tab\nEnd tab    \n        Double start\nMixed           spaces"
    );

    // code_blocks=true: line 1 is also flagged.
    let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
        spaces_per_tab: PositiveUsize::from_const(4),
        code_blocks: true,
    });
    let result_on = rule_on.check(&ctx).unwrap();
    assert_eq!(result_on.len(), 6, "got {result_on:?}");
    assert_eq!(result_on[0].line, 1);
    assert_eq!(result_on[0].message, "Found leading tab, use 4 spaces instead");
    assert_eq!(
        rule_on.fix(&ctx).unwrap(),
        "    Start tab\nMiddle    tab\nEnd tab    \n        Double start\nMixed           spaces"
    );
}

#[test]
fn test_mixed_tabs_and_spaces_detailed() {
    // All four lines have tabs mixed with leading spaces -> all classified as indented code blocks.
    // Default code_blocks=false skips all of them.
    let content =
        "  \tTwo spaces then tab\n\t  Tab then two spaces\n \t \t Space tab space tab\n\t\t  Two tabs then spaces";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let rule_off = MD010NoHardTabs::default();
    let result_off = rule_off.check(&ctx).unwrap();
    assert!(
        result_off.is_empty(),
        "all lines are indented code blocks, got {result_off:?}"
    );
    assert_eq!(
        rule_off.fix(&ctx).unwrap(),
        "  \tTwo spaces then tab\n\t  Tab then two spaces\n \t \t Space tab space tab\n\t\t  Two tabs then spaces",
        "content preserved unchanged"
    );

    // code_blocks=true: 5 tab groups flagged and fixed across all lines.
    let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
        spaces_per_tab: PositiveUsize::from_const(4),
        code_blocks: true,
    });
    let result_on = rule_on.check(&ctx).unwrap();
    assert_eq!(result_on.len(), 5, "got {result_on:?}");
    assert_eq!(
        rule_on.fix(&ctx).unwrap(),
        "      Two spaces then tab\n      Tab then two spaces\n           Space tab space tab\n          Two tabs then spaces"
    );
}

#[test]
fn test_empty_lines_with_only_tabs_variations() {
    let rule = MD010NoHardTabs::default();

    // Various empty line patterns
    let content = "\t\n\t\t\n\t\t\t\n\t \t\n \t \t \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    // All lines start with tabs at column 0 -> indented code block.
    // Default code_blocks=false skips them regardless of spaces_per_tab.
    let content = "\tOne tab\n\t\tTwo tabs\n\t\t\tThree tabs";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let rule2_off = MD010NoHardTabs::new(2);
    assert!(rule2_off.check(&ctx).unwrap().is_empty(), "indented code block skipped");
    assert_eq!(
        rule2_off.fix(&ctx).unwrap(),
        "\tOne tab\n\t\tTwo tabs\n\t\t\tThree tabs",
        "content preserved with spaces_per_tab=2"
    );

    let rule8_off = MD010NoHardTabs::new(8);
    assert!(rule8_off.check(&ctx).unwrap().is_empty(), "indented code block skipped");
    assert_eq!(
        rule8_off.fix(&ctx).unwrap(),
        "\tOne tab\n\t\tTwo tabs\n\t\t\tThree tabs",
        "content preserved with spaces_per_tab=8"
    );

    // code_blocks=true: spaces_per_tab controls substitution width.
    let rule2_on = MD010NoHardTabs::from_config_struct(MD010Config {
        spaces_per_tab: PositiveUsize::from_const(2),
        code_blocks: true,
    });
    assert_eq!(rule2_on.fix(&ctx).unwrap(), "  One tab\n    Two tabs\n      Three tabs");

    let rule8_on = MD010NoHardTabs::from_config_struct(MD010Config {
        spaces_per_tab: PositiveUsize::from_const(8),
        code_blocks: true,
    });
    assert_eq!(
        rule8_on.fix(&ctx).unwrap(),
        "        One tab\n                Two tabs\n                        Three tabs"
    );
}

#[test]
fn test_configuration_code_blocks_parameter() {
    let content = "Normal\ttab\n\n```javascript\nfunction\tfoo() {\n\treturn\ttrue;\n}\n```\n\nAnother\ttab";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Default code_blocks=false: only tabs outside the fenced block are flagged.
    let rule_off = MD010NoHardTabs::new(4);
    let result_off = rule_off.check(&ctx).unwrap();
    assert_eq!(result_off.len(), 2, "only prose tabs flagged, got {result_off:?}");
    assert_eq!(result_off[0].line, 1);
    assert_eq!(result_off[1].line, 9);
    let fixed_off = rule_off.fix(&ctx).unwrap();
    assert!(fixed_off.contains("function\tfoo()"), "fenced code block preserved");
    assert!(fixed_off.contains("Normal    tab"), "prose tab fixed");
    assert_eq!(
        fixed_off,
        "Normal    tab\n\n```javascript\nfunction\tfoo() {\n\treturn\ttrue;\n}\n```\n\nAnother    tab"
    );

    // code_blocks=true: tabs inside the fenced block are also flagged.
    let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
        spaces_per_tab: PositiveUsize::from_const(4),
        code_blocks: true,
    });
    let result_on = rule_on.check(&ctx).unwrap();
    assert_eq!(result_on.len(), 5, "got {result_on:?}");
    assert_eq!(result_on[0].line, 1);
    assert_eq!(result_on[1].line, 4);
    assert_eq!(result_on[2].line, 5);
    assert_eq!(result_on[3].line, 5);
    assert_eq!(result_on[4].line, 9);
    assert_eq!(
        rule_on.fix(&ctx).unwrap(),
        "Normal    tab\n\n```javascript\nfunction    foo() {\n    return    true;\n}\n```\n\nAnother    tab"
    );
}

#[test]
fn test_consecutive_vs_separate_tabs() {
    // Line 1 "\t\t\tThree consecutive" starts with 3 tabs at column 0 -> indented code block.
    // Default code_blocks=false skips it; line 2 alignment tabs are flagged (3 separate groups).
    let content = "\t\t\tThree consecutive\nOne\tthen\tanother\t";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let rule_off = MD010NoHardTabs::default();
    let result_off = rule_off.check(&ctx).unwrap();
    assert_eq!(
        result_off.len(),
        3,
        "3 separate alignment tab groups on line 2, got {result_off:?}"
    );
    assert_eq!(result_off[0].line, 2);
    assert_eq!(result_off[0].message, "Found tab for alignment, use spaces instead");
    assert_eq!(result_off[1].line, 2);
    assert_eq!(result_off[1].message, "Found tab for alignment, use spaces instead");
    assert_eq!(result_off[2].line, 2);
    assert_eq!(result_off[2].message, "Found tab for alignment, use spaces instead");
    assert_eq!(
        rule_off.fix(&ctx).unwrap(),
        "\t\t\tThree consecutive\nOne    then    another    "
    );

    // code_blocks=true: line 1 consecutive-tab group is also flagged.
    let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
        spaces_per_tab: PositiveUsize::from_const(4),
        code_blocks: true,
    });
    let result_on = rule_on.check(&ctx).unwrap();
    assert_eq!(result_on.len(), 4, "got {result_on:?}");
    assert_eq!(result_on[0].line, 1);
    assert_eq!(result_on[0].message, "Found 3 leading tabs, use 12 spaces instead");
    assert_eq!(
        rule_on.fix(&ctx).unwrap(),
        "            Three consecutive\nOne    then    another    "
    );
}

#[test]
fn test_fix_preserves_content_structure() {
    let content = "# Header\n\n\tIndented paragraph\n\n- List\n\t- Nested\n\t\t- Double nested\n\n```\n\tCode block\n```\n\n> Quote\n> \tWith tab\n\n| Col1\t| Col2\t|\n|---\t|---\t|\n| Data\t| Data\t|";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Default code_blocks=false: indented paragraph (line 3) and code block content (line 10)
    // are preserved; list item tabs and table/quote tabs outside code blocks are fixed.
    let rule_off = MD010NoHardTabs::default();
    let fixed_off = rule_off.fix(&ctx).unwrap();
    assert!(fixed_off.contains("# Header"), "headers preserved");
    assert!(
        fixed_off.contains("\tIndented paragraph"),
        "indented code block preserved, got: {fixed_off:?}"
    );
    assert!(fixed_off.contains("    - Nested"), "list indentation converted");
    assert!(
        fixed_off.contains("        - Double nested"),
        "double list indentation converted"
    );
    assert!(fixed_off.contains("\tCode block"), "fenced code block tab preserved");
    assert!(fixed_off.contains(">     With tab"), "quote tab converted");
    assert!(fixed_off.contains("| Col1    | Col2    |"), "table tabs converted");
    assert_eq!(
        fixed_off,
        "# Header\n\n\tIndented paragraph\n\n- List\n    - Nested\n        - Double nested\n\n```\n\tCode block\n```\n\n> Quote\n>     With tab\n\n| Col1    | Col2    |\n|---    |---    |\n| Data    | Data    |"
    );

    // code_blocks=true: indented paragraph and fenced code block tabs are also fixed.
    let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
        spaces_per_tab: PositiveUsize::from_const(4),
        code_blocks: true,
    });
    let fixed_on = rule_on.fix(&ctx).unwrap();
    assert!(
        fixed_on.contains("    Indented paragraph"),
        "indented paragraph converted with code_blocks=true, got: {fixed_on:?}"
    );
    assert!(fixed_on.contains("    Code block"), "fenced code block tab converted");
    assert_eq!(
        fixed_on,
        "# Header\n\n    Indented paragraph\n\n- List\n    - Nested\n        - Double nested\n\n```\n    Code block\n```\n\n> Quote\n>     With tab\n\n| Col1    | Col2    |\n|---    |---    |\n| Data    | Data    |"
    );
}

#[test]
fn test_edge_cases() {
    let rule = MD010NoHardTabs::default();

    // Test edge cases
    let content = "\t"; // Single tab only
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].message, "Empty line contains tab");

    // Test file ending with tab
    let content2 = "Text\t";
    let ctx2 = LintContext::new(content2, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should detect both tabs (inline code spans are not excluded like code blocks)
    assert_eq!(result.len(), 2, "Should detect tabs in inline code and outside");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Text with `inline    code` and    tab outside");
}

#[test]
fn test_roundtrip_fix_then_recheck_simple() {
    let rule = MD010NoHardTabs::default();
    let content = "\tIndented\nNormal\tline\nNo tabs";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx2 = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx2).unwrap();
    assert!(
        warnings.is_empty(),
        "After fix, re-check should produce 0 warnings but got: {warnings:?}"
    );
}

#[test]
fn test_roundtrip_fix_then_recheck_code_blocks() {
    let rule = MD010NoHardTabs::default();
    let content = "Text\twith\ttab\n```makefile\ntarget:\n\tcommand\n```\nMore\ttabs";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx2 = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx2).unwrap();
    assert!(
        warnings.is_empty(),
        "After fix, re-check should produce 0 warnings but got: {warnings:?}"
    );
}

#[test]
fn test_roundtrip_fix_then_recheck_custom_spaces() {
    let rule = MD010NoHardTabs::new(2);
    let content = "\tOne tab\n\t\tTwo tabs\n\t\t\tThree tabs";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx2 = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx2).unwrap();
    assert!(
        warnings.is_empty(),
        "After fix, re-check should produce 0 warnings but got: {warnings:?}"
    );
}

#[test]
fn test_roundtrip_fix_then_recheck_mixed_content() {
    let rule = MD010NoHardTabs::default();
    let content = "# Header\n\n\tIndented paragraph\n\n- List\n\t- Nested\n\t\t- Double nested\n\n```\n\tCode block\n```\n\n> Quote\n> \tWith tab\n\n| Col1\t| Col2\t|\n|---\t|---\t|\n| Data\t| Data\t|";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx2 = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx2).unwrap();
    assert!(
        warnings.is_empty(),
        "After fix, re-check should produce 0 warnings but got: {warnings:?}"
    );
}

#[test]
fn test_roundtrip_fix_then_recheck_empty_lines_with_tabs() {
    let rule = MD010NoHardTabs::default();
    let content = "Normal line\n\t\t\n\t\nAnother line";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx2 = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx2).unwrap();
    assert!(
        warnings.is_empty(),
        "After fix, re-check should produce 0 warnings but got: {warnings:?}"
    );
}

#[test]
fn test_roundtrip_fix_then_recheck_html_comments() {
    let rule = MD010NoHardTabs::default();
    let content = "<!-- Start of comment\nUser: \t\tuser\nPassword:\tpass\n-->\nNormal\tline";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx2 = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx2).unwrap();
    assert!(
        warnings.is_empty(),
        "After fix, re-check should produce 0 warnings but got: {warnings:?}"
    );
}
