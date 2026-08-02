use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD045NoAltText;

#[test]
fn test_valid_alt_text() {
    let rule = MD045NoAltText::new();
    let content = "![Alt text](image.png)\n![Another description](path/to/image.jpg)";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_missing_alt_text() {
    let rule = MD045NoAltText::new();
    let content = "![](image.png)";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].fix.is_none(), "MD045 should not offer auto-fix");
}

#[test]
fn test_empty_alt_text() {
    let rule = MD045NoAltText::new();
    let content = "![ ](image.png)";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].fix.is_none());
}

#[test]
fn test_multiple_images() {
    let rule = MD045NoAltText::new();
    let content = "![Alt text](image1.png)\n![](image2.png)\n![ ](image3.png)";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    // fix() should return content unchanged
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_complex_urls() {
    let rule = MD045NoAltText::new();
    let content = "![](https://example.com/image.png?param=value#fragment)";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_mixed_content() {
    let rule = MD045NoAltText::new();
    let content = "# Images\n\nSome text here\n\n![Alt text](image1.png)\n\nMore text\n\n![](image2.png)";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 9);
}

#[test]
fn test_inline_images() {
    let rule = MD045NoAltText::new();
    let content = "Text with ![Alt text](inline1.png) and ![](inline2.png) images.";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_images_in_code_blocks() {
    let rule = MD045NoAltText::new();
    let content = r#"# Documentation

Here's an actual image:
![](actual-image.png)

Here's how to use images in markdown:

```markdown
![](example1.png)
![ ](example2.png)
![Alt text](example3.png)
```

Another actual image:
![  ](another-image.png)

And in inline code: `![](inline.png)` should also be ignored.
"#;

    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only detect the two actual images outside code blocks
    assert_eq!(result.len(), 2, "Should only detect images outside code blocks");
    assert_eq!(result[0].line, 4); // actual-image.png
    assert_eq!(result[1].line, 15); // another-image.png
}

/// A wiki embed has no alt-text slot, so no edit to the source can satisfy
/// MD045. `![[note]]` transcludes the target's content instead of rendering an
/// image, and per the Obsidian docs the pipe in `![[Engelbart.jpg|100]]` sets
/// the rendered dimensions.
#[test]
fn test_wiki_embeds_are_never_reported() {
    let rule = MD045NoAltText::new();
    let embeds = [
        "![[screenshot.png]]",
        "![[Target]]",
        "![[Target#A heading]]",
        "![[Target#^b15695]]",
        "![[image.png|]]",
        "![[image.png|   ]]",
        "![[image.png|300]]",
        "![[image.png|640x480]]",
        "![[subfolder/image.png|A descriptive alt]]",
        "![[my diagram.png]]",
        "- ![[diagram.png]]",
        "> ![[diagram.png]]",
        "Text ![[diagram.png]] more text.",
    ];

    // Wikilinks are enabled for every flavor, so the exemption cannot be
    // gated on Obsidian.
    for flavor in [
        rumdl_lib::config::MarkdownFlavor::Obsidian,
        rumdl_lib::config::MarkdownFlavor::Standard,
    ] {
        for embed in embeds {
            let content = format!("# Test\n\n{embed}\n");
            let ctx = rumdl_lib::lint_context::LintContext::new(&content, flavor, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "{flavor:?}: {embed} has no alt-text slot, got: {result:?}"
            );
        }
    }
}

#[test]
fn test_markdown_images_beside_wiki_embeds_are_still_reported() {
    let rule = MD045NoAltText::new();
    let content = "![Standard alt](image1.png)\n![[wikilink-no-alt.png]]\n![[wikilink-with-alt.png|Alt text]]\n![](standard-no-alt.png)";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Obsidian, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Only the markdown image without alt text should warn, got: {result:?}"
    );
    assert_eq!(result[0].line, 4);
}

#[test]
fn test_wiki_embed_in_code_block() {
    let rule = MD045NoAltText::new();
    // The markdown image on the last line is the positive control: without it
    // this file is silent for reasons that have nothing to do with the fence.
    let content = "```\n![[in-code.png]]\n```\n\n![[outside-code.png]]\n\n![](outside.png)";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Obsidian, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Only the markdown image should warn, got: {result:?}");
    assert_eq!(result[0].line, 7);
}

#[test]
fn test_obsidian_wikilink_non_image_link_ignored() {
    // Non-image wikilinks [[page]] should not be checked by MD045
    let rule = MD045NoAltText::new();
    let content = "# Test\n\n[[some-page]]\n[[another|Display text]]";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Obsidian, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Non-image wikilinks should not trigger MD045");
}

#[test]
fn test_descriptive_filenames_not_used_for_alt() {
    // MD045 is diagnostic-only; it should NOT generate alt text from filenames
    let rule = MD045NoAltText::new();
    let content = "![](user-profile.jpg)\n![](product_screenshot.png)\n![](logo-dark-mode.svg)";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);

    // fix() should return content unchanged
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "MD045 should not modify content (diagnostic-only)");
}
