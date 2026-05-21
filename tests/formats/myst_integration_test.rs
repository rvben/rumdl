use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{
    CodeBlockStyle, CodeFenceStyle, MD013LineLength, MD031BlanksAroundFences, MD038NoSpaceInCode,
    MD040FencedCodeLanguage, MD046CodeBlockStyle, MD048CodeFenceStyle,
};

fn myst_ctx(content: &str) -> LintContext<'_> {
    LintContext::new(content, MarkdownFlavor::MyST, None)
}

fn standard_ctx(content: &str) -> LintContext<'_> {
    LintContext::new(content, MarkdownFlavor::Standard, None)
}

// ==================== MD040: Fenced Code Language ====================

#[test]
fn test_md040_myst_backtick_directive_no_warning() {
    let content = "```{note}\nThis is a note.\n```\n";
    let ctx = myst_ctx(content);
    let rule = MD040FencedCodeLanguage::default();
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "MyST directive should not trigger MD040: {warnings:?}"
    );
}

#[test]
fn test_md040_myst_directive_with_argument() {
    let content = "```{code-cell} python\nprint('hello')\n```\n";
    let ctx = myst_ctx(content);
    let rule = MD040FencedCodeLanguage::default();
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "MyST directive with argument should not trigger MD040: {warnings:?}"
    );
}

#[test]
fn test_md040_myst_empty_braces_still_warns() {
    let content = "```{}\ncode\n```\n";
    let ctx = myst_ctx(content);
    let rule = MD040FencedCodeLanguage::default();
    let warnings = rule.check(&ctx).unwrap();
    assert!(!warnings.is_empty(), "Empty braces should still trigger MD040");
}

#[test]
fn test_md040_standard_flavor_flags_myst_directive() {
    let content = "```{note}\nThis is a note.\n```\n";
    let ctx = standard_ctx(content);
    let rule = MD040FencedCodeLanguage::default();
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        !warnings.is_empty(),
        "Standard flavor should flag {{note}} as missing language"
    );
}

// ==================== MD048: Code Fence Style ====================

#[test]
fn test_md048_myst_colon_directive_not_counted() {
    // MyST colon directives should not influence fence style detection
    let content = ":::{note}\nContent\n:::\n\n```python\ncode\n```\n";
    let ctx = myst_ctx(content);
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "MyST colon directive should not trigger MD048: {warnings:?}"
    );
}

#[test]
fn test_md048_myst_backtick_directive_not_counted() {
    // MyST backtick directives should not influence fence style detection
    let content = "```{note}\nContent\n```\n\n~~~python\ncode\n~~~\n";
    let ctx = myst_ctx(content);
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let warnings = rule.check(&ctx).unwrap();
    // Only the tilde fence should be counted; the backtick directive is skipped
    assert!(
        warnings.is_empty(),
        "MyST backtick directive should not trigger MD048: {warnings:?}"
    );
}

// ==================== MD031: Blanks Around Fences ====================

#[test]
fn test_md031_myst_colon_directive_needs_blanks() {
    let content = "Some text\n:::{note}\nContent\n:::\nMore text\n";
    let ctx = myst_ctx(content);
    let rule = MD031BlanksAroundFences::new(true);
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(
        warnings.len(),
        2,
        "Should warn about missing blanks before and after: {warnings:?}"
    );
}

#[test]
fn test_md031_myst_colon_directive_with_blanks_ok() {
    let content = "Some text\n\n:::{note}\nContent\n:::\n\nMore text\n";
    let ctx = myst_ctx(content);
    let rule = MD031BlanksAroundFences::new(true);
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "Properly spaced MyST directive should not warn: {warnings:?}"
    );
}

#[test]
fn test_md031_myst_fix_inserts_blanks() {
    let content = "Some text\n:::{note}\nContent\n:::\nMore text\n";
    let ctx = myst_ctx(content);
    let rule = MD031BlanksAroundFences::new(true);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Some text\n\n:::{note}\nContent\n:::\n\nMore text\n");
}

// ==================== MD046: Code Block Style ====================

#[test]
fn test_md046_myst_colon_directive_not_counted_as_code_block() {
    // MyST colon directives should not be counted as code blocks for style detection
    let content = ":::{note}\nContent\n:::\n\n```python\ncode\n```\n";
    let ctx = myst_ctx(content);
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "MyST directive should not affect code block style: {warnings:?}"
    );
}

// ==================== MD038: No Space in Code ====================

#[test]
fn test_md038_myst_role_not_flagged() {
    // MyST role with content that has leading space — should not be flagged in MyST
    let content = "See {ref}` my-label` for details.\n";
    let ctx = myst_ctx(content);
    let rule = MD038NoSpaceInCode::default();
    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings.is_empty(), "MyST role should not trigger MD038: {warnings:?}");
}

// ==================== MD013: Line Length ====================

#[test]
fn test_md013_myst_comment_not_flagged() {
    // A long MyST comment with spaces (so trailing-token forgiveness doesn't apply)
    let content = "% This is a very long comment that exceeds the default line length limit of eighty characters and should be skipped by the linter\n";
    let ctx = myst_ctx(content);
    let rule = MD013LineLength::default();
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "MyST comment should not trigger MD013: {warnings:?}"
    );
}

#[test]
fn test_md013_standard_flavor_flags_percent_line() {
    // Same long line but in standard flavor — should be flagged
    let content = "% This is a very long comment that exceeds the default line length limit of eighty characters and should be flagged by the linter\n";
    let ctx = standard_ctx(content);
    let rule = MD013LineLength::default();
    let warnings = rule.check(&ctx).unwrap();
    assert!(!warnings.is_empty(), "Standard flavor should flag long % line");
}

// ==================== Content-bearing directive body linting ====================

#[test]
fn test_myst_content_directive_body_not_in_code_block() {
    let content = "```{note}\nThis is **markdown** content with a [link](url).\n```\n";
    let ctx = myst_ctx(content);
    // Line 1 (body) should NOT be in_code_block
    assert!(
        !ctx.lines[1].in_code_block,
        "Content directive body should not be in_code_block"
    );
    assert!(
        ctx.lines[1].in_myst_directive,
        "Content directive body should be in_myst_directive"
    );
}

#[test]
fn test_myst_code_directive_body_stays_in_code_block() {
    let content = "```{code-cell} python\nprint('hello')\n```\n";
    let ctx = myst_ctx(content);
    // Line 1 (body) SHOULD be in_code_block
    assert!(
        ctx.lines[1].in_code_block,
        "Code directive body should remain in_code_block"
    );
}

// ==================== Nested directives ====================

#[test]
fn test_myst_nested_colon_directives() {
    let content = "::::{note}\n:::{warning}\nInner content\n:::\nOuter content\n::::\n";
    let ctx = myst_ctx(content);
    for i in 0..6 {
        assert!(ctx.lines[i].in_myst_directive, "line {i} should be in_myst_directive");
        assert!(!ctx.lines[i].in_code_block, "line {i} should not be in_code_block");
    }
}

// ==================== Directive options ====================

#[test]
fn test_myst_directive_options_not_in_code_block() {
    let content = "```{figure} image.png\n:alt: An image\n:width: 80%\n\nCaption text\n```\n";
    let ctx = myst_ctx(content);
    // Option lines should be in_myst_directive but not in_code_block
    assert!(ctx.lines[1].in_myst_directive);
    assert!(!ctx.lines[1].in_code_block);
    assert!(ctx.lines[2].in_myst_directive);
    assert!(!ctx.lines[2].in_code_block);
}
