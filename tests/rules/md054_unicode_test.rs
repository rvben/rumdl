use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD054LinkImageStyle;

#[test]
fn test_unicode_edge_cases() {
    // Test handling of Unicode edge cases
    let rule = MD054LinkImageStyle::default();

    // Test with Unicode characters that might break byte indexing
    let content = r#"
[Unicode with combining characters café̷̲̤̠̆](https://example.com/café)
[Unicode with zero width joiners 👨‍👩‍👧‍👦](https://example.com/family)
[Unicode with RTL characters مرحبا שלום](https://example.com/rtl)
[🔥🌟✨Unicode link with lots of emojis 🌈⭐💫🌠](https://example.com/emoji)
"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Unicode characters should not trigger warnings");

    // Test with very long Unicode strings that might cause overflow
    let content_long = r#"
[This is a very long link text with a mix of Latin and Unicode characters: 
café, ñáéíóú, こんにちは, привет, 汉字, مرحبا, שלום, 
and many many more characters to ensure we have a lengthy text that 
could potentially cause issues with byte indexing if not handled properly.
This text is intentionally very long to test edge cases with string length handling.
](https://example.com/long-unicode)
"#;

    let ctx = LintContext::new(content_long);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Long Unicode content should not cause panic");

    // Test with mixed URL styles containing Unicode
    let content_mixed = r#"
Here's a standard link: [Unicode café](https://example.com/café)
And an autolink: <https://example.com/汉字>
And a shortcut reference: [🔥 emoji shortcut]
And a collapsed reference: [café][]
And a full reference: [Unicode 汉字][unicode-ref]

[🔥 emoji shortcut]: https://emoji.example.com
[café]: https://café.example.com
[unicode-ref]: https://unicode.example.com/汉字
"#;

    // Test with a restricted style configuration
    let rule_restricted = MD054LinkImageStyle::new(true, false, true, true, true, true);
    let ctx = LintContext::new(content_mixed);
    let result = rule_restricted.check(&ctx).unwrap();
    assert!(
        !result.is_empty(),
        "Restricted styles with Unicode should generate warnings"
    );

    // Test with boundaries at Unicode character boundaries
    let content_boundaries = r#"
Text before [Unicode link at exact چmulti-byteڇ character boundary](https://example.com)
"#;

    let ctx = LintContext::new(content_boundaries);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Unicode character boundaries should not cause issues");
}

#[test]
fn test_unicode_images() {
    // Test handling of Unicode characters in image links
    let rule = MD054LinkImageStyle::default();

    let content = r#"
![Unicode café alt text](https://example.com/café.jpg)
![Unicode 汉字 alt text](https://example.com/汉字.png)
![Emoji 🔥 alt text][emoji-ref]
![Mixed Unicode ñáéíóú alt text][ref]

[emoji-ref]: https://example.com/emoji/🔥.jpg
[ref]: https://example.com/unicode/ñáéíóú.png
"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Unicode in image links should not trigger warnings");

    // Test with restricted styles
    let rule_restricted = MD054LinkImageStyle::new(true, true, false, true, true, true);

    let content_mixed = r#"
![Unicode image](https://example.com/café.jpg)
![Another Unicode image][unicode-ref]

[unicode-ref]: https://example.com/汉字.png
"#;

    let ctx = LintContext::new(content_mixed);
    let result = rule_restricted.check(&ctx).unwrap();
    assert!(
        !result.is_empty(),
        "Restricted styles with Unicode images should generate warnings"
    );
}

#[test]
fn test_shortcut_link() {
    let rule = MD054LinkImageStyle::default();

    // Test for multi-byte character after shortcut link
    let shortcut_lnk = "[https://www.example.com]例";
    let ctx = LintContext::new(shortcut_lnk);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}
