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
    assert_eq!(fixed, "![Image description](image.png)");
}

#[test]
fn test_empty_alt_text() {
    let rule = MD045NoAltText::new();
    let content = "![ ](image.png)";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "![Image description](image.png)");
}

#[test]
fn test_multiple_images() {
    let rule = MD045NoAltText::new();
    let content = "![Alt text](image1.png)\n![](image2.png)\n![ ](image3.png)";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "![Alt text](image1.png)\n![Image description](image2.png)\n![Image description](image3.png)");
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
        "![Image description](https://example.com/image.png?param=value#fragment)"
    );
}

#[test]
fn test_mixed_content() {
    let rule = MD045NoAltText::new();
    let content =
        "# Images\n\nSome text here\n\n![Alt text](image1.png)\n\nMore text\n\n![](image2.png)";
    let ctx = rumdl::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Images\n\nSome text here\n\n![Alt text](image1.png)\n\nMore text\n\n![Image description](image2.png)");
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
        "Text with ![Alt text](inline1.png) and ![Image description](inline2.png) images."
    );
}
