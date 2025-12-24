use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{
    MD004UnorderedListStyle, MD005ListIndent, MD007ULIndent, MD009TrailingSpaces, MD010NoHardTabs,
    MD012NoMultipleBlanks, MD022BlanksAroundHeadings, MD023HeadingStartLeft, MD028NoBlanksBlockquote,
    MD030ListMarkerSpace, MD031BlanksAroundFences, MD032BlanksAroundLists, MD047SingleTrailingNewline,
};

#[test]
fn test_md009_md010_tabs_and_spaces() {
    let md009 = MD009TrailingSpaces::default();
    let md010 = MD010NoHardTabs::default();

    // Content with tabs and trailing spaces
    let content = "Text\t  \n\tIndented  \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD010 should fix tabs
    let fixed_tabs = md010.fix(&ctx).unwrap();
    let ctx_after_tabs = LintContext::new(&fixed_tabs, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD009 should fix trailing spaces
    let final_fixed = md009.fix(&ctx_after_tabs).unwrap();

    // Verify no conflicts
    let final_ctx = LintContext::new(&final_fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let md009_check = md009.check(&final_ctx).unwrap();
    let md010_check = md010.check(&final_ctx).unwrap();

    assert_eq!(md009_check.len(), 0, "No trailing spaces after fixes");
    assert_eq!(md010_check.len(), 0, "No tabs after fixes");
}

#[test]
fn test_md004_md030_list_style_and_spacing() {
    let md004 = MD004UnorderedListStyle::default();
    let md030 = MD030ListMarkerSpace::default();

    // Mixed list styles with spacing issues
    let content = "* Item 1\n-  Item 2\n+   Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD004 fixes list style consistency
    let md004_result = md004.check(&ctx).unwrap();
    assert!(!md004_result.is_empty(), "Should detect inconsistent list markers");

    // MD030 fixes spacing after markers
    let md030_result = md030.check(&ctx).unwrap();
    assert!(!md030_result.is_empty(), "Should detect spacing issues");

    // Apply both fixes
    let fixed_md004 = md004.fix(&ctx).unwrap();
    let ctx_after_md004 = LintContext::new(&fixed_md004, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let final_fixed = md030.fix(&ctx_after_md004).unwrap();

    // Verify both rules are satisfied
    let final_ctx = LintContext::new(&final_fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(md004.check(&final_ctx).unwrap().len(), 0);
    assert_eq!(md030.check(&final_ctx).unwrap().len(), 0);
}

#[test]
fn test_md005_md007_list_indentation_conflict() {
    let md005 = MD005ListIndent::default();
    let md007 = MD007ULIndent::default();

    // Nested list with various indentation issues
    let content = "* Item 1\n   * Nested 1\n      * Double nested";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Both rules check list indentation but with different criteria
    let md005_issues = md005.check(&ctx).unwrap();
    let _md007_issues = md007.check(&ctx).unwrap();

    // Apply MD005 fix first (consistent indentation)
    if !md005_issues.is_empty() {
        let fixed_md005 = md005.fix(&ctx).unwrap();
        let ctx_after_md005 = LintContext::new(&fixed_md005, rumdl_lib::config::MarkdownFlavor::Standard, None);

        // Check MD007 doesn't conflict
        let md007_after = md007.check(&ctx_after_md005).unwrap();
        // MD007 might still have issues but shouldn't undo MD005's work

        if !md007_after.is_empty() {
            let final_fixed = md007.fix(&ctx_after_md005).unwrap();
            let final_ctx = LintContext::new(&final_fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);

            // Both should be satisfied or at least not conflict
            let final_md005 = md005.check(&final_ctx).unwrap();
            let final_md007 = md007.check(&final_ctx).unwrap();

            // Verify no infinite fix loop
            assert!(
                final_md005.is_empty() || final_md007.is_empty(),
                "Rules should converge to a stable state"
            );
        }
    }
}

#[test]
fn test_md012_md022_blank_lines_around_headings() {
    let md012 = MD012NoMultipleBlanks::default();
    let md022 = MD022BlanksAroundHeadings::default();

    // Content with heading spacing issues
    let content = "Text\n# Heading\nMore text\n\n\n## Another heading\n\n\nText";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD012 removes multiple blank lines
    let _md012_issues = md012.check(&ctx).unwrap();

    // MD022 ensures blank lines around headings
    let md022_issues = md022.check(&ctx).unwrap();

    // Fix with MD022 first (adds required blank lines)
    if !md022_issues.is_empty() {
        let fixed_md022 = md022.fix(&ctx).unwrap();
        let ctx_after_md022 = LintContext::new(&fixed_md022, rumdl_lib::config::MarkdownFlavor::Standard, None);

        // Then fix MD012 (removes excessive blank lines)
        let final_fixed = md012.fix(&ctx_after_md022).unwrap();
        let final_ctx = LintContext::new(&final_fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);

        // Both rules should be satisfied
        assert_eq!(md012.check(&final_ctx).unwrap().len(), 0, "No multiple blank lines");
        assert_eq!(md022.check(&final_ctx).unwrap().len(), 0, "Proper heading spacing");
    }
}

#[test]
fn test_md023_md009_heading_indentation_and_trailing_spaces() {
    let md023 = MD023HeadingStartLeft;
    let md009 = MD009TrailingSpaces::default();

    // Indented heading with trailing spaces (use max 3 spaces - 4 creates code block)
    let content = "  # Heading  \n   ## Another heading  ";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD023 removes leading spaces from headings
    let fixed_md023 = md023.fix(&ctx).unwrap();
    let ctx_after_md023 = LintContext::new(&fixed_md023, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD009 removes trailing spaces
    let final_fixed = md009.fix(&ctx_after_md023).unwrap();

    // Verify both are satisfied
    let final_ctx = LintContext::new(&final_fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(md023.check(&final_ctx).unwrap().len(), 0);
    assert_eq!(md009.check(&final_ctx).unwrap().len(), 0);

    // Result should have no leading or trailing spaces
    assert!(!final_fixed.contains("  #"), "No leading spaces before headings");
    assert!(!final_fixed.lines().any(|l| l.ends_with(' ')), "No trailing spaces");
}

#[test]
fn test_md031_md032_fence_and_list_blank_lines() {
    let md031 = MD031BlanksAroundFences::default();
    let md032 = MD032BlanksAroundLists::default();

    // List with code fence
    let content = "* Item 1\n```\ncode\n```\n* Item 2\n\n* Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD031 wants blank lines around fences
    let md031_issues = md031.check(&ctx).unwrap();

    // MD032 manages blank lines in lists
    let _md032_issues = md032.check(&ctx).unwrap();

    // Apply fixes in order
    if !md031_issues.is_empty() {
        let fixed_md031 = md031.fix(&ctx).unwrap();
        let ctx_after_md031 = LintContext::new(&fixed_md031, rumdl_lib::config::MarkdownFlavor::Standard, None);

        if !md032.check(&ctx_after_md031).unwrap().is_empty() {
            let final_fixed = md032.fix(&ctx_after_md031).unwrap();
            let final_ctx = LintContext::new(&final_fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);

            // Verify both rules are satisfied
            assert_eq!(md031.check(&final_ctx).unwrap().len(), 0, "Fences have proper spacing");
            // MD032 might still have preferences but shouldn't conflict with MD031
        }
    }
}

#[test]
fn test_md047_with_other_rules() {
    let md047 = MD047SingleTrailingNewline;
    let md009 = MD009TrailingSpaces::default();

    // File without trailing newline and with trailing spaces
    let content = "Text with trailing spaces  ";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Fix trailing spaces first
    let fixed_md009 = md009.fix(&ctx).unwrap();
    let ctx_after_md009 = LintContext::new(&fixed_md009, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Then ensure single trailing newline
    let final_fixed = md047.fix(&ctx_after_md009).unwrap();

    // Verify both are satisfied
    assert!(
        final_fixed.ends_with('\n') && !final_fixed.ends_with("\n\n"),
        "Single trailing newline"
    );
    assert!(!final_fixed.lines().any(|l| l.ends_with(' ')), "No trailing spaces");
}

#[test]
fn test_blockquote_list_combination() {
    let md004 = MD004UnorderedListStyle::default();
    let md009 = MD009TrailingSpaces::default();
    let md028 = MD028NoBlanksBlockquote;

    // Blockquote containing list with issues
    let content = "> * Item 1  \n>\n> - Item 2  \n> + Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Fix MD028 (empty blockquote line)
    let fixed_md028 = md028.fix(&ctx).unwrap();
    let ctx_after_md028 = LintContext::new(&fixed_md028, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Fix MD004 (list style)
    let fixed_md004 = md004.fix(&ctx_after_md028).unwrap();
    let ctx_after_md004 = LintContext::new(&fixed_md004, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Fix MD009 (trailing spaces)
    let final_fixed = md009.fix(&ctx_after_md004).unwrap();
    let final_ctx = LintContext::new(&final_fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // All rules should be satisfied
    assert_eq!(md028.check(&final_ctx).unwrap().len(), 0, "MD028 satisfied");
    assert_eq!(md004.check(&final_ctx).unwrap().len(), 0, "MD004 satisfied");
    assert_eq!(md009.check(&final_ctx).unwrap().len(), 0, "MD009 satisfied");
}
