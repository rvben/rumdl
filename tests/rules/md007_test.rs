use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD007ULIndent;

#[test]
fn test_valid_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n  * Item 2\n    * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Expected no warnings for valid indentation, but got {} warnings",
        result.len()
    );
}

#[test]
fn test_invalid_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n   * Item 2\n      * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    println!("test_invalid_list_indent: result.len() = {}", result.len());
    for (i, w) in result.iter().enumerate() {
        println!("  warning {}: line={}, column={}", i, w.line, w.column);
    }
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].column, 4);
    assert_eq!(result[1].line, 3);
    assert_eq!(result[1].column, 7);
}

#[test]
fn test_mixed_indentation() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n  * Item 2\n   * Item 3\n  * Item 4";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    println!("test_mixed_indentation: result.len() = {}", result.len());
    for (i, w) in result.iter().enumerate() {
        println!("  warning {}: line={}, column={}", i, w.line, w.column);
    }
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[0].column, 4);
}

#[test]
fn test_fix_indentation() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n   * Item 2\n      * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    let expected = "* Item 1\n  * Item 2\n    * Item 3";
    assert_eq!(result, expected);
}

#[test]
fn test_md007_in_yaml_code_block() {
    let rule = MD007ULIndent::default();
    let content = r#"```yaml
repos:
-   repo: https://github.com/rvben/rumdl
    rev: v0.5.0
    hooks:
    -   id: rumdl-check
```"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "MD007 should not trigger inside a code block, but got warnings: {:?}",
        result
    );
}

#[test]
fn test_blockquoted_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "> * Item 1\n>   * Item 2\n>     * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Expected no warnings for valid blockquoted list indentation, but got {:?}",
        result
    );
}

#[test]
fn test_blockquoted_list_invalid_indent() {
    let rule = MD007ULIndent::default();
    let content = "> * Item 1\n>    * Item 2\n>       * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Expected 2 warnings for invalid blockquoted list indentation, got {:?}", result);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[1].line, 3);
}

#[test]
fn test_nested_blockquote_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "> > * Item 1\n> >   * Item 2\n> >     * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Expected no warnings for valid nested blockquoted list indentation, but got {:?}",
        result
    );
}

#[test]
fn test_blockquote_list_with_code_block() {
    let rule = MD007ULIndent::default();
    let content = "> * Item 1\n>   * Item 2\n>   ```\n>   code\n>   ```\n>   * Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "MD007 should not trigger inside a code block within a blockquote, but got warnings: {:?}",
        result
    );
}
