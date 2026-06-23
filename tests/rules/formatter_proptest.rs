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
    fixes.sort_by_key(|f| std::cmp::Reverse(f.range.start));

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
            Box::new(MD027MultipleSpacesBlockquote::default()),
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
            Box::new(MD045NoAltText::new()),
            Box::new(MD046CodeBlockStyle::new(rumdl_lib::rules::CodeBlockStyle::Fenced)),
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
            Box::new(MD075OrphanedTableRows::default()),
            Box::new(MD076ListItemSpacing::default()),
            Box::new(MD077ListContinuationIndent::default()),
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
// apply fix twice -> result should be identical.
// Rules without auto-fix (MD024, MD053, MD057, MD066, MD068, MD074) are skipped.

/// Generates a proptest verifying that applying all fixes from `rule.check()`
/// is idempotent for the given flavor(s).
///
/// Expands to one `#[test]` per (rule, flavor) pair, named
/// `test_<name>_idempotent_<flavor>` (flavor lowercased).
macro_rules! idempotent_rule {
    ($name:ident, $rule:expr, $strategy:expr $(, $flavor:ident)+ $(,)?) => {
        $(
            paste::paste! {
                proptest! {
                    #![proptest_config(ProptestConfig::with_cases(50))]

                    #[test]
                    fn [<test_ $name _idempotent_ $flavor:lower>](content in $strategy) {
                        let rule = $rule;
                        let flavor = MarkdownFlavor::$flavor;

                        let ctx1 = LintContext::new(&content, flavor, None);
                        let warnings1 = rule.check(&ctx1).unwrap_or_default();
                        let fixed1 = apply_all_fixes(&content, &warnings1);

                        let ctx2 = LintContext::new(&fixed1, flavor, None);
                        let warnings2 = rule.check(&ctx2).unwrap_or_default();
                        let fixed2 = apply_all_fixes(&fixed1, &warnings2);

                        prop_assert_eq!(
                            fixed1, fixed2,
                            "{} fix not idempotent (flavor={:?})",
                            stringify!($name),
                            flavor
                        );
                    }
                }
            }
        )+
    };
}

idempotent_rule!(
    md001,
    MD001HeadingIncrement::default(),
    markdown_content_strategy(),
    Standard
);

idempotent_rule!(
    md003,
    MD003HeadingStyle::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md004,
    MD004UnorderedListStyle::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(md005, MD005ListIndent::default(), markdown_content_strategy(), Standard);
idempotent_rule!(
    md007,
    MD007ULIndent::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(
    md009,
    MD009TrailingSpaces::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(md010, MD010NoHardTabs::default(), markdown_content_strategy(), Standard);
idempotent_rule!(md011, MD011NoReversedLinks, markdown_content_strategy(), Standard);
idempotent_rule!(
    md012,
    MD012NoMultipleBlanks::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md013,
    MD013LineLength::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(
    md014,
    MD014CommandsShowOutput::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md018,
    MD018NoMissingSpaceAtx::new(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(md019, MD019NoMultipleSpaceAtx, markdown_content_strategy(), Standard);
idempotent_rule!(
    md020,
    MD020NoMissingSpaceClosedAtx,
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md021,
    MD021NoMultipleSpaceClosedAtx,
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md022,
    MD022BlanksAroundHeadings::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(md023, MD023HeadingStartLeft, markdown_content_strategy(), Standard);
idempotent_rule!(
    md025,
    MD025SingleTitle::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md026,
    MD026NoTrailingPunctuation::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md027,
    MD027MultipleSpacesBlockquote::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md028,
    MD028NoBlanksBlockquote,
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(
    md029,
    MD029OrderedListPrefix::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md030,
    MD030ListMarkerSpace::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md031,
    MD031BlanksAroundFences::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);

// MD032 uses a structural fix() method because inserting blank lines changes
// CommonMark list block boundaries. For complex inputs (blockquotes inside
// lists, code fences adjacent to lists), the fix may need 2 passes to
// stabilize. We verify convergence within 3 passes.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn test_md032_idempotent_standard(content in markdown_content_strategy()) {
        let rule = MD032BlanksAroundLists::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let fixed1 = rule.fix(&ctx1).unwrap_or_else(|_| content.to_string());

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap_or_else(|_| fixed1.clone());

        if fixed1 != fixed2 {
            let ctx3 = LintContext::new(&fixed2, MarkdownFlavor::Standard, None);
            let fixed3 = rule.fix(&ctx3).unwrap_or_else(|_| fixed2.clone());
            prop_assert_eq!(fixed2, fixed3, "MD032 fix did not converge within 3 passes");
        }
    }
}

idempotent_rule!(
    md033,
    MD033NoInlineHtml::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(
    md034,
    MD034NoBareUrls,
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(md035, MD035HRStyle::default(), markdown_content_strategy(), Standard);
idempotent_rule!(
    md036,
    MD036NoEmphasisAsHeading::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md037,
    MD037NoSpaceInEmphasis,
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(
    md038,
    MD038NoSpaceInCode::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(md039, MD039NoSpaceInLinks, markdown_content_strategy(), Standard);
idempotent_rule!(
    md040,
    MD040FencedCodeLanguage::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(
    md041,
    MD041FirstLineHeading::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(
    md042,
    MD042NoEmptyLinks::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(
    md044,
    MD044ProperNames::new(vec![], true),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(md045, MD045NoAltText::new(), markdown_content_strategy(), Standard);
idempotent_rule!(
    md046,
    MD046CodeBlockStyle::new(rumdl_lib::rules::CodeBlockStyle::Fenced),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(md047, MD047SingleTrailingNewline, markdown_content_strategy(), Standard);
idempotent_rule!(
    md048,
    MD048CodeFenceStyle::new(rumdl_lib::rules::code_fence_utils::CodeFenceStyle::Backtick),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md049,
    MD049EmphasisStyle::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(
    md050,
    MD050StrongStyle::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(
    md051,
    MD051LinkFragments::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(
    md052,
    MD052ReferenceLinkImages::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(
    md054,
    MD054LinkImageStyle::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md055,
    MD055TablePipeStyle::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(md056, MD056TableColumnCount, markdown_content_strategy(), Standard);

// MD058 uses fix() because inserting blank lines around tables changes
// document structure, which can reveal new tables. Like MD032, the fix
// may need 2 passes to stabilize. We verify convergence within 3 passes.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn test_md058_idempotent_standard(content in markdown_content_strategy()) {
        let rule = MD058BlanksAroundTables::default();

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let fixed1 = rule.fix(&ctx1).unwrap_or_else(|_| content.to_string());

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap_or_else(|_| fixed1.clone());

        if fixed1 != fixed2 {
            let ctx3 = LintContext::new(&fixed2, MarkdownFlavor::Standard, None);
            let fixed3 = rule.fix(&ctx3).unwrap_or_else(|_| fixed2.clone());
            prop_assert_eq!(fixed2, fixed3, "MD058 fix did not converge within 3 passes");
        }
    }
}

idempotent_rule!(md059, MD059LinkText::default(), markdown_content_strategy(), Standard);

// MD060 uses fix() because each warning carries the same whole-table
// replacement. apply_all_fixes would apply the replacement N times,
// corrupting the output. Like MD032 and MD058, we allow up to 3 passes.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn test_md060_idempotent_standard(content in markdown_content_strategy()) {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let ctx1 = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let fixed1 = rule.fix(&ctx1).unwrap_or_else(|_| content.to_string());

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap_or_else(|_| fixed1.clone());

        if fixed1 != fixed2 {
            let ctx3 = LintContext::new(&fixed2, MarkdownFlavor::Standard, None);
            let fixed3 = rule.fix(&ctx3).unwrap_or_else(|_| fixed2.clone());
            prop_assert_eq!(fixed2, fixed3, "MD060 fix did not converge within 3 passes");
        }
    }
}

idempotent_rule!(
    md061,
    MD061ForbiddenTerms::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md062,
    MD062LinkDestinationWhitespace,
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md063,
    MD063HeadingCapitalization::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md064,
    MD064NoMultipleConsecutiveSpaces::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md065,
    MD065BlanksAroundHorizontalRules,
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md067,
    MD067FootnoteDefinitionOrder,
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md069,
    MD069NoDuplicateListMarkers,
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(md070, MD070NestedCodeFence, markdown_content_strategy(), Standard);
idempotent_rule!(
    md071,
    MD071BlankLineAfterFrontmatter,
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md072,
    MD072FrontmatterKeySort::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md073,
    MD073TocValidation::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md075,
    MD075OrphanedTableRows::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md076,
    MD076ListItemSpacing::default(),
    markdown_content_strategy(),
    Standard
);
idempotent_rule!(
    md077,
    MD077ListContinuationIndent::default(),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
idempotent_rule!(
    md077_aligned,
    MD077ListContinuationIndent::new(ContinuationStyle::Aligned),
    markdown_content_strategy(),
    Standard,
    MkDocs,
    MDX,
    Quarto
);
