use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD055TablePipeStyle;

#[test]
fn test_name() {
    let rule = MD055TablePipeStyle::default();
    assert_eq!(rule.name(), "MD055");
}

#[test]
fn test_consistent_pipe_styles() {
    let rule = MD055TablePipeStyle::default();

    // Leading and trailing pipes consistently
    let content = r#"
| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1   | Cell 2   |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // No leading or trailing pipes consistently
    let content = r#"
Header 1 | Header 2
-------- | --------
Cell 1   | Cell 2
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_inconsistent_pipe_styles() {
    let rule = MD055TablePipeStyle::default();

    let content = r#"
| Header 1 | Header 2 |
| -------- | -------- |
Cell 1   | Cell 2
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 4);
    assert!(result[0].message.contains("Table pipe style"));
}

#[test]
fn test_leading_and_trailing_style() {
    let rule = MD055TablePipeStyle::new("leading_and_trailing");

    // Consistent with leading_and_trailing style
    let content = r#"
| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1   | Cell 2   |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Inconsistent with leading_and_trailing style
    let content = r#"
Header 1 | Header 2
-------- | --------
Cell 1   | Cell 2
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3); // Three rows, all need fixes
}

#[test]
fn test_no_leading_or_trailing_style() {
    let rule = MD055TablePipeStyle::new("no_leading_or_trailing");

    // Consistent with no_leading_or_trailing style
    let content = r#"
Header 1 | Header 2
-------- | --------
Cell 1   | Cell 2
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Inconsistent with no_leading_or_trailing style
    let content = r#"
| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1   | Cell 2   |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3); // Three rows, all need fixes
}

#[test]
fn test_leading_only_style() {
    let rule = MD055TablePipeStyle::new("leading_only");

    // Consistent with leading_only style
    let content = r#"
| Header 1 | Header 2
| -------- | --------
| Cell 1   | Cell 2
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Content with leading pipes only should not trigger warnings with leading_only style"
    );

    // Inconsistent with leading_only style (has trailing pipes)
    let content = r#"
| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1   | Cell 2   |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        3,
        "Content with both leading and trailing pipes should be flagged when style is leading_only"
    );

    // Fix should correctly convert to leading_only style
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed);
    let result = rule.check(&fixed_ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_trailing_only_style() {
    let rule = MD055TablePipeStyle::new("trailing_only");

    // Consistent with trailing_only style
    let content = r#"
Header 1 | Header 2 |
-------- | -------- |
Cell 1   | Cell 2   |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Content with trailing pipes only should not trigger warnings with trailing_only style"
    );

    // Inconsistent with trailing_only style (has leading pipes)
    let content = r#"
| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1   | Cell 2   |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3, "Content with both leading and trailing pipes should be flagged when style is trailing_only");

    // Fix should correctly convert to trailing_only style
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed);
    let result = rule.check(&fixed_ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_code_blocks_ignored() {
    let rule = MD055TablePipeStyle::default();

    let content = r#"
| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1   | Cell 2   |

```markdown
| This is a table in a code block |
Header with inconsistent style | that should be ignored
```
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_fix() {
    // Test fix for leading_and_trailing style
    let rule = MD055TablePipeStyle::new("leading_and_trailing");
    let content = r#"
Header 1 | Header 2
-------- | --------
Cell 1   | Cell 2
    "#;
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed);
    let result = rule.check(&fixed_ctx).unwrap();
    assert_eq!(result.len(), 0, "Fixed content should have no warnings");

    // Test fix for no_leading_or_trailing style
    let rule = MD055TablePipeStyle::new("no_leading_or_trailing");
    let content = r#"
| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1   | Cell 2   |
    "#;
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed);
    let result = rule.check(&fixed_ctx).unwrap();
    assert_eq!(result.len(), 0, "Fixed content should have no warnings");

    // Test fix for leading_only style
    let rule = MD055TablePipeStyle::new("leading_only");
    let content = r#"
Header 1 | Header 2 |
-------- | -------- |
Cell 1   | Cell 2   |
    "#;
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed);
    let result = rule.check(&fixed_ctx).unwrap();
    assert_eq!(result.len(), 0, "Fixed content should have no warnings");

    // Test fix for trailing_only style
    let rule = MD055TablePipeStyle::new("trailing_only");
    let content = r#"
| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1   | Cell 2   |
    "#;
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed);
    let result = rule.check(&fixed_ctx).unwrap();
    assert_eq!(result.len(), 0, "Fixed content should have no warnings");
}
