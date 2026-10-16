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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Table without leading/trailing pipes but consistent columns
    let content = r#"
Header 1 | Header 2 | Header 3
-------- | -------- | --------
Cell 1.1 | Cell 1.2 | Cell 1.3
Cell 2.1 | Cell 2.2 | Cell 2.3
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Table with alignment specifiers
    let content = r#"
| Left | Center | Right |
|:-----|:------:|------:|
| 1    | 2      | 3     |
| 4    | 5      | 6     |
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert!(result.contains("| Cell 1.1 | Cell 1.2 |  |"));
}

#[test]
fn test_fix_keeps_extra_cells_with_content() {
    let rule = MD056TableColumnCount;

    let content = r#"
| Header 1 | Header 2 | Header 3 |
| -------- | -------- | -------- |
| Cell 1.1 | Cell 1.2 | Cell 1.3 | Extra |
    "#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(rule.check(&ctx).unwrap().len(), 1);
    assert_eq!(rule.fix(&ctx).unwrap(), content);
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

/// Pipes inside code spans should be treated as content, not delimiters, for all flavors
#[test]
fn test_pipes_in_code_spans_not_delimiters() {
    let rule = MD056TableColumnCount;

    let content = r#"
| Python type | Example  |
| ----------- | -------- |
| Union       | `x | y`  |
| Dict        | `dict[str, int]` |
"#;

    // MkDocs flavor: pipes in code spans are content
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "MkDocs flavor should treat pipes in backticks as content, not delimiters"
    );

    // Standard/GFM flavor: pipes in code spans are also content
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Standard flavor should treat pipes in code spans as content, not delimiters"
    );
}

/// Test various code span patterns with pipes in MkDocs flavor
#[test]
fn test_mkdocs_flavor_various_code_spans_with_pipes() {
    let rule = MD056TableColumnCount;

    let content = r#"
| Type | Syntax |
| ---- | ------ |
| OR   | `a | b` |
| Pipe | `|` |
| Multi | `a | b | c` |
"#;

    // MkDocs: all rows should have 2 columns
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "MkDocs should handle multiple pipes in code spans");
}

/// Test that escaped pipes work correctly in both flavors
#[test]
fn test_escaped_pipes_both_flavors() {
    let rule = MD056TableColumnCount;

    let content = r#"
| Col1 | Col2 |
| ---- | ---- |
| a \| b | c |
"#;

    // Escaped pipes should work in both flavors
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Escaped pipes should work in Standard flavor");

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Escaped pipes should work in MkDocs flavor");
}

/// Test fix preserves inline code with pipes in MkDocs flavor
#[test]
fn test_mkdocs_flavor_fix_preserves_inline_code_pipes() {
    let rule = MD056TableColumnCount;

    // Table with missing column but also inline code with pipe
    let content = r#"
| Type | Syntax | Description |
| ---- | ------ | ----------- |
| OR   | `a | b` |
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.fix(&ctx).unwrap();

    // The fix should add an empty cell, preserving the inline code
    assert!(result.contains("`a | b`"), "Fix should preserve inline code with pipe");
}

/// Test for issue #479: pipe in code span should not cause column count mismatch
#[test]
fn test_issue_479_pipe_in_code_span_table() {
    let content = "| Operator | Dunder Method   |\n|----------|-----------------|\n| `&`      | `__and__`       |\n| `|`      | `__or__`        |\n| `^`      | `__xor__`       |\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD056TableColumnCount;
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Pipe in code span should not cause column count mismatch, got: {result:?}"
    );
}

/// Number of body cells pulldown-cmark reads between the `S` and `E` sentinel
/// cells of a one-row table, which sidesteps its padding and truncation.
fn pulldown_cells_between_sentinels(row: &str) -> usize {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    let doc = format!("| H1 | H2 | H3 | H4 | H5 |\n|----|----|----|----|----|\n{row}\n");
    let mut cells: Vec<String> = Vec::new();
    let mut in_body_row = false;
    let mut in_cell = false;
    for event in Parser::new_ext(&doc, Options::ENABLE_TABLES) {
        match event {
            Event::Start(Tag::TableRow) => in_body_row = true,
            Event::Start(Tag::TableCell) if in_body_row => {
                in_cell = true;
                cells.push(String::new());
            }
            Event::End(TagEnd::TableCell) => in_cell = false,
            Event::Text(text) | Event::Code(text) if in_cell => {
                cells.last_mut().unwrap().push_str(&text);
            }
            _ => {}
        }
    }
    let start = cells.iter().position(|c| c == "S").expect("S sentinel");
    let end = cells.iter().position(|c| c == "E").expect("E sentinel");
    end - start - 1
}

fn md056_warnings(content: &str, flavor: rumdl_lib::config::MarkdownFlavor) -> Vec<String> {
    let ctx = LintContext::new(content, flavor, None);
    MD056TableColumnCount
        .check(&ctx)
        .unwrap()
        .into_iter()
        .map(|w| w.message)
        .collect()
}

/// GFM escapes a pipe after any number of backslashes, so a cell holding
/// `a\\|b` is one cell and the three-column table is consistent.
#[test]
fn test_backslash_before_pipe_escapes_it_whatever_the_parity() {
    use rumdl_lib::config::MarkdownFlavor;

    for backslashes in 1..=4 {
        let slashes = "\\".repeat(backslashes);
        for cell in [format!("a{slashes}|b"), format!("`a{slashes}|b`")] {
            let row = format!("| S | {cell} | E |");
            assert_eq!(pulldown_cells_between_sentinels(&row), 1, "oracle: {row}");
            let content = format!("| H1 | H2 | H3 |\n|----|----|----|\n{row}\n");
            for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::Obsidian] {
                assert_eq!(
                    md056_warnings(&content, flavor),
                    Vec::<String>::new(),
                    "{flavor:?}: {row}"
                );
            }
        }
    }
}

/// Python-Markdown discards backslash pairs before splitting, so under MkDocs
/// an even run leaves the pipe as a separator.
#[test]
fn test_mkdocs_backslash_pairs_leave_the_pipe_a_separator() {
    use rumdl_lib::config::MarkdownFlavor;

    for backslashes in 1..=4 {
        let row = format!("| S | a{}|b | E |", "\\".repeat(backslashes));
        let content = format!("| H1 | H2 | H3 |\n|----|----|----|\n{row}\n");
        let warnings = md056_warnings(&content, MarkdownFlavor::MkDocs);
        if backslashes % 2 == 0 {
            assert_eq!(warnings.len(), 1, "even run splits: {row}: {warnings:?}");
        } else {
            assert!(warnings.is_empty(), "odd run escapes: {row}: {warnings:?}");
        }
    }
}
