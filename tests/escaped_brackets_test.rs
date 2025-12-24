use rumdl_lib::lint_context::LintContext;

#[test]
fn test_escaped_brackets_in_lint_context() {
    // Test content with various escaped bracket patterns
    let content = r#"This is not a link: \[escaped text\]
This is a real link: [actual link](https://example.com)
Reference style: \[not a reference\][ref]
Real reference: [real reference][ref]

[ref]: https://example.com

Images too: \![not an image](image.jpg)
Real image: ![actual image](image.jpg)"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Per CommonMark spec, pulldown-cmark 0.13.0 correctly handles escaped brackets:
    // - \[escaped text\] → NOT a link (escaped brackets produce literal text)
    // - [actual link](url) → LINK
    // - \[not a reference\][ref] → literal text + shortcut reference [ref] → LINK
    // - [real reference][ref] → LINK (reference style)
    // - \![not an image](url) → literal "!" + LINK (escape only affects the "!")
    // - ![actual image](url) → IMAGE
    //
    // Total: 4 links, 1 image
    assert_eq!(ctx.links.len(), 4, "Should detect 4 links");
    assert_eq!(ctx.images.len(), 1, "Should detect 1 real image");
}

#[test]
fn test_complex_escaped_brackets() {
    let content = r#"Edge cases:
\\[not escaped because double backslash](url)
\\\[escaped with three backslashes\]
Text with \[brackets\] in the middle
Multiple \[escaped\] \[brackets\] on same line"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Per CommonMark spec:
    // - \\[text](url) → literal "\" + LINK (first \ escapes second \, leaving [text](url))
    // - \\\[text\] → literal "\" + literal "[text]" (two \ become one, third \ escapes [)
    // - \[brackets\] → literal text, not a link
    //
    // pulldown-cmark 0.13.0 correctly handles these cases
    assert_eq!(ctx.links.len(), 1, "Should detect 1 link from \\\\[...](url)");
}

#[test]
fn test_nested_brackets_with_escapes() {
    let content = r#"Complex: [outer \[escaped inner\] text](url)
Nested reference: [text with \[brackets\]][ref]
[ref]: https://example.com"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Should correctly parse links with escaped brackets inside
    assert_eq!(ctx.links.len(), 2);
    assert!(ctx.links[0].text.contains("escaped inner"));
}
