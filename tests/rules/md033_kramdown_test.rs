use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD033NoInlineHtml;

#[test]
fn test_md033_kramdown_extensions() {
    let rule = MD033NoInlineHtml::default();

    // Kramdown extensions should not be flagged as HTML
    let content = r#"{::comment}
This is a comment that won't be rendered
{:/comment}

{::nomarkdown}
<div>This HTML is intentionally allowed</div>
{:/nomarkdown}

{::options parse_block_html="true" /}"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not flag Kramdown extensions as HTML");
}

#[test]
fn test_md033_kramdown_block_attributes() {
    let rule = MD033NoInlineHtml::default();

    // Block attributes should not be flagged
    let content = r#"```ruby
puts "Hello"
```
{:.language-ruby .numberLines}

| Table | Header |
|-------|--------|
| Cell  | Cell   |
{:.table-striped}

> Blockquote
{:#special-quote}"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not flag block attributes as HTML");
}

#[test]
fn test_md033_mixed_html_and_kramdown() {
    let rule = MD033NoInlineHtml::default();

    let content = r#"{::comment}
This is fine
{:/comment}

<div>This is HTML and should be flagged</div>

{:.class}

<span>Another HTML tag</span>

{::options key="value" /}"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // MD033 reports only opening tags
    assert_eq!(result.len(), 2, "Should only flag actual HTML tags (opening tags only)");
    assert!(result[0].message.contains("<div>"));
    assert!(result[1].message.contains("<span>"));
}

#[test]
fn test_md033_nested_kramdown_extensions() {
    let rule = MD033NoInlineHtml::default();

    let content = r#"{::comment}
Outer comment
{::comment}
Nested comment
{:/comment}
Back to outer
{:/comment}"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should handle nested Kramdown extensions");
}

#[test]
fn test_md033_invalid_kramdown_patterns() {
    let rule = MD033NoInlineHtml::default();

    // These look similar but are not valid Kramdown
    // and should not prevent HTML detection
    let content = r#"{:comment}  <!-- Missing second colon -->
{/comment}

<div>Regular HTML</div>

{just text in braces}

<span>More HTML</span>"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Should detect the actual HTML tags (opening tags only)
    assert_eq!(
        result.len(),
        2,
        "Should detect HTML tags despite invalid Kramdown patterns (opening tags only)"
    );
    assert!(result[0].message.contains("<div>"));
    assert!(result[1].message.contains("<span>"));
}

#[test]
fn test_md033_kramdown_with_allowed_html() {
    let rule = MD033NoInlineHtml::with_allowed(vec!["br".to_string(), "img".to_string()]);

    let content = r#"{::comment}
Comment here
{:/comment}

<br/>  <!-- Allowed -->
<img src="test.jpg"/>  <!-- Allowed -->
<div>Not allowed</div>

{:.class}"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Should flag opening div tag only
    assert_eq!(result.len(), 1, "Should only flag non-allowed HTML (opening tag only)");
    assert!(result[0].message.contains("<div>"));
}
