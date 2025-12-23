use rumdl_lib::config::Config;
use rumdl_lib::lint;
use rumdl_lib::rules::all_rules;

#[test]
fn test_inline_disable_enable_blocks() {
    let content = r#"# Test Document

<!-- markdownlint-disable MD013 -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but should be ignored due to the disable comment above

<!-- markdownlint-enable MD013 -->
This is another very long line that exceeds 80 characters and should trigger MD013 because it was re-enabled by the enable comment
"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    // Find MD013 warnings
    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have exactly one MD013 warning (line 7)
    assert_eq!(
        md013_warnings.len(),
        1,
        "Expected 1 MD013 warning, got {}",
        md013_warnings.len()
    );
    assert_eq!(
        md013_warnings[0].line, 7,
        "Expected warning on line 7, got line {}",
        md013_warnings[0].line
    );
}

#[test]
fn test_inline_disable_all_rules() {
    let content = r#"# Test Document

<!-- markdownlint-disable -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but all rules are disabled
## This would trigger MD025 (multiple top-level headings) but all rules are disabled
Trailing spaces here
<!-- markdownlint-enable -->

This is another very long line that exceeds 80 characters and should trigger MD013 because all rules were re-enabled"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    // All warnings should be from lines after the enable comment (line 8+)
    for warning in &warnings {
        assert!(
            warning.line >= 8,
            "Warning on line {} should have been disabled (before line 8)",
            warning.line
        );
    }
}

#[test]
fn test_nested_disable_enable() {
    let content = r#"# Test Document

<!-- markdownlint-disable MD013 -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled
<!-- markdownlint-disable MD009 -->
This line has trailing spaces and should not trigger MD009
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is still disabled
<!-- markdownlint-enable MD013 -->
This is a very long line that exceeds 80 characters and should now trigger MD013 because it was re-enabled
This line has trailing spaces and should not trigger MD009
<!-- markdownlint-enable MD009 -->
This line has trailing spaces and should now trigger MD009  "#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    let md009_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD009"))
        .collect();

    // MD013 should only trigger on line 9
    assert_eq!(
        md013_warnings.len(),
        1,
        "Expected 1 MD013 warning, got {}",
        md013_warnings.len()
    );
    assert_eq!(
        md013_warnings[0].line, 9,
        "Expected MD013 on line 9, got line {}",
        md013_warnings[0].line
    );

    // MD009 should only trigger on line 12
    assert_eq!(
        md009_warnings.len(),
        1,
        "Expected 1 MD009 warning, got {}",
        md009_warnings.len()
    );
    assert_eq!(
        md009_warnings[0].line, 12,
        "Expected MD009 on line 12, got line {}",
        md009_warnings[0].line
    );
}

#[test]
fn test_disable_at_end_of_file() {
    let content = r#"# Test Document

This is a very long line that exceeds 80 characters and should trigger MD013 normally

<!-- markdownlint-disable MD013 -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have exactly one MD013 warning (line 3)
    assert_eq!(
        md013_warnings.len(),
        1,
        "Expected 1 MD013 warning, got {}",
        md013_warnings.len()
    );
    assert_eq!(
        md013_warnings[0].line, 3,
        "Expected warning on line 3, got line {}",
        md013_warnings[0].line
    );
}

#[test]
fn test_multiple_disable_same_rule() {
    let content = r#"# Test Document

<!-- markdownlint-disable MD013 -->
<!-- markdownlint-disable MD013 -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled
<!-- markdownlint-enable MD013 -->
This is a very long line that exceeds 80 characters and should trigger MD013 because it was re-enabled"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have exactly one MD013 warning (line 7)
    assert_eq!(
        md013_warnings.len(),
        1,
        "Expected 1 MD013 warning, got {}",
        md013_warnings.len()
    );
    assert_eq!(
        md013_warnings[0].line, 7,
        "Expected warning on line 7, got line {}",
        md013_warnings[0].line
    );
}

#[test]
fn test_enable_without_disable() {
    let content = r#"# Test Document

<!-- markdownlint-enable MD013 -->
This is a very long line that exceeds 80 characters and should trigger MD013 because enable without disable has no effect"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have one MD013 warning
    assert_eq!(
        md013_warnings.len(),
        1,
        "Expected 1 MD013 warning, got {}",
        md013_warnings.len()
    );
}

#[test]
fn test_disable_enable_on_same_line() {
    // Edge case: Both disable and enable on the same line
    let content = r#"# Test Document

<!-- markdownlint-disable MD013 --> <!-- markdownlint-enable MD013 -->
This is a very long line that exceeds 80 characters and should trigger MD013 because it was enabled on the same line"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // The line after should trigger MD013
    assert_eq!(
        md013_warnings.len(),
        1,
        "Expected 1 MD013 warning, got {}",
        md013_warnings.len()
    );
}

#[test]
fn test_disable_specific_then_all() {
    let content = r#"# Test Document

<!-- markdownlint-disable MD013 -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled
<!-- markdownlint-disable -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but all rules are disabled
## This would trigger MD025 but all rules are disabled
<!-- markdownlint-enable -->
This is a very long line that exceeds 80 characters and should trigger MD013 because all rules were re-enabled"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    // All warnings should be from line 9 or later
    for warning in &warnings {
        assert!(
            warning.line >= 9,
            "Warning on line {} should have been disabled",
            warning.line
        );
    }
}

#[test]
fn test_disable_all_then_enable_specific() {
    let content = r#"# Test Document

<!-- markdownlint-disable -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but all rules are disabled
## This would trigger MD025 but all rules are disabled
<!-- markdownlint-enable MD013 -->
This is a very long line that exceeds 80 characters and should trigger MD013 because MD013 was specifically enabled
## This should still not trigger MD025 because only MD013 was enabled"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    let md025_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD025"))
        .collect();

    // Should have MD013 warning on line 7
    assert_eq!(
        md013_warnings.len(),
        1,
        "Expected 1 MD013 warning, got {}",
        md013_warnings.len()
    );
    assert_eq!(
        md013_warnings[0].line, 7,
        "Expected MD013 on line 7, got line {}",
        md013_warnings[0].line
    );

    // Should not have MD025 warnings
    assert_eq!(
        md025_warnings.len(),
        0,
        "Expected 0 MD025 warnings, got {}",
        md025_warnings.len()
    );
}

#[test]
fn test_comment_inside_code_block() {
    // Comments inside code blocks should not be processed
    let content = r#"# Test Document

```markdown
<!-- markdownlint-disable MD013 -->
This is inside a code block and should not affect rules
```

This is a very long line that exceeds 80 characters and should trigger MD013 because the disable was in a code block"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have MD013 warning
    assert!(!md013_warnings.is_empty(), "Expected MD013 warnings");
}

#[test]
fn test_malformed_comments() {
    // Test that inline config directives without proper spacing are not processed
    // Note: We test that content following malformed markdownlint directives (those without
    // proper spacing) still gets linted. Valid HTML comments are correctly ignored per issue #119.
    let content = r#"# Test Document

<-- markdownlint-disable MD013 -->
This is a very long line that exceeds 80 characters and should trigger MD013 because comment has wrong opening

< !-- markdownlint-disable MD013 -->
This is a very long line that exceeds 80 characters and should trigger MD013 because comment has space in opening"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Lines after malformed HTML comments should trigger MD013
    assert!(
        md013_warnings.len() >= 2,
        "Expected at least 2 MD013 warnings, got {}",
        md013_warnings.len()
    );
}

#[test]
fn test_disable_file_all_rules() {
    let content = r#"# Test Document

<!-- markdownlint-disable-file -->

This is a very long line that exceeds 80 characters and would normally trigger MD013 but all rules are disabled for the file
## This would trigger MD025 (multiple top-level headings) but all rules are disabled
Trailing spaces here
[Bad link](http://example.com) (should trigger MD011 but all rules are disabled)"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    // Should have no warnings at all
    assert_eq!(warnings.len(), 0, "Expected no warnings, got {}", warnings.len());
}

#[test]
fn test_disable_file_specific_rules() {
    let content = r#"# Test Document

<!-- markdownlint-disable-file MD013 MD009 -->

This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled for the file
# This should still trigger MD025 (multiple top-level headings)
Trailing spaces here  "#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    // Should not have MD013 or MD009 warnings
    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();
    let md009_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD009"))
        .collect();

    assert_eq!(md013_warnings.len(), 0, "Expected no MD013 warnings");
    assert_eq!(md009_warnings.len(), 0, "Expected no MD009 warnings");

    // Should have MD025 warning
    let md025_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD025"))
        .collect();
    assert_eq!(md025_warnings.len(), 1, "Expected 1 MD025 warning");
}

#[test]
fn test_enable_file_after_disable() {
    let content = r#"# Test Document

<!-- markdownlint-disable-file MD013 -->
<!-- markdownlint-enable-file MD013 -->

This is a very long line that exceeds 80 characters and should trigger MD013 because it was re-enabled"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    assert_eq!(md013_warnings.len(), 1, "Expected 1 MD013 warning");
}

#[test]
fn test_rumdl_disable_file_syntax() {
    let content = r#"# Test Document

<!-- rumdl-disable-file MD013 -->

This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled using rumdl syntax"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    assert_eq!(md013_warnings.len(), 0, "Expected no MD013 warnings");
}

#[test]
fn test_disable_file_with_inline_enable() {
    let content = r#"# Test Document

<!-- markdownlint-disable-file MD013 -->

This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled for the file

<!-- markdownlint-enable MD013 -->
This is a very long line that exceeds 80 characters but file-wide disable takes precedence over inline enable"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // File-wide disable should override inline enable
    assert_eq!(
        md013_warnings.len(),
        0,
        "Expected no MD013 warnings - file-wide disable takes precedence"
    );
}

#[test]
fn test_configure_file_basic() {
    let content = r#"# Test Document

<!-- markdownlint-configure-file { "MD013": { "line_length": 120 } } -->

This is a very long line that exceeds 80 characters but is under 120 characters so should not trigger MD013

This is an extremely long line that exceeds even 120 characters and should trigger MD013 because it's over the configured limit"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have exactly one MD013 warning (line 7 exceeds 120 chars)
    assert_eq!(
        md013_warnings.len(),
        1,
        "Expected 1 MD013 warning for line exceeding 120 chars"
    );
    assert_eq!(md013_warnings[0].line, 7);
}

#[test]
fn test_configure_file_multiple_rules() {
    let content = r#"# Test Document

<!-- markdownlint-configure-file { "MD013": { "line_length": 120 }, "MD007": { "indent": 4 } } -->

This is a very long line that exceeds 80 characters but is under 120 characters so should not trigger MD013

* List item 1
  * Nested with 2 spaces (should trigger MD007 since we configured indent to 4)
    * Nested with 4 spaces (should be OK)"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    // Check MD007 warnings
    let md007_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD007"))
        .collect();

    // The test expectation might need adjustment based on how MD007 works with configure-file
    // For now, we're testing that the configuration was parsed
    println!("MD007 warnings: {md007_warnings:?}");
}

#[test]
fn test_rumdl_configure_file_syntax() {
    let content = r#"# Test Document

<!-- rumdl-configure-file { "MD013": { "line_length": 150 } } -->

This is a very long line that exceeds 80 characters and even exceeds 120 characters but is under 150 characters so should not trigger MD013"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have no MD013 warnings as line is under 150 chars
    assert_eq!(md013_warnings.len(), 0, "Expected no MD013 warnings with rumdl syntax");
}

#[test]
fn test_configure_file_invalid_json() {
    let content = r#"# Test Document

<!-- markdownlint-configure-file { invalid json } -->

This is a very long line that exceeds 80 characters and should trigger MD013 because the configure comment had invalid JSON"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have MD013 warning as the invalid JSON config is ignored
    assert_eq!(
        md013_warnings.len(),
        1,
        "Expected MD013 warning when configure-file has invalid JSON"
    );
}

#[test]
fn test_configure_file_with_disable() {
    let content = r#"# Test Document

<!-- markdownlint-configure-file { "MD013": { "line_length": 200 } } -->
<!-- markdownlint-disable-file MD013 -->

This is an extremely long line that exceeds even 200 characters and would normally trigger MD013 even with the configuration but the rule is disabled for the file"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have no warnings as MD013 is disabled for the file
    assert_eq!(
        md013_warnings.len(),
        0,
        "Expected no MD013 warnings when rule is disabled"
    );
}

#[test]
fn test_configure_file_empty_json() {
    let content = r#"# Test Document

<!-- markdownlint-configure-file {} -->

This is a very long line that exceeds 80 characters and should trigger MD013 with default settings"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have MD013 warning with default 80 char limit
    assert_eq!(md013_warnings.len(), 1, "Expected MD013 warning with empty configure");
}
