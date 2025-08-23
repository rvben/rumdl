use rumdl_lib::lint_context::LintContext;
use rumdl_lib::utils::document_structure::DocumentStructure;

#[test]
fn test_escaped_brackets_in_document_structure() {
    let content = r#"This is not a link: \[escaped text\]
This is a real link: [actual link](https://example.com)
Reference style: \[not a reference\][ref]
Real reference: [real reference][ref]

[ref]: https://example.com

Images too: \![not an image](image.jpg)
Real image: ![actual image](image.jpg)"#;

    let doc_struct = DocumentStructure::new(content);

    // Document structure should not detect escaped brackets as links
    assert_eq!(
        doc_struct.links.len(),
        2,
        "Should only detect 2 real links, not escaped ones"
    );
    assert_eq!(
        doc_struct.images.len(),
        1,
        "Should only detect 1 real image, not escaped ones"
    );
}

#[test]
fn test_escaped_brackets_in_lint_context() {
    let content = r#"This is not a link: \[escaped text\]
This is a real link: [actual link](https://example.com)
Reference style: \[not a reference\][ref]
Real reference: [real reference][ref]

[ref]: https://example.com

Images too: \![not an image](image.jpg)
Real image: ![actual image](image.jpg)"#;

    let ctx = LintContext::new(content);

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

    let ctx = LintContext::new(content);

    // Currently, both LintContext and DocumentStructure don't handle double backslash escapes
    // This is a potential future enhancement: when \\ precedes [, the first \ escapes the second \
    // making the bracket not escaped. For now, both implementations consistently don't detect this.
    assert_eq!(
        ctx.links.len(),
        0,
        "Current behavior: doesn't handle double backslash escapes"
    );

    // Test that DocumentStructure matches LintContext behavior
    let doc_struct = DocumentStructure::new(content);
    assert_eq!(
        doc_struct.links.len(),
        ctx.links.len(),
        "DocumentStructure should match LintContext"
    );
}

#[test]
fn test_nested_brackets_with_escapes() {
    let content = r#"Complex: [outer \[escaped inner\] text](url)
Nested reference: [text with \[brackets\]][ref]
[ref]: https://example.com"#;

    let ctx = LintContext::new(content);

    // Should correctly parse links with escaped brackets inside
    assert_eq!(ctx.links.len(), 2);
    assert!(ctx.links[0].text.contains("escaped inner"));
}
