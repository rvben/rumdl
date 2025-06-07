use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::code_block_utils::CodeBlockStyle;
use rumdl::MD046CodeBlockStyle;

#[test]
fn test_consistent_fenced_blocks() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "# Code blocks\n\n```\ncode here\n```\n\n```rust\nmore code\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_indented_blocks() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
    let content =
        "# Code blocks\n\n    code here\n    more code\n\n    another block\n    continues";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_blocks_prefer_fenced() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "# Mixed blocks\n\n```\nfenced block\n```\n\n    indented block";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("# Mixed blocks"), "Should preserve headings");
    assert!(
        fixed.contains("```\nfenced block\n```"),
        "Should preserve fenced blocks"
    );
    assert!(
        fixed.contains("```\nindented block"),
        "Should convert indented blocks to fenced"
    );
}

#[test]
fn test_mixed_blocks_prefer_indented() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
    let content = "# Mixed blocks\n\n```\nfenced block\n```\n\n    indented block";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        3,
        "Should detect all parts of the inconsistent fenced code block"
    );
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("# Mixed blocks"), "Should preserve headings");
    assert!(
        fixed.contains("    fenced block") && !fixed.contains("```\nfenced block\n```"),
        "Should convert fenced blocks to indented"
    );
    assert!(
        fixed.contains("    indented block"),
        "Should preserve indented blocks"
    );
}

#[test]
fn test_consistent_style_fenced_first() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "# Mixed blocks\n\n```\nfenced block\n```\n\n    indented block";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty(), "Should detect inconsistent code blocks");
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("# Mixed blocks"), "Should preserve headings");
    assert!(
        fixed.contains("```\nfenced block\n```"),
        "Should preserve fenced blocks"
    );
    assert!(
        fixed.contains("```\nindented block"),
        "Should convert indented blocks to fenced"
    );
}

#[test]
fn test_consistent_style_indented_first() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "# Mixed blocks\n\n    indented block\n\n```\nfenced block\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty(), "Should detect inconsistent code blocks");
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("# Mixed blocks"), "Should preserve headings");
    assert!(
        fixed.contains("    indented block"),
        "Should preserve indented blocks"
    );
    assert!(
        fixed.contains("    fenced block") && !fixed.contains("```\nfenced block\n```"),
        "Should convert fenced blocks to indented"
    );
}

#[test]
fn test_empty_content() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_code_blocks() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fenced_with_language() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "```rust\nlet x = 42;\n```\n\n```python\nprint('hello')\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_convert_indented_preserves_content() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "    let x = 42;\n    println!(\"{}\", x);";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty(), "Should detect indented code blocks");
    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("```\nlet x = 42;\nprintln!"),
        "Should convert indented to fenced and preserve content"
    );
}

#[test]
fn test_markdown_example_blocks() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    // This is a common pattern in markdown documentation showing markdown examples
    let content = r#"# Documentation

## Example Usage

```markdown
Here's how to use code blocks:

```python
def hello():
    print("Hello, world!")
```

And here's another example:

```javascript
console.log("Hello");
```
```

The above shows proper markdown syntax.
"#;
    
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    
    // Should not detect any issues with markdown example blocks
    assert!(result.is_empty(), "Should not flag code blocks inside markdown examples as unclosed");
    
    // Test that fix doesn't break markdown examples
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(content, fixed, "Should not modify markdown example blocks");
}

#[test]
fn test_unclosed_code_block() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "# Test\n\n```python\ndef hello():\n    print('world')\n\nThis content is inside an unclosed block.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should detect exactly one unclosed code block");
    assert!(result[0].message.contains("never closed"), "Should mention that the block is unclosed");
    assert_eq!(result[0].line, 3, "Should point to the opening fence line");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("```python"), "Should preserve the opening fence");
    assert!(fixed.ends_with("```"), "Should add closing fence at the end");
}

#[test]
fn test_unclosed_tilde_code_block() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "# Test\n\n~~~javascript\nfunction test() {\n  return 42;\n}\n\nMore content here.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should detect exactly one unclosed code block");
    assert!(result[0].message.contains("never closed"), "Should mention that the block is unclosed");
    assert!(result[0].message.contains("~~~"), "Should mention the tilde fence marker");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("~~~javascript"), "Should preserve the opening fence");
    assert!(fixed.ends_with("~~~"), "Should add closing tilde fence at the end");
}

#[test]
fn test_nested_code_block_opening() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "# Test\n\n```bash\n\n```markdown\n\n# Hello world\n\n```\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();


    assert_eq!(result.len(), 1, "Should detect exactly one nested code block issue");
    assert_eq!(result[0].line, 3, "Should flag the opening line (```bash) not the nested line");
    assert!(result[0].message.contains("Code block '```' should be closed before starting new one at line 5"),
           "Should explain the problem clearly");

    // Test fix - should add closing fence before the nested opening
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("```bash\n\n```\n\n```markdown"), "Should close bash block before markdown block");
}

#[test]
fn test_nested_code_block_different_languages() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "```python\n\n```javascript\ncode here\n```\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1, "Should detect nested opening");
    assert_eq!(result[0].line, 1, "Should flag the opening python block");
    assert!(result[0].message.contains("Code block '```' should be closed before starting new one at line 3"));
}

#[test]
fn test_nested_markdown_blocks_allowed() {
    // This tests that we're detecting ALL nested openings, including markdown
    // The user's requirement was that nested openings should be flagged "unless the code block is 'markdown' type"
    // But based on our conversation, we're now flagging ALL nested openings
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "```bash\n\n```markdown\n# Example\n```\n```\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // This test case has both nested opening AND unclosed blocks, so we get multiple warnings
    assert!(result.len() >= 1, "Should detect at least the nested markdown opening");
    // Find the nested opening warning (should be on line 1)
    let nested_warning = result.iter().find(|w| w.line == 1);
    assert!(nested_warning.is_some(), "Should flag the opening bash block");
}
