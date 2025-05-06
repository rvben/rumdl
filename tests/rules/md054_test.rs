use rumdl::lint_context::LintContext;
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

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Mixed styles but with configuration allowing all styles
    let content = r#"
This is a document with [inline links](https://example.com).
Here's an <https://example.com> autolink.
Here's a [collapsed][] link.
[collapsed]: https://example.com
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
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

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
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

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_fix_unsupported() {
    let rule = MD054LinkImageStyle::default();

    let content = r#"
This has [inline](https://example.com) and <https://example.org> links.
    "#;

    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx);
    assert!(result.is_err());
}

#[test]
fn test_url_inline_style() {
    let rule = MD054LinkImageStyle::new(true, true, true, true, true, false);

    let content = r#"
This is a [https://example.com](https://example.com) URL-inline link.
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
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

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
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
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
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
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Test with disallowed styles
    let rule_restricted = MD054LinkImageStyle::new(false, true, true, true, true, true);

    let content_mixed = r#"
[Unicode link](https://example.com/café)
<https://example.com/unicode/汉字>
    "#;

    let ctx_mixed = LintContext::new(content_mixed);
    let result = rule_restricted.check(&ctx_mixed).unwrap();
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

    let ctx_long = LintContext::new(content_long);
    let result = rule.check(&ctx_long).unwrap();
    assert_eq!(result.len(), 0);

    // Test with reversed link syntax containing Unicode
    let content_reversed = r#"
This is a reversed link with Unicode: (Unicode café)[https://example.com/café]
    "#;

    // This should be caught by MD011, not MD054, so no warnings here
    let ctx_reversed = LintContext::new(content_reversed);
    let result = rule.check(&ctx_reversed).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_image_styles() {
    // Default: all styles allowed
    let rule = MD054LinkImageStyle::default();
    let content = r#"
An ![inline image](img.png).
An ![collapsed image][].
A ![full image][ref].
A ![shortcut image].

[collapsed image]: img.png
[ref]: img.png
[shortcut image]: img.png
    "#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "All image styles should be valid by default"
    );

    // Disallow collapsed style
    let rule_no_collapse = MD054LinkImageStyle::new(true, false, true, true, true, true);
    let content_mix = r#"
An ![inline image](img.png).
An ![collapsed image][].

[collapsed image]: img.png
    "#;
    let ctx_mix = LintContext::new(content_mix);
    let result = rule_no_collapse.check(&ctx_mix).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should flag disallowed collapsed image style"
    );
    assert_eq!(result[0].line, 3);
    assert!(result[0].message.contains("collapsed"));

    // Ensure images are ignored in code spans
    let content_code = r#"
This has an `![image](img.png)` in inline code.
And `![collapsed][]`
And `![full][ref]`
And `![shortcut]`

[collapsed]: img.png
[ref]: img.png
[shortcut]: img.png
    "#;
    let ctx_code = LintContext::new(content_code);
    let result = rule.check(&ctx_code).unwrap();
    assert!(
        result.is_empty(),
        "Image styles in code spans should be ignored"
    );
}

#[test]
fn test_shortcut_edge_cases() {
    // Default: all styles allowed
    let rule = MD054LinkImageStyle::default();

    // Ensure [shortcut] isn't confused with [collapsed][] or [full][ref]
    let content = r#"
Link [shortcut] followed by [another].
Link [collapsed][] followed by text.
Link [full][ref] followed by text.

[shortcut]: /shortcut
[another]: /another
[collapsed]: /collapsed
[ref]: /full
    "#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Shortcut detection should not interfere with other types"
    );

    // Disallow shortcut, ensure others are still detected correctly
    let rule_no_shortcut = MD054LinkImageStyle::new(true, true, true, true, false, true);
    let content_flag_shortcut = r#"
[Okay collapsed][]
[Okay full][ref]
[Not okay shortcut]

[Okay collapsed]: /
[ref]: /
[Not okay shortcut]: /
    "#;
    let ctx_flag_shortcut = LintContext::new(content_flag_shortcut);
    let result = rule_no_shortcut.check(&ctx_flag_shortcut).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 4);
    assert!(result[0].message.contains("shortcut"));
}

#[test]
fn test_html_comments_are_ignored() {
    let rule = MD054LinkImageStyle::new(false, false, false, false, false, false); // Disallow all styles
    let content = r#"
<!-- This is a comment with an autolink: <https://example.com> -->
<!-- Unicode autolink: <https://example.com/汉字> -->
<!-- [inline link](https://example.com) -->
<!-- [Unicode café link](https://example.com/café) -->
"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Links in HTML comments should not be flagged"
    );
}

#[test]
fn test_autolink_unicode_in_and_outside_comments() {
    let rule = MD054LinkImageStyle::new(false, true, true, true, true, true); // Disallow autolink
    let content = r#"
This is an autolink: <https://example.com/汉字>
<!-- This is a comment with an autolink: <https://example.com/汉字> -->
"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Only autolink outside comment should be flagged"
    );
    assert_eq!(result[0].line, 2);
    assert_eq!(
        result[0].message,
        "Link/image style 'autolink' is not consistent with document"
    );
}

#[test]
fn test_mixed_styles_in_and_outside_comments() {
    let rule = MD054LinkImageStyle::new(false, false, true, true, false, false); // Only full and inline allowed
    let content = r#"
[inline link](https://example.com)
[full ref][ref]
[shortcut]
<https://example.com>
<!-- [shortcut] and <https://example.com> in comment should not be flagged -->
<!-- [shortcut] <https://example.com> -->

[ref]: https://example.com
[shortcut]: https://example.com
"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        2,
        "Only shortcut and autolink outside comments should be flagged"
    );
    assert!(result.iter().any(|w| w.message.contains("shortcut")));
    assert!(result.iter().any(|w| w.message.contains("autolink")));
}
