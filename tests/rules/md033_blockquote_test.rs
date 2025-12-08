//! Tests for MD033 handling of fenced code blocks inside blockquotes
//! Addresses issue #105

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD033NoInlineHtml;

#[test]
fn test_md033_html_in_fenced_code_inside_blockquote() {
    let rule = MD033NoInlineHtml::default();

    // HTML inside a fenced code block inside a blockquote should NOT be flagged
    let content = r#"> This is quoted text:
>
> ```html
> <a data-hx-post="/click">Click Me!</a>
> ```
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT flag HTML tags inside fenced code blocks within blockquotes
    assert_eq!(
        result.len(),
        0,
        "Should not flag HTML inside fenced code blocks in blockquotes"
    );
}

#[test]
fn test_md033_html_in_fenced_code_outside_blockquote() {
    let rule = MD033NoInlineHtml::default();

    // HTML inside a fenced code block (not in blockquote) should NOT be flagged
    let content = r#"```html
<a data-hx-post="/click">Click Me!</a>
```
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT flag HTML tags inside fenced code blocks
    assert_eq!(result.len(), 0, "Should not flag HTML inside fenced code blocks");
}

#[test]
fn test_md033_html_outside_fenced_code_inside_blockquote() {
    let rule = MD033NoInlineHtml::default();

    // HTML outside code block but inside blockquote SHOULD be flagged
    let content = r#"> This is quoted text with <div>HTML</div>
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should flag HTML tags outside code blocks
    assert_eq!(result.len(), 1, "Should flag HTML tags outside code blocks"); // Only <div>
}

#[test]
fn test_md033_mixed_blockquote_with_code_and_html() {
    let rule = MD033NoInlineHtml::default();

    // Mix of HTML in and outside code blocks within blockquote
    let content = r##"> Text with <span>inline HTML</span>
>
> ```html
> <a href="#">Link</a>
> ```
>
> More text with <div>HTML</div>
"##;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag HTML outside code blocks: <span>, <div> (opening tags only)
    // Should NOT flag <a> inside code block
    assert_eq!(result.len(), 2, "Should flag only HTML outside code blocks");
    assert!(result.iter().any(|w| w.message.contains("<span>")));
    assert!(result.iter().any(|w| w.message.contains("<div>")));
    assert!(
        !result.iter().any(|w| w.message.contains("<a")),
        "Should not flag <a> inside code block"
    );
}

#[test]
fn test_md033_nested_blockquote_with_code() {
    let rule = MD033NoInlineHtml::default();

    // Nested blockquote with code block
    let content = r#">> Nested quote
>>
>> ```html
>> <button>Click</button>
>> ```
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT flag HTML inside code block even in nested blockquote
    assert_eq!(
        result.len(),
        0,
        "Should not flag HTML inside code blocks in nested blockquotes"
    );
}

#[test]
fn test_md033_blockquote_indented_code() {
    let rule = MD033NoInlineHtml::default();

    // Indented code inside blockquote (CommonMark supports this)
    let content = r#"> Normal text
>
>     <indented>code</indented>
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Indented code blocks inside blockquotes are tricky and not yet supported
    // This is a separate issue from fenced code blocks in blockquotes
    // The HTML should NOT be flagged if it's in an indented code block
    assert_eq!(result.len(), 0, "Should not flag HTML in indented code blocks");
}

#[test]
fn test_md033_issue_105_exact_example() {
    // Exact example from issue #105
    let rule = MD033NoInlineHtml::default();

    let content = r#"> It's worth mentioning that, if you prefer, you can use the [`data-`](https://html.spec.whatwg.org/multipage/dom.html#attr-data-*) prefix when using htmx:
>
> ```html
> <a data-hx-post="/click">Click Me!</a>
> ```
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT flag the HTML inside the fenced code block
    assert_eq!(
        result.len(),
        0,
        "Issue #105: Should not flag HTML inside fenced code blocks within blockquotes"
    );
}

#[test]
fn test_md033_blockquote_multiple_code_blocks() {
    let rule = MD033NoInlineHtml::default();

    // Multiple code blocks in same blockquote with HTML between them
    let content = r#"> First paragraph
>
> ```html
> <div>First block</div>
> ```
>
> Middle paragraph with <span>HTML</span>
>
> ```html
> <div>Second block</div>
> ```
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should flag HTML between code blocks but not inside them
    assert_eq!(result.len(), 1, "Should flag HTML between code blocks");
    assert!(result.iter().any(|w| w.message.contains("<span>")));
    assert!(
        !result.iter().any(|w| w.message.contains("<div>")),
        "Should not flag HTML in code blocks"
    );
}

#[test]
fn test_md033_blockquote_tilde_fences() {
    let rule = MD033NoInlineHtml::default();

    // Tilde fences in blockquote
    let content = r#"> Using tildes:
>
> ~~~html
> <button>Click</button>
> ~~~
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT flag HTML in tilde-fenced code blocks
    assert_eq!(result.len(), 0, "Should not flag HTML in tilde-fenced code blocks");
}

#[test]
fn test_md033_blockquote_unclosed_code_block() {
    let rule = MD033NoInlineHtml::default();

    // Unclosed code block in blockquote extends to end of blockquote
    let content = r#"> Start of blockquote
>
> ```html
> <div>Unclosed block
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT flag HTML in unclosed code block
    assert_eq!(result.len(), 0, "Should not flag HTML in unclosed code blocks");
}

#[test]
fn test_md033_blockquote_empty_lines_in_fence() {
    let rule = MD033NoInlineHtml::default();

    // Empty blockquote lines inside fenced code block
    let content = r#"> ```html
>
> <span>Content</span>
>
> ```
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT flag HTML with empty lines in code block
    assert_eq!(result.len(), 0, "Should not flag HTML in code blocks with empty lines");
}

#[test]
fn test_md033_blockquote_language_specific_fence() {
    let rule = MD033NoInlineHtml::default();

    // Language-specific fence in blockquote
    let content = r##"> ```javascript
> const html = "<div>test</div>";
> ```
"##;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT flag HTML in language-specific fenced code blocks
    assert_eq!(
        result.len(),
        0,
        "Should not flag HTML in language-specific fenced code blocks"
    );
}

#[test]
fn test_md033_blockquote_indented_fence() {
    let rule = MD033NoInlineHtml::default();

    // Indented fence (spaces after blockquote marker) in blockquote
    let content = r#">   ```html
>   <p>Indented fence</p>
>   ```
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT flag HTML in indented fences
    assert_eq!(result.len(), 0, "Should not flag HTML in indented fenced code blocks");
}
