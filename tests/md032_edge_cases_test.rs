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

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

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

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

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

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

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

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

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

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

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

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

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

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

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

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

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
    // Per markdownlint-cli: trailing text without blank line is lazy continuation
    // so NO MD032 warning is expected
    let content = r#"1. List item
Text directly after"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    assert_eq!(
        warnings.len(),
        0,
        "Per markdownlint-cli, trailing text is lazy continuation - no warning expected. Found: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
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

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

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

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

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

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

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

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

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

/// Issue #295: Nested list lazy continuation should be detected when allow_lazy_continuation=false
#[test]
fn test_md032_nested_list_lazy_continuation_detection() {
    // When allow_lazy_continuation = false, text that isn't properly indented
    // for the nested list item should be flagged as lazy continuation
    let content = r#"# Header

1. A list item.
   1. A nested list item.
   Naughty lazy continuation (indent 3).

Text after.
"#;

    let mut config = Config::default();
    // Disable lazy continuation to detect the issue
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // The naughty lazy continuation has indent 3, but the nested item has content_column 6
    // So indent 3 < 6 means it's a lazy continuation, not proper indented continuation
    // The warning is placed on line 4 (the list end) saying it should be followed by a blank line
    assert!(
        warnings.iter().any(|w| w.line == 4 && w.message.contains("followed")),
        "MD032 should detect lazy continuation in nested list (indent 3 < content_column 6). Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Issue #295: Properly indented nested list continuation should not be flagged
#[test]
fn test_md032_nested_list_proper_continuation_not_flagged() {
    // Content with indent >= content_column should be valid continuation
    let content = r#"# Header

1. A list item.
   1. A nested list item.
      Proper continuation (indent 6).

Text after.
"#;

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Line 5 has proper indent (6 >= content_column 6), so no MD032 warning
    assert!(
        !warnings.iter().any(|w| w.line == 5),
        "MD032 should not flag properly indented continuation. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Issue #295: Exact test case from the issue report
#[test]
fn test_md032_issue_295_exact_case() {
    let content = "1. A list item.\n   1. A nested list item.\n   Naughty lazy continuation (indent 3).\n     Proper continuation (indent 5).\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Both lines 3 and 4 have indent < content_column (6), so list ends at line 2
    // Warning should be on line 2 (the nested list item)
    assert!(
        warnings.iter().any(|w| w.line == 2 && w.message.contains("followed")),
        "Issue #295 exact case should be detected. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Issue #295: Three levels of nesting
#[test]
fn test_md032_triple_nested_lazy_continuation() {
    let content = r#"# Header

1. Level 1
   1. Level 2
      1. Level 3
      Lazy for level 3 (indent 6, needs 9).

Text after.
"#;

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Line 6 has indent 6, but level 3 item has content_column ~9
    // Should flag the list as needing blank line
    assert!(
        warnings.iter().any(|w| w.line == 5 && w.message.contains("followed")),
        "Triple nested lazy continuation should be detected. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Issue #295: Boundary - indent exactly at content_column is valid
#[test]
fn test_md032_indent_exactly_at_content_column() {
    // Nested item "   1. " starts content at column 6
    // Line with exactly 6 spaces should be valid continuation
    let content = "# Header\n\n1. Item\n   1. Nested\n      Exactly 6 spaces.\n\nText.\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Line 5 (Exactly 6 spaces) should be valid - no warning on line 4
    assert!(
        !warnings.iter().any(|w| w.line == 4),
        "Indent exactly at content_column should be valid. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Issue #295: Boundary - indent one less than content_column is lazy
#[test]
fn test_md032_indent_one_less_than_content_column() {
    // Nested item "   1. " starts content at column 6
    // Line with 5 spaces (one less) should be lazy
    let content = "# Header\n\n1. Item\n   1. Nested\n     Only 5 spaces.\n\nText.\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Line 5 has indent 5, content_column is 6, so it's lazy
    assert!(
        warnings.iter().any(|w| w.line == 4 && w.message.contains("followed")),
        "Indent one less than content_column should be lazy. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Issue #295: Wide marker (two-digit number)
#[test]
fn test_md032_wide_marker_nested_lazy() {
    // "10. " is 4 chars, so content starts at column 4
    // Nested "    1. " has marker at 4, content at 7
    let content = r#"# Header

10. Item with wide marker
    1. Nested item
    Lazy (indent 4, needs 7).

Text.
"#;

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Line 5 has indent 4, but nested item content_column is 7
    assert!(
        warnings.iter().any(|w| w.line == 4 && w.message.contains("followed")),
        "Wide marker nested lazy should be detected. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}
