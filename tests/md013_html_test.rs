/// Issue #339: Self-closing script/style tags should not mark subsequent lines as in_html_block
#[test]
fn test_md013_issue_339_self_closing_script_tag() {
    use rumdl_lib::config::{Config, MarkdownFlavor};
    use rumdl_lib::rules;

    // Test that lines after a self-closing script tag are still processed
    let content = r#"# Header

Line before that is long enough to need reflowing and this should work properly without any issues at all.

<script async src="//platform.twitter.com/widgets.js" charset="utf-8"></script>

Line after script that is very long and exceeds 80 characters and should definitely be reflowed now.

More text after that should also be processed and flagged for line length violations in the linter.
"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md013_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD013").collect();

    let warnings = rumdl_lib::lint(content, &md013_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Should find warnings on lines 3, 7, and 9 (all long lines)
    assert!(
        warnings.len() >= 3,
        "Issue #339: Should detect line length issues AFTER self-closing script tag. Found {} warnings: {:?}",
        warnings.len(),
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );

    // Verify that lines after the script tag are being processed
    assert!(
        warnings.iter().any(|w| w.line == 7),
        "Issue #339: Should detect line length on line 7 (after script tag). Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
    assert!(
        warnings.iter().any(|w| w.line == 9),
        "Issue #339: Should detect line length on line 9 (after script tag). Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Issue #339: Style tags on same line should not mark subsequent lines
#[test]
fn test_md013_issue_339_self_closing_style_tag() {
    use rumdl_lib::config::{Config, MarkdownFlavor};
    use rumdl_lib::rules;

    let content = r#"# Header

<style>.class { color: red; }</style>

This line comes after the style tag and has a very long length that exceeds eighty characters maximum.
"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md013_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD013").collect();

    let warnings = rumdl_lib::lint(content, &md013_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Should find warning on line 5 (after style tag)
    assert!(
        warnings.iter().any(|w| w.line == 5),
        "Issue #339: Should detect line length AFTER self-closing style tag. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}
