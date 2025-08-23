use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD037NoSpaceInEmphasis;

#[test]
fn test_md037_with_kramdown_span_ial() {
    let rule = MD037NoSpaceInEmphasis;

    // Emphasis with spaces but has span IAL - should not trigger
    let content = "This is * emphasized *{:.highlight} text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not flag spaces when emphasis has span IAL");

    // Emphasis with spaces and no IAL - should trigger
    let content = "This is * emphasized * text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].message.contains("Spaces inside emphasis"));
}

#[test]
fn test_md037_various_span_ial_patterns() {
    let rule = MD037NoSpaceInEmphasis;

    let content = r#"Some * text *{:.class} here
Another ** bold **{:#id} example
Yet _another_{:style="color: red"} one
And __double__{:.class #id} underscore"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not flag any emphasis with span IAL");
}

#[test]
fn test_md037_mixed_with_and_without_ial() {
    let rule = MD037NoSpaceInEmphasis;

    let content = r#"Good: *text*{:.class}
Bad: * text *
Good: **bold**{:#id}
Bad: ** bold **
Good: _italic_{:attr="value"}
Bad: _ italic _"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3, "Should only flag emphasis without IAL");

    // Check that all warnings are for the lines without IAL
    assert_eq!(result[0].line, 2);
    assert_eq!(result[1].line, 4);
    assert_eq!(result[2].line, 6);
}

#[test]
fn test_md037_span_ial_on_links() {
    let rule = MD037NoSpaceInEmphasis;

    // Links can also have span IAL
    let content = r#"[link text](url){:target="_blank"}
[another * link *](url){:.external}"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should handle span IAL on links");
}

#[test]
fn test_md037_inline_code_with_ial() {
    let rule = MD037NoSpaceInEmphasis;

    // Inline code with IAL (though less common)
    let content = "`code`{:#special-code}";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should handle span IAL on inline code");
}

#[test]
fn test_md037_false_positive_ial() {
    let rule = MD037NoSpaceInEmphasis;

    // Not actually span IAL - missing colon or special char, or space before IAL
    let content = r#"Some * text * {not-ial}
Another * text * {:.class}"#; // Space before IAL

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Should flag invalid IAL patterns");
}
