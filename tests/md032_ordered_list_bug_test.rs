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

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

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
fn test_md032_actual_non_1_start() {
    let content = r#"Some text here.
2. This list starts with 2
3. Next item"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    // Should have a warning - list starts with 2
    assert_eq!(
        warnings.len(),
        1,
        "MD032 should report when a list actually starts with non-1"
    );
    assert_eq!(warnings[0].line, 2, "Warning should be on line 2");
}
