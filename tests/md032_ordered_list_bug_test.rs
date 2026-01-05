use rumdl_lib::config::{Config, MarkdownFlavor};
/// Test for MD032 false positives with ordered lists
/// This test verifies that MD032 doesn't report false positives for properly structured ordered lists
use rumdl_lib::rules;

#[test]
fn test_md032_ordered_list_continuation() {
    let content = r#"**Files checked (in order):**

1. `.rumdl.toml`
2. `rumdl.toml`
3. `pyproject.toml` (must contain `[tool.rumdl]` section)

This allows you to set personal preferences."#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Should have NO warnings - this is a properly formatted ordered list
    assert_eq!(
        warnings.len(),
        0,
        "MD032 should not report false positives for ordered list continuations. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_paragraph_not_list() {
    // CommonMark: A line starting with "2." after a paragraph (without blank line)
    // is NOT parsed as a list - it's part of the paragraph
    let content = r#"Some text here.
2. This list starts with 2
3. Next item"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Should have NO warnings - CommonMark parses this as a single paragraph, not a list
    // A list starting with non-1 requires a blank line before it to be recognized
    assert_eq!(
        warnings.len(),
        0,
        "MD032 should not report because content is parsed as paragraph, not list. Warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_backslash_continuation() {
    // Test for issue #91: backslash continuation in list items
    let content = r#"# Header

1. Foo\
   This line is a part of Foo

   ```bash
   true
   ```

1. Bar

   Body for Bar"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Should have NO warnings - backslash continuation is valid
    assert_eq!(
        warnings.len(),
        0,
        "MD032 should not report errors for lists with backslash continuations. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}
