use rumdl_lib::config::Config;
use rumdl_lib::inline_config::InlineConfig;
use rumdl_lib::lint;
use rumdl_lib::rules::all_rules;

#[test]
fn test_markdownlint_disable_enable() {
    let content = r#"# Test Document

<!-- markdownlint-disable MD013 -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled by the comment above

<!-- markdownlint-enable MD013 -->
This is another very long line that exceeds 80 characters and should trigger MD013 because it was re-enabled
"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(content, &rules, false).unwrap();

    // Find MD013 warnings
    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have exactly one MD013 warning (line 7)
    assert_eq!(md013_warnings.len(), 1);
    assert_eq!(md013_warnings[0].line, 7);
}

#[test]
fn test_markdownlint_disable_line() {
    let content = r#"# Test Document

This is a very long line that exceeds 80 characters and would normally trigger MD013 <!-- markdownlint-disable-line MD013 -->

This is another very long line that exceeds 80 characters and should trigger MD013 because it's not disabled
"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(content, &rules, false).unwrap();

    // Find MD013 warnings
    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have exactly one MD013 warning (line 5, not line 3)
    assert_eq!(md013_warnings.len(), 1);
    assert_eq!(md013_warnings[0].line, 5);
}

#[test]
fn test_markdownlint_disable_next_line() {
    let content = r#"# Test Document

<!-- markdownlint-disable-next-line MD013 -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled

This is another very long line that exceeds 80 characters and should trigger MD013 because it's not disabled
"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(content, &rules, false).unwrap();

    // Find MD013 warnings
    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have exactly one MD013 warning (line 6, not line 4)
    assert_eq!(md013_warnings.len(), 1);
    assert_eq!(md013_warnings[0].line, 6);
}

#[test]
fn test_markdownlint_capture_restore() {
    let content = r#"# Test Document

<!-- markdownlint-disable MD013 MD009 -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled

<!-- markdownlint-capture -->
<!-- markdownlint-disable MD025 -->
# This heading would trigger MD025 but it's disabled
<!-- markdownlint-restore -->

This is another very long line that exceeds 80 characters and should not trigger MD013 (still disabled)
# This heading should trigger MD025 (was restored)
"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(content, &rules, false).unwrap();

    // Find warnings by rule
    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();
    let md025_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD025"))
        .collect();
    let md009_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD009"))
        .collect();

    // MD013 should be disabled throughout
    assert_eq!(md013_warnings.len(), 0);

    // MD009 should be disabled throughout (trailing spaces)
    assert_eq!(md009_warnings.len(), 0);

    // MD025 should only be disabled between capture and restore
    // Line 12 should have MD025 warning
    assert_eq!(md025_warnings.len(), 1);
    assert_eq!(md025_warnings[0].line, 12);
}

#[test]
fn test_global_disable_enable() {
    let content = r#"# Test Document

<!-- markdownlint-disable -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but all rules are disabled

# This would trigger MD025 (single title) but all rules are disabled

Trailing spaces here

<!-- markdownlint-enable -->
This is another very long line that exceeds 80 characters and should trigger MD013 because rules are re-enabled
"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(content, &rules, false).unwrap();

    // All warnings should be from lines after the enable comment
    for warning in &warnings {
        assert!(
            warning.line >= 11,
            "Warning on line {} should have been disabled",
            warning.line
        );
    }

    // Should have at least one MD013 warning on line 11
    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();
    assert!(!md013_warnings.is_empty());
}

#[test]
fn test_multiple_rules_in_comment() {
    let content = r#"# Test Document

<!-- markdownlint-disable MD013 MD009 -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled

<!-- markdownlint-enable MD013 -->
This is another very long line that exceeds 80 characters and should trigger MD013 but MD009 is still disabled

<!-- markdownlint-enable MD009 -->
Trailing spaces should now trigger MD009   
"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(content, &rules, false).unwrap();

    // Find warnings by rule
    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();
    let md009_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD009"))
        .collect();

    // MD013 warning on line 7
    assert_eq!(md013_warnings.len(), 1);
    assert_eq!(md013_warnings[0].line, 7);

    // Debug: Check inline config state for line 10
    let inline_config = InlineConfig::from_content(content);
    let md009_disabled_at_10 = inline_config.is_rule_disabled("MD009", 10);
    assert!(
        !md009_disabled_at_10,
        "MD009 should not be disabled at line 10, but it is!"
    );

    // MD009 warning on line 10
    assert_eq!(md009_warnings.len(), 1);
    assert_eq!(md009_warnings[0].line, 10);
}

#[test]
fn test_rumdl_syntax_compatibility() {
    let content = r#"# Test Document

<!-- rumdl-disable MD013 -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled using rumdl syntax

<!-- rumdl-enable MD013 -->
This is another very long line that exceeds 80 characters and should trigger MD013 because it was re-enabled

This is a very long line that exceeds 80 characters but is disabled for this line only using rumdl syntax <!-- rumdl-disable-line MD013 -->

<!-- rumdl-disable-next-line MD013 -->
This is a very long line that exceeds 80 characters but is disabled by the previous line using rumdl syntax
"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(content, &rules, false).unwrap();

    // Find MD013 warnings
    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have exactly one MD013 warning (line 7)
    assert_eq!(md013_warnings.len(), 1);
    assert_eq!(md013_warnings[0].line, 7);
}

#[test]
fn test_inline_config_parsing() {
    let content = r#"# Test

<!-- markdownlint-disable MD001 -->
<!-- markdownlint-disable-line MD002 -->
<!-- markdownlint-disable-next-line MD003 -->
Text
<!-- markdownlint-capture -->
<!-- markdownlint-disable MD004 -->
<!-- markdownlint-restore -->
<!-- markdownlint-enable MD001 -->
"#;

    let config = InlineConfig::from_content(content);

    // MD001 should be disabled at line 5
    assert!(config.is_rule_disabled("MD001", 5));

    // MD002 should be disabled only at line 4
    assert!(config.is_rule_disabled("MD002", 4));
    assert!(!config.is_rule_disabled("MD002", 5));

    // MD003 should be disabled only at line 6 (next line after comment)
    assert!(config.is_rule_disabled("MD003", 6));
    assert!(!config.is_rule_disabled("MD003", 5));

    // MD004 should not be disabled after restore
    assert!(!config.is_rule_disabled("MD004", 10));

    // MD001 should be enabled after line 11
    assert!(!config.is_rule_disabled("MD001", 11));
}

#[test]
fn test_rumdl_capture_restore() {
    let content = r#"# Test Document

<!-- rumdl-disable MD013 MD009 -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled

<!-- rumdl-capture -->
<!-- rumdl-disable MD025 -->
# This heading would trigger MD025 but it's disabled
<!-- rumdl-restore -->
This is another very long line that exceeds 80 characters and should not trigger MD013 (still disabled)
# This heading should trigger MD025 (was restored)
"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(content, &rules, false).unwrap();

    // Find warnings by rule
    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();
    let md025_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD025"))
        .collect();
    let md009_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD009"))
        .collect();

    // MD013 should be disabled throughout
    assert_eq!(md013_warnings.len(), 0);

    // MD009 should be disabled throughout (trailing spaces)
    assert_eq!(md009_warnings.len(), 0);

    // MD025 should only be disabled between capture and restore
    // Line 11 should have MD025 warning
    assert_eq!(md025_warnings.len(), 1);
    assert_eq!(md025_warnings[0].line, 11);
}

#[test]
fn test_md009_simple() {
    let content = "Test  ";
    let rules = all_rules(&Config::default());
    let warnings = lint(content, &rules, false).unwrap();

    let md009_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD009"))
        .collect();

    assert_eq!(
        md009_warnings.len(),
        1,
        "Expected 1 MD009 warning but got {}",
        md009_warnings.len()
    );
}
