use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD040FencedCodeLanguage;

#[test]
fn test_valid_code_blocks() {
    let rule = MD040FencedCodeLanguage;
    let content = "```rust\nfn main() {}\n```\n```python\nprint('hello')\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_missing_language() {
    let rule = MD040FencedCodeLanguage;
    let content = "```\nsome code\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "```text\nsome code\n```");
}

#[test]
fn test_multiple_code_blocks() {
    let rule = MD040FencedCodeLanguage;
    let content = "```rust\nfn main() {}\n```\n```\nsome code\n```\n```python\nprint('hello')\n```";
    let ctx = LintContext::new(content);
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
    let rule = MD040FencedCodeLanguage;
    let content = "```\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "```text\n```");
}

#[test]
fn test_indented_code_block() {
    let rule = MD040FencedCodeLanguage;
    let content = "  ```\n  some code\n  ```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "```text\n  some code\n```");
}

#[test]
fn test_mixed_code_blocks() {
    let rule = MD040FencedCodeLanguage;
    let content = "```rust\nfn main() {}\n```\nSome text\n```\nmore code\n```\n```js\nconsole.log('hi');\n```";
    let ctx = LintContext::new(content);
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
    let rule = MD040FencedCodeLanguage;
    let content = "```   \nsome code\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "```text\nsome code\n```");
}

#[test]
fn test_nested_code_blocks_no_false_positives() {
    let rule = MD040FencedCodeLanguage;
    // Test the case where we have a markdown code block containing python code
    // The inner ```python and ``` should NOT be treated as separate code blocks
    let content = "```markdown\n1. First item\n\n   ```python\n   code_in_list()\n   ```\n\n2. Second item\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Should find no issues - the closing ``` should not be flagged as missing language
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
    let rule = MD040FencedCodeLanguage;
    // Test that indented closing fences are not treated as new opening fences
    let content = "```markdown\nSome content\n   ```python\n   code()\n   ```\nMore content\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Should find no issues - the indented ``` should not close the outer block
    assert!(result.is_empty(), "Indented closing fences should not be flagged");

    // Test that fix doesn't add 'text' to closing fences
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, content,
        "Fix should not modify indented content inside code blocks"
    );
    assert!(
        !fixed.contains("```text"),
        "Fix should not add 'text' to closing fences"
    );
}
