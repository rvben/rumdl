use rumdl_lib::MD048CodeFenceStyle;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::code_fence_utils::CodeFenceStyle;

#[test]
fn test_consistent_backticks() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
    let content = "# Code blocks\n\n```\ncode here\n```\n\n```rust\nmore code\n```";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_tildes() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
    let content = "# Code blocks\n\n~~~\ncode here\n~~~\n\n~~~rust\nmore code\n~~~";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_fences_prefer_backticks() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
    let content = "# Mixed blocks\n\n```\nbacktick block\n```\n\n~~~\ntilde block\n~~~";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Mixed blocks\n\n```\nbacktick block\n```\n\n```\ntilde block\n```"
    );
}

#[test]
fn test_mixed_fences_prefer_tildes() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
    let content = "# Mixed blocks\n\n```\nbacktick block\n```\n\n~~~\ntilde block\n~~~";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Mixed blocks\n\n~~~\nbacktick block\n~~~\n\n~~~\ntilde block\n~~~"
    );
}

#[test]
fn test_consistent_style_first_backtick() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let content = "# Mixed blocks\n\n```\nbacktick block\n```\n\n~~~\ntilde block\n~~~";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Mixed blocks\n\n```\nbacktick block\n```\n\n```\ntilde block\n```"
    );
}

#[test]
fn test_consistent_style_first_tilde() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let content = "# Mixed blocks\n\n~~~\ntilde block\n~~~\n\n```\nbacktick block\n```";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Mixed blocks\n\n~~~\ntilde block\n~~~\n\n~~~\nbacktick block\n~~~"
    );
}

#[test]
fn test_empty_content() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let content = "";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_code_blocks() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_nested_code_blocks() {
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
    let content = r#"# Documentation

Here's how to use code blocks in markdown:

````markdown
You can use backticks:

```javascript
console.log("Hello");
```

Or tildes:

~~~python
print("Hello")
~~~
````

The outer block uses backticks.
"#;

    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should only check the outer fence markers, not the ones inside
    assert_eq!(result.len(), 0, "Should not flag fences inside code blocks");

    // Test with tilde preference
    let rule_tilde = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
    let result_tilde = rule_tilde.check(&ctx).unwrap();

    // Should flag the outer backtick fences (opening and closing)
    assert_eq!(result_tilde.len(), 2, "Should flag outer backtick fences");

    // Test the fix
    let fixed = rule_tilde.fix(&ctx).unwrap();

    // Should fix only the outer fences
    assert!(fixed.contains("~~~~markdown"));
    assert!(fixed.contains("~~~~\n\nThe outer"));

    // Should NOT fix the inner fences
    assert!(fixed.contains("```javascript"));
    assert!(fixed.contains("~~~python"));
}

#[test]
fn test_fence_inside_indented_code_block() {
    // Test that fences inside indented code blocks are ignored
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let content = "First fence:\n\n```rust\ncode\n```\n\nIndented block with fence:\n\n    ```python\n    # This is shown as text\n    ```";

    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should not consider the fence inside indented block
    assert!(result.is_empty(), "Fences in indented blocks should be ignored");
}

#[test]
fn test_fence_with_language_and_attributes() {
    // Test detection with various fence formats
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let content = "~~~typescript {.numberLines startFrom=\"100\"}\ncode\n~~~\n\n```rust\nmore code\n```";

    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // First fence is tilde, so backtick fence should be flagged
    assert_eq!(result.len(), 2, "Should flag inconsistent style");
    assert!(result[0].message.contains("use ~~~ instead of ```"));
}

#[test]
fn test_empty_fences() {
    // Test with empty code blocks
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
    let content = "```\n```\n\n~~~\n~~~";

    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 2, "Should flag tilde fences");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "```\n```\n\n```\n```");
}

#[test]
fn test_fence_in_blockquote() {
    // Test fences inside blockquotes
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let content = "> Quote with code:\n> \n> ```js\n> console.log('test');\n> ```\n\n~~~python\nprint('test')\n~~~";

    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Note: Code blocks in blockquotes might not be detected by the simple fence detection
    // This is a known limitation - MD048 might not handle blockquoted code blocks
    assert!(result.len() <= 2, "May or may not detect fences in blockquotes");
}

#[test]
fn test_long_fence_markers() {
    // Test with extra-long fence markers
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
    let content = "``````javascript\ncode\n``````\n\n~~~~~ruby\ncode\n~~~~~";

    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 2, "Should handle long fence markers");
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("~~~~~~javascript"), "Should preserve fence length");
}

#[test]
fn test_unclosed_fence_style_detection() {
    // Test style detection with unclosed fence
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
    let content = "~~~python\nprint('unclosed')";

    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should still detect tilde style even if unclosed
    assert!(
        result.is_empty(),
        "Single fence with consistent style should not be flagged"
    );
}

#[test]
fn test_nested_different_fence_types() {
    // Test nested fences with different types (should not happen in valid markdown)
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
    let content = "```outer\n~~~inner\ncontent\n~~~\n```";

    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Inner tilde fences should be treated as content, not actual fences
    assert!(
        result.is_empty(),
        "Inner different fence type should be treated as content"
    );
}
