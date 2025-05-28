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
    let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());

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
    let rule = MD055TablePipeStyle::new("no_leading_or_trailing".to_string());

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
    let rule = MD055TablePipeStyle::new("leading_only".to_string());

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
    let rule = MD055TablePipeStyle::new("trailing_only".to_string());

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
    let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());
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
    let rule = MD055TablePipeStyle::new("no_leading_or_trailing".to_string());
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
    let rule = MD055TablePipeStyle::new("leading_only".to_string());
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
    let rule = MD055TablePipeStyle::new("trailing_only".to_string());
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

#[test]
fn test_false_positives_not_flagged() {
    let rule = MD055TablePipeStyle::default();

    // Test 1: Regular text mentioning pipe characters should not be flagged
    let content = r#"
# MD055 - Table pipe style

## Description

This rule is triggered when table rows in a Markdown file have inconsistent pipe styles.

In Markdown tables, you can include or omit leading and trailing pipe characters (`|`). This rule enforces a consistent style for these pipes.
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Regular text mentioning pipe characters should not be flagged"
    );

    // Test 2: Bullet points with pipe characters should not be flagged
    let content = r#"
The `style` parameter can have the following values:

- `consistent` (default): All table rows should use the same style as the first table row
- `leading*and*trailing`: All table rows must have both leading and trailing pipes (`| cell | cell |`)
  - `leading*only`: All table rows must have leading pipes and no trailing pipes (`| cell | cell`)
  - `trailing*only`: All table rows must have trailing pipes and no leading pipes (`cell | cell |`)
  - `no*leading*or*trailing`: All table rows must have neither leading nor trailing pipes (`cell | cell`)
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Bullet points with pipe characters should not be flagged"
    );

    // Test 3: Inline code with pipe characters should not be flagged
    let content = r#"
You can use pipes in inline code like `| cell | cell |` without issues.

Also backticks with pipes: ``| some | code |`` should be ignored.
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Inline code with pipe characters should not be flagged"
    );

    // Test 4: Lines with pipes but no table structure should not be flagged
    let content = r#"
This is a line with | some | pipes | but no table structure.

Another line | with | pipes | that | doesn't | form | a | table.
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Lines with pipes but no table structure should not be flagged"
    );

    // Test 5: Single line with pipes (no delimiter row) should not be flagged
    let content = r#"
| This | looks | like | a | table | row |

But there's no delimiter row, so it's not a table.
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Single line with pipes (no delimiter row) should not be flagged"
    );
}

#[test]
fn test_actual_tables_are_still_flagged() {
    let rule = MD055TablePipeStyle::default();

    // Test that actual tables with inconsistent styles are still flagged
    let content = r#"
| Header 1 | Header 2 |
| -------- | -------- |
Cell 1   | Cell 2
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Actual tables with inconsistent styles should still be flagged"
    );
    assert_eq!(result[0].line, 4, "The inconsistent row should be flagged");
}

#[test]
fn test_table_detection_requires_delimiter_row() {
    let rule = MD055TablePipeStyle::default();

    // Test that a proper table structure is required (header + delimiter + rows)
    let content = r#"
| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1   | Cell 2   |
| Cell 3   | Cell 4   |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Properly formatted table should not be flagged"
    );

    // Test that without delimiter row, it's not considered a table
    let content = r#"
| Header 1 | Header 2 |
| Cell 1   | Cell 2   |
| Cell 3   | Cell 4   |
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Lines without delimiter row should not be considered a table"
    );
}

#[test]
fn test_mixed_content_with_tables() {
    let rule = MD055TablePipeStyle::default();

    // Test content that mixes regular text with pipes and actual tables
    let content = r#"
# Document with mixed content

This line mentions pipe characters (`|`) in regular text.

- Bullet point with pipes: `| cell | cell |`
- Another bullet: uses pipes (`|`) in description

Now here's an actual table:

| Header 1 | Header 2 |
| -------- | -------- |
Cell 1   | Cell 2

And more text with | pipes | that | aren't | tables.
    "#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Only the actual table row should be flagged"
    );
    assert_eq!(
        result[0].line, 13,
        "The inconsistent table row should be flagged"
    );
    assert!(
        result[0].message.contains("Table pipe style"),
        "Should be a table pipe style warning"
    );
}

#[test]
fn test_fix_does_not_corrupt_non_tables() {
    let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());

    // Test that fix doesn't corrupt non-table content
    let content = r#"
# MD055 - Table pipe style

In Markdown tables, you can include or omit leading and trailing pipe characters (`|`).

- `leading*and*trailing`: All table rows must have both leading and trailing pipes (`| cell | cell |`)
- `no*leading*or*trailing`: All table rows must have neither leading nor trailing pipes (`cell | cell`)

This line has | some | pipes | but | isn't | a | table.
    "#;

    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();

    // The content should be unchanged since there are no actual tables
    assert_eq!(
        content.trim(),
        fixed.trim(),
        "Non-table content should not be modified by fix"
    );

    // Verify no warnings are generated
    let fixed_ctx = LintContext::new(&fixed);
    let result = rule.check(&fixed_ctx).unwrap();
    assert_eq!(result.len(), 0, "Fixed content should have no warnings");
}
