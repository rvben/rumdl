// Property-based tests for formatter robustness and idempotency
// These tests use proptest to generate random markdown content and verify:
// 1. Rules don't crash on arbitrary input
// 2. Fixes are idempotent (applying twice gives same result)

use proptest::prelude::*;
use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::{LintWarning, Rule};
use rumdl_lib::rules::*;

/// Apply all fixes from warnings to content
fn apply_all_fixes(content: &str, warnings: &[LintWarning]) -> String {
    let mut fixes: Vec<_> = warnings.iter().filter_map(|w| w.fix.as_ref()).collect();
    fixes.sort_by(|a, b| b.range.start.cmp(&a.range.start));

    let mut result = content.to_string();
    for fix in fixes {
        // Validate range is within bounds and on character boundaries
        if fix.range.end <= result.len()
            && result.is_char_boundary(fix.range.start)
            && result.is_char_boundary(fix.range.end)
        {
            result.replace_range(fix.range.clone(), &fix.replacement);
        }
    }
    result
}

/// Strategy for generating markdown-like content
fn markdown_content_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(markdown_line_strategy(), 0..20).prop_map(|lines| lines.join("\n"))
}

/// Strategy for generating individual markdown lines
fn markdown_line_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // Headings
        (
            1..7u8,
            any::<String>().prop_filter("valid heading text", |s| { s.len() < 100 && !s.contains('\n') })
        )
            .prop_map(|(level, text)| format!("{} {}", "#".repeat(level as usize), text)),
        // List items
        any::<String>()
            .prop_filter("valid list text", |s| s.len() < 100 && !s.contains('\n'))
            .prop_map(|text| format!("- {text}")),
        // Ordered list items
        (
            1..100u32,
            any::<String>().prop_filter("valid list text", |s| { s.len() < 100 && !s.contains('\n') })
        )
            .prop_map(|(num, text)| format!("{num}. {text}")),
        // Blockquotes
        any::<String>()
            .prop_filter("valid quote text", |s| s.len() < 100 && !s.contains('\n'))
            .prop_map(|text| format!("> {text}")),
        // Code blocks
        prop::collection::vec(
            any::<String>().prop_filter("valid code", |s| s.len() < 50 && !s.contains("```")),
            0..5
        )
        .prop_map(|lines| format!("```\n{}\n```", lines.join("\n"))),
        // Inline code
        any::<String>()
            .prop_filter("valid inline code", |s| s.len() < 50 && !s.contains('`'))
            .prop_map(|text| format!("`{text}`")),
        // Links
        (
            any::<String>().prop_filter("valid link text", |s| s.len() < 30 && !s.contains(&['[', ']'][..])),
            any::<String>().prop_filter("valid url", |s| s.len() < 50 && !s.contains(&['(', ')'][..]))
        )
            .prop_map(|(text, url)| format!("[{text}]({url})")),
        // Emphasis
        any::<String>()
            .prop_filter("valid emphasis text", |s| s.len() < 50 && !s.contains('*'))
            .prop_map(|text| format!("*{text}*")),
        // Strong
        any::<String>()
            .prop_filter("valid strong text", |s| s.len() < 50 && !s.contains("**"))
            .prop_map(|text| format!("**{text}**")),
        // Plain text
        any::<String>().prop_filter("valid text", |s| s.len() < 200 && !s.contains('\n')),
        // Blank line
        Just("".to_string()),
        // Horizontal rule
        prop_oneof![
            Just("---".to_string()),
            Just("***".to_string()),
            Just("___".to_string()),
        ],
        // Images
        (
            any::<String>().prop_filter("valid alt text", |s| s.len() < 30 && !s.contains(&['[', ']'][..])),
            any::<String>().prop_filter("valid url", |s| s.len() < 50 && !s.contains(&['(', ')'][..]))
        )
            .prop_map(|(alt, url)| format!("![{alt}]({url})")),
        // HTML inline
        any::<String>()
            .prop_filter("valid html text", |s| s.len() < 50 && !s.contains(&['<', '>'][..]))
            .prop_map(|text| format!("<span>{text}</span>")),
        // Tables
        (
            any::<String>().prop_filter("valid cell", |s| s.len() < 20 && !s.contains(&['|', '\n'][..])),
            any::<String>().prop_filter("valid cell", |s| s.len() < 20 && !s.contains(&['|', '\n'][..]))
        )
            .prop_map(|(c1, c2)| format!("| {c1} | {c2} |\n| --- | --- |")),
    ]
}

/// Strategy for generating completely random strings (for crash testing)
fn random_content_strategy() -> impl Strategy<Value = String> {
    any::<String>().prop_filter("reasonable size", |s| s.len() < 10000)
}

// ============================================================================
// Crash Resistance Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn test_lint_context_no_crash(content in random_content_strategy()) {
        // LintContext creation should never crash
        let _ = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let _ = LintContext::new(&content, MarkdownFlavor::MkDocs, None);
        let _ = LintContext::new(&content, MarkdownFlavor::MDX, None);
        let _ = LintContext::new(&content, MarkdownFlavor::Quarto, None);
    }

    #[test]
    fn test_rules_no_crash(content in markdown_content_strategy()) {
        let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);

        // All rules should never crash on check() or fix()
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(MD001HeadingIncrement::default()),
            Box::new(MD003HeadingStyle::default()),
            Box::new(MD004UnorderedListStyle::default()),
            Box::new(MD005ListIndent::default()),
            Box::new(MD007ULIndent::default()),
            Box::new(MD009TrailingSpaces::default()),
            Box::new(MD010NoHardTabs::default()),
            Box::new(MD011NoReversedLinks),
            Box::new(MD012NoMultipleBlanks::default()),
            Box::new(MD013LineLength::default()),
            Box::new(MD014CommandsShowOutput::default()),
            Box::new(MD018NoMissingSpaceAtx::new()),
            Box::new(MD019NoMultipleSpaceAtx),
            Box::new(MD020NoMissingSpaceClosedAtx),
            Box::new(MD021NoMultipleSpaceClosedAtx),
            Box::new(MD022BlanksAroundHeadings::default()),
            Box::new(MD023HeadingStartLeft),
            Box::new(MD024NoDuplicateHeading::default()),
            Box::new(MD025SingleTitle::default()),
            Box::new(MD026NoTrailingPunctuation::default()),
            Box::new(MD027MultipleSpacesBlockquote),
            Box::new(MD028NoBlanksBlockquote),
            Box::new(MD029OrderedListPrefix::default()),
            Box::new(MD030ListMarkerSpace::default()),
            Box::new(MD031BlanksAroundFences::default()),
            Box::new(MD032BlanksAroundLists::default()),
            Box::new(MD033NoInlineHtml::default()),
            Box::new(MD034NoBareUrls),
            Box::new(MD035HRStyle::default()),
            Box::new(MD036NoEmphasisAsHeading::default()),
            Box::new(MD037NoSpaceInEmphasis),
            Box::new(MD038NoSpaceInCode::default()),
            Box::new(MD039NoSpaceInLinks),
            Box::new(MD040FencedCodeLanguage::default()),
            Box::new(MD041FirstLineHeading::default()),
            Box::new(MD042NoEmptyLinks::default()),
            Box::new(MD043RequiredHeadings::new(vec![])),
            Box::new(MD044ProperNames::new(vec![], true)),
            Box::new(MD045NoAltText::default()),
            Box::new(MD046CodeBlockStyle::new(rumdl_lib::rules::code_block_utils::CodeBlockStyle::Fenced)),
            Box::new(MD047SingleTrailingNewline),
            Box::new(MD048CodeFenceStyle::new(rumdl_lib::rules::code_fence_utils::CodeFenceStyle::Backtick)),
            Box::new(MD049EmphasisStyle::default()),
            Box::new(MD050StrongStyle::default()),
            Box::new(MD051LinkFragments::default()),
            Box::new(MD052ReferenceLinkImages::default()),
            Box::new(MD053LinkImageReferenceDefinitions::default()),
            Box::new(MD054LinkImageStyle::default()),
            Box::new(MD055TablePipeStyle::default()),
            Box::new(MD056TableColumnCount),
            Box::new(MD057ExistingRelativeLinks::default()),
            Box::new(MD058BlanksAroundTables::default()),
            Box::new(MD059LinkText::default()),
            Box::new(MD060TableFormat::default()),
            Box::new(MD061ForbiddenTerms::default()),
            Box::new(MD062LinkDestinationWhitespace),
            Box::new(MD063HeadingCapitalization::default()),
            Box::new(MD064NoMultipleConsecutiveSpaces::default()),
            Box::new(MD065BlanksAroundHorizontalRules),
            Box::new(MD066FootnoteValidation),
            Box::new(MD067FootnoteDefinitionOrder),
            Box::new(MD068EmptyFootnoteDefinition),
            Box::new(MD069NoDuplicateListMarkers),
            Box::new(MD070NestedCodeFence),
            Box::new(MD071BlankLineAfterFrontmatter),
            Box::new(MD072FrontmatterKeySort::default()),
            Box::new(MD073TocValidation::default()),
            Box::new(MD074MkDocsNav::default()),
        ];

        for rule in &rules {
            let _ = rule.check(&ctx);
            let _ = rule.fix(&ctx);
        }
    }
}

// ============================================================================
// Idempotency Tests
// ============================================================================
// Rules with auto-fix capability are tested for idempotency:
// apply fix twice → result should be identical.
// Rules without auto-fix (MD024, MD053, MD057, MD066, MD068, MD074) are skipped.

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn test_md001_idempotent(content in markdown_content_strategy()) {
        let rule = MD001HeadingIncrement::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD001 fix not idempotent");
    }

    #[test]
    fn test_md003_idempotent(content in markdown_content_strategy()) {
        let rule = MD003HeadingStyle::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD003 fix not idempotent");
    }

    #[test]
    fn test_md004_idempotent(content in markdown_content_strategy()) {
        let rule = MD004UnorderedListStyle::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD004 fix not idempotent");
    }

    #[test]
    fn test_md005_idempotent(content in markdown_content_strategy()) {
        let rule = MD005ListIndent::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD005 fix not idempotent");
    }

    #[test]
    fn test_md007_idempotent(content in markdown_content_strategy()) {
        let rule = MD007ULIndent::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD007 fix not idempotent");
    }

    #[test]
    fn test_md009_idempotent(content in markdown_content_strategy()) {
        let rule = MD009TrailingSpaces::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD009 fix not idempotent");
    }

    #[test]
    fn test_md010_idempotent(content in markdown_content_strategy()) {
        let rule = MD010NoHardTabs::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD010 fix not idempotent");
    }

    #[test]
    fn test_md011_idempotent(content in markdown_content_strategy()) {
        let rule = MD011NoReversedLinks;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD011 fix not idempotent");
    }

    #[test]
    fn test_md012_idempotent(content in markdown_content_strategy()) {
        let rule = MD012NoMultipleBlanks::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD012 fix not idempotent");
    }

    #[test]
    fn test_md014_idempotent(content in markdown_content_strategy()) {
        let rule = MD014CommandsShowOutput::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD014 fix not idempotent");
    }

    #[test]
    fn test_md018_idempotent(content in markdown_content_strategy()) {
        let rule = MD018NoMissingSpaceAtx::new();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD018 fix not idempotent");
    }

    #[test]
    fn test_md019_idempotent(content in markdown_content_strategy()) {
        let rule = MD019NoMultipleSpaceAtx;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD019 fix not idempotent");
    }

    #[test]
    fn test_md020_idempotent(content in markdown_content_strategy()) {
        let rule = MD020NoMissingSpaceClosedAtx;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD020 fix not idempotent");
    }

    #[test]
    fn test_md021_idempotent(content in markdown_content_strategy()) {
        let rule = MD021NoMultipleSpaceClosedAtx;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD021 fix not idempotent");
    }

    #[test]
    fn test_md022_idempotent(content in markdown_content_strategy()) {
        let rule = MD022BlanksAroundHeadings::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD022 fix not idempotent");
    }

    #[test]
    fn test_md023_idempotent(content in markdown_content_strategy()) {
        let rule = MD023HeadingStartLeft;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD023 fix not idempotent");
    }

    #[test]
    fn test_md025_idempotent(content in markdown_content_strategy()) {
        let rule = MD025SingleTitle::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD025 fix not idempotent");
    }

    #[test]
    fn test_md026_idempotent(content in markdown_content_strategy()) {
        let rule = MD026NoTrailingPunctuation::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD026 fix not idempotent");
    }

    #[test]
    fn test_md027_idempotent(content in markdown_content_strategy()) {
        let rule = MD027MultipleSpacesBlockquote;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD027 fix not idempotent");
    }

    #[test]
    fn test_md028_idempotent(content in markdown_content_strategy()) {
        let rule = MD028NoBlanksBlockquote;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD028 fix not idempotent");
    }

    #[test]
    fn test_md029_idempotent(content in markdown_content_strategy()) {
        let rule = MD029OrderedListPrefix::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD029 fix not idempotent");
    }

    #[test]
    fn test_md030_idempotent(content in markdown_content_strategy()) {
        let rule = MD030ListMarkerSpace::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD030 fix not idempotent");
    }

    #[test]
    fn test_md031_idempotent(content in markdown_content_strategy()) {
        let rule = MD031BlanksAroundFences::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD031 fix not idempotent");
    }

    #[test]
    fn test_md032_idempotent(content in markdown_content_strategy()) {
        // MD032 uses a structural fix() method because inserting blank lines
        // changes CommonMark list block boundaries. For complex inputs
        // (blockquotes inside lists, code fences adjacent to lists), the fix
        // may need 2 passes to stabilize. We verify convergence within 3 passes.
        let rule = MD032BlanksAroundLists::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let fixed1 = rule.fix(&ctx1).unwrap_or_else(|_| content.to_string());

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap_or_else(|_| fixed1.clone());

        if fixed1 != fixed2 {
            // Allow one more pass for complex cases (blockquotes in lists, etc.)
            let ctx3 = LintContext::new(&fixed2, MarkdownFlavor::Standard, None);
            let fixed3 = rule.fix(&ctx3).unwrap_or_else(|_| fixed2.clone());
            prop_assert_eq!(fixed2, fixed3, "MD032 fix did not converge within 3 passes");
        }
    }

    #[test]
    fn test_md033_idempotent(content in markdown_content_strategy()) {
        let rule = MD033NoInlineHtml::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD033 fix not idempotent");
    }

    #[test]
    fn test_md034_idempotent(content in markdown_content_strategy()) {
        let rule = MD034NoBareUrls;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD034 fix not idempotent");
    }

    #[test]
    fn test_md035_idempotent(content in markdown_content_strategy()) {
        let rule = MD035HRStyle::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD035 fix not idempotent");
    }

    #[test]
    fn test_md036_idempotent(content in markdown_content_strategy()) {
        let rule = MD036NoEmphasisAsHeading::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD036 fix not idempotent");
    }

    #[test]
    fn test_md037_idempotent(content in markdown_content_strategy()) {
        let rule = MD037NoSpaceInEmphasis;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD037 fix not idempotent");
    }

    #[test]
    fn test_md038_idempotent(content in markdown_content_strategy()) {
        let rule = MD038NoSpaceInCode::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD038 fix not idempotent");
    }

    #[test]
    fn test_md039_idempotent(content in markdown_content_strategy()) {
        let rule = MD039NoSpaceInLinks;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD039 fix not idempotent");
    }

    #[test]
    fn test_md040_idempotent(content in markdown_content_strategy()) {
        let rule = MD040FencedCodeLanguage::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD040 fix not idempotent");
    }

    #[test]
    fn test_md041_idempotent(content in markdown_content_strategy()) {
        let rule = MD041FirstLineHeading::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD041 fix not idempotent");
    }

    #[test]
    fn test_md042_idempotent(content in markdown_content_strategy()) {
        let rule = MD042NoEmptyLinks::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD042 fix not idempotent");
    }

    #[test]
    fn test_md044_idempotent(content in markdown_content_strategy()) {
        let rule = MD044ProperNames::new(vec![], true);

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD044 fix not idempotent");
    }

    #[test]
    fn test_md045_idempotent(content in markdown_content_strategy()) {
        let rule = MD045NoAltText::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD045 fix not idempotent");
    }

    #[test]
    fn test_md046_idempotent(content in markdown_content_strategy()) {
        let rule = MD046CodeBlockStyle::new(rumdl_lib::rules::code_block_utils::CodeBlockStyle::Fenced);

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD046 fix not idempotent");
    }

    #[test]
    fn test_md047_idempotent(content in markdown_content_strategy()) {
        let rule = MD047SingleTrailingNewline;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD047 fix not idempotent");
    }

    #[test]
    fn test_md048_idempotent(content in markdown_content_strategy()) {
        let rule = MD048CodeFenceStyle::new(rumdl_lib::rules::code_fence_utils::CodeFenceStyle::Backtick);

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD048 fix not idempotent");
    }

    #[test]
    fn test_md049_idempotent(content in markdown_content_strategy()) {
        let rule = MD049EmphasisStyle::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD049 fix not idempotent");
    }

    #[test]
    fn test_md050_idempotent(content in markdown_content_strategy()) {
        let rule = MD050StrongStyle::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD050 fix not idempotent");
    }

    #[test]
    fn test_md051_idempotent(content in markdown_content_strategy()) {
        let rule = MD051LinkFragments::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD051 fix not idempotent");
    }

    #[test]
    fn test_md052_idempotent(content in markdown_content_strategy()) {
        let rule = MD052ReferenceLinkImages::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD052 fix not idempotent");
    }

    #[test]
    fn test_md054_idempotent(content in markdown_content_strategy()) {
        let rule = MD054LinkImageStyle::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD054 fix not idempotent");
    }

    #[test]
    fn test_md055_idempotent(content in markdown_content_strategy()) {
        let rule = MD055TablePipeStyle::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD055 fix not idempotent");
    }

    #[test]
    fn test_md056_idempotent(content in markdown_content_strategy()) {
        let rule = MD056TableColumnCount;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD056 fix not idempotent");
    }

    #[test]
    fn test_md058_idempotent(content in markdown_content_strategy()) {
        // MD058 uses fix() because inserting blank lines around tables changes
        // document structure, which can reveal new tables. Like MD032, the fix
        // may need 2 passes to stabilize. We verify convergence within 3 passes.
        let rule = MD058BlanksAroundTables::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let fixed1 = rule.fix(&ctx1).unwrap_or_else(|_| content.to_string());

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap_or_else(|_| fixed1.clone());

        if fixed1 != fixed2 {
            // Allow one more pass for complex cases (tables in lists, etc.)
            let ctx3 = LintContext::new(&fixed2, MarkdownFlavor::Standard, None);
            let fixed3 = rule.fix(&ctx3).unwrap_or_else(|_| fixed2.clone());
            prop_assert_eq!(fixed2, fixed3, "MD058 fix did not converge within 3 passes");
        }
    }

    #[test]
    fn test_md059_idempotent(content in markdown_content_strategy()) {
        let rule = MD059LinkText::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD059 fix not idempotent");
    }

    #[test]
    fn test_md060_idempotent(content in markdown_content_strategy()) {
        let rule = MD060TableFormat::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD060 fix not idempotent");
    }

    #[test]
    fn test_md061_idempotent(content in markdown_content_strategy()) {
        let rule = MD061ForbiddenTerms::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD061 fix not idempotent");
    }

    #[test]
    fn test_md062_idempotent(content in markdown_content_strategy()) {
        let rule = MD062LinkDestinationWhitespace;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD062 fix not idempotent");
    }

    #[test]
    fn test_md063_idempotent(content in markdown_content_strategy()) {
        let rule = MD063HeadingCapitalization::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD063 fix not idempotent");
    }

    #[test]
    fn test_md064_idempotent(content in markdown_content_strategy()) {
        let rule = MD064NoMultipleConsecutiveSpaces::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD064 fix not idempotent");
    }

    #[test]
    fn test_md065_idempotent(content in markdown_content_strategy()) {
        let rule = MD065BlanksAroundHorizontalRules;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD065 fix not idempotent");
    }

    #[test]
    fn test_md067_idempotent(content in markdown_content_strategy()) {
        let rule = MD067FootnoteDefinitionOrder;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD067 fix not idempotent");
    }

    #[test]
    fn test_md069_idempotent(content in markdown_content_strategy()) {
        let rule = MD069NoDuplicateListMarkers;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD069 fix not idempotent");
    }

    #[test]
    fn test_md070_idempotent(content in markdown_content_strategy()) {
        let rule = MD070NestedCodeFence;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD070 fix not idempotent");
    }

    #[test]
    fn test_md071_idempotent(content in markdown_content_strategy()) {
        let rule = MD071BlankLineAfterFrontmatter;

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD071 fix not idempotent");
    }

    #[test]
    fn test_md072_idempotent(content in markdown_content_strategy()) {
        let rule = MD072FrontmatterKeySort::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD072 fix not idempotent");
    }

    #[test]
    fn test_md073_idempotent(content in markdown_content_strategy()) {
        let rule = MD073TocValidation::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings1 = rule.check(&ctx1).unwrap_or_default();
        let fixed1 = apply_all_fixes(&content, &warnings1);

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap_or_default();
        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

        prop_assert_eq!(fixed1, fixed2, "MD073 fix not idempotent");
    }
}
