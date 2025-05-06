use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD053LinkImageReferenceDefinitions;

#[test]
fn test_all_references_used() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[link1][id1]\n[link2][id2]\n![image][id3]\n\n[id1]: http://example.com/1\n[id2]: http://example.com/2\n[id3]: http://example.com/3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_unused_reference() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[link1][id1]\n\n[id1]: http://example.com/1\n[id2]: http://example.com/2";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "[link1][id1]\n\n[id1]: http://example.com/1");
}

#[test]
fn test_shortcut_reference() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[example]\n\n[example]: http://example.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_unused_references() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[link1][id1]\n\n[id1]: http://example.com/1\n[id2]: http://example.com/2\n[id3]: http://example.com/3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "[link1][id1]\n\n[id1]: http://example.com/1");
}

#[test]
fn test_case_insensitive() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[example][ID]\n\n[id]: http://example.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_content() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_references() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_only_unused_references() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[id1]: http://example.com/1\n[id2]: http://example.com/2";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "");
}

#[test]
fn test_mixed_used_unused_references() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[link][used]\nSome text\n\n[used]: http://example.com/used\n[unused]: http://example.com/unused";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "[link][used]\nSome text\n\n[used]: http://example.com/used"
    );
}

#[test]
fn test_valid_reference_definitions() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[ref]: https://example.com\n[ref] is a link";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_unused_reference_definition() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[unused]: https://example.com\nThis has no references";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_multiple_references() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content =
        "[ref1]: https://example1.com\n[ref2]: https://example2.com\n[ref1] and [ref2] are links";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_image_references() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[img]: image.png\n![Image][img]";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_references() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[ref]: https://example.com\n[img]: image.png\n[ref] is a link and ![Image][img] is an image";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_ignored_definitions() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[ignored]: https://example.com\nNo references here";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_case_sensitivity() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[REF]: https://example.com\n[ref] is a link";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_unused_references() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[unused]: https://example.com\n[used]: https://example.com\n[used] is a link";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert!(!result.contains("[unused]"));
}

#[test]
fn test_with_document_structure() {
    let rule = MD053LinkImageReferenceDefinitions::new();
    let content = "[link1][id1]\n[link2][id2]\n![image][id3]\n\n[id1]: http://example.com/1\n[id2]: http://example.com/2\n[id3]: http://example.com/3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}
