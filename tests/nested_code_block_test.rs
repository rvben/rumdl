use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD031BlanksAroundFences, MD040FencedCodeLanguage};

#[test]
fn test_md031_should_not_modify_nested_code_blocks() {
    let rule = MD031BlanksAroundFences::default();

    // Test content with nested code blocks (common in documentation)
    let content = r#"# Documentation

## Example

````markdown
```
def hello():
    print("Hello, world!")
```
````

More text"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // MD031 should NOT flag the inner ``` as needing blank lines
    // because they're just text content inside the outer code block
    assert_eq!(warnings.len(), 0, "MD031 should not flag nested code blocks");
}

#[test]
fn test_md031_should_handle_deeply_nested_code_blocks() {
    let rule = MD031BlanksAroundFences::default();

    // Test with 5-backtick outer block containing 4-backtick and 3-backtick blocks
    let content = r#"`````markdown
````python
```bash
echo "hello"
```
````
`````"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should not flag any of the inner fence markers
    assert_eq!(warnings.len(), 0, "MD031 should not flag any nested fence markers");
}

#[test]
fn test_md040_respects_disable_comments_in_nested_blocks() {
    let rule = MD040FencedCodeLanguage;

    // Test that disable comments work correctly
    let content = r#"# Test

<!-- rumdl-disable MD040 -->

````markdown
```
This has no language but should not be flagged
```
````

<!-- rumdl-enable MD040 -->

```
This should be flagged
```"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the last code block (line 13)
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 13);
}

#[test]
fn test_md031_fix_should_not_corrupt_nested_blocks() {
    let rule = MD031BlanksAroundFences::default();

    // Content where MD031 might try to add blank lines inside nested blocks
    let content = r#"````markdown
```
content
```
````"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // The fix should NOT add blank lines inside the nested code block
    // The content inside the outer block should remain unchanged
    assert_eq!(fixed, content, "MD031 fix should not modify content inside code blocks");
}

#[test]
fn test_documentation_example_preservation() {
    let md031 = MD031BlanksAroundFences::default();
    let md040 = MD040FencedCodeLanguage;

    // Real documentation example that was getting corrupted
    let content = r#"### ✅ Correct

````markdown
```python
def hello():
    print("Hello, world!")
```

```javascript
console.log("Hello, world!");
```
````

### ❌ Incorrect

<!-- rumdl-disable MD040 -->

````markdown
```
def hello():
    print("Hello, world!")
```
````

<!-- rumdl-enable MD040 -->"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD031 should not flag anything inside the code blocks
    let md031_warnings = md031.check(&ctx).unwrap();
    assert_eq!(
        md031_warnings.len(),
        0,
        "MD031 should not flag content inside ````markdown blocks"
    );

    // MD040 should not flag the code block in the "incorrect" section due to disable comment
    let md040_warnings = md040.check(&ctx).unwrap();
    assert_eq!(md040_warnings.len(), 0, "MD040 should respect disable comments");

    // Fixes should not modify the content
    let md031_fixed = md031.fix(&ctx).unwrap();
    assert_eq!(md031_fixed, content, "MD031 should not modify documentation examples");
}
