use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD058BlanksAroundTables;

#[test]
fn test_name() {
    let rule = MD058BlanksAroundTables::default();
    assert_eq!(rule.name(), "MD058");
}

#[test]
fn test_proper_blank_lines() {
    let rule = MD058BlanksAroundTables::default();

    // Table with proper blank lines before and after
    let content = r#"
Some text before the table.

| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1.1 | Cell 1.2 |

Some text after the table.
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_missing_blank_line_before() {
    let rule = MD058BlanksAroundTables::default();

    let content = r#"
Some text before the table.
| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1.1 | Cell 1.2 |

Some text after the table.
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[0].message, "Missing blank line before table");
}

#[test]
fn test_missing_blank_line_after() {
    let rule = MD058BlanksAroundTables::default();

    let content = r#"
Some text before the table.

| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1.1 | Cell 1.2 |
Some text after the table.
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 6);
    assert_eq!(result[0].message, "Missing blank line after table");
}

#[test]
fn test_missing_blank_lines_both() {
    let rule = MD058BlanksAroundTables::default();

    let content = r#"
Some text before the table.
| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1.1 | Cell 1.2 |
Some text after the table.
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.iter().any(|w| w.message == "Missing blank line before table"));
    assert!(result.iter().any(|w| w.message == "Missing blank line after table"));
}

#[test]
fn test_multiple_tables() {
    let rule = MD058BlanksAroundTables::default();

    let content = r#"
Some text before tables.

| Table 1 Header 1 | Table 1 Header 2 |
| --------------- | --------------- |
| Table 1 Cell 1.1 | Table 1 Cell 1.2 |

Some text between tables.

| Table 2 Header 1 | Table 2 Header 2 |
| --------------- | --------------- |
| Table 2 Cell 1.1 | Table 2 Cell 1.2 |

Some text after tables.
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Missing blank lines for second table
    let content = r#"
Some text before tables.

| Table 1 Header 1 | Table 1 Header 2 |
| --------------- | --------------- |
| Table 1 Cell 1.1 | Table 1 Cell 1.2 |

Some text between tables.
| Table 2 Header 1 | Table 2 Header 2 |
| --------------- | --------------- |
| Table 2 Cell 1.1 | Table 2 Cell 1.2 |
Some text after tables.
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_code_blocks_ignored() {
    let rule = MD058BlanksAroundTables::default();

    let content = r#"
Some text.

```markdown
| This is a table in a code block |
| ------------------------------ |
| It should be ignored           |
```

| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1.1 | Cell 1.2 |

More text.
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_table_at_document_start() {
    let rule = MD058BlanksAroundTables::default();

    // Table at the start of the document doesn't need blank line before
    let content = r#"| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1.1 | Cell 1.2 |

Some text after the table.
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_table_at_document_end() {
    let rule = MD058BlanksAroundTables::default();

    // Table at the end of the document doesn't need blank line after
    let content = r#"
Some text before the table.

| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1.1 | Cell 1.2 |"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_fix_missing_blank_lines() {
    let rule = MD058BlanksAroundTables::default();

    let content = r#"Text before.
| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1   | Cell 2   |
Text after."#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("Text before.\n\n| Header"));
    assert!(fixed.contains("Cell 2   |\n\nText after"));
}
