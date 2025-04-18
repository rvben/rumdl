use rumdl::rule::Rule;
use rumdl::rules::MD007ULIndent;

#[test]
fn test_valid_list_indent() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n  * Item 2\n    * Item 3";
    let result = rule.check(content).unwrap();
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
    let result = rule.check(content).unwrap();
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
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[0].column, 4);
}

#[test]
fn test_fix_indentation() {
    let rule = MD007ULIndent::default();
    let content = "* Item 1\n   * Item 2\n      * Item 3";
    let result = rule.fix(content).unwrap();
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
    let result = rule.check(content).unwrap();
    assert!(
        result.is_empty(),
        "MD007 should not trigger inside a code block, but got warnings: {:?}",
        result
    );
}
