// Regression test for Issue #39 - remaining edge cases
// Tests cross-file link detection and Liquid template handling

use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD051LinkFragments;

#[test]
fn test_issue_39_cross_file_links() {
    // Test that cross-file links are properly ignored
    let rule = MD051LinkFragments::new();

    let content = r#"# Test Document

## Valid Heading

Links that should be ignored (cross-file):

- [Link to tags page](/tags#pyinstaller) - absolute path, should be ignored
- [Another path link](/categories#rust) - absolute path, should be ignored
- [Relative path](../other/page#section) - relative path with file, should be ignored
- [Just path](/blog#archive) - absolute path, should be ignored
- [File with extension](other.md#heading) - has file extension, should be ignored

Valid internal links:

- [Valid link](#valid-heading) - should pass
- [Invalid link](#missing-heading) - should fail
"#;

    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the one invalid internal link
    assert_eq!(warnings.len(), 1, "Should only warn about #missing-heading");
    assert!(warnings[0].message.contains("#missing-heading"));

    // Verify cross-file links are not flagged
    for warning in &warnings {
        assert!(!warning.message.contains("#pyinstaller"));
        assert!(!warning.message.contains("#rust"));
        assert!(!warning.message.contains("#section"));
        assert!(!warning.message.contains("#archive"));
        // Check for the specific cross-file anchor, not just "heading" which appears in error message
        assert!(!warning.message.contains("other.md#heading"));
    }
}

#[test]
fn test_issue_39_liquid_templates() {
    // Test that Liquid template syntax is properly handled
    let rule = MD051LinkFragments::new();

    let content = r#"# Test Document

## Valid Heading

Liquid template links that should be ignored:

- [Liquid with filter]({{ "/tags#alternative-data-streams" | relative_url }}) - has filter, should be ignored
- [Another liquid]({{ page.url }}#section) - liquid variable, should be ignored
- [Complex liquid]({{ site.baseurl }}/{{ page.category }}#heading) - complex liquid, should be ignored
- [Jekyll include]({% include nav.html %}#footer) - Jekyll tag, should be ignored
- [Jekyll post link]({% post_url 2023-03-25-post %}#section) - Jekyll tag, should be ignored

Valid internal links:

- [Valid link](#valid-heading) - should pass
- [Invalid link](#missing-heading) - should fail
"#;

    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the one invalid internal link
    assert_eq!(warnings.len(), 1, "Should only warn about #missing-heading");
    assert!(warnings[0].message.contains("#missing-heading"));

    // Verify Liquid templates are not flagged incorrectly
    for warning in &warnings {
        // Should not include filter syntax in the fragment
        assert!(!warning.message.contains("| relative_url"));
        assert!(!warning.message.contains("}}"));
        assert!(!warning.message.contains("%}"));
    }
}

#[test]
fn test_issue_39_underscore_edge_cases_jekyll() {
    // Test underscore handling with Jekyll/kramdown GFM anchor style
    use rumdl_lib::utils::anchor_styles::AnchorStyle;

    let rule = MD051LinkFragments::with_anchor_style(AnchorStyle::KramdownGfm);

    let content = r#"# Test Underscore Cases

### PHP $_REQUEST

### sched_debug

#### Update login_type

#### Add ldap_monitor to delegator$

Test links (using Jekyll/kramdown expected anchors):

- [PHP Request](#php-_request)
- [Sched Debug](#sched_debug)
- [Update Login](#update-login_type)
- [LDAP Monitor](#add-ldap_monitor-to-delegator)
"#;

    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    // All links should be valid with Jekyll anchor style
    assert_eq!(warnings.len(), 0, "All Jekyll-style links should be valid");
}
