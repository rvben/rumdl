use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD001HeadingIncrement;

#[test]
pub fn test_md001_unicode_valid() {
    let rule = MD001HeadingIncrement;
    let content = "# Heading with café\n## Heading with 汉字\n### Heading with emoji 🔥\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Valid Unicode headings with proper increment should not trigger warnings"
    );
}

#[test]
pub fn test_md001_unicode_invalid() {
    let rule = MD001HeadingIncrement;
    let content = "# Heading with café\n### Heading with 汉字\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Skipped heading level with Unicode should trigger warning"
    );
    assert_eq!(result[0].line, 2);
    assert_eq!(
        result[0].message,
        "Heading level should be 2 for this level"
    );
}

#[test]
pub fn test_md001_unicode_fix() {
    let rule = MD001HeadingIncrement;
    let content = "# Café heading\n### 汉字 heading\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result, "# Café heading\n## 汉字 heading\n",
        "Fix should properly handle Unicode characters"
    );
}

#[test]
pub fn test_md001_unicode_multiple_violations() {
    let rule = MD001HeadingIncrement;
    let content = "# café\n### 汉字\n##### 🔥\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        2,
        "Multiple violations with Unicode should be detected"
    );
    assert_eq!(result[0].line, 2);
    assert_eq!(result[1].line, 3);
}

#[test]
pub fn test_md001_unicode_atx_and_setext() {
    let rule = MD001HeadingIncrement;
    let content = "# Heading café\nHeading 汉字\n---------\n### Heading 🔥\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Valid Unicode headings with mixed styles should not trigger warnings"
    );
}

#[test]
pub fn test_md001_unicode_complex() {
    let rule = MD001HeadingIncrement;
    let content = "# 汉字 café 🔥\n## مرحبا こんにちは\n### Mixed Unicode: ñáéíóú привет שלום\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Valid Unicode headings with complex characters should not trigger warnings"
    );

    let invalid_content = "# 汉字 café 🔥\n### مرحبا こんにちは\n";
    let ctx = LintContext::new(invalid_content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Skipped heading level with complex Unicode should trigger warning"
    );

    let ctx = LintContext::new(invalid_content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, "# 汉字 café 🔥\n## مرحبا こんにちは\n",
        "Fix should properly handle complex Unicode characters"
    );
}
