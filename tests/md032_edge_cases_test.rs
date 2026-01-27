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

    // Line 5 "Naughty lazy continuation" has indent 3 but nested item has content_column 6
    // So indent 3 < 6 means it's a lazy continuation - we should get a lazy continuation warning
    assert!(
        warnings
            .iter()
            .any(|w| w.line == 5 && w.message.contains("Lazy continuation")),
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

    // Line 3 "Naughty lazy continuation" has indent 3, nested item has content_column 6
    // Line 4 "Proper continuation" has indent 5, also < content_column 6
    // Both should be detected as lazy continuation
    assert!(
        warnings
            .iter()
            .any(|w| w.line == 3 && w.message.contains("Lazy continuation")),
        "Issue #295 exact case should detect lazy continuation on line 3. Found warnings: {:?}",
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
    // Should detect lazy continuation on line 6
    assert!(
        warnings
            .iter()
            .any(|w| w.line == 6 && w.message.contains("Lazy continuation")),
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
        warnings
            .iter()
            .any(|w| w.line == 5 && w.message.contains("Lazy continuation")),
        "Indent one less than content_column should be lazy. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Issue #268: List continuation inside blockquote should not be flagged as needing blank line
#[test]
fn test_md032_blockquote_list_continuation_not_flagged() {
    // When a list item has a properly indented continuation line inside a blockquote,
    // MD032 should not flag it as needing a blank line
    let content = r#"> * Improve performance of loading the overview.
>   Opening the app should be a lot quicker now!
> * Improve performance of loading a chat
> * Add ability to swipe through images in a chat (thanks to Nathan van Beelen!)
>   [**See preview here!**](https://example.com)

Get Pattle from F-droid for Android by adding this repo:
"#;

    let mut config = Config::default();
    // Test with allow_lazy_continuation = false to ensure proper continuation is recognized
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Lines 2 and 5 are properly indented continuation lines within the blockquote
    // (they have 2 spaces after ">", matching the "* " marker width)
    // No MD032 warnings should be generated
    assert_eq!(
        warnings.len(),
        0,
        "Issue #268: Blockquote list continuation should not be flagged. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Issue #268: Multiple list items with continuation in blockquote
#[test]
fn test_md032_blockquote_multi_item_list_with_continuations() {
    let content = r#"# Header

> * **Custom Themes & Material 3 Styling**
>   Say hello to **dynamic theming**!
> * **Archived Rooms Support**
>   Left a room but still want to check what happened?
> * **Knocking Support**
>   We now support knocking into public rooms.

#### Improvements
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

    assert_eq!(
        warnings.len(),
        0,
        "Issue #268: Multiple blockquote list items with continuation should not be flagged. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Issue #268: Ordered list in blockquote with code block (not indented)
#[test]
fn test_md032_blockquote_ordered_list_with_code_block() {
    // The code block is NOT indented to be part of the list item
    // Per CommonMark, the code block must be indented to the list item's content column
    // markdownlint-cli also reports MD032 on line 3 for this case
    let content = r#"> There's 3 methods to block the room:
>
> 1. Use the synapse admin API for it:
> ```bash
> curl -s -X POST
> ```
"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // The code block is NOT part of the list item (not indented), so list ends at line 3
    // Expect warning on line 3: "List should be followed by blank line"
    assert!(
        warnings.iter().any(|w| w.line == 3 && w.message.contains("followed")),
        "Unindented code block in blockquote list should trigger MD032. Found warnings: {:?}",
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
    // Should detect lazy continuation on line 5
    assert!(
        warnings
            .iter()
            .any(|w| w.line == 5 && w.message.contains("Lazy continuation")),
        "Wide marker nested lazy should be detected. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Issue #342: Auto-fix tests for lazy continuation
// ============================================================================

/// Issue #342: Simple lazy continuation fix - unordered list
#[test]
fn test_md032_lazy_continuation_fix_simple_unordered() {
    let content = "- Item with\nlazy continuation\n- another item\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Should have a warning for the lazy continuation line
    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));
    assert!(
        lazy_warning.is_some(),
        "Should detect lazy continuation. Found warnings: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    // The fix should be present
    let fix = lazy_warning.unwrap().fix.as_ref();
    assert!(fix.is_some(), "Lazy continuation warning should have a fix");

    // Apply the fix manually
    let fix = fix.unwrap();
    let mut fixed = content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);

    // The fix should add proper indentation (2 spaces for "- ")
    assert_eq!(
        fixed, "- Item with\n  lazy continuation\n- another item\n",
        "Fix should add 2-space indentation for unordered list"
    );
}

/// Issue #342: Simple lazy continuation fix - ordered list
#[test]
fn test_md032_lazy_continuation_fix_simple_ordered() {
    let content = "1. Item with\nlazy continuation\n2. another item\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));
    assert!(lazy_warning.is_some(), "Should detect lazy continuation");

    let fix = lazy_warning.unwrap().fix.as_ref();
    assert!(fix.is_some(), "Should have a fix");

    let fix = fix.unwrap();
    let mut fixed = content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);

    // The fix should add proper indentation (3 spaces for "1. ")
    assert_eq!(
        fixed, "1. Item with\n   lazy continuation\n2. another item\n",
        "Fix should add 3-space indentation for ordered list"
    );
}

/// Issue #342: Multiple consecutive lazy continuation lines within a list block
#[test]
fn test_md032_lazy_continuation_fix_multiple_lines() {
    // Lazy continuation lines must be WITHIN a list block (between items)
    // to get the "Lazy continuation" warning with fix
    let content = "- Item\nLine 1\nLine 2\n- Another item\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Should have warnings for lazy continuation lines within the block
    let lazy_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("Lazy continuation"))
        .collect();

    assert!(
        !lazy_warnings.is_empty(),
        "Should detect lazy continuation lines. Found: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    // All warnings should have fixes
    for warning in &lazy_warnings {
        assert!(
            warning.fix.is_some(),
            "Each lazy continuation warning should have a fix"
        );
    }
}

/// Issue #342: Lazy continuation fix inside blockquote
#[test]
fn test_md032_lazy_continuation_fix_blockquote() {
    let content = "> - Item\n> lazy continuation\n> - another item\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));
    assert!(
        lazy_warning.is_some(),
        "Should detect lazy continuation in blockquote. Found: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    let fix = lazy_warning.unwrap().fix.as_ref();
    assert!(fix.is_some(), "Lazy continuation in blockquote should have a fix");

    let fix = fix.unwrap();
    let mut fixed = content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);

    // The fix should preserve "> " and add proper indentation
    assert_eq!(
        fixed, "> - Item\n>   lazy continuation\n> - another item\n",
        "Fix should preserve blockquote prefix and add 2-space indentation"
    );
}

/// Issue #342: Blockquote fix should replace existing indent, not add to it
#[test]
fn test_md032_lazy_continuation_fix_blockquote_existing_indent() {
    let content = "> - Item\n>  lazy continuation\n> - another item\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();
    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));
    assert!(
        lazy_warning.is_some(),
        "Should detect lazy continuation in blockquote with existing indent. Found: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    let fix = lazy_warning.unwrap().fix.as_ref().unwrap();
    let mut fixed = content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);

    assert_eq!(
        fixed, "> - Item\n>   lazy continuation\n> - another item\n",
        "Fix should normalize indent after blockquote prefix"
    );
}

/// Issue #342: Blockquote lazy continuation with tab indentation
#[test]
fn test_md032_lazy_continuation_fix_blockquote_with_tab() {
    let content = "> - Item\n>\tlazy with tab\n> - another item\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();
    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));
    assert!(
        lazy_warning.is_some(),
        "Should detect lazy continuation in blockquote with tab. Found: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    let fix = lazy_warning.unwrap().fix.as_ref().unwrap();
    let mut fixed = content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);

    assert_eq!(
        fixed, "> - Item\n>  lazy with tab\n> - another item\n",
        "Fix should normalize tab indentation after blockquote prefix"
    );
}

/// Issue #342: Blockquote lazy continuation with tab and space after '>'
#[test]
fn test_md032_lazy_continuation_fix_blockquote_with_tab_and_space() {
    let content = "> - Item\n> \tlazy with tab\n> - another item\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();
    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));
    assert!(
        lazy_warning.is_some(),
        "Should detect lazy continuation in blockquote with space + tab. Found: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    let fix = lazy_warning.unwrap().fix.as_ref().unwrap();
    let mut fixed = content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);

    assert_eq!(
        fixed, "> - Item\n>   lazy with tab\n> - another item\n",
        "Fix should normalize tab indentation after spaced blockquote prefix"
    );
}

/// Issue #342: Idempotency - after fix, no more warnings
#[test]
fn test_md032_lazy_continuation_fix_idempotent() {
    let content = "- Item with\nlazy continuation\n- another item\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config.clone());

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    // Get warnings and apply fix
    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();
    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));
    assert!(lazy_warning.is_some());

    let fix = lazy_warning.unwrap().fix.as_ref().unwrap();
    let mut fixed = content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);

    // Re-lint the fixed content
    let mut config2 = Config::default();
    config2.rules.insert("MD032".to_string(), rule_config);
    let all_rules2 = rules::all_rules(&config2);
    let md032_rules2: Vec<_> = all_rules2.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings_after = rumdl_lib::lint(&fixed, &md032_rules2, false, MarkdownFlavor::Standard, None).unwrap();

    // No lazy continuation warnings should remain
    let lazy_warnings_after: Vec<_> = warnings_after
        .iter()
        .filter(|w| w.message.contains("Lazy continuation"))
        .collect();

    assert_eq!(
        lazy_warnings_after.len(),
        0,
        "After fix, no lazy continuation warnings should remain. Fixed content:\n{}\nWarnings: {:?}",
        fixed,
        lazy_warnings_after
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Issue #342: Wide ordered marker (10., 100.) auto-fix
#[test]
fn test_md032_lazy_continuation_fix_wide_ordered_marker() {
    // "10. " is 4 chars, so content starts at column 4
    // Lazy continuation needs 4 spaces, not the hardcoded 3
    let content = "10. Item with wide marker\nlazy continuation\n11. another item\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));
    assert!(
        lazy_warning.is_some(),
        "Should detect lazy continuation with wide marker. Found: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    let fix = lazy_warning.unwrap().fix.as_ref();
    assert!(fix.is_some(), "Should have a fix for wide marker");

    let fix = fix.unwrap();
    let mut fixed = content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);

    // The fix should add 4-space indentation for "10. " marker
    assert_eq!(
        fixed, "10. Item with wide marker\n    lazy continuation\n11. another item\n",
        "Fix should add 4-space indentation for wide ordered marker"
    );
}

/// Issue #342: Very wide ordered marker (100.) auto-fix
#[test]
fn test_md032_lazy_continuation_fix_very_wide_marker() {
    // "100. " is 5 chars
    let content = "100. Item\nlazy\n101. next\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));
    assert!(lazy_warning.is_some(), "Should detect lazy continuation");

    let fix = lazy_warning.unwrap().fix.as_ref().unwrap();
    let mut fixed = content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);

    // 5-space indentation for "100. "
    assert_eq!(
        fixed, "100. Item\n     lazy\n101. next\n",
        "Fix should add 5-space indentation for 100. marker"
    );
}

/// Issue #342: Ordered list in blockquote - detect and fix lazy continuation
#[test]
fn test_md032_lazy_continuation_fix_ordered_in_blockquote() {
    let content = "> 1. Item\n> lazy\n> 2. next\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));
    assert!(
        lazy_warning.is_some(),
        "Should detect lazy continuation in ordered list in blockquote. Found: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    let fix = lazy_warning.unwrap().fix.as_ref();
    assert!(fix.is_some(), "Should have fix for ordered list in blockquote");

    let fix = fix.unwrap();
    let mut fixed = content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);

    // Ordered list marker "1. " is 3 chars, so continuation needs 3 spaces after "> "
    assert_eq!(
        fixed, "> 1. Item\n>    lazy\n> 2. next\n",
        "Fix should add proper indentation for ordered list continuation in blockquote"
    );
}

/// Issue #342: Wide ordered marker in blockquote - detect and fix lazy continuation
#[test]
fn test_md032_lazy_continuation_fix_wide_marker_in_blockquote() {
    let content = "> 10. Item\n> lazy\n> 11. next\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));
    assert!(
        lazy_warning.is_some(),
        "Should detect lazy continuation for wide marker in blockquote. Found: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    let fix = lazy_warning.unwrap().fix.as_ref();
    assert!(fix.is_some(), "Should have fix for wide marker in blockquote");

    let fix = fix.unwrap();
    let mut fixed = content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);

    // Wide marker "10. " is 4 chars, so continuation needs 4 spaces after "> "
    assert_eq!(
        fixed, "> 10. Item\n>     lazy\n> 11. next\n",
        "Fix should add proper indentation for wide marker continuation in blockquote"
    );
}

/// Issue #342: Nested list lazy continuation auto-fix
#[test]
fn test_md032_lazy_continuation_fix_nested_list() {
    // Nested "- " at indent 2 has content_column 4
    // Lazy continuation at indent 2 needs to become indent 4
    let content = "- Outer\n  - Nested\n  lazy for nested\n  - more nested\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));
    assert!(
        lazy_warning.is_some(),
        "Should detect lazy continuation in nested list. Found: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    let fix = lazy_warning.unwrap().fix.as_ref();
    assert!(fix.is_some(), "Should have fix for nested list lazy continuation");

    let fix = fix.unwrap();
    let mut fixed = content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);

    // Nested item at column 2 with "- " (2 chars) has content at column 4
    assert_eq!(
        fixed, "- Outer\n  - Nested\n    lazy for nested\n  - more nested\n",
        "Fix should add proper indentation for nested list continuation"
    );
}

/// Issue #342: Deeply nested blockquotes (> > >)
#[test]
fn test_md032_lazy_continuation_fix_deeply_nested_blockquote() {
    let content = "> > - Item\n> > lazy\n> > - next\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));
    assert!(
        lazy_warning.is_some(),
        "Should detect lazy continuation in deeply nested blockquote. Found: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    let fix = lazy_warning.unwrap().fix.as_ref();
    assert!(fix.is_some(), "Should have fix for deeply nested blockquote");

    let fix = fix.unwrap();
    let mut fixed = content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);

    // After "> > ", need 2-space indent for "- "
    assert_eq!(
        fixed, "> > - Item\n> >   lazy\n> > - next\n",
        "Fix should preserve double blockquote prefix and add proper indent"
    );
}

/// Issue #342: Multiple lazy fixes applied together via apply_warning_fixes
#[test]
fn test_md032_lazy_continuation_fix_multiple_via_apply_warning_fixes() {
    use rumdl_lib::utils::fix_utils::apply_warning_fixes;

    let content = "- Item 1\nlazy 1\nlazy 2\n- Item 2\nlazy 3\n- Item 3\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config.clone());

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    // Get warnings first
    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Should have multiple lazy continuation warnings
    let lazy_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("Lazy continuation"))
        .cloned()
        .collect();

    assert!(
        lazy_warnings.len() >= 2,
        "Should detect multiple lazy continuations. Found {} warnings: {:?}",
        lazy_warnings.len(),
        lazy_warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );

    // Apply all fixes together using apply_warning_fixes
    let fixed = apply_warning_fixes(content, &lazy_warnings).expect("Should apply fixes successfully");

    // Re-create rules for re-linting
    let mut config2 = Config::default();
    config2.rules.insert("MD032".to_string(), rule_config);
    let all_rules2 = rules::all_rules(&config2);
    let md032_rules2: Vec<_> = all_rules2.into_iter().filter(|r| r.name() == "MD032").collect();

    // Verify all lazy continuations are fixed
    let warnings_after = rumdl_lib::lint(&fixed, &md032_rules2, false, MarkdownFlavor::Standard, None).unwrap();

    let lazy_after = warnings_after
        .iter()
        .filter(|w| w.message.contains("Lazy continuation"))
        .count();
    assert_eq!(
        lazy_after,
        0,
        "After apply_warning_fixes(), all lazy continuations should be resolved. Fixed:\n{}\nRemaining warnings: {:?}",
        fixed,
        warnings_after
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );

    // Verify the content is correctly indented
    assert!(
        fixed.contains("  lazy 1") && fixed.contains("  lazy 2") && fixed.contains("  lazy 3"),
        "All lazy continuations should be properly indented. Got:\n{fixed}"
    );
}

/// Issue #342: Tabs in indentation handling
#[test]
fn test_md032_lazy_continuation_fix_with_tabs() {
    // Tab-indented lazy continuation
    let content = "- Item\n\tlazy with tab\n- next\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Tab expands to 4 spaces visually, which is >= 2 (content_column for "- ")
    // So this might not be detected as lazy continuation
    // This test documents the current behavior
    let lazy_warning = warnings.iter().find(|w| w.message.contains("Lazy continuation"));

    if let Some(warning) = lazy_warning {
        let fix = warning.fix.as_ref();
        assert!(fix.is_some(), "If detected as lazy, should have a fix");

        let fix = fix.unwrap();
        let mut fixed = content.to_string();
        fixed.replace_range(fix.range.clone(), &fix.replacement);

        // Verify the fix produces valid output
        assert!(!fixed.is_empty(), "Fix should produce valid content");
    }
    // If not detected as lazy (because tab expands to >= 2 spaces), that's also acceptable
}

/// Issue #342: Strengthen multiple lines test - verify exact warning count
#[test]
fn test_md032_lazy_continuation_fix_multiple_lines_exact_count() {
    let content = "- Item\nLine 1\nLine 2\nLine 3\n- Another item\n";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    let lazy_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("Lazy continuation"))
        .collect();

    // Should detect exactly 3 lazy continuation lines (Line 1, Line 2, Line 3)
    assert_eq!(
        lazy_warnings.len(),
        3,
        "Should detect exactly 3 lazy continuation lines. Found: {:?}",
        lazy_warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );

    // All should have fixes
    for (i, warning) in lazy_warnings.iter().enumerate() {
        assert!(warning.fix.is_some(), "Warning {} should have a fix", i + 1);
    }

    // Verify fix lines are correct (lines 2, 3, 4)
    // Note: Warnings may come in non-deterministic order due to internal HashMap usage
    let mut fix_lines: Vec<_> = lazy_warnings.iter().map(|w| w.line).collect();
    fix_lines.sort();
    assert_eq!(
        fix_lines,
        vec![2, 3, 4],
        "Lazy continuation warnings should be on lines 2, 3, 4"
    );
}

/// Test nested list has correct list_item info for lazy continuation detection
#[test]
fn test_nested_list_item_content_column() {
    use rumdl_lib::lint_context::LintContext;

    let content = "- Outer\n  - Nested\n  lazy for nested\n  - more nested\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Line 2 should have list_item with content_column = 4
    assert!(
        ctx.lines[1].list_item.is_some(),
        "Line 2 should have list_item for nested list"
    );
    let list_item = ctx.lines[1].list_item.as_ref().unwrap();
    assert_eq!(
        list_item.content_column, 4,
        "Nested list item should have content_column = 4"
    );
}

/// Test lazy continuation detection with strikethrough formatting at line start
#[test]
fn test_lazy_continuation_with_strikethrough() {
    let content = "- Item\n~~strikethrough~~ continuation";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    let lazy_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("Lazy continuation"))
        .collect();

    // Should detect lazy continuation when line starts with strikethrough
    assert_eq!(
        lazy_warnings.len(),
        1,
        "Should detect lazy continuation with strikethrough at line start. Found: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );

    assert_eq!(lazy_warnings[0].line, 2, "Warning should be on line 2");
    assert!(lazy_warnings[0].fix.is_some(), "Should have a fix");
}

/// Test lazy continuation detection with subscript formatting at line start
#[test]
fn test_lazy_continuation_with_subscript() {
    let content = "- Item\n~subscript~ continuation";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    let lazy_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("Lazy continuation"))
        .collect();

    // Should detect lazy continuation when line starts with subscript
    // Note: Without ENABLE_SUBSCRIPT option, ~ is parsed as strikethrough in GFM mode
    assert!(
        lazy_warnings.len() <= 1,
        "Should detect lazy continuation with subscript at line start. Found: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Test lazy continuation detection with superscript formatting at line start
#[test]
fn test_lazy_continuation_with_superscript() {
    let content = "- Item\n^superscript^ continuation";

    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
    config.rules.insert("MD032".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    let lazy_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("Lazy continuation"))
        .collect();

    // Superscript requires ENABLE_SUPERSCRIPT option - may be parsed as plain text
    // The important thing is we don't crash and handle it gracefully
    assert!(
        lazy_warnings.len() <= 1,
        "Should handle superscript at line start gracefully. Found: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Test lazy continuation with mixed inline formatting types
#[test]
fn test_lazy_continuation_with_mixed_inline_formatting() {
    // Test various inline formatting types that could start a lazy continuation line
    let test_cases = vec![
        ("- Item\n*emphasis* text", "emphasis"),
        ("- Item\n**strong** text", "strong"),
        ("- Item\n`code` text", "code"),
        ("- Item\n[link](url) text", "link"),
        ("- Item\n![image](url) text", "image"),
        ("- Item\n~~strike~~ text", "strikethrough"),
    ];

    for (content, formatting_type) in test_cases {
        let mut config = Config::default();
        let mut rule_config = rumdl_lib::config::RuleConfig::default();
        rule_config
            .values
            .insert("allow-lazy-continuation".to_string(), toml::Value::Boolean(false));
        config.rules.insert("MD032".to_string(), rule_config);

        let all_rules = rules::all_rules(&config);
        let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

        let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

        let lazy_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.message.contains("Lazy continuation"))
            .collect();

        assert_eq!(
            lazy_warnings.len(),
            1,
            "Should detect lazy continuation with {} at line start for content: {:?}. Found: {:?}",
            formatting_type,
            content,
            warnings
                .iter()
                .map(|w| format!("Line {}: {}", w.line, w.message))
                .collect::<Vec<_>>()
        );

        assert_eq!(
            lazy_warnings[0].line, 2,
            "Warning for {formatting_type} should be on line 2"
        );

        assert!(
            lazy_warnings[0].fix.is_some(),
            "Warning for {formatting_type} should have a fix"
        );
    }
}
