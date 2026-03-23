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

#[test]
fn test_obsidian_wikilink_image_without_alt() {
    let rule = MD045NoAltText::new();
    let content = "# Test\n\n![[screenshot.png]]";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Obsidian, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Wikilink image without alt text should warn");
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_obsidian_wikilink_image_with_alt() {
    let rule = MD045NoAltText::new();
    let content = "# Test\n\n![[screenshot.png|Screenshot of the app]]";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Obsidian, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Wikilink image with pipe alt text should not warn, got: {result:?}"
    );
}

#[test]
fn test_obsidian_wikilink_image_with_whitespace_only_alt() {
    let rule = MD045NoAltText::new();
    let content = "![[image.png|   ]]";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Obsidian, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Wikilink image with whitespace-only alt should warn");
}

#[test]
fn test_obsidian_wikilink_image_pipe_in_filename() {
    // Pipe with no text after it should still count as no alt text
    let rule = MD045NoAltText::new();
    let content = "![[image.png|]]";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Obsidian, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Wikilink image with pipe but no alt text should warn");
}

#[test]
fn test_obsidian_mixed_image_styles() {
    let rule = MD045NoAltText::new();
    let content = "![Standard alt](image1.png)\n![[wikilink-no-alt.png]]\n![[wikilink-with-alt.png|Alt text]]\n![](standard-no-alt.png)";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Obsidian, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        2,
        "Should warn on wikilink without alt and standard without alt, got: {result:?}"
    );
}

#[test]
fn test_obsidian_wikilink_image_with_path() {
    let rule = MD045NoAltText::new();
    let content = "![[subfolder/image.png|A descriptive alt]]";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Obsidian, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Wikilink image with path and alt text should not warn"
    );
}

#[test]
fn test_obsidian_wikilink_image_in_code_block() {
    let rule = MD045NoAltText::new();
    let content = "```\n![[in-code.png]]\n```\n\n![[outside-code.png]]";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Obsidian, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Only the image outside the code block should warn");
    assert_eq!(result[0].line, 5);
}

#[test]
fn test_standard_flavor_wikilink_image_still_detected() {
    // Wikilinks are always enabled in pulldown-cmark, not just for Obsidian.
    // Verify that MD045 correctly handles wikilink images in Standard flavor too.
    let rule = MD045NoAltText::new();

    let content = "![[image.png]]";
    let ctx = rumdl_lib::lint_context::LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Wikilink image without alt should warn in Standard flavor"
    );

    let content_with_alt = "![[image.png|Alt text here]]";
    let ctx2 =
        rumdl_lib::lint_context::LintContext::new(content_with_alt, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result2 = rule.check(&ctx2).unwrap();
    assert!(
        result2.is_empty(),
        "Wikilink image with alt should not warn in Standard flavor, got: {result2:?}"
    );
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
