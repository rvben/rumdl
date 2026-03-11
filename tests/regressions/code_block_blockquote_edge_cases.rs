//! Edge case tests for code block detection inside blockquotes
//! Tests potential side effects and spec compliance issues

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD033NoInlineHtml;

#[test]
fn test_deeply_nested_blockquote_performance() {
    // Test that deeply nested blockquotes don't cause performance issues
    let rule = MD033NoInlineHtml::default();

    // 20 levels of nesting should complete quickly
    let content = ">>>>>>>>>>>>>>>>>> ```html\n>>>>>>>>>>>>>>>>>> <div>test</div>\n>>>>>>>>>>>>>>>>>> ```\n";

    let start = std::time::Instant::now();
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    let elapsed = start.elapsed();

    // Should complete in under 100ms even with deep nesting
    assert!(elapsed.as_millis() < 100, "Deep nesting took {}ms", elapsed.as_millis());
    assert_eq!(
        result.len(),
        0,
        "Should not flag HTML in deeply nested blockquote code blocks"
    );
}

#[test]
fn test_fence_boundary_alignment() {
    // Test that code block boundaries are correctly identified
    let rule = MD033NoInlineHtml::default();

    let content = r#"> ```html
> <div>inside</div>
> ```
<span>outside</span>
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should flag only the <span> outside the code block (opening tag only)
    assert_eq!(result.len(), 1, "Should flag HTML outside code block");
    assert!(result.iter().any(|w| w.message.contains("<span>")));
    assert!(
        !result.iter().any(|w| w.message.contains("<div>")),
        "Should not flag HTML inside code block"
    );
}

#[test]
fn test_blank_line_detection_in_blockquotes() {
    // Test indented code block detection with various blank line scenarios
    let rule = MD033NoInlineHtml::default();

    // Empty blockquote line before indented code
    let content = r#"> text
>
>     <code>indented</code>
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Indented code after blank blockquote line should be detected as code
    assert_eq!(
        result.len(),
        0,
        "Should not flag HTML in indented code after blank line"
    );
}

#[test]
fn test_list_vs_code_in_blockquotes() {
    // Ensure list items aren't mistaken for code
    let rule = MD033NoInlineHtml::default();

    let content = r#"> - List item with <span>HTML</span>
>     code continuation
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should flag HTML in list item (opening tag only), not in code continuation
    assert!(!result.is_empty(), "Should flag HTML in list item");
    assert!(result.iter().any(|w| w.message.contains("<span>")));
}

#[test]
fn test_malformed_blockquote_markers() {
    // Test handling of edge cases with blockquote markers
    let rule = MD033NoInlineHtml::default();

    let content = r#"> text
>not-valid-quote
> ```
> <div>is this in code?</div>
> ```
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Line without space after > breaks the blockquote
    // The code block may not be properly detected
    // This documents current behavior
    println!("Malformed marker test: {} issues found", result.len());
}

#[test]
fn test_mixed_blockquote_nesting_levels() {
    // Test transitioning between different nesting levels
    let rule = MD033NoInlineHtml::default();

    let content = r#">> Deep nested
>
> ```html
> <p>Back to single level</p>
> ```
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should handle transition correctly
    assert_eq!(result.len(), 0, "Should handle nesting level transitions");
}

#[test]
fn test_indentation_preservation() {
    // Test that indentation is correctly handled after stripping blockquote markers
    let rule = MD033NoInlineHtml::default();

    // Indented code needs blank line before it, even in blockquotes
    let content = r#"> Normal text
>
>     <code>4 spaces after marker</code>
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should be detected as indented code block after blank line
    assert_eq!(
        result.len(),
        0,
        "Should detect indented code with preserved indentation"
    );
}

#[test]
fn test_fence_with_varying_indentation() {
    // Test fences with different indentation levels
    let rule = MD033NoInlineHtml::default();

    let content = r#"> ```html
>   <div>indented content</div>
> ```
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Indentation inside fence should not matter
    assert_eq!(result.len(), 0, "Should handle indented content in fences");
}

#[test]
fn test_unicode_in_blockquote_code() {
    // Test Unicode content handling
    let rule = MD033NoInlineHtml::default();

    let content = r#"> ```html
> <div>日本語 содержание محتوى</div>
> ```
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 0, "Should handle Unicode in blockquote code blocks");
}

#[test]
fn test_very_long_lines_in_blockquote() {
    // Test performance with very long lines
    let rule = MD033NoInlineHtml::default();

    let long_content = "x".repeat(10000);
    let content = format!("> ```html\n> <div>{long_content}</div>\n> ```\n");

    let start = std::time::Instant::now();
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed.as_millis() < 500, "Long lines took {}ms", elapsed.as_millis());
    assert_eq!(result.len(), 0, "Should handle very long lines");
}

#[test]
fn test_multiple_fences_same_blockquote() {
    // Test multiple fenced code blocks in single blockquote
    let rule = MD033NoInlineHtml::default();

    let content = r#"> ```html
> <div>first</div>
> ```
>
> Some text with <b>HTML</b>
>
> ```html
> <div>second</div>
> ```
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the <b> tag between code blocks (opening tag only)
    assert_eq!(result.len(), 1, "Should flag HTML between code blocks");
    assert!(result.iter().any(|w| w.message.contains("<b>")));
    assert!(!result.iter().any(|w| w.message.contains("<div>")));
}

#[test]
fn test_tab_indentation_in_blockquote() {
    // Test tab vs space handling in blockquote
    // Per CommonMark spec, HTML blocks have higher priority than indented code blocks
    // When a line starts with an HTML tag like <div>, it's parsed as HtmlBlock, not CodeBlock
    let rule = MD033NoInlineHtml::default();

    // Structure: blockquote text, blank blockquote line, tab-indented HTML
    // pulldown-cmark parses this as HtmlBlock, not indented CodeBlock
    let content = "> text\n>\n> \t<div>code</div>\n";

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // HTML block is flagged by MD033 (HTML takes precedence over indented code)
    assert_eq!(result.len(), 1, "HTML block inside blockquote should be flagged");
    assert!(result.iter().any(|w| w.message.contains("<div>")));
}
