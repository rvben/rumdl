use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD042NoEmptyLinks;

#[test]
fn test_mkdocs_paragraph_anchor_not_flagged() {
    // Issue #100: MkDocs paragraph anchors should not be flagged as empty links
    let rule = MD042NoEmptyLinks::new();

    // Valid MkDocs anchor syntax
    let content = "[](){ #anchor_name }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag [](){{ #anchor_name }} in MkDocs mode (issue #100). Got: {result:?}"
    );
}

#[test]
fn test_mkdocs_paragraph_anchor_in_context() {
    let rule = MD042NoEmptyLinks::new();

    // Paragraph with anchor
    let content = r#"This is a paragraph.
[](){ #my-anchor }

Another paragraph.
[](){ #another-anchor }"#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag paragraph anchors in MkDocs mode. Got: {result:?}"
    );
}

#[test]
fn test_mkdocs_list_item_anchor() {
    let rule = MD042NoEmptyLinks::new();

    // List items with anchors
    let content = r#"- First item
  [](){ #first }
- Second item
  [](){ #second }"#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag list item anchors in MkDocs mode. Got: {result:?}"
    );
}

#[test]
fn test_mkdocs_anchor_with_classes() {
    let rule = MD042NoEmptyLinks::new();

    // Anchors can also have classes
    let content = "[](){ #anchor .class1 .class2 }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag anchors with classes in MkDocs mode. Got: {result:?}"
    );
}

#[test]
fn test_standard_mode_still_flags_empty_links() {
    let rule = MD042NoEmptyLinks::new();

    // In standard mode, this should still be flagged
    let content = "[](){ #anchor_name }";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should flag [](){{ #anchor_name }} in Standard mode. Got: {result:?}"
    );
}

#[test]
fn test_actual_empty_links_still_flagged_in_mkdocs() {
    let rule = MD042NoEmptyLinks::new();

    // Actual empty links without the attribute syntax should still be flagged
    let content = "[]() without attributes";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should still flag actual empty links in MkDocs mode. Got: {result:?}"
    );
}

#[test]
fn test_empty_link_with_url_not_flagged_in_mkdocs() {
    // MD042 only flags empty URLs, not empty text
    let rule = MD042NoEmptyLinks::new();

    // Empty text with valid URL is not flagged (accessibility concern, not "empty link")
    let content = "[](https://example.com)";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Empty text with valid URL should not be flagged. Got: {result:?}"
    );
}
