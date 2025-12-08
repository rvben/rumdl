use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD031BlanksAroundFences;

#[test]
fn test_kramdown_block_attributes_auto_detected() {
    // Kramdown attributes should be auto-detected without configuration
    let rule = MD031BlanksAroundFences::default();
    let content = r#"# Title

```bash
echo hello
```
{:.wrap}

Some text"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should auto-detect Kramdown block attributes");
}

#[test]
fn test_non_kramdown_braces_still_flagged() {
    // Lines with braces that don't match Kramdown IAL syntax should still be flagged
    let rule = MD031BlanksAroundFences::default();
    let content = r#"# Title

```bash
echo hello
```
{not kramdown}

Some text"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should flag non-Kramdown brace lines");
    assert!(result[0].message.contains("No blank line after"));
}

#[test]
fn test_kramdown_css_class_variants() {
    // Test various Kramdown attribute syntax variations
    let rule = MD031BlanksAroundFences::default();

    // Class attribute
    let content1 = r#"```bash
echo hello
```
{:.wrap}"#;
    let ctx = LintContext::new(content1, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert!(rule.check(&ctx).unwrap().is_empty());

    // ID attribute
    let content2 = r#"```bash
echo hello
```
{:#my-code}"#;
    let ctx = LintContext::new(content2, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert!(rule.check(&ctx).unwrap().is_empty());

    // Multiple attributes
    let content3 = r#"```bash
echo hello
```
{:.wrap #my-code .highlight}"#;
    let ctx = LintContext::new(content3, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert!(rule.check(&ctx).unwrap().is_empty());
}

#[test]
fn test_kramdown_no_blank_after_attribute() {
    // Kramdown attributes don't need blank lines after them
    let rule = MD031BlanksAroundFences::default();
    let content = r#"```bash
echo hello
```
{:.wrap}
Some text immediately after"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // This should not flag since the Kramdown attribute is auto-detected
    assert!(result.is_empty(), "Should auto-detect Kramdown attributes");
}

#[test]
fn test_normal_code_blocks_still_checked() {
    // Normal code blocks without attributes should still be checked
    let rule = MD031BlanksAroundFences::default();
    let content = r#"# Title

```bash
echo hello
```
Some text immediately after"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should still flag missing blank line for normal code blocks"
    );
}

#[test]
fn test_fix_preserves_kramdown_attributes() {
    let rule = MD031BlanksAroundFences::default();
    let content = r#"Text before
```bash
echo hello
```
{:.wrap}
Text after"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Should add blank line before the code block but not after (due to attribute)
    let expected = r#"Text before

```bash
echo hello
```
{:.wrap}
Text after"#;
    assert_eq!(fixed, expected);
}
