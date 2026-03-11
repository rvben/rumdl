//! Tests for LineInfo.visual_indent field
//!
//! This tests that visual_indent correctly computes the visual column width
//! of leading whitespace with proper CommonMark tab expansion.
//!
//! Per CommonMark spec, tabs expand to the next column that is a multiple of 4.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;

/// Test basic visual_indent computation with spaces only
#[test]
fn test_visual_indent_spaces_only() {
    let content = "no indent\n one space\n  two spaces\n   three spaces\n    four spaces\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.lines[0].visual_indent, 0, "No indent");
    assert_eq!(ctx.lines[1].visual_indent, 1, "One space");
    assert_eq!(ctx.lines[2].visual_indent, 2, "Two spaces");
    assert_eq!(ctx.lines[3].visual_indent, 3, "Three spaces");
    assert_eq!(ctx.lines[4].visual_indent, 4, "Four spaces");
}

/// Test tab expansion at column 0 (tab expands to column 4)
#[test]
fn test_visual_indent_tab_at_column_0() {
    let content = "\tcode\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Tab at column 0 expands to column 4
    assert_eq!(
        ctx.lines[0].visual_indent, 4,
        "Tab at column 0 should give visual_indent=4"
    );
    assert_eq!(ctx.lines[0].indent, 1, "Byte indent should be 1 (one tab character)");
}

/// Test tab expansion at various starting columns
/// Per CommonMark: tabs expand to next column that is a multiple of 4
#[test]
fn test_visual_indent_mixed_space_tab() {
    // 1 space + tab: column 1 → 4 (tab expands 3 columns)
    let content1 = " \tcode\n";
    let ctx1 = LintContext::new(content1, MarkdownFlavor::Standard, None);
    assert_eq!(ctx1.lines[0].visual_indent, 4, "1 space + tab = column 4");
    assert_eq!(ctx1.lines[0].indent, 2, "Byte indent = 2 (space + tab)");

    // 2 spaces + tab: column 2 → 4 (tab expands 2 columns)
    let content2 = "  \tcode\n";
    let ctx2 = LintContext::new(content2, MarkdownFlavor::Standard, None);
    assert_eq!(ctx2.lines[0].visual_indent, 4, "2 spaces + tab = column 4");
    assert_eq!(ctx2.lines[0].indent, 3, "Byte indent = 3 (2 spaces + tab)");

    // 3 spaces + tab: column 3 → 4 (tab expands 1 column)
    let content3 = "   \tcode\n";
    let ctx3 = LintContext::new(content3, MarkdownFlavor::Standard, None);
    assert_eq!(ctx3.lines[0].visual_indent, 4, "3 spaces + tab = column 4");
    assert_eq!(ctx3.lines[0].indent, 4, "Byte indent = 4 (3 spaces + tab)");

    // 4 spaces + tab: column 4 → 8 (tab expands to next 4-boundary)
    let content4 = "    \tcode\n";
    let ctx4 = LintContext::new(content4, MarkdownFlavor::Standard, None);
    assert_eq!(ctx4.lines[0].visual_indent, 8, "4 spaces + tab = column 8");
    assert_eq!(ctx4.lines[0].indent, 5, "Byte indent = 5 (4 spaces + tab)");
}

/// Test multiple consecutive tabs
#[test]
fn test_visual_indent_multiple_tabs() {
    // Two tabs: column 0 → 4 → 8
    let content1 = "\t\tcode\n";
    let ctx1 = LintContext::new(content1, MarkdownFlavor::Standard, None);
    assert_eq!(ctx1.lines[0].visual_indent, 8, "Two tabs = column 8");
    assert_eq!(ctx1.lines[0].indent, 2, "Byte indent = 2 (two tab characters)");

    // Three tabs: column 0 → 4 → 8 → 12
    let content2 = "\t\t\tcode\n";
    let ctx2 = LintContext::new(content2, MarkdownFlavor::Standard, None);
    assert_eq!(ctx2.lines[0].visual_indent, 12, "Three tabs = column 12");
}

/// Test that visual_indent is correctly used to detect indented code blocks
/// These patterns would fail with naive `starts_with("    ") || starts_with('\t')`
#[test]
fn test_visual_indent_code_block_detection_edge_cases() {
    // Pattern: 1 space + tab = visual column 4 (IS an indented code block)
    let content1 = " \tcode block\n";
    let ctx1 = LintContext::new(content1, MarkdownFlavor::Standard, None);
    assert!(
        ctx1.lines[0].visual_indent >= 4,
        "1 space + tab should be detected as indented code block (visual_indent={})",
        ctx1.lines[0].visual_indent
    );

    // Pattern: 2 spaces + tab = visual column 4 (IS an indented code block)
    let content2 = "  \tcode block\n";
    let ctx2 = LintContext::new(content2, MarkdownFlavor::Standard, None);
    assert!(
        ctx2.lines[0].visual_indent >= 4,
        "2 spaces + tab should be detected as indented code block (visual_indent={})",
        ctx2.lines[0].visual_indent
    );

    // Pattern: 3 spaces + tab = visual column 4 (IS an indented code block)
    let content3 = "   \tcode block\n";
    let ctx3 = LintContext::new(content3, MarkdownFlavor::Standard, None);
    assert!(
        ctx3.lines[0].visual_indent >= 4,
        "3 spaces + tab should be detected as indented code block (visual_indent={})",
        ctx3.lines[0].visual_indent
    );

    // Pattern: 3 spaces only = visual column 3 (NOT an indented code block)
    let content4 = "   not code\n";
    let ctx4 = LintContext::new(content4, MarkdownFlavor::Standard, None);
    assert!(
        ctx4.lines[0].visual_indent < 4,
        "3 spaces should NOT be detected as indented code block (visual_indent={})",
        ctx4.lines[0].visual_indent
    );
}

/// Test that byte-based indent field is preserved for substring extraction
#[test]
fn test_indent_vs_visual_indent_distinction() {
    // Tab at column 0
    let content1 = "\tcode\n";
    let ctx1 = LintContext::new(content1, MarkdownFlavor::Standard, None);
    // indent = byte count for substring extraction
    assert_eq!(ctx1.lines[0].indent, 1, "Byte indent should be 1");
    // visual_indent = visual column width for comparison
    assert_eq!(ctx1.lines[0].visual_indent, 4, "Visual indent should be 4");

    // Mixed whitespace
    let content2 = "  \tcode\n";
    let ctx2 = LintContext::new(content2, MarkdownFlavor::Standard, None);
    assert_eq!(ctx2.lines[0].indent, 3, "Byte indent = 2 spaces + 1 tab = 3 bytes");
    assert_eq!(ctx2.lines[0].visual_indent, 4, "Visual indent = column 4");
}

/// Test blank lines have zero indent
#[test]
fn test_visual_indent_blank_lines() {
    let content = "text\n\n    indented\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.lines[0].visual_indent, 0, "No indent on text line");
    assert_eq!(ctx.lines[1].visual_indent, 0, "Blank line has zero visual_indent");
    assert_eq!(ctx.lines[2].visual_indent, 4, "Indented line has visual_indent=4");
}

/// Test that rules using visual_indent correctly skip indented code blocks
#[test]
fn test_rule_skips_tab_indented_code() {
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD020NoMissingSpaceClosedAtx;

    // Tab-indented content that looks like a heading but should be code
    let content = "\t# Not a heading #\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let rule = MD020NoMissingSpaceClosedAtx;
    let warnings = rule.check(&ctx).unwrap();

    // Should not flag this as a heading issue - it's in a code block
    assert!(
        warnings.is_empty(),
        "Tab-indented heading-like content should be treated as code block, not heading"
    );
}

/// Test mixed whitespace with space+tab pattern (the critical edge case)
#[test]
fn test_space_tab_pattern_code_block_detection() {
    use rumdl_lib::config::Config;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD025SingleTitle;

    // This pattern would fail with naive starts_with check:
    // " \t# Heading" - 1 space + tab = 4 visual columns = code block
    let content = " \t# Heading\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let rule = MD025SingleTitle::from_config(&Config::default());
    let warnings = rule.check(&ctx).unwrap();

    // The "heading" is actually in a code block (4 visual columns of indent)
    // so it should not be flagged
    assert!(
        warnings.is_empty(),
        "Space+tab indented heading should be treated as code block: visual_indent={}",
        ctx.lines[0].visual_indent
    );
}
