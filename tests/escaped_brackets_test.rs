use rumdl_lib::lint_context::LintContext;

#[test]
fn test_escaped_brackets_in_lint_context() {
    let content = r#"This is not a link: \[escaped text\]
This is a real link: [actual link](https://example.com)
Reference style: \[not a reference\][ref]
Real reference: [real reference][ref]

[ref]: https://example.com

Images too: \![not an image](image.jpg)
Real image: ![actual image](image.jpg)"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);

    // LintContext correctly handles escaped brackets
    assert_eq!(ctx.links.len(), 2, "Should only detect 2 real links");
    assert_eq!(ctx.images.len(), 1, "Should only detect 1 real image");
}

#[test]
fn test_complex_escaped_brackets() {
    let content = r#"Edge cases:
\\[not escaped because double backslash](url)
\\\[escaped with three backslashes\]
Text with \[brackets\] in the middle
Multiple \[escaped\] \[brackets\] on same line"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);

    // Currently, LintContext doesn't handle double backslash escapes
    // This is a potential future enhancement: when \\ precedes [, the first \ escapes the second \
    // making the bracket not escaped. For now, the implementation doesn't detect this.
    assert_eq!(
        ctx.links.len(),
        0,
        "Current behavior: doesn't handle double backslash escapes"
    );

    // LintContext behavior is now the canonical implementation
}

#[test]
fn test_nested_brackets_with_escapes() {
    let content = r#"Complex: [outer \[escaped inner\] text](url)
Nested reference: [text with \[brackets\]][ref]
[ref]: https://example.com"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);

    // Should correctly parse links with escaped brackets inside
    assert_eq!(ctx.links.len(), 2);
    assert!(ctx.links[0].text.contains("escaped inner"));
}
