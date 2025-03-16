use rumdl::rule::Rule;
use rumdl::rules::MD054LinkImageStyle;

#[test]
fn test_name() {
    let rule = MD054LinkImageStyle::default();
    assert_eq!(rule.name(), "MD054");
}

#[test]
fn test_consistent_link_styles() {
    let rule = MD054LinkImageStyle::default();

    // All inline links - should be valid
    let content = r#"
This is a document with [inline links](https://example.com).
Here's another [link](https://example2.com).
    "#;

    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);

    // Mixed styles but with configuration allowing all styles
    let content = r#"
This is a document with [inline links](https://example.com).
Here's an <https://example.com> autolink.
Here's a [collapsed][] link.
[collapsed]: https://example.com
    "#;

    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_inconsistent_link_styles() {
    // Test with configuration disallowing autolinks
    let rule = MD054LinkImageStyle::new(false, true, true, true, true, true);

    let content = r#"
This is a document with [inline links](https://example.com).
Here's an <https://example.com> autolink.
    "#;

    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
    assert_eq!(
        result[0].message,
        "Link/image style 'autolink' is not consistent with document"
    );
}

#[test]
fn test_code_blocks_ignored() {
    let rule = MD054LinkImageStyle::new(false, true, true, true, true, true);

    let content = r#"
This is a document with [inline links](https://example.com).

```markdown
Here's an <https://example.com> autolink in a code block.
```

This is an inline code with a link: `<https://example.com>`
    "#;

    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_fix_unsupported() {
    let rule = MD054LinkImageStyle::default();

    let content = r#"
This has [inline](https://example.com) and <https://example.org> links.
    "#;

    let result = rule.fix(content);
    assert!(result.is_err());
}

#[test]
fn test_url_inline_style() {
    let rule = MD054LinkImageStyle::new(true, true, true, true, true, false);

    let content = r#"
This is a [https://example.com](https://example.com) URL-inline link.
    "#;

    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(
        result[0].message,
        "Link/image style 'url_inline' is not consistent with document"
    );
}

#[test]
fn test_full_and_shortcut_references() {
    let rule = MD054LinkImageStyle::new(true, true, false, true, false, true);

    let content = r#"
This is an [inline link](https://example.com).
This is a [full reference][ref] link.
This is a [shortcut] reference.

[ref]: https://example.com
[shortcut]: https://shortcut.com
    "#;

    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    assert!(result
        .iter()
        .any(|w| w.line == 3 && w.message.contains("full")));
    assert!(result
        .iter()
        .any(|w| w.line == 4 && w.message.contains("shortcut")));
}

#[test]
fn test_all_link_types() {
    // Test to make sure we can detect all link types
    let rule = MD054LinkImageStyle::default();

    let content = r#"
[Inline link](https://example.com)
<https://example.com>
[Collapsed][]
[Full reference][full]
[Shortcut]
[https://example.com](https://example.com)

[Collapsed]: https://example.com
[full]: https://example.com/full
[Shortcut]: https://example.com/shortcut
    "#;

    // Should be valid since all styles are allowed by default
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_unicode_support() {
    // Test handling of Unicode characters in links
    let rule = MD054LinkImageStyle::default();

    let content = r#"
[Unicode café link](https://example.com/café)
<https://example.com/café>
[Unicode emoji 🔗][emoji-ref]
[Unicode 汉字 characters][han]
[🔗 emoji shortcut]
[café][]

[emoji-ref]: https://example.com/emoji/🔗
[han]: https://example.com/汉字
[🔗 emoji shortcut]: https://emoji.example.com
[café]: https://example.com/café
    "#;

    // Should be valid since all styles are allowed by default
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);

    // Test with disallowed styles
    let rule_restricted = MD054LinkImageStyle::new(false, true, true, true, true, true);

    let content_mixed = r#"
[Unicode link](https://example.com/café)
<https://example.com/unicode/汉字>
    "#;

    let result = rule_restricted.check(content_mixed).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
    assert_eq!(
        result[0].message,
        "Link/image style 'autolink' is not consistent with document"
    );

    // Test with long Unicode content that might cause byte indexing issues
    let content_long = r#"
This is a very long line with some [Unicode content including many characters like café, 汉字, ñáéíóú, こんにちは, привет, שלום, مرحبا, and many more symbols like ⚡🔥🌟✨🌈⭐💫🌠 in a very long text](https://example.com/unicode).
    "#;

    let result = rule.check(content_long).unwrap();
    assert_eq!(result.len(), 0);

    // Test with reversed link syntax containing Unicode
    let content_reversed = r#"
This is a reversed link with Unicode: (Unicode café)[https://example.com/café]
    "#;

    // This should be caught by MD011, not MD054, so no warnings here
    let result = rule.check(content_reversed).unwrap();
    assert_eq!(result.len(), 0);
}
