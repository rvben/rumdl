use rumdl::rule::Rule;
use rumdl::rules::MD052ReferenceLinkImages;

#[test]
fn test_valid_reference_link() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "[example][id]\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_reference_link() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "[example][id]\n\n[other]: http://example.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_valid_reference_image() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "![example][id]\n\n[id]: http://example.com/image.jpg";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_reference_image() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "![example][id]\n\n[other]: http://example.com/image.jpg";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_shortcut_reference() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "[example]\n\n[example]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_shortcut_reference() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "[example]\n\n[other]: http://example.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_multiple_references() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "[link1][id1]\n[link2][id2]\n![image][id3]\n\n[id1]: http://example.com/1\n[id2]: http://example.com/2\n[id3]: http://example.com/3";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_case_insensitive() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "[example][ID]\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_content() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_references() {
    let rule = MD052ReferenceLinkImages::new();
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_inline_code_spans() {
    let rule = MD052ReferenceLinkImages::new();

    // Test reference links in inline code spans (should be ignored)
    let content = "[valid][id]\n\n`[example][missing]`\n\n[id]: http://example.com";
    let result = rule.check(content).unwrap();

    // We should have no warnings - the reference in inline code should be ignored
    assert_eq!(
        result.len(),
        0,
        "Reference link in inline code span should be ignored"
    );

    // Test with multiple code spans and mix of valid and invalid references
    let content =
        "`[invalid][missing]` and [valid][id] and `[another][nowhere]`\n\n[id]: http://example.com";

    let result = rule.check(content).unwrap();

    // Only valid references should be checked, the ones in code spans should be ignored
    assert_eq!(
        result.len(),
        0,
        "Only references outside code spans should be checked"
    );

    // Test with a reference link in inline code followed by a real invalid reference
    let content = "`[example][code]` and [invalid][missing]\n\n[id]: http://example.com";

    let result = rule.check(content).unwrap();

    // Only the real invalid reference should be caught
    assert_eq!(
        result.len(),
        1,
        "Only real invalid references should be caught"
    );
    assert_eq!(result[0].line, 1, "Warning should be on line 1");
    assert!(
        result[0].message.contains("missing"),
        "Warning should be about 'missing'"
    );

    // Test with shortcut references in code spans
    let content = "`[shortcut]` and [valid][id]\n\n[id]: http://example.com";

    let result = rule.check(content).unwrap();

    // No warnings for shortcut references in code spans
    assert_eq!(
        result.len(),
        0,
        "Shortcut references in code spans should be ignored"
    );
}
