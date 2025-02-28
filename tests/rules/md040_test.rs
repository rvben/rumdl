use rumdl::rules::MD040FencedCodeLanguage;
use rumdl::rule::Rule;

#[test]
fn test_valid_code_blocks() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "```rust\nfn main() {}\n```\n```python\nprint('hello')\n```";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_missing_language() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "```\nsome code\n```";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "```text\nsome code\n```");
}

#[test]
fn test_multiple_code_blocks() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "```rust\nfn main() {}\n```\n```\nsome code\n```\n```python\nprint('hello')\n```";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "```rust\nfn main() {}\n```\n```text\nsome code\n```\n```python\nprint('hello')\n```");
}

#[test]
fn test_empty_code_block() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "```\n```";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "```text\n```");
}

#[test]
fn test_indented_code_block() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "  ```\n  some code\n  ```";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "```text\n  some code\n```");
}

#[test]
fn test_mixed_code_blocks() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "```rust\nfn main() {}\n```\nSome text\n```\nmore code\n```\n```js\nconsole.log('hi');\n```";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "```rust\nfn main() {}\n```\nSome text\n```text\nmore code\n```\n```js\nconsole.log('hi');\n```");
}

#[test]
fn test_preserve_whitespace() {
    let rule = MD040FencedCodeLanguage::default();
    let content = "```   \nsome code\n```";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "```text\nsome code\n```");
} 