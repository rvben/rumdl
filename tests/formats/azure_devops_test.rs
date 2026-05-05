use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD031BlanksAroundFences;
use rumdl_lib::rules::{CodeBlockStyle, MD046CodeBlockStyle};

// ==================== MD031 fix() integration tests ====================

#[test]
fn test_md031_fix_inserts_blank_before_colon_fence() {
    // Missing blank line before ::: opener — fix() must insert one.
    let content = "Some text\n:::python\ncode\n:::\n";
    let ctx = LintContext::new(content, MarkdownFlavor::AzureDevOps, None);
    let rule = MD031BlanksAroundFences::new(true);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Some text\n\n:::python\ncode\n:::\n");
}

#[test]
fn test_md031_fix_inserts_blank_after_colon_fence() {
    // Missing blank line after ::: closer — fix() must insert one.
    let content = ":::python\ncode\n:::\nSome text\n";
    let ctx = LintContext::new(content, MarkdownFlavor::AzureDevOps, None);
    let rule = MD031BlanksAroundFences::new(true);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, ":::python\ncode\n:::\n\nSome text\n");
}

#[test]
fn test_md031_fix_inserts_blanks_both_sides_of_colon_fence() {
    // Missing blank lines on BOTH sides — fix() must insert both.
    let content = "Some text\n:::python\ncode\n:::\nMore text\n";
    let ctx = LintContext::new(content, MarkdownFlavor::AzureDevOps, None);
    let rule = MD031BlanksAroundFences::new(true);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Some text\n\n:::python\ncode\n:::\n\nMore text\n");
}

#[test]
fn test_md031_fix_leaves_conforming_colon_fence_unchanged() {
    // Document already has correct blank lines — fix() must be idempotent.
    let content = "Some text\n\n:::python\ncode\n:::\n\nMore text\n";
    let ctx = LintContext::new(content, MarkdownFlavor::AzureDevOps, None);
    let rule = MD031BlanksAroundFences::new(true);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "fix() must not alter a conforming document");
}

#[test]
fn test_md031_fix_standard_flavor_ignores_colon_fences() {
    // Standard flavor does not recognise ::: fences, so no blank-line fix is applied.
    let content = "Some text\n:::python\ncode\n:::\nMore text\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let rule = MD031BlanksAroundFences::new(true);
    let fixed = rule.fix(&ctx).unwrap();
    // Standard flavor treats ::: lines as plain prose; no colon-fence fix should be emitted.
    assert_eq!(
        fixed, content,
        "Standard flavor: fix() must not alter content around colon fences"
    );
}

fn azure_ctx(content: &str) -> LintContext<'_> {
    LintContext::new(content, MarkdownFlavor::AzureDevOps, None)
}

#[test]
fn test_md031_colon_fence_missing_blank_before() {
    let content = "Some text\n::: mermaid\ndiagram\n:::\n";
    let ctx = azure_ctx(content);
    let rule = MD031BlanksAroundFences::new(true);
    let warnings = rule.check(&ctx).unwrap();
    assert!(!warnings.is_empty(), "should warn: no blank line before colon fence");
    assert!(
        warnings[0].message.contains("before"),
        "message: {:?}",
        warnings[0].message
    );
}

#[test]
fn test_md031_colon_fence_missing_blank_after() {
    let content = "::: mermaid\ndiagram\n:::\nSome text\n";
    let ctx = azure_ctx(content);
    let rule = MD031BlanksAroundFences::new(true);
    let warnings = rule.check(&ctx).unwrap();
    assert!(!warnings.is_empty(), "should warn: no blank line after colon fence");
    assert!(
        warnings[0].message.contains("after"),
        "message: {:?}",
        warnings[0].message
    );
}

#[test]
fn test_md031_colon_fence_with_blank_lines_no_warning() {
    let content = "Some text\n\n::: mermaid\ndiagram\n:::\n\nSome text\n";
    let ctx = azure_ctx(content);
    let rule = MD031BlanksAroundFences::new(true);
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "should not warn when blank lines present: {warnings:?}"
    );
}

#[test]
fn test_md031_standard_flavor_colon_not_enforced() {
    let content = "Some text\n::: mermaid\ndiagram\n:::\nSome text\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let rule = MD031BlanksAroundFences::new(true);
    let warnings = rule.check(&ctx).unwrap();
    for w in &warnings {
        assert!(!w.message.contains("colon"), "unexpected warning: {w:?}");
    }
}

#[test]
fn test_md046_colon_fence_with_inner_backtick_not_counted() {
    // A colon fence that contains ``` inside should not affect MD046 style detection
    let content = "::: mermaid\n```\nsome content\n```\n:::\n\n```rust\nfn main() {}\n```\n";
    let ctx = azure_ctx(content);
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "MD046 should not flag colon fence content: {warnings:?}"
    );
}

#[test]
fn test_md048_colon_fence_with_inner_backtick_not_counted() {
    use rumdl_lib::rules::CodeFenceStyle;
    use rumdl_lib::rules::MD048CodeFenceStyle;
    // ::: block containing ``` inside should not affect MD048 style detection
    let content = "::: mermaid\n```\ncontent\n```\n:::\n\n~~~rust\nfn main() {}\n~~~\n";
    let ctx = azure_ctx(content);
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let warnings = rule.check(&ctx).unwrap();
    // The only real fence is ~~~rust — no inconsistency, no warning
    assert!(
        warnings.is_empty(),
        "MD048 should not flag colon fence content: {warnings:?}"
    );
}

#[test]
fn test_colon_fence_only_opaque_in_azure_flavor() {
    // In Azure DevOps flavor, ::: fences mark lines as code blocks.
    // In Standard flavor, the same lines are plain prose (not code blocks).
    let content = "::: mermaid\ngraph TD\n:::\n";

    let azure = azure_ctx(content);
    // Opener line and content line are both in_code_block in Azure flavor
    assert!(
        azure.lines[0].in_code_block,
        "azure: fence opener must be in_code_block"
    );
    assert!(
        azure.lines[1].in_code_block,
        "azure: fence content must be in_code_block"
    );

    let standard = LintContext::new(content, MarkdownFlavor::Standard, None);
    // Standard has no colon fence concept — lines are regular prose
    assert!(
        !standard.lines[0].in_code_block,
        "standard: ::: line must not be in_code_block"
    );
    assert!(
        !standard.lines[1].in_code_block,
        "standard: content must not be in_code_block"
    );
}

#[test]
fn test_link_parser_does_not_flag_content_inside_colon_fence() {
    use rumdl_lib::rules::MD034NoBareUrls;

    let content = "::: mermaid\nA --> https://example.com/very/long/path\n:::\n";
    let ctx = azure_ctx(content);
    let rule = MD034NoBareUrls;
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "MD034 must not flag URLs inside colon fence: {warnings:?}"
    );
}

#[test]
fn test_multiple_colon_fences_in_document() {
    use rumdl_lib::rules::MD013LineLength;

    let long = "A".repeat(150);
    let content = format!("# Heading\n\n::: mermaid\n{long}\n:::\n\nNormal paragraph.\n\n::: mermaid\n{long}\n:::\n");
    let ctx = azure_ctx(&content);
    let rule = MD013LineLength::default();
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "MD013 must not fire in any colon fence: {warnings:?}"
    );
}

#[test]
fn test_pandoc_flavor_not_affected() {
    use rumdl_lib::rules::MD013LineLength;

    // In Pandoc flavor, ::: is a fenced div — content is Markdown and is linted as prose.
    // Use a line with spaces so MD013's single-word exemption doesn't apply.
    let long_line = "word ".repeat(30); // 150 chars with spaces
    let content = format!("::: note\n{long_line}\n:::\n");
    let ctx = LintContext::new(&content, MarkdownFlavor::Pandoc, None);
    let rule = MD013LineLength::default();
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        !warnings.is_empty(),
        "Pandoc flavor: MD013 must fire inside fenced div — content is not suppressed"
    );
}

#[test]
fn test_unclosed_colon_fence_does_not_panic() {
    // Unclosed fence — detection should handle gracefully
    let content = "::: mermaid\ndiagram content\n";
    let ctx = azure_ctx(content);
    // All lines after opener should be marked as code block
    assert!(ctx.lines[0].in_code_block);
    assert!(ctx.lines[1].in_code_block);
}

#[test]
fn test_autodoc_marker_treated_as_code_fence_in_azure_flavor() {
    use rumdl_lib::rules::MD013LineLength;

    // ::: module.Class looks like autodoc but should be treated as code fence in azure flavor
    let long = "A".repeat(150);
    let content = format!("::: module.Class\n{long}\n:::\n");
    let ctx = azure_ctx(&content);
    let rule = MD013LineLength::default();
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "azure_devops: ::: module.Class should be opaque code fence"
    );
}

#[test]
fn test_tab_indented_opener_is_not_a_colon_fence() {
    // A tab before ::: is not a valid opener — tabs are not counted as indentation.
    // Note: pulldown-cmark may mark the tab-indented line itself as in_code_block via
    // CommonMark's indented-code-block rule (tab = 4 spaces), but no colon fence range
    // should be created and the subsequent content line must not be in_code_block.
    let content = "\t::: mermaid\ncontent\n:::\n";
    let ctx = azure_ctx(content);
    assert!(
        ctx.colon_fence_ranges().is_empty(),
        "tab-indented ::: must not create a colon fence range"
    );
    assert!(
        !ctx.lines[1].in_code_block,
        "content after tab-indented ::: must not be in_code_block"
    );
}

#[test]
fn test_bare_closer_without_opener_is_not_a_block() {
    // A bare ::: with no preceding opener must not corrupt state
    let content = "Some text\n:::\nMore text\n";
    let ctx = azure_ctx(content);
    assert!(
        !ctx.lines[0].in_code_block,
        "prose before bare ::: must not be in_code_block"
    );
    assert!(
        !ctx.lines[1].in_code_block,
        "bare ::: without opener must not be in_code_block"
    );
    assert!(
        !ctx.lines[2].in_code_block,
        "prose after bare ::: must not be in_code_block"
    );
}

#[test]
fn test_leading_spaces_1_2_3_are_valid_openers() {
    // 0–3 leading spaces are all valid opener indentation levels
    for spaces in 1..=3usize {
        let indent = " ".repeat(spaces);
        let content = format!("{indent}::: mermaid\ncontent\n{indent}:::\n");
        let ctx = azure_ctx(&content);
        assert!(
            ctx.lines[0].in_code_block,
            "{spaces}-space indent: opener must be in_code_block"
        );
        assert!(
            ctx.lines[1].in_code_block,
            "{spaces}-space indent: content must be in_code_block"
        );
        assert!(
            ctx.lines[2].in_code_block,
            "{spaces}-space indent: closer must be in_code_block"
        );
    }
}

#[test]
fn test_four_leading_spaces_is_not_a_colon_fence() {
    // 4 leading spaces disqualifies the opener (CommonMark indented code block rules).
    // pulldown-cmark may mark the 4-space line itself as in_code_block, but no colon
    // fence range should be created and the subsequent content line must not be in_code_block.
    let content = "    ::: mermaid\ncontent\n:::\n";
    let ctx = azure_ctx(content);
    assert!(
        ctx.colon_fence_ranges().is_empty(),
        "4-space indent: must not create a colon fence range"
    );
    assert!(
        !ctx.lines[1].in_code_block,
        "4-space indent: following content must not be in_code_block"
    );
}
