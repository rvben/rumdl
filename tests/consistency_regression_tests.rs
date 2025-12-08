use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;
/// Basic regression tests for CLI/LSP consistency
///
/// Simple tests to ensure core functionality works and prevents regressions.
use rumdl_lib::utils::fix_utils::{apply_warning_fixes, validate_fix_range};

/// Test that basic rules don't produce empty fix ranges
#[test]
fn test_no_empty_fix_ranges() {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD009TrailingSpaces::new(2, false)),
        Box::new(MD010NoHardTabs::new(4)),
        Box::new(MD011NoReversedLinks),
        Box::new(MD018NoMissingSpaceAtx::new()),
        Box::new(MD019NoMultipleSpaceAtx::new()),
        Box::new(MD038NoSpaceInCode::new()),
        Box::new(MD039NoSpaceInLinks::new()),
    ];

    let test_contents = vec![
        "Text with trailing spaces    ",
        "Text\twith\ttabs",
        "(https://example.com)[reversed link]",
        "#Missing space",
        "#  Multiple spaces",
        "Text with ` spaced code `",
        "Text with [ spaced link ]( url )",
    ];

    for rule in &rules {
        for content in &test_contents {
            let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).expect("Rule check should succeed");

            for warning in &warnings {
                if let Some(fix) = &warning.fix {
                    // Critical test: Fix range must not be empty
                    assert!(
                        !fix.range.is_empty(),
                        "Rule {} produced empty fix range for content: '{}'",
                        rule.name(),
                        content
                    );

                    // Validate range is within bounds
                    assert!(
                        validate_fix_range(content, fix).is_ok(),
                        "Rule {} produced invalid fix range for content: '{}'",
                        rule.name(),
                        content
                    );
                }
            }
        }
    }
}

/// Test CLI vs LSP consistency for basic cases
#[test]
fn test_cli_lsp_consistency() {
    let test_cases = vec![
        (
            "Text with trailing spaces    ",
            Box::new(MD009TrailingSpaces::new(2, false)) as Box<dyn Rule>,
        ),
        ("Text\twith\ttabs", Box::new(MD010NoHardTabs::new(4)) as Box<dyn Rule>),
        (
            "(https://example.com)[Click here]",
            Box::new(MD011NoReversedLinks) as Box<dyn Rule>,
        ),
        (
            "#Missing space",
            Box::new(MD018NoMissingSpaceAtx::new()) as Box<dyn Rule>,
        ),
        (
            "Text with ` spaced code `",
            Box::new(MD038NoSpaceInCode::new()) as Box<dyn Rule>,
        ),
        (
            "Text with [ spaced link ]( url )",
            Box::new(MD039NoSpaceInLinks::new()) as Box<dyn Rule>,
        ),
    ];

    for (content, rule) in test_cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).expect("Rule check should succeed");

        if !warnings.is_empty() {
            // Apply fixes using both methods
            let cli_fixed = rule.fix(&ctx).expect("CLI fix should succeed");
            let lsp_fixed = apply_warning_fixes(content, &warnings).expect("LSP fix should succeed");

            // Critical test: Results should be identical
            assert_eq!(
                cli_fixed,
                lsp_fixed,
                "Rule {} produced different CLI vs LSP results for content: '{}'\nCLI: '{}'\nLSP: '{}'",
                rule.name(),
                content,
                cli_fixed,
                lsp_fixed
            );
        }
    }
}

/// Test that fix ranges are within content bounds
#[test]
fn test_fix_ranges_within_bounds() {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD009TrailingSpaces::new(2, false)),
        Box::new(MD010NoHardTabs::new(4)),
        Box::new(MD011NoReversedLinks),
        Box::new(MD038NoSpaceInCode::new()),
        Box::new(MD039NoSpaceInLinks::new()),
    ];

    let test_contents = vec![
        "Simple text",
        "Multi\nline\ncontent",
        "Unicode: 🚀 emoji test",
        "Mixed: ASCII + 中文 + русский",
        "",  // Empty
        "a", // Single character
    ];

    for rule in &rules {
        for content in &test_contents {
            let content_len = content.len();
            let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).expect("Rule check should succeed");

            for warning in &warnings {
                if let Some(fix) = &warning.fix {
                    assert!(
                        fix.range.start <= content_len,
                        "Rule {} fix range start {} > content length {} for: '{}'",
                        rule.name(),
                        fix.range.start,
                        content_len,
                        content
                    );

                    assert!(
                        fix.range.end <= content_len,
                        "Rule {} fix range end {} > content length {} for: '{}'",
                        rule.name(),
                        fix.range.end,
                        content_len,
                        content
                    );

                    assert!(
                        fix.range.start <= fix.range.end,
                        "Rule {} fix range start {} > end {} for: '{}'",
                        rule.name(),
                        fix.range.start,
                        fix.range.end,
                        content
                    );
                }
            }
        }
    }
}

/// Test that fixes actually improve content
#[test]
fn test_fixes_improve_content() {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD009TrailingSpaces::new(2, false)),
        Box::new(MD010NoHardTabs::new(4)),
        Box::new(MD011NoReversedLinks),
        Box::new(MD018NoMissingSpaceAtx::new()),
        Box::new(MD019NoMultipleSpaceAtx::new()),
        Box::new(MD038NoSpaceInCode::new()),
        Box::new(MD039NoSpaceInLinks::new()),
    ];

    let problematic_contents = vec![
        "Text with trailing spaces    ",
        "Text\twith\ttabs",
        "(https://example.com)[Click here]",
        "#Missing space",
        "#  Multiple spaces",
        "Text with ` spaced code `",
        "Text with [ spaced link ]( url )",
    ];

    for rule in &rules {
        for content in &problematic_contents {
            let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
            let initial_warnings = rule.check(&ctx).expect("Initial check should succeed");

            if !initial_warnings.is_empty() {
                // Apply the fix
                let fixed_content = rule.fix(&ctx).expect("Fix should succeed");

                // Check the fixed content has fewer or equal warnings
                let fixed_ctx = LintContext::new(&fixed_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
                let remaining_warnings = rule.check(&fixed_ctx).expect("Fixed content check should succeed");

                assert!(
                    remaining_warnings.len() <= initial_warnings.len(),
                    "Rule {} fix made things worse for content: '{}'\nInitial warnings: {}\nRemaining warnings: {}",
                    rule.name(),
                    content,
                    initial_warnings.len(),
                    remaining_warnings.len()
                );

                // Fixed content should be different if there were warnings
                assert_ne!(
                    fixed_content,
                    *content,
                    "Rule {} didn't change content despite warnings for: '{}'",
                    rule.name(),
                    content
                );
            }
        }
    }
}
