use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD054LinkImageStyle;

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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Mixed styles but with configuration allowing all styles
    let content = r#"
This is a document with [inline links](https://example.com).
Here's an <https://example.com> autolink.
Here's a [collapsed][] link.
[collapsed]: https://example.com
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[0].message, "Link/image style 'autolink' is not allowed");
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_fix_default_config_is_noop() {
    // With the default config (all styles allowed), fix is a no-op even though
    // the rule now supports auto-fixing in the general case.
    let rule = MD054LinkImageStyle::default();

    let content = r#"
This has [inline](https://example.com) and <https://example.org> links.
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_url_inline_style() {
    let rule = MD054LinkImageStyle::new(true, true, true, true, true, false);

    let content = r#"
This is a [https://example.com](https://example.com) URL-inline link.
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "Link/image style 'url-inline' is not allowed");
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.iter().any(|w| w.line == 3 && w.message.contains("full")));
    assert!(result.iter().any(|w| w.line == 4 && w.message.contains("shortcut")));
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Test with disallowed styles
    let rule_restricted = MD054LinkImageStyle::new(false, true, true, true, true, true);

    let content_mixed = r#"
[Unicode link](https://example.com/café)
<https://example.com/unicode/汉字>
    "#;

    let ctx_mixed = LintContext::new(content_mixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule_restricted.check(&ctx_mixed).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[0].message, "Link/image style 'autolink' is not allowed");

    // Test with long Unicode content that might cause byte indexing issues
    let content_long = r#"
This is a very long line with some [Unicode content including many characters like café, 汉字, ñáéíóú, こんにちは, привет, שלום, مرحبا, and many more symbols like ⚡🔥🌟✨🌈⭐💫🌠 in a very long text](https://example.com/unicode).
    "#;

    let ctx_long = LintContext::new(content_long, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx_long).unwrap();
    assert_eq!(result.len(), 0);

    // Test with reversed link syntax containing Unicode
    let content_reversed = r#"
This is a reversed link with Unicode: (Unicode café)[https://example.com/café]
    "#;

    // This should be caught by MD011, not MD054, so no warnings here
    let ctx_reversed = LintContext::new(content_reversed, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "All image styles should be valid by default");

    // Disallow collapsed style
    let rule_no_collapse = MD054LinkImageStyle::new(true, false, true, true, true, true);
    let content_mix = r#"
An ![inline image](img.png).
An ![collapsed image][].

[collapsed image]: img.png
    "#;
    let ctx_mix = LintContext::new(content_mix, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule_no_collapse.check(&ctx_mix).unwrap();
    assert_eq!(result.len(), 1, "Should flag disallowed collapsed image style");
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
    let ctx_code = LintContext::new(content_code, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx_code).unwrap();
    assert!(result.is_empty(), "Image styles in code spans should be ignored");
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx_flag_shortcut = LintContext::new(content_flag_shortcut, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Links in HTML comments should not be flagged");
}

#[test]
fn test_autolink_unicode_in_and_outside_comments() {
    let rule = MD054LinkImageStyle::new(false, true, true, true, true, true); // Disallow autolink
    let content = r#"
This is an autolink: <https://example.com/汉字>
<!-- This is a comment with an autolink: <https://example.com/汉字> -->
"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Only autolink outside comment should be flagged");
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "Link/image style 'autolink' is not allowed");
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        2,
        "Only shortcut and autolink outside comments should be flagged"
    );
    assert!(result.iter().any(|w| w.message.contains("shortcut")));
    assert!(result.iter().any(|w| w.message.contains("autolink")));
}

#[test]
fn test_task_list_checkboxes_not_flagged_as_shortcut() {
    // Task list checkboxes should not be flagged as shortcut links (fixes issue #221)
    let rule = MD054LinkImageStyle::new(true, true, true, true, false, true); // shortcut = false

    let content = r#"
# Todos

- [ ] Task 1
- [x] Task 2
- [X] Task 3
* [ ] Task 4
+ [x] Task 5

[actual shortcut]: https://example.com
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Task list checkboxes should NOT be flagged
    // Only the actual shortcut link should be flagged (if there was one in the content)
    // But wait, there's no actual shortcut link usage in the content, only the definition
    // So there should be no warnings
    assert_eq!(
        result.len(),
        0,
        "Task list checkboxes should not be flagged as shortcut links"
    );
}

#[test]
fn test_task_list_vs_shortcut_distinction() {
    // Test that we can distinguish between task lists and actual shortcut links (fixes issue #221)
    let rule = MD054LinkImageStyle::new(true, true, true, true, false, true); // shortcut = false

    let content = r#"
- [ ] This is a task list item
- [actual link] This is a shortcut link that should be flagged

[actual link]: https://example.com
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Only the shortcut link should be flagged, not the task list checkbox
    assert_eq!(result.len(), 1, "Should flag shortcut link but not task list checkbox");
    assert!(result[0].message.contains("shortcut"));
    assert_eq!(result[0].line, 3, "Should flag the line with the shortcut link");
}

#[test]
fn test_html_tags_not_flagged_as_autolink() {
    // HTML tags should not be flagged as autolink links (fixes issue #222)
    let rule = MD054LinkImageStyle::new(false, true, true, true, true, true); // autolink = false

    let content = r#"
# Example

One line.<br>
Second line.

<https://example.com> This should be flagged
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // HTML tag <br> should NOT be flagged
    // Only the actual autolink <https://example.com> should be flagged
    assert_eq!(result.len(), 1, "Should flag autolink but not HTML tag");
    assert!(result[0].message.contains("autolink"));
    assert_eq!(result[0].line, 7, "Should flag the line with the autolink");
}

#[test]
fn test_various_html_tags_not_flagged() {
    // Test various HTML tags that should not be flagged (fixes issue #222)
    let rule = MD054LinkImageStyle::new(false, true, true, true, true, true); // autolink = false

    let content = r#"
<div>Content</div>
<span>Text</span>
<br>
<hr>
<img src="image.png">
<p>Paragraph</p>
<strong>Bold</strong>
<em>Italic</em>

<https://example.com> This should be flagged
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Only the autolink should be flagged, not any HTML tags
    assert_eq!(result.len(), 1, "Should flag autolink but not HTML tags");
    assert!(result[0].message.contains("autolink"));
}

#[test]
fn test_autolink_urls_still_detected() {
    // Test that actual autolink URLs are still correctly detected (fixes issue #222)
    let rule = MD054LinkImageStyle::new(false, true, true, true, true, true); // autolink = false

    let content = r#"
<br> HTML tag should not be flagged
<https://example.com> Autolink should be flagged
<http://example.com> HTTP autolink should be flagged
<mailto:test@example.com> Mailto should be flagged
<ftp://example.com> FTP should be flagged
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // All autolinks should be flagged, but not the HTML tag
    assert_eq!(result.len(), 4, "Should flag all autolinks but not HTML tag");
    assert!(result.iter().all(|w| w.message.contains("autolink")));
}

// ---------------------------------------------------------------------------
// Inline-disable directives must suppress *both* the warning and the auto-fix.
//
// Pre-fix behavior: the framework filtered the warning between `check()` and
// the user, but `Rule::fix()` and the per-warning `Fix` field both bypassed
// that filter — so a `<!-- markdownlint-disable-line MD054 -->` annotation
// would silently rewrite the link anyway. These tests pin the new contract:
// disabled lines are untouched at *every* layer (check, per-warning fix, and
// rule-level fix).
// ---------------------------------------------------------------------------

#[test]
fn test_disable_line_skips_link_fix() {
    let rule = MD054LinkImageStyle::new(true, true, true, false, true, true); // inline disallowed

    // The first line carries an inline-disable-line directive; the second
    // does not. Both contain the same disallowed inline link.
    let content = "[a](https://x.com) <!-- markdownlint-disable-line MD054 -->\n[b](https://y.com)\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    // Disabled link must remain bit-identical; non-disabled link is rewritten
    // to one of the other allowed styles (full reference is the planner's
    // default expansion). The exact target doesn't matter — what matters is
    // that the disabled line is preserved verbatim.
    assert!(
        fixed.starts_with("[a](https://x.com) <!-- markdownlint-disable-line MD054 -->\n"),
        "disable-line directive must protect link from fix; got:\n{fixed}"
    );
    assert!(
        !fixed.contains("[b](https://y.com)"),
        "non-disabled inline link must be rewritten; got:\n{fixed}"
    );
}

#[test]
fn test_disable_next_line_skips_link_fix() {
    let rule = MD054LinkImageStyle::new(true, true, true, false, true, true);

    let content = "<!-- markdownlint-disable-next-line MD054 -->\n[a](https://x.com)\n[b](https://y.com)\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("[a](https://x.com)"),
        "disable-next-line must protect the following link; got:\n{fixed}"
    );
    assert!(
        !fixed.contains("[b](https://y.com)"),
        "non-disabled inline link must be rewritten; got:\n{fixed}"
    );
}

#[test]
fn test_persistent_disable_skips_link_fix() {
    let rule = MD054LinkImageStyle::new(true, true, true, false, true, true);

    let content = concat!(
        "[a](https://before.com)\n",
        "<!-- markdownlint-disable MD054 -->\n",
        "[b](https://disabled.com)\n",
        "[c](https://still-disabled.com)\n",
        "<!-- markdownlint-enable MD054 -->\n",
        "[d](https://after.com)\n",
    );
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("[b](https://disabled.com)"),
        "persistent disable must protect [b]; got:\n{fixed}"
    );
    assert!(
        fixed.contains("[c](https://still-disabled.com)"),
        "persistent disable must protect [c] until re-enabled; got:\n{fixed}"
    );
    assert!(
        !fixed.contains("[a](https://before.com)"),
        "[a] (before disable) must still be rewritten; got:\n{fixed}"
    );
    assert!(
        !fixed.contains("[d](https://after.com)"),
        "[d] (after re-enable) must still be rewritten; got:\n{fixed}"
    );
}

#[test]
fn test_disable_line_skips_image_fix() {
    let rule = MD054LinkImageStyle::new(true, true, true, false, true, true);

    let content = "![a](https://x.com/img.png) <!-- markdownlint-disable-line MD054 -->\n![b](https://y.com/img.png)\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.starts_with("![a](https://x.com/img.png) <!-- markdownlint-disable-line MD054 -->\n"),
        "disable-line must protect image from fix; got:\n{fixed}"
    );
    assert!(
        !fixed.contains("![b](https://y.com/img.png)"),
        "non-disabled image must be rewritten; got:\n{fixed}"
    );
}

// ---------------------------------------------------------------------------
// Explicit empty titles (`[t](url "")`, `[t](url '')`, `[t](url ())`) carry
// no semantic title text but do carry author intent — MD054's auto-fix must
// preserve them. In particular, conversion to autolink (`<url>`) would silently
// drop the delimiters, so the planner's `has_title` reachability check must
// see explicit-empty as truthy.
// ---------------------------------------------------------------------------

#[test]
fn test_explicit_empty_title_blocks_autolink_conversion() {
    // Allow only autolink. With `[https://x.com](https://x.com)` (url-inline)
    // as the source, autolink is the only reachable target; the planner's
    // `has_title` gate is the only thing standing between an explicit empty
    // title and a destructive rewrite.
    let rule = MD054LinkImageStyle::new(true, false, false, false, false, false); // autolink only

    let with_title = "[https://x.com](https://x.com \"\")\n";
    let ctx = LintContext::new(with_title, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, with_title,
        "explicit empty title must block autolink conversion (no other reachable target); got:\n{fixed}"
    );

    // Sanity-check the converse — without the explicit empty title, the same
    // input *does* convert to autolink, proving the title is the deciding
    // factor.
    let without_title = "[https://x.com](https://x.com)\n";
    let ctx = LintContext::new(without_title, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, "<https://x.com>\n",
        "without explicit empty title, url-inline must convert to autolink; got:\n{fixed}"
    );
}

#[test]
fn test_explicit_empty_title_round_trips_to_full_reference() {
    // When autolink isn't reachable (because of `has_title`) but `full` is
    // allowed, the planner must fall through to a target that *does* preserve
    // the title delimiters. The emitted ref-def must carry `""` so the
    // round-trip stays lossless.
    let rule = MD054LinkImageStyle::new(false, false, true, false, false, false); // full only

    let content = "[t](https://x.com \"\")\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("[t][") && fixed.contains("\"\""),
        "fix must convert to a ref form whose definition preserves \"\"; got:\n{fixed}"
    );
}

#[test]
fn test_disable_line_clears_per_warning_fix_metadata() {
    // `check()` itself doesn't filter disabled warnings (the framework does
    // that on the way out), so the per-warning `Fix` it attaches is observed
    // directly by tooling that bypasses the framework filter (snapshot tests,
    // LSP single-warning code actions in some paths). Pin that the planner
    // produces no edit for a disabled line in the first place — there's
    // nothing for downstream consumers to misuse.
    let rule = MD054LinkImageStyle::new(true, true, true, false, true, true);
    let content = "[a](https://x.com) <!-- markdownlint-disable-line MD054 -->\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();
    for w in result {
        assert!(
            w.fix.is_none(),
            "disabled line must not produce a per-warning Fix; got fix={:?}",
            w.fix
        );
    }
}
