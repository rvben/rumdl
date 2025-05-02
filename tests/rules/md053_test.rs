use rumdl::rule::Rule;
use rumdl::rules::DefinitionStyle;
use rumdl::rules::MD053LinkImageReferenceDefinitions;

#[test]
fn test_all_references_used() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[link1][id1]\n[link2][id2]\n![image][id3]\n\n[id1]: http://example.com/1\n[id2]: http://example.com/2\n[id3]: http://example.com/3";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_unused_reference() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[link1][id1]\n\n[id1]: http://example.com/1\n[id2]: http://example.com/2";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "[link1][id1]\n\n[id1]: http://example.com/1");
}

#[test]
fn test_shortcut_reference() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[example]\n\n[example]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_unused_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[link1][id1]\n\n[id1]: http://example.com/1\n[id2]: http://example.com/2\n[id3]: http://example.com/3";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "[link1][id1]\n\n[id1]: http://example.com/1");
}

#[test]
fn test_case_insensitive() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[example][ID]\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_content() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_only_unused_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[id1]: http://example.com/1\n[id2]: http://example.com/2";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "");
}

#[test]
fn test_mixed_used_unused_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[link][used]\nSome text\n\n[used]: http://example.com/used\n[unused]: http://example.com/unused";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "[link][used]\nSome text\n\n[used]: http://example.com/used"
    );
}

#[test]
fn test_valid_reference_definitions() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[ref]: https://example.com\n[ref] is a link";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_unused_reference_definition() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[unused]: https://example.com\nThis has no references";
    let result = rule.check(content).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_multiple_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content =
        "[ref1]: https://example1.com\n[ref2]: https://example2.com\n[ref1] and [ref2] are links";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_image_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[img]: image.png\n![Image][img]";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[ref]: https://example.com\n[img]: image.png\n[ref] is a link and ![Image][img] is an image";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_ignored_definitions() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[ignored]: https://example.com\nNo references here";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_case_sensitivity() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[REF]: https://example.com\n[ref] is a link";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_unused_references() {
    let rule = MD053LinkImageReferenceDefinitions::new(DefinitionStyle::default());
    let content = "[unused]: https://example.com\n[used]: https://example.com\n[used] is a link";
    let result = rule.fix(content).unwrap();
    assert!(!result.contains("[unused]"));
}
