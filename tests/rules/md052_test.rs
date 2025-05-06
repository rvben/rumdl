use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD052ReferenceLinkImages;

#[test]
fn test_valid_reference_link() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "[example][id]\n\n[id]: http://example.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_reference_link() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "[example][id]\n\n[other]: http://example.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_valid_reference_image() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "![example][id]\n\n[id]: http://example.com/image.jpg";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_reference_image() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "![example][id]\n\n[other]: http://example.com/image.jpg";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_shortcut_reference() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "[example]\n\n[example]: http://example.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_shortcut_reference() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "[example]\n\n[other]: http://example.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_multiple_references() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "[link1][id1]\n[link2][id2]\n![image][id3]\n\n[id1]: http://example.com/1\n[id2]: http://example.com/2\n[id3]: http://example.com/3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_case_insensitive() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "[example][ID]\n\n[id]: http://example.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_content() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_references() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}
