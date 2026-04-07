use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD040FencedCodeLanguage;
use rumdl_lib::rules::md040_fenced_code_language::md040_config::{LanguageStyle, MD040Config};
use std::collections::HashMap;

/// Roundtrip helper: fix then re-check must yield 0 violations
fn assert_roundtrip(content: &str, config: MD040Config) {
    let rule = MD040FencedCodeLanguage::with_config(config.clone());
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let ctx2 = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule2 = MD040FencedCodeLanguage::with_config(config);
    let warnings = rule2.check(&ctx2).unwrap();
    assert!(
        warnings.is_empty(),
        "Roundtrip failed: fix then re-check produced {} warnings on:\n{fixed}",
        warnings.len()
    );
}

#[test]
fn test_roundtrip_missing_language() {
    assert_roundtrip("```\nsome code\n```\n", MD040Config::default());
}

#[test]
fn test_roundtrip_multiple_missing_languages() {
    assert_roundtrip(
        "```\nfirst\n```\n\n```python\nsecond\n```\n\n```\nthird\n```\n",
        MD040Config::default(),
    );
}

#[test]
fn test_roundtrip_consistent_mode() {
    assert_roundtrip(
        "```bash\necho hi\n```\n\n```sh\necho there\n```\n\n```bash\necho again\n```\n",
        MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        },
    );
}

#[test]
fn test_roundtrip_consistent_mode_preferred_alias() {
    let mut preferred = HashMap::new();
    preferred.insert("Shell".to_string(), "sh".to_string());
    assert_roundtrip(
        "```bash\necho hi\n```\n\n```sh\necho there\n```\n",
        MD040Config {
            style: LanguageStyle::Consistent,
            preferred_aliases: preferred,
            ..Default::default()
        },
    );
}

#[test]
fn test_roundtrip_indented_missing_language() {
    assert_roundtrip("- item\n  ```\n  code\n  ```\n", MD040Config::default());
}

#[test]
fn test_roundtrip_tilde_fences() {
    assert_roundtrip("~~~\ncode\n~~~\n", MD040Config::default());
}

#[test]
fn test_roundtrip_tilde_consistent() {
    assert_roundtrip(
        "~~~bash\necho hi\n~~~\n\n~~~sh\necho there\n~~~\n",
        MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        },
    );
}

#[test]
fn test_valid_code_blocks() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "```rust\nfn main() {}\n```\n```python\nprint('hello')\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_missing_language() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "```\nsome code\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "```text\nsome code\n```");
}

#[test]
fn test_multiple_code_blocks() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "```rust\nfn main() {}\n```\n```\nsome code\n```\n```python\nprint('hello')\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "```rust\nfn main() {}\n```\n```text\nsome code\n```\n```python\nprint('hello')\n```"
    );
}

#[test]
fn test_empty_code_block() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "```\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "```text\n```");
}

#[test]
fn test_indented_code_block() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "  ```\n  some code\n  ```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "  ```text\n  some code\n  ```");
}

#[test]
fn test_mixed_code_blocks() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "```rust\nfn main() {}\n```\nSome text\n```\nmore code\n```\n```js\nconsole.log('hi');\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "```rust\nfn main() {}\n```\nSome text\n```text\nmore code\n```\n```js\nconsole.log('hi');\n```"
    );
}

#[test]
fn test_preserve_whitespace() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "```   \nsome code\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "```text\nsome code\n```");
}

#[test]
fn test_nested_code_blocks_no_false_positives() {
    let rule = MD040FencedCodeLanguage::default();
    // Test the case where we have a markdown code block containing python code
    // The inner ```python and ``` should NOT be treated as separate code blocks
    // NOTE: Using 4-space indent so inner fences don't close the outer block (per CommonMark)
    let content = "```markdown\n1. First item\n\n    ```python\n    code_in_list()\n    ```\n\n2. Second item\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Should find no issues - the 4-space indented inner fences are content, not code blocks
    assert!(
        result.is_empty(),
        "Nested code blocks should not generate false positives"
    );

    // Test that fix doesn't modify the content incorrectly
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "Fix should not modify correctly nested code blocks");
}

#[test]
fn test_indented_closing_fence_not_flagged() {
    let rule = MD040FencedCodeLanguage::default();
    // Test that 4-space indented fences are not treated as separate code blocks
    // NOTE: Per CommonMark, 0-3 space indent DOES close the outer block
    // Using 4-space indent to ensure content is treated as part of outer block
    let content = "```markdown\nSome content\n    ```python\n    code()\n    ```\nMore content\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Should find no issues - 4-space indented fences are content, not code blocks
    assert!(
        result.is_empty(),
        "4-space indented fences should be content, not separate blocks"
    );

    // Test that fix doesn't add 'text' to content inside code blocks
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, content,
        "Fix should not modify indented content inside code blocks"
    );
    assert!(
        !fixed.contains("```text"),
        "Fix should not add 'text' to content inside code blocks"
    );
}
