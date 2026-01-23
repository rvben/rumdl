// Comprehensive idempotency tests for all fixable rules
// Ensures that applying a fix twice produces the same result as applying it once

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::{LintWarning, Rule};
use rumdl_lib::rules::*;

// Import style enums from their submodules
use rumdl_lib::rules::code_fence_utils::CodeFenceStyle;
use rumdl_lib::rules::emphasis_style::EmphasisStyle;
use rumdl_lib::rules::heading_utils::HeadingStyle;
use rumdl_lib::rules::strong_style::StrongStyle;

#[allow(dead_code)]
/// Apply a single fix to content
fn apply_fix(content: &str, fix: &rumdl_lib::rule::Fix) -> String {
    let mut result = content.to_string();
    result.replace_range(fix.range.clone(), &fix.replacement);
    result
}

/// Apply all fixes from warnings to content, processing in reverse order to maintain valid indices
fn apply_all_fixes(content: &str, warnings: &[LintWarning]) -> String {
    // Sort fixes by start position in reverse order to apply from end to start
    let mut fixes: Vec<_> = warnings.iter().filter_map(|w| w.fix.as_ref()).collect();
    fixes.sort_by(|a, b| b.range.start.cmp(&a.range.start));

    let mut result = content.to_string();
    for fix in fixes {
        if fix.range.end <= result.len() {
            result.replace_range(fix.range.clone(), &fix.replacement);
        }
    }
    result
}

/// Helper function to test idempotency of a rule fix
fn assert_fix_idempotent(rule: &dyn Rule, content: &str, rule_name: &str) {
    // First fix
    let ctx1 = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings1 = rule.check(&ctx1).unwrap_or_default();
    let content1 = apply_all_fixes(content, &warnings1);

    // Second fix
    let ctx2 = LintContext::new(&content1, MarkdownFlavor::Standard, None);
    let warnings2 = rule.check(&ctx2).unwrap_or_default();
    let content2 = apply_all_fixes(&content1, &warnings2);

    assert_eq!(
        content1, content2,
        "{rule_name} fix is not idempotent!\nAfter first fix:\n{content1:?}\nAfter second fix:\n{content2:?}"
    );
}

#[allow(dead_code)]
/// Helper to test idempotency with custom config - currently unused but kept for future tests
fn assert_fix_idempotent_with_flavor(rule: &dyn Rule, content: &str, rule_name: &str, flavor: MarkdownFlavor) {
    // First fix
    let ctx1 = LintContext::new(content, flavor, None);
    let warnings1 = rule.check(&ctx1).unwrap_or_default();
    let content1 = apply_all_fixes(content, &warnings1);

    // Second fix
    let ctx2 = LintContext::new(&content1, flavor, None);
    let warnings2 = rule.check(&ctx2).unwrap_or_default();
    let content2 = apply_all_fixes(&content1, &warnings2);

    assert_eq!(
        content1, content2,
        "{rule_name} fix is not idempotent!\nAfter first fix:\n{content1:?}\nAfter second fix:\n{content2:?}"
    );
}

// ============================================================================
// MD001 - Heading Increment
// ============================================================================

#[test]
fn test_md001_fix_idempotent() {
    let rule = MD001HeadingIncrement::default();
    let content = "# Title\n\n### Skipped Level\n\n## Back\n";
    assert_fix_idempotent(&rule, content, "MD001");
}

// ============================================================================
// MD003 - Heading Style
// ============================================================================

#[test]
fn test_md003_fix_idempotent_atx() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
    let content = "Title\n=====\n\nSubtitle\n--------\n";
    assert_fix_idempotent(&rule, content, "MD003");
}

#[test]
fn test_md003_fix_idempotent_setext() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "# Title\n\n## Subtitle\n";
    assert_fix_idempotent(&rule, content, "MD003");
}

// ============================================================================
// MD004 - Unordered List Style
// ============================================================================

#[test]
fn test_md004_fix_idempotent_dash() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
    let content = "* Item 1\n+ Item 2\n* Item 3\n";
    assert_fix_idempotent(&rule, content, "MD004");
}

#[test]
fn test_md004_fix_idempotent_asterisk() {
    let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Asterisk);
    let content = "- Item 1\n+ Item 2\n- Item 3\n";
    assert_fix_idempotent(&rule, content, "MD004");
}

// ============================================================================
// MD005 - List Indent
// ============================================================================

#[test]
fn test_md005_fix_idempotent() {
    let rule = MD005ListIndent::default();
    let content = "-   Item 1\n  -   Nested\n    -   Deep\n";
    assert_fix_idempotent(&rule, content, "MD005");
}

// ============================================================================
// MD007 - UL Indent
// ============================================================================

#[test]
fn test_md007_fix_idempotent() {
    let rule = MD007ULIndent::default();
    let content = "-   Item\n   -   Wrong indent\n";
    assert_fix_idempotent(&rule, content, "MD007");
}

// ============================================================================
// MD009 - Trailing Spaces
// ============================================================================

#[test]
fn test_md009_fix_idempotent() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line with trailing spaces   \n\nAnother line   \n";
    assert_fix_idempotent(&rule, content, "MD009");
}

// ============================================================================
// MD010 - No Hard Tabs
// ============================================================================

#[test]
fn test_md010_fix_idempotent() {
    let rule = MD010NoHardTabs::default();
    let content = "Line with\ttab\n\nAnother\t\ttabs\n";
    assert_fix_idempotent(&rule, content, "MD010");
}

// ============================================================================
// MD012 - No Multiple Blanks
// ============================================================================

#[test]
fn test_md012_fix_idempotent() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "# Title\n\n\n\nParagraph\n\n\n\nEnd\n";
    assert_fix_idempotent(&rule, content, "MD012");
}

// ============================================================================
// MD014 - Commands Show Output
// ============================================================================

#[test]
fn test_md014_fix_idempotent() {
    let rule = MD014CommandsShowOutput::default();
    let content = "```sh\n$ echo hello\n$ ls\n```\n";
    assert_fix_idempotent(&rule, content, "MD014");
}

// ============================================================================
// MD018 - No Missing Space ATX
// ============================================================================

#[test]
fn test_md018_fix_idempotent() {
    let rule = MD018NoMissingSpaceAtx;
    let content = "#Title\n\n##Subtitle\n";
    assert_fix_idempotent(&rule, content, "MD018");
}

// ============================================================================
// MD019 - No Multiple Space ATX
// ============================================================================

#[test]
fn test_md019_fix_idempotent() {
    let rule = MD019NoMultipleSpaceAtx;
    let content = "#  Title\n\n##   Subtitle\n";
    assert_fix_idempotent(&rule, content, "MD019");
}

// ============================================================================
// MD020 - No Missing Space Closed ATX
// ============================================================================

#[test]
fn test_md020_fix_idempotent() {
    let rule = MD020NoMissingSpaceClosedAtx;
    let content = "#Title#\n\n##Subtitle##\n";
    assert_fix_idempotent(&rule, content, "MD020");
}

// ============================================================================
// MD021 - No Multiple Space Closed ATX
// ============================================================================

#[test]
fn test_md021_fix_idempotent() {
    let rule = MD021NoMultipleSpaceClosedAtx;
    let content = "#  Title  #\n\n##   Subtitle   ##\n";
    assert_fix_idempotent(&rule, content, "MD021");
}

// ============================================================================
// MD022 - Blanks Around Headings
// ============================================================================

#[test]
fn test_md022_fix_idempotent() {
    let rule = MD022BlanksAroundHeadings::default();
    let content = "# Title\nParagraph\n## Subtitle\nMore text\n";
    assert_fix_idempotent(&rule, content, "MD022");
}

// ============================================================================
// MD023 - Heading Start Left
// ============================================================================

#[test]
fn test_md023_fix_idempotent() {
    let rule = MD023HeadingStartLeft;
    let content = "  # Indented Title\n\n   ## More Indented\n";
    assert_fix_idempotent(&rule, content, "MD023");
}

// ============================================================================
// MD026 - No Trailing Punctuation
// ============================================================================

#[test]
fn test_md026_fix_idempotent() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Title:\n\n## Subtitle!\n";
    assert_fix_idempotent(&rule, content, "MD026");
}

// ============================================================================
// MD027 - Multiple Spaces Blockquote
// ============================================================================

#[test]
fn test_md027_fix_idempotent() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = ">  Multiple spaces\n>   More spaces\n";
    assert_fix_idempotent(&rule, content, "MD027");
}

// ============================================================================
// MD028 - No Blanks Blockquote
// ============================================================================

#[test]
fn test_md028_fix_idempotent() {
    let rule = MD028NoBlanksBlockquote;
    let content = "> First quote\n\n> Second quote\n";
    assert_fix_idempotent(&rule, content, "MD028");
}

// ============================================================================
// MD029 - Ordered List Prefix
// ============================================================================

#[test]
fn test_md029_fix_idempotent_ordered() {
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let content = "1. First\n1. Second\n1. Third\n";
    assert_fix_idempotent(&rule, content, "MD029");
}

#[test]
fn test_md029_fix_idempotent_one() {
    let rule = MD029OrderedListPrefix::new(ListStyle::One);
    let content = "1. First\n2. Second\n3. Third\n";
    assert_fix_idempotent(&rule, content, "MD029");
}

// ============================================================================
// MD030 - List Marker Space
// ============================================================================

#[test]
fn test_md030_fix_idempotent() {
    let rule = MD030ListMarkerSpace::default();
    let content = "-  Two spaces\n-   Three spaces\n";
    assert_fix_idempotent(&rule, content, "MD030");
}

// ============================================================================
// MD031 - Blanks Around Fences
// ============================================================================

#[test]
fn test_md031_fix_idempotent() {
    let rule = MD031BlanksAroundFences::default();
    let content = "Text\n```\ncode\n```\nMore text\n";
    assert_fix_idempotent(&rule, content, "MD031");
}

// ============================================================================
// MD032 - Blanks Around Lists
// ============================================================================

#[test]
fn test_md032_fix_idempotent() {
    let rule = MD032BlanksAroundLists::default();
    let content = "Text\n- Item 1\n- Item 2\nMore text\n";
    assert_fix_idempotent(&rule, content, "MD032");
}

// ============================================================================
// MD032 - Blanks Around Lists - Edge Cases
// ============================================================================

// TODO: Known idempotency bug found by proptest
// The fix keeps adding blank lines on subsequent passes
#[test]
#[ignore = "MD032 idempotency bug - discovered by proptest"]
fn test_md032_edge_case_proptest_found() {
    let rule = MD032BlanksAroundLists::default();
    // This specific input triggers the bug
    let content = "- \n# \n**\n2. \n# ";
    assert_fix_idempotent(&rule, content, "MD032");
}

// ============================================================================
// MD034 - No Bare URLs
// ============================================================================

#[test]
fn test_md034_fix_idempotent() {
    let rule = MD034NoBareUrls;
    let content = "Visit https://example.com for more info.\n";
    assert_fix_idempotent(&rule, content, "MD034");
}

// ============================================================================
// MD035 - HR Style
// ============================================================================

#[test]
fn test_md035_fix_idempotent() {
    let rule = MD035HRStyle::new("---".to_string());
    let content = "# Title\n\n***\n\nText\n";
    assert_fix_idempotent(&rule, content, "MD035");
}

// ============================================================================
// MD037 - Spaces Around Emphasis
// ============================================================================

#[test]
fn test_md037_fix_idempotent() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "This is * emphasized * text.\n";
    assert_fix_idempotent(&rule, content, "MD037");
}

// ============================================================================
// MD038 - No Space In Code
// ============================================================================

#[test]
fn test_md038_fix_idempotent() {
    let rule = MD038NoSpaceInCode::default();
    let content = "Use ` code ` here.\n";
    assert_fix_idempotent(&rule, content, "MD038");
}

// ============================================================================
// MD039 - No Space In Links
// ============================================================================

#[test]
fn test_md039_fix_idempotent() {
    let rule = MD039NoSpaceInLinks;
    let content = "Click [ here ](https://example.com) to continue.\n";
    assert_fix_idempotent(&rule, content, "MD039");
}

// ============================================================================
// MD044 - Proper Names
// ============================================================================

#[test]
fn test_md044_fix_idempotent() {
    let rule = MD044ProperNames::new(
        vec!["JavaScript".to_string(), "TypeScript".to_string()],
        false, // code_blocks
    );
    let content = "Learn javascript and typescript.\n";
    assert_fix_idempotent(&rule, content, "MD044");
}

// ============================================================================
// MD047 - Single Trailing Newline
// ============================================================================

#[test]
fn test_md047_fix_idempotent_no_newline() {
    let rule = MD047SingleTrailingNewline;
    let content = "Content without trailing newline";
    assert_fix_idempotent(&rule, content, "MD047");
}

#[test]
fn test_md047_fix_idempotent_multiple_newlines() {
    let rule = MD047SingleTrailingNewline;
    let content = "Content with multiple trailing newlines\n\n\n";
    assert_fix_idempotent(&rule, content, "MD047");
}

// ============================================================================
// MD048 - Code Fence Style
// ============================================================================

#[test]
fn test_md048_fix_idempotent_backtick() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
    let content = "~~~\ncode\n~~~\n";
    assert_fix_idempotent(&rule, content, "MD048");
}

#[test]
fn test_md048_fix_idempotent_tilde() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
    let content = "```\ncode\n```\n";
    assert_fix_idempotent(&rule, content, "MD048");
}

// ============================================================================
// MD049 - Emphasis Style
// ============================================================================

#[test]
fn test_md049_fix_idempotent_asterisk() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
    let content = "This is _emphasized_ text.\n";
    assert_fix_idempotent(&rule, content, "MD049");
}

#[test]
fn test_md049_fix_idempotent_underscore() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Underscore);
    let content = "This is *emphasized* text.\n";
    assert_fix_idempotent(&rule, content, "MD049");
}

// ============================================================================
// MD050 - Strong Style
// ============================================================================

#[test]
fn test_md050_fix_idempotent_asterisk() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = "This is __strong__ text.\n";
    assert_fix_idempotent(&rule, content, "MD050");
}

#[test]
fn test_md050_fix_idempotent_underscore() {
    let rule = MD050StrongStyle::new(StrongStyle::Underscore);
    let content = "This is **strong** text.\n";
    assert_fix_idempotent(&rule, content, "MD050");
}

// ============================================================================
// MD058 - Blanks Around Tables
// ============================================================================

#[test]
fn test_md058_fix_idempotent() {
    let rule = MD058BlanksAroundTables::default();
    let content = "Text\n| A | B |\n|---|---|\n| 1 | 2 |\nMore text\n";
    assert_fix_idempotent(&rule, content, "MD058");
}

// ============================================================================
// MD064 - No Multiple Consecutive Spaces
// ============================================================================

#[test]
fn test_md064_fix_idempotent() {
    let rule = MD064NoMultipleConsecutiveSpaces::default();
    let content = "Text with  multiple   spaces.\n";
    assert_fix_idempotent(&rule, content, "MD064");
}

// ============================================================================
// MD065 - Blanks Around Horizontal Rules
// ============================================================================

#[test]
fn test_md065_fix_idempotent() {
    let rule = MD065BlanksAroundHorizontalRules;
    let content = "Text\n---\nMore text\n";
    assert_fix_idempotent(&rule, content, "MD065");
}

// ============================================================================
// MD071 - Blank Line After Frontmatter
// ============================================================================

#[test]
fn test_md071_fix_idempotent() {
    let rule = MD071BlankLineAfterFrontmatter;
    let content = "---\ntitle: Test\n---\n# Title\n";
    assert_fix_idempotent(&rule, content, "MD071");
}

// ============================================================================
// MD072 - Frontmatter Key Sort
// ============================================================================

#[test]
fn test_md072_fix_idempotent() {
    let rule = MD072FrontmatterKeySort::default();
    let content = "---\nzebra: 1\napple: 2\n---\n\n# Title\n";
    assert_fix_idempotent(&rule, content, "MD072");
}

// ============================================================================
// Complex scenarios combining multiple issues
// ============================================================================

#[test]
fn test_complex_document_idempotent() {
    // Test a document that triggers multiple rules
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD012NoMultipleBlanks::default()),
        Box::new(MD022BlanksAroundHeadings::default()),
        Box::new(MD032BlanksAroundLists::default()),
        Box::new(MD047SingleTrailingNewline),
    ];

    let content = "# Title   \n\n\n\nParagraph\n- Item 1\n- Item 2\nMore text\n\n";

    let mut current = content.to_string();

    // Apply all fixes once
    for rule in &rules {
        let ctx = LintContext::new(&current, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap_or_default();
        current = apply_all_fixes(&current, &warnings);
    }
    let after_first = current.clone();

    // Apply all fixes again
    for rule in &rules {
        let ctx = LintContext::new(&current, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap_or_default();
        current = apply_all_fixes(&current, &warnings);
    }
    let after_second = current;

    assert_eq!(
        after_first, after_second,
        "Combined fixes are not idempotent!\nAfter first pass:\n{after_first:?}\nAfter second pass:\n{after_second:?}"
    );
}
