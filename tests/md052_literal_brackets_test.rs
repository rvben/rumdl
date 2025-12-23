use rumdl_lib::config::{Config, MarkdownFlavor};
/// Test for MD052 false positives with literal brackets
/// This test verifies that MD052 doesn't report false positives for literal text in brackets
use rumdl_lib::rules;

#[test]
fn test_md052_literal_brackets_not_reference() {
    let content = r#"The output is colorized and the `[from ...]` annotation is globally aligned for easy scanning."#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md052_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD052").collect();

    let warnings = rumdl_lib::lint(content, &md052_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Should have NO warnings - [from ...] in backticks is literal text, not a reference
    assert_eq!(
        warnings.len(),
        0,
        "MD052 should not report literal text in backticks as reference links. Found warnings: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_md052_javascript_literals_not_flagged() {
    // Test for JavaScript/JSON literals that should not be flagged as references
    // Note: Uses full reference syntax [text][label] since shortcut-syntax is disabled by default
    let content = r#"### Eval output
(kind: ok) ["null"]
[null]
[true]
[false]
[NaN]
[Infinity]
[object Object]

But this [link text][actual-reference] should be flagged."#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md052_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD052").collect();

    let warnings = rumdl_lib::lint(content, &md052_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Should only flag [link text][actual-reference], not the JavaScript literals
    assert_eq!(
        warnings.len(),
        1,
        "MD052 should not flag JavaScript literals. Found warnings: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(
        warnings[0].message.contains("actual-reference"),
        "Should flag the actual undefined reference"
    );
}

#[test]
fn test_md052_actual_broken_reference() {
    let content = r#"This is a [broken reference][nonexistent].

Some other text."#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md052_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD052").collect();

    let warnings = rumdl_lib::lint(content, &md052_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Should have a warning - this is an actual broken reference
    assert_eq!(warnings.len(), 1, "MD052 should report actual broken references");
}
