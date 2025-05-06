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
