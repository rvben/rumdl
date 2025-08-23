use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD056TableColumnCount;

#[test]
fn test_name() {
    let rule = MD056TableColumnCount;
    assert_eq!(rule.name(), "MD056");
}

#[test]
fn test_consistent_column_count() {
    let rule = MD056TableColumnCount;

    // Regular table with consistent column count
    let content = r#"
| Header 1 | Header 2 | Header 3 |
| -------- | -------- | -------- |
| Cell 1.1 | Cell 1.2 | Cell 1.3 |
| Cell 2.1 | Cell 2.2 | Cell 2.3 |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Table without leading/trailing pipes but consistent columns
    let content = r#"
Header 1 | Header 2 | Header 3
-------- | -------- | --------
Cell 1.1 | Cell 1.2 | Cell 1.3
Cell 2.1 | Cell 2.2 | Cell 2.3
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_inconsistent_column_count() {
    let rule = MD056TableColumnCount;

    let content = r#"
| Header 1 | Header 2 | Header 3 |
| -------- | -------- | -------- |
| Cell 1.1 | Cell 1.2 |
| Cell 2.1 | Cell 2.2 | Cell 2.3 | Extra |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 4); // 2 columns instead of 3
    assert_eq!(result[1].line, 5); // 4 columns instead of 3
    assert!(result[0].message.contains("2 cells, but expected 3"));
    assert!(result[1].message.contains("4 cells, but expected 3"));
}

#[test]
fn test_complex_tables() {
    let rule = MD056TableColumnCount;

    // Table with empty cells
    let content = r#"
| Header 1 | Header 2 | Header 3 |
| -------- | -------- | -------- |
| Cell 1.1 |          | Cell 1.3 |
|          | Cell 2.2 |          |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Table with alignment specifiers
    let content = r#"
| Left | Center | Right |
|:-----|:------:|------:|
| 1    | 2      | 3     |
| 4    | 5      | 6     |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_code_blocks_ignored() {
    let rule = MD056TableColumnCount;

    let content = r#"
| Header 1 | Header 2 | Header 3 |
| -------- | -------- | -------- |
| Cell 1.1 | Cell 1.2 | Cell 1.3 |

```markdown
| Bad table | with | inconsistent | columns |
| --- | --- |
| Too few | columns |
```
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_fix_too_few_columns() {
    let rule = MD056TableColumnCount;

    let content = r#"
| Header 1 | Header 2 | Header 3 |
| -------- | -------- | -------- |
| Cell 1.1 | Cell 1.2 |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert!(result.contains("| Cell 1.1 | Cell 1.2 |  |"));
}

#[test]
fn test_fix_too_many_columns() {
    let rule = MD056TableColumnCount;

    let content = r#"
| Header 1 | Header 2 | Header 3 |
| -------- | -------- | -------- |
| Cell 1.1 | Cell 1.2 | Cell 1.3 | Extra |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert!(result.contains("| Cell 1.1 | Cell 1.2 | Cell 1.3 |"));
    assert!(!result.contains("Extra"));
}

#[test]
fn test_table_row_detection() {
    let rule = MD056TableColumnCount;

    // Make sure non-table lines don't trigger warnings
    let content = r#"
This is a paragraph that happens to have | pipe characters
but isn't actually a table row.

| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1.1 | Cell 1.2 |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}
