use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::inline_config::InlineConfig;
use rumdl_lib::lint;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
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
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
        None,
    )
    .unwrap();

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
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
        None,
    )
    .unwrap();

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
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
        None,
    )
    .unwrap();

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
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
        None,
    )
    .unwrap();

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
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
        None,
    )
    .unwrap();

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
    let content = format!(
        "# Test Document\n\
         \n\
         <!-- markdownlint-disable MD013 MD009 -->\n\
         This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled\n\
         \n\
         <!-- markdownlint-enable MD013 -->\n\
         This is another very long line that exceeds 80 characters and should trigger MD013 but MD009 is still disabled\n\
         \n\
         <!-- markdownlint-enable MD009 -->\n\
         Trailing spaces should now trigger MD009{trailing}\n",
        trailing = "   "
    );

    let rules = all_rules(&Config::default());
    let warnings = lint(
        &content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
        None,
    )
    .unwrap();

    // Find warnings by rule
    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();
    let md009_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD009"))
        .collect();

    // MD013 on line 8 (after re-enable, long line)
    assert_eq!(md013_warnings.len(), 1);

    // MD009 on line 11 (after re-enable, trailing spaces)
    let inline_config = InlineConfig::from_content(&content);
    let md009_disabled_at_11 = inline_config.is_rule_disabled("MD009", 11);
    assert!(
        !md009_disabled_at_11,
        "MD009 should not be disabled at line 11, but it is!"
    );
    assert_eq!(md009_warnings.len(), 1);
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
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
        None,
    )
    .unwrap();

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
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
        None,
    )
    .unwrap();

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
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
        None,
    )
    .unwrap();

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

#[test]
fn test_inline_config_with_alias_disable_enable() {
    // Test that aliases work in inline comments (e.g., line-length instead of MD013)
    let content = r#"# Test Document

<!-- rumdl-disable line-length -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled using the alias line-length

<!-- rumdl-enable line-length -->
This is another very long line that exceeds 80 characters and should trigger MD013 because it was re-enabled
"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
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
        "Expected 1 MD013 warning but got {}: {:?}",
        md013_warnings.len(),
        md013_warnings
    );
    assert_eq!(md013_warnings[0].line, 7);
}

#[test]
fn test_inline_config_with_alias_disable_next_line() {
    // Test that aliases work with disable-next-line
    let content = r#"# Test Document

<!-- rumdl-disable-next-line line-length -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled using the alias

This is another very long line that exceeds 80 characters and should trigger MD013 because it's not disabled
"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have exactly one MD013 warning (line 6, not line 4)
    assert_eq!(md013_warnings.len(), 1);
    assert_eq!(md013_warnings[0].line, 6);
}

#[test]
fn test_inline_config_with_mixed_alias_and_rule_id() {
    // Test that mixing aliases and rule IDs works in the same comment
    // Note: Line 8 has trailing spaces at the end to trigger MD009
    let content = "# Test Document\n\n<!-- rumdl-disable line-length MD009 -->\nThis is a very long line that exceeds 80 characters and would normally trigger MD013\nLine with trailing spaces that would trigger MD009\n\n<!-- rumdl-enable MD013 no-trailing-spaces -->\nThis is another very long line that exceeds 80 characters and should trigger MD013   \n";

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
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

    // MD013 should only trigger on line 8 (after re-enable)
    assert_eq!(
        md013_warnings.len(),
        1,
        "Expected 1 MD013 warning but got {}",
        md013_warnings.len()
    );
    assert_eq!(md013_warnings[0].line, 8);

    // MD009 should only trigger on line 8 (after re-enable using alias no-trailing-spaces)
    assert_eq!(
        md009_warnings.len(),
        1,
        "Expected 1 MD009 warning but got {}",
        md009_warnings.len()
    );
    assert_eq!(md009_warnings[0].line, 8);
}

#[test]
fn test_inline_config_alias_case_insensitive() {
    // Test that aliases are case-insensitive
    let content = r#"# Test Document

<!-- rumdl-disable LINE-LENGTH -->
This is a very long line that exceeds 80 characters and would normally trigger MD013 but is disabled using uppercase alias

<!-- rumdl-enable Line-Length -->
This is another very long line that exceeds 80 characters and should trigger MD013 because it was re-enabled
"#;

    let rules = all_rules(&Config::default());
    let warnings = lint(
        content,
        &rules,
        false,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
        None,
    )
    .unwrap();

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| *n == "MD013"))
        .collect();

    // Should have exactly one MD013 warning (line 7)
    assert_eq!(md013_warnings.len(), 1);
    assert_eq!(md013_warnings[0].line, 7);
}

// =============================================================================
// Fix mode + inline config integration tests (issue #501)
//
// These tests verify that --fix mode respects inline disable comments.
// The bug: fix coordinator had zero awareness of inline config, so rules
// would modify content even inside disable blocks.
// =============================================================================

/// Helper: create context from content and call rule.fix()
fn fix_with_rule(rule: &dyn Rule, content: &str) -> String {
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    rule.fix(&ctx).unwrap()
}

// --- MD013: The exact bug reported in issue #501 (reflow ignores disable) ---

#[test]
fn test_fix_md013_reflow_respects_disable_enable() {
    use rumdl_lib::rules::MD013LineLength;
    use rumdl_lib::rules::md013_line_length::md013_config::MD013Config;
    use rumdl_lib::types::LineLength;

    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(80),
        reflow: true,
        ..Default::default()
    });

    let long = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.";
    let content = format!("# Test\n\n<!-- rumdl-disable MD013 -->\n{long}\n<!-- rumdl-enable MD013 -->\n\n{long}\n");

    let fixed = fix_with_rule(&rule, &content);
    let lines: Vec<&str> = fixed.lines().collect();

    // Line 4 (inside disable block) must be preserved exactly
    assert_eq!(lines[3], long, "Line inside disable block should not be reflowed");

    // Lines after the enable comment should be reflowed to <= 80 chars
    let non_disabled_lines: Vec<&str> = lines[6..].iter().copied().filter(|l| !l.is_empty()).collect();
    assert!(
        !non_disabled_lines.is_empty(),
        "Reflowed content should exist after enable comment"
    );
    for line in &non_disabled_lines {
        assert!(
            line.len() <= 80,
            "Reflowed line should be <= 80 chars, got {} chars: '{line}'",
            line.len()
        );
    }
}

#[test]
fn test_fix_md013_reflow_respects_disable_line() {
    use rumdl_lib::rules::MD013LineLength;
    use rumdl_lib::rules::md013_line_length::md013_config::MD013Config;
    use rumdl_lib::types::LineLength;

    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(80),
        reflow: true,
        ..Default::default()
    });

    let long = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore.";
    let content = format!("# Test\n\n{long} <!-- rumdl-disable-line MD013 -->\n\n{long}\n");

    let fixed = fix_with_rule(&rule, &content);
    let lines: Vec<&str> = fixed.lines().collect();

    // Line 3 (disabled via disable-line) should be preserved
    assert!(
        lines[2].len() > 80,
        "Disabled line should be preserved as-is, got: '{}'",
        lines[2]
    );

    // Line 5 (not disabled) should be reflowed
    assert!(
        lines[4].len() <= 80,
        "Non-disabled line should be reflowed to <= 80 chars, got: '{}'",
        lines[4]
    );
}

#[test]
fn test_fix_md013_reflow_respects_disable_next_line() {
    use rumdl_lib::rules::MD013LineLength;
    use rumdl_lib::rules::md013_line_length::md013_config::MD013Config;
    use rumdl_lib::types::LineLength;

    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(80),
        reflow: true,
        ..Default::default()
    });

    let long = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore.";
    let content = format!("# Test\n\n<!-- rumdl-disable-next-line MD013 -->\n{long}\n\n{long}\n");

    let fixed = fix_with_rule(&rule, &content);
    let lines: Vec<&str> = fixed.lines().collect();

    // Line 4 (disabled by previous comment) should be preserved
    assert!(
        lines[3].len() > 80,
        "Line after disable-next-line should be preserved as-is"
    );

    // Line 6 (not disabled) should be reflowed
    let non_disabled: Vec<&str> = lines[5..].iter().copied().filter(|l| !l.is_empty()).collect();
    for line in &non_disabled {
        assert!(line.len() <= 80, "Non-disabled line should be reflowed, got: '{line}'");
    }
}

/// Default-mode reflow reports each over-long line on its own and gives every
/// such warning the fix for the whole paragraph. A disabled line sharing a
/// paragraph with an enabled over-long line must still be left as written.
#[test]
fn test_fix_md013_reflow_keeps_a_disabled_line_inside_a_paragraph() {
    use rumdl_lib::rules::MD013LineLength;
    use rumdl_lib::rules::md013_line_length::md013_config::MD013Config;
    use rumdl_lib::types::LineLength;

    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(80),
        reflow: true,
        ..Default::default()
    });

    let long = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore.";
    for document in [
        format!("# Test\n\n<!-- rumdl-disable-next-line MD013 -->\n{long}\n{long}\n"),
        format!("# Test\n\n{long} <!-- rumdl-disable-line MD013 -->\n{long}\n"),
        format!("# Test\n\n{long}\n{long} <!-- rumdl-disable-line MD013 -->\n"),
        format!("# Test\n\n> {long}\n> {long} <!-- rumdl-disable-line MD013 -->\n"),
    ] {
        // The fix ends at the end of the paragraph's last line: with its line
        // ending, with a CRLF one, and at the end of a file that has none.
        for content in [
            document.clone(),
            document.replace('\n', "\r\n"),
            document.trim_end().to_string(),
        ] {
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            let enabled: Vec<_> = warnings
                .iter()
                .filter(|w| !ctx.inline_config().is_rule_disabled("MD013", w.line))
                .collect();
            assert!(
                !enabled.is_empty(),
                "the over-long enabled line is still reported: {content:?}"
            );
            assert!(
                enabled.iter().all(|w| w.fix.is_none()),
                "no fix may be offered, it would rewrite a disabled line: {content:?}"
            );
            assert_eq!(fix_with_rule(&rule, &content), content);
        }
    }

    // Control: without the directive the same paragraph is rewrapped.
    let content = format!("# Test\n\n{long}\n{long}\n");
    assert_ne!(fix_with_rule(&rule, &content), content);
}

/// MD048 reports the two fences of a block on their own lines, each with a fix
/// for that line alone. Only the opener can carry a directive, since nothing
/// between the fences is one, so it must exempt the block: a closing fence
/// converted on its own no longer closes its opener.
#[test]
fn test_fix_md048_keeps_the_closing_fence_of_a_block_disabled_at_its_opener() {
    use rumdl_lib::MD048CodeFenceStyle;
    use rumdl_lib::rules::code_fence_utils::CodeFenceStyle;

    let backtick = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
    let tilde = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);

    // Each document with the rule that wants its block converted and the
    // 1-indexed line of the block's opening fence.
    let documents: [(&MD048CodeFenceStyle, &str, usize); 6] = [
        (
            &backtick,
            "<!-- rumdl-disable-next-line MD048 -->\n~~~text\ncode\n~~~\n\nAfter.\n",
            2,
        ),
        (
            &backtick,
            "<!-- markdownlint-disable-next-line MD048 -->\n~~~text\ncode\n~~~\n\nAfter.\n",
            2,
        ),
        (
            &backtick,
            "<!-- rumdl-disable-next-line -->\n~~~text\ncode\n~~~\n\nAfter.\n",
            2,
        ),
        // The converted closing fence would also be lengthened past the inner one.
        (
            &backtick,
            "<!-- rumdl-disable-next-line MD048 -->\n~~~markdown\n```rust\ncode\n```\n~~~\n\nAfter.\n",
            2,
        ),
        (
            &backtick,
            "- item\n\n  <!-- rumdl-disable-next-line MD048 -->\n  ~~~text\n  code\n  ~~~\n\n  After.\n",
            4,
        ),
        (
            &tilde,
            "<!-- rumdl-disable-next-line MD048 -->\n```text\ncode\n```\n\nAfter.\n",
            2,
        ),
    ];
    for (rule, document, opener) in documents {
        for content in [
            document.to_string(),
            document.replace('\n', "\r\n"),
            document.trim_end().to_string(),
        ] {
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert!(ctx.inline_config().is_rule_disabled("MD048", opener), "{content:?}");
            // The opener is still reported, for the inline-config filter to drop
            // and credit to the directive. The closing fence is not reported.
            let reported: Vec<usize> = rule.check(&ctx).unwrap().iter().map(|w| w.line).collect();
            assert_eq!(reported, [opener], "{content:?}");
            assert_eq!(fix_with_rule(rule, &content), content);
        }
    }

    // Control: without the directive both fences are reported and converted.
    let content = "~~~text\ncode\n~~~\n\nAfter.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let reported: Vec<usize> = backtick.check(&ctx).unwrap().iter().map(|w| w.line).collect();
    assert_eq!(reported, [1, 3]);
    assert_eq!(fix_with_rule(&backtick, content), "```text\ncode\n```\n\nAfter.\n");

    // Control: the block after a disabled one is converted, both fences.
    let content = "<!-- rumdl-disable-next-line MD048 -->\n~~~text\ncode\n~~~\n\n~~~text\nmore\n~~~\n";
    assert_eq!(
        fix_with_rule(&backtick, content),
        "<!-- rumdl-disable-next-line MD048 -->\n~~~text\ncode\n~~~\n\n```text\nmore\n```\n"
    );
}

/// A fix replaces whole lines and ends where the next line starts. A disabled
/// line that only borders the paragraph, on either side, withholds nothing.
#[test]
fn test_fix_md013_reflow_rewraps_a_paragraph_bordered_by_disabled_lines() {
    use rumdl_lib::rules::MD013LineLength;
    use rumdl_lib::rules::md013_line_length::md013_config::MD013Config;
    use rumdl_lib::types::LineLength;

    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(80),
        reflow: true,
        ..Default::default()
    });

    let long = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore.";
    let heading = format!("# {long} <!-- rumdl-disable-line MD013 -->");
    // Line 1 and line 3 are disabled headings; the paragraph is line 2 alone.
    let document = format!("{heading}\n{long}\n{heading}\n");
    for (content, eol) in [
        (document.clone(), "\n"),
        (document.replace('\n', "\r\n"), "\r\n"),
        // The paragraph is the last line of a file with no final line ending.
        (format!("{heading}\n{long}"), "\n"),
    ] {
        let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
        assert!(ctx.inline_config().is_rule_disabled("MD013", 1), "{content:?}");
        assert!(!ctx.inline_config().is_rule_disabled("MD013", 2), "{content:?}");
        let fixable: Vec<usize> = rule
            .check(&ctx)
            .unwrap()
            .iter()
            .filter(|w| w.fix.is_some())
            .map(|w| w.line)
            .collect();
        assert_eq!(fixable, [2], "{content:?}");

        let fixed = fix_with_rule(&rule, &content);
        let lines: Vec<&str> = fixed.split(eol).collect();
        assert_eq!(lines[0], heading, "{fixed:?}");
        assert_eq!(
            lines[1..3].join(" "),
            long,
            "the paragraph is rewrapped onto two lines: {fixed:?}"
        );
        assert!(lines[1..3].iter().all(|line| line.len() <= 80), "{fixed:?}");
        assert_eq!(
            fixed.matches(heading.as_str()).count(),
            content.matches(heading.as_str()).count(),
            "{fixed:?}"
        );
    }
}

/// A bounded matrix over reflow modes, containers, directive placements and
/// line endings: a line MD013 is disabled on comes out of the fix exactly as
/// written, whichever line of its paragraph or list item the directive is on.
#[test]
fn test_fix_md013_reflow_never_rewrites_a_disabled_line_in_any_mode() {
    use rumdl_lib::rules::MD013LineLength;
    use rumdl_lib::rules::md013_line_length::md013_config::{MD013Config, ReflowMode};
    use rumdl_lib::types::LineLength;

    let long = "Lorem ipsum dolor sit amet. Consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore.";
    let directive = "<!-- rumdl-disable-line MD013 -->";
    let mut documents = 0;
    let mut rewritten = Vec::new();
    for mode in [
        ReflowMode::Default,
        ReflowMode::Normalize,
        ReflowMode::SentencePerLine,
        ReflowMode::SentencePack,
        ReflowMode::SemanticLineBreaks,
    ] {
        let rule = MD013LineLength::from_config_struct(MD013Config {
            line_length: LineLength::from_const(80),
            reflow: true,
            reflow_mode: mode,
            ..Default::default()
        });
        for (first, rest) in [("", ""), ("> ", "> "), ("- ", "  "), ("1. ", "   "), ("> - ", ">   ")] {
            for disabled in 0..3 {
                let source_lines: Vec<String> = (0..3)
                    .map(|line| {
                        let prefix = if line == 0 { first } else { rest };
                        if line == disabled {
                            format!("{prefix}{long} {directive}")
                        } else {
                            format!("{prefix}{long}")
                        }
                    })
                    .collect();
                // With a final line ending, with CRLF ones, and with none at
                // the end of the file.
                for (eol, final_eol) in [("\n", "\n"), ("\r\n", "\r\n"), ("\n", "")] {
                    let content = format!("{}{final_eol}", source_lines.join(eol));
                    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                    assert!(
                        ctx.inline_config().is_rule_disabled("MD013", disabled + 1),
                        "fixture: the directive must disable line {}: {content:?}",
                        disabled + 1
                    );
                    documents += 1;
                    let fixed = rule.fix(&ctx).unwrap();
                    let kept = fixed.split(eol).filter(|line| *line == source_lines[disabled]).count();
                    if kept != 1 {
                        rewritten.push(format!(
                            "{mode:?}, container {first:?}, line {} disabled: {content:?} became {fixed:?}",
                            disabled + 1
                        ));
                    }
                }
            }
        }
    }
    assert_eq!(documents, 225);
    assert!(
        rewritten.is_empty(),
        "{} of {documents} documents had a disabled line rewritten:\n{}",
        rewritten.len(),
        rewritten.join("\n")
    );
}

#[test]
fn test_fix_md013_reflow_respects_capture_restore() {
    use rumdl_lib::rules::MD013LineLength;
    use rumdl_lib::rules::md013_line_length::md013_config::MD013Config;
    use rumdl_lib::types::LineLength;

    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(80),
        reflow: true,
        ..Default::default()
    });

    let long = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore.";
    let content = format!(
        "# Test\n\n<!-- rumdl-disable MD013 -->\n{long}\n<!-- rumdl-capture -->\n<!-- rumdl-enable MD013 -->\n{long}\n<!-- rumdl-restore -->\n{long}\n"
    );

    let fixed = fix_with_rule(&rule, &content);
    let lines: Vec<&str> = fixed.lines().collect();

    // Line 4 (disabled) should be preserved
    assert_eq!(lines[3], long, "Initially disabled line should be preserved");

    // Line 7 (re-enabled between capture/restore) should be reflowed
    assert!(
        lines[6].len() <= 80,
        "Re-enabled line should be reflowed, got: '{}'",
        lines[6]
    );

    // Line 9 (after restore, back to disabled) should be preserved
    let last_long_idx = fixed.lines().count() - 1;
    let last_line = fixed.lines().last().unwrap();
    assert_eq!(
        last_line,
        long,
        "Line after restore (back to disabled) should be preserved (line {})",
        last_long_idx + 1
    );
}

// --- MD009: Category 2 rule (iterates lines directly in fix) ---

#[test]
fn test_fix_md009_respects_disable_enable() {
    use rumdl_lib::rules::MD009TrailingSpaces;

    let rule = MD009TrailingSpaces::new(2, false);

    let content =
        "# Test\n\n<!-- rumdl-disable MD009 -->\ntrailing   \n<!-- rumdl-enable MD009 -->\n\nalso trailing   \n";

    let fixed = fix_with_rule(&rule, content);
    let lines: Vec<&str> = fixed.lines().collect();

    // Line 4 (disabled) should keep trailing spaces
    assert!(
        lines[3].ends_with("   "),
        "Disabled line should keep trailing spaces, got: '{}'",
        lines[3]
    );

    // Line 7 (enabled) should have trailing spaces removed
    assert_eq!(lines[6], "also trailing", "Enabled line should be trimmed");
}

#[test]
fn test_fix_md009_strict_respects_disable_line() {
    use rumdl_lib::rules::MD009TrailingSpaces;

    let rule = MD009TrailingSpaces::new(2, true); // strict mode

    let content = "trailing   <!-- rumdl-disable-line MD009 -->\ntrailing   \n";

    let fixed = fix_with_rule(&rule, content);
    let lines: Vec<&str> = fixed.lines().collect();

    // Line 1 (disabled) should keep trailing spaces before the comment
    assert!(
        lines[0].contains("trailing   "),
        "Disabled line should keep trailing spaces, got: '{}'",
        lines[0]
    );

    // Line 2 (enabled, strict) should be fully trimmed
    assert_eq!(lines[1], "trailing");
}

// --- MD034: Category 1 rule (calls self.check() then filters in fix) ---

#[test]
fn test_fix_md034_respects_disable_enable() {
    use rumdl_lib::rules::MD034NoBareUrls;

    let rule = MD034NoBareUrls;

    let content = "# Test\n\n<!-- rumdl-disable MD034 -->\nVisit http://example.com for info\n<!-- rumdl-enable MD034 -->\n\nVisit http://other.com for info\n";

    let fixed = fix_with_rule(&rule, content);
    let lines: Vec<&str> = fixed.lines().collect();

    // Line 4 (disabled) should keep bare URL
    assert!(
        lines[3].contains("http://example.com") && !lines[3].contains("<http://example.com>"),
        "Disabled line should keep bare URL without angle brackets"
    );

    // Line 7 (enabled) should have URL wrapped
    assert!(
        lines[6].contains("<http://other.com>"),
        "Enabled line should wrap bare URL, got: '{}'",
        lines[6]
    );
}

// --- MD022: Heading spacing rule with context awareness ---

#[test]
fn test_fix_md022_respects_disable_enable() {
    use rumdl_lib::rules::MD022BlanksAroundHeadings;

    let rule = MD022BlanksAroundHeadings::default();

    // Two headings without required blank lines above.
    // First is disabled, second is not.
    let content = "# Top\n<!-- rumdl-disable MD022 -->\ntext\n## Disabled Heading\ntext\n<!-- rumdl-enable MD022 -->\ntext\n## Enabled Heading\ntext\n";

    let fixed = fix_with_rule(&rule, content);

    // The disabled heading should NOT have a blank line inserted above it
    assert!(
        fixed.contains("text\n## Disabled Heading"),
        "Disabled heading should not get blank line inserted above"
    );

    // The enabled heading should get a blank line inserted above
    assert!(
        fixed.contains("\n\n## Enabled Heading"),
        "Enabled heading should get blank line inserted, fixed content:\n{fixed}"
    );
}

// --- MD046: Complex state machine (fenced/indented conversion) ---

#[test]
fn test_fix_md046_respects_disable_enable() {
    use rumdl_lib::rules::MD046CodeBlockStyle;

    // Target style: indented (fenced blocks should be converted)
    let rule = MD046CodeBlockStyle::new(rumdl_lib::rules::CodeBlockStyle::Indented);

    let content = "# Test\n\n<!-- rumdl-disable MD046 -->\n```\npreserved fenced\n```\n<!-- rumdl-enable MD046 -->\n\n```\nconverted to indented\n```\n";

    let fixed = fix_with_rule(&rule, content);

    // Disabled fenced block should be preserved as-is
    assert!(
        fixed.contains("```\npreserved fenced\n```"),
        "Disabled fenced block should be preserved, got:\n{fixed}"
    );

    // Enabled fenced block should be converted to indented
    assert!(
        fixed.contains("    converted to indented"),
        "Enabled fenced block should be converted to indented, got:\n{fixed}"
    );
}

// --- Cross-cutting: globally disabled region should not be modified ---

#[test]
fn test_fix_global_disable_preserves_all_content() {
    // Test that a globally disabled region is untouched by any rule
    let content =
        "# Test\n\n<!-- rumdl-disable -->\nBare URL: http://example.com\nTrailing spaces   \n<!-- rumdl-enable -->\n";

    let rules_to_test: Vec<Box<dyn Rule>> = vec![
        Box::new(rumdl_lib::rules::MD009TrailingSpaces::new(2, false)),
        Box::new(rumdl_lib::rules::MD034NoBareUrls),
    ];

    for rule in &rules_to_test {
        let fixed = fix_with_rule(rule.as_ref(), content);
        assert_eq!(
            fixed,
            content,
            "Rule {} should not modify globally disabled content",
            rule.name()
        );
    }
}
