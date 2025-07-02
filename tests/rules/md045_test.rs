use rumdl::rule::Rule;
use rumdl::rules::MD045NoAltText;

#[test]
fn test_valid_alt_text() {
    let rule = MD045NoAltText::new();
    let content = "![Alt text](image.png)\n![Another description](path/to/image.jpg)";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_missing_alt_text() {
    let rule = MD045NoAltText::new();
    let content = "![](image.png)";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "![TODO: Add image description](image.png)");
}

#[test]
fn test_empty_alt_text() {
    let rule = MD045NoAltText::new();
    let content = "![ ](image.png)";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "![TODO: Add image description](image.png)");
}

#[test]
fn test_multiple_images() {
    let rule = MD045NoAltText::new();
    let content = "![Alt text](image1.png)\n![](image2.png)\n![ ](image3.png)";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "![Alt text](image1.png)\n![TODO: Add image description](image2.png)\n![TODO: Add image description](image3.png)"
    );
}

#[test]
fn test_complex_urls() {
    let rule = MD045NoAltText::new();
    let content = "![](https://example.com/image.png?param=value#fragment)";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "![TODO: Add image description](https://example.com/image.png?param=value#fragment)"
    );
}

#[test]
fn test_mixed_content() {
    let rule = MD045NoAltText::new();
    let content = "# Images\n\nSome text here\n\n![Alt text](image1.png)\n\nMore text\n\n![](image2.png)";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Images\n\nSome text here\n\n![Alt text](image1.png)\n\nMore text\n\n![TODO: Add image description](image2.png)"
    );
}

#[test]
fn test_inline_images() {
    let rule = MD045NoAltText::new();
    let content = "Text with ![Alt text](inline1.png) and ![](inline2.png) images.";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "Text with ![Alt text](inline1.png) and ![TODO: Add image description](inline2.png) images."
    );
}

#[test]
fn test_placeholder_clarity() {
    let rule = MD045NoAltText::new();
    let content = "![](screenshot.png)\n![  ](diagram.svg)";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Should detect both images with missing alt text");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("TODO: Add image description"),
        "Fixed content should include TODO reminder"
    );
    assert_eq!(
        fixed,
        "![TODO: Add image description](screenshot.png)\n![TODO: Add image description](diagram.svg)"
    );
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

    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should only detect the two actual images outside code blocks
    assert_eq!(result.len(), 2, "Should only detect images outside code blocks");
    assert_eq!(result[0].line, 4); // actual-image.png
    assert_eq!(result[1].line, 15); // another-image.png

    // Test the fix
    let fixed = rule.fix(&ctx).unwrap();

    // Should fix only the images outside code blocks
    assert!(fixed.contains("![TODO: Add image description](actual-image.png)"));
    assert!(fixed.contains("![TODO: Add image description](another-image.png)"));

    // Should NOT fix images inside code blocks
    assert!(fixed.contains("```markdown\n![](example1.png)"));
    assert!(fixed.contains("![ ](example2.png)"));
    assert!(fixed.contains("`![](inline.png)`"));
}
