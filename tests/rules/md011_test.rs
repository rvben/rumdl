use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD011NoReversedLinks;

#[test]
fn test_md011_valid() {
    let rule = MD011NoReversedLinks {};
    let content = "[text](link)\n[more text](another/link)\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md011_invalid() {
    let rule = MD011NoReversedLinks {};
    let content = "(link)[text]\n(another/link)[more text]\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
}

#[test]
fn test_md011_mixed() {
    let rule = MD011NoReversedLinks {};
    let content = "[text](link)\n(link)[reversed]\n[text](link)\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_md011_fix() {
    let rule = MD011NoReversedLinks {};
    let content = "(link)[text]\n(another/link)[more text]\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "[text](link)\n[more text](another/link)\n");
}

#[test]
fn test_md011_url_in_brackets() {
    let rule = MD011NoReversedLinks {};
    // When URL is in the square brackets, we should detect and swap correctly
    let content = "(foobar)[https://boofar]\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result, "[foobar](https://boofar)\n",
        "Should swap text and URL correctly when URL is in brackets"
    );
}

#[test]
fn test_md011_various_url_patterns() {
    let rule = MD011NoReversedLinks {};
    // Test different URL patterns
    let content = "(text)[http://example.com]\n(more text)[https://example.org]\n(link text)[www.example.com]\n(description)[/path/to/page]\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "[text](http://example.com)\n[more text](https://example.org)\n[link text](www.example.com)\n[description](/path/to/page)\n"
    );
}

#[test]
fn test_md011_descriptive_text_detection() {
    let rule = MD011NoReversedLinks {};
    // When one part is clearly descriptive text and the other is a URL
    let content = "(Click here for more info)[https://example.com]\n(https://docs.rs)[Read the documentation]\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "[Click here for more info](https://example.com)\n[Read the documentation](https://docs.rs)\n"
    );
}

#[test]
fn test_md011_fragment_links() {
    let rule = MD011NoReversedLinks {};
    // Fragment links should be detected as URLs
    let content = "(introduction)[#introduction]\n(#getting-started)[Getting Started]\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "[introduction](#introduction)\n[Getting Started](#getting-started)\n"
    );
}

#[test]
fn test_md011_relative_paths() {
    let rule = MD011NoReversedLinks {};
    // Relative paths should be detected as URLs
    let content = "(Guide)[./docs/guide.md]\n(../README.md)[Parent README]\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "[Guide](./docs/guide.md)\n[Parent README](../README.md)\n");
}

#[test]
fn test_md011_mailto_links() {
    let rule = MD011NoReversedLinks {};
    // Mailto links should be detected as URLs
    let content = "(Contact us)[mailto:support@example.com]\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "[Contact us](mailto:support@example.com)\n");
}

#[test]
fn test_md011_domain_with_tld() {
    let rule = MD011NoReversedLinks {};
    // Domains with common TLDs should be detected
    let content = "(Example Site)[example.com]\n(docs.rust-lang.org)[Rust Docs]\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "[Example Site](example.com)\n[Rust Docs](docs.rust-lang.org)\n");
}

#[test]
fn test_md011_ambiguous_cases() {
    let rule = MD011NoReversedLinks {};
    // When both parts are ambiguous, assume standard reversed pattern (URL in parens)
    let content = "(foo)[bar]\n(test)[demo]\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "[bar](foo)\n[demo](test)\n");
}

#[test]
fn test_md011_both_urls() {
    let rule = MD011NoReversedLinks {};
    // When both parts look like URLs, prefer parentheses as URL (standard reversed pattern)
    let content = "(https://example.com)[https://other.com]\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "[https://other.com](https://example.com)\n");
}

#[test]
fn test_md011_mixed_complexity() {
    let rule = MD011NoReversedLinks {};
    // Complex real-world scenarios
    let content = r#"
(Learn more about Rust)[https://www.rust-lang.org]
(../guide.md)[Parent Guide]
(API Reference)[#api-reference]
(simple)[link]
(https://github.com)[Check out the code]
"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        r#"
[Learn more about Rust](https://www.rust-lang.org)
[Parent Guide](../guide.md)
[API Reference](#api-reference)
[link](simple)
[Check out the code](https://github.com)
"#
    );
}

#[test]
fn test_md011_not_false_positive_on_hashtags() {
    let rule = MD011NoReversedLinks {};
    // Hashtags with spaces or non-URL characters should not be detected as fragment links
    // Note: Current implementation will still treat valid-looking fragments as URLs
    // This test documents expected behavior for clear non-URL cases
    let content = "(define)[macro]\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // Ambiguous case: defaults to standard reversed pattern
    assert_eq!(result, "[macro](define)\n");
}
