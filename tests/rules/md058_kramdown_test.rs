use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD058BlanksAroundTables;

#[test]
fn test_kramdown_block_attributes_auto_detected() {
    // Kramdown attributes should be auto-detected without configuration
    let rule = MD058BlanksAroundTables::default();
    let content = r#"# Title

| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |
{:.striped}

Some text"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should auto-detect Kramdown block attributes");
}

#[test]
fn test_non_kramdown_braces_are_a_table_row() {
    // A brace line that is not a Kramdown IAL is text, and GFM reads text
    // directly under the rows as one more row, so the blank line after it
    // ends the table in time.
    let rule = MD058BlanksAroundTables::default();
    let content = r#"# Title

| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |
{not kramdown}

Some text"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "the brace line is a table row: {result:?}");
}

#[test]
fn test_kramdown_table_css_class_variants() {
    // Test various Kramdown attribute syntax variations
    let rule = MD058BlanksAroundTables::default();

    // Class attribute
    let content1 = r#"Text

| Col1 | Col2 |
|------|------|
| A    | B    |
{:.striped}

Text"#;
    let ctx = LintContext::new(content1, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert!(rule.check(&ctx).unwrap().is_empty());

    // ID attribute
    let content2 = r#"Text

| Col1 | Col2 |
|------|------|
| A    | B    |
{:#my-table}

Text"#;
    let ctx = LintContext::new(content2, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert!(rule.check(&ctx).unwrap().is_empty());

    // Multiple attributes
    let content3 = r#"Text

| Col1 | Col2 |
|------|------|
| A    | B    |
{:.striped #my-table .responsive}

Text"#;
    let ctx = LintContext::new(content3, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert!(rule.check(&ctx).unwrap().is_empty());
}

#[test]
fn test_normal_tables_still_checked() {
    // Normal tables without attributes should still be checked
    let rule = MD058BlanksAroundTables::default();
    let content = r#"# Title

| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |
## Heading immediately after"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should still flag missing blank line for normal tables"
    );
}

#[test]
fn test_table_before_still_checked() {
    // Should still check for blank lines before table
    let rule = MD058BlanksAroundTables::default();
    let content = r#"# Title
| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |
{:.striped}

Text"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should flag missing blank line before table");
    assert!(result[0].message.contains("Missing blank line before"));
}

#[test]
fn test_fix_preserves_kramdown_attributes() {
    let rule = MD058BlanksAroundTables::default();
    let content = r#"Text before
| Col1 | Col2 |
|------|------|
| A    | B    |
{:.striped}
Text after"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Should add blank line before the table but not after (due to attribute)
    let expected = r#"Text before

| Col1 | Col2 |
|------|------|
| A    | B    |
{:.striped}
Text after"#;
    assert_eq!(fixed, expected);
}
