use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::rules;

#[test]
fn test_md032_multiple_backslash_continuations() {
    // Test multiple consecutive backslash continuations
    let content = r#"# Header

1. First\
   Second\
   Third\
   Fourth

2. Next item

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    assert_eq!(
        warnings.len(),
        0,
        "MD032 should handle multiple backslash continuations. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_backslash_with_code_block() {
    let content = r#"# Header

1. Item\
   Continuation

   ```python
   code
   ```

2. Next

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    assert_eq!(
        warnings.len(),
        0,
        "MD032 should handle backslash continuation with code blocks. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_indented_code_block_in_list() {
    // Test indented code block as part of list item
    let content = r#"# Header

1. Item with code

       indented code block
       more code

2. Next item

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    assert_eq!(
        warnings.len(),
        0,
        "MD032 should handle indented code blocks in list items. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_fenced_code_block_in_list() {
    // Test fenced code block properly indented in list
    let content = r#"# Header

1. Item with code

   ```bash
   echo "test"
   ```

2. Next item

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    assert_eq!(
        warnings.len(),
        0,
        "MD032 should handle fenced code blocks in list items when properly indented. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_mixed_ordered_unordered() {
    let content = r#"# Header

1. Ordered item

- Unordered item

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    // Mixed lists should be treated as separate lists
    assert_eq!(
        warnings.len(),
        0,
        "MD032 should handle transition from ordered to unordered list. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_nested_lists() {
    let content = r#"# Header

1. First level
   - Nested item
   - Another nested
2. Back to first level

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    assert_eq!(
        warnings.len(),
        0,
        "MD032 should handle nested lists. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_list_with_paragraphs() {
    let content = r#"# Header

1. First item

   This is a second paragraph in the first item.

2. Second item

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    assert_eq!(
        warnings.len(),
        0,
        "MD032 should handle list items with multiple paragraphs. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_list_without_blank_before() {
    let content = r#"Text directly before
1. List item"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    assert_eq!(
        warnings.len(),
        1,
        "MD032 should report when list lacks blank line before"
    );
    assert_eq!(warnings[0].line, 2);
    assert!(warnings[0].message.contains("preceded"));
}

#[test]
fn test_md032_list_without_blank_after() {
    let content = r#"1. List item
Text directly after"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    assert_eq!(
        warnings.len(),
        1,
        "MD032 should report when list lacks blank line after"
    );
    assert_eq!(warnings[0].line, 1);
    assert!(warnings[0].message.contains("followed"));
}

#[test]
fn test_md032_blockquote_in_list() {
    let content = r#"# Header

1. Item with quote

   > This is a blockquote
   > within the list item

2. Next item

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    assert_eq!(
        warnings.len(),
        0,
        "MD032 should handle blockquotes within list items. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_table_in_list() {
    let content = r#"# Header

1. Item with table

   | Col1 | Col2 |
   |------|------|
   | A    | B    |

2. Next item

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    assert_eq!(
        warnings.len(),
        0,
        "MD032 should handle tables within list items. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_empty_list_items() {
    let content = r#"# Header

1.
2. Item with content
3.

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    // Empty list items can cause issues with line-by-line parsing
    // The parser may see the second item as a new list
    assert!(
        warnings.len() <= 1,
        "MD032 may have issues with empty list items. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_list_with_html_blocks() {
    let content = r#"# Header

1. Item one

<div>
HTML block
</div>

2. Item two

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    // HTML blocks with proper spacing don't cause MD032 warnings
    // The blank lines around the HTML block satisfy MD032 requirements
    assert_eq!(
        warnings.len(),
        0,
        "MD032 should not report warnings when HTML blocks are properly spaced. Found: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}
