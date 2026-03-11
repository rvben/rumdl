use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD037NoSpaceInEmphasis;

#[test]
fn test_regression_xxxx_content_replacement_bug() {
    let rule = MD037NoSpaceInEmphasis;

    // Regression test for the critical bug where MD037 was replacing content with 'X' characters
    // This tests the specific case that triggered the bug: emphasis with spaces after inline code
    let test_cases = vec![
        // Test case from the bug report: inline code followed by emphasis with spaces
        (
            "**simple emphasis with spaces** and `code` and **another emphasis**",
            "**simple emphasis with spaces** and `code` and **another emphasis**",
        ),
        (
            "1. **Use `force_exclude` in your configuration file:**",
            "1. **Use `force_exclude` in your configuration file:**",
        ),
        // Additional edge cases that could trigger similar bugs
        ("`code` and * bad emphasis * here", "`code` and *bad emphasis* here"),
        (
            "Code `let x = 1;` with * spaces * and more",
            "Code `let x = 1;` with *spaces* and more",
        ),
        (
            "Multiple `code` spans and * bad * and `more code` text",
            "Multiple `code` spans and *bad* and `more code` text",
        ),
        (
            "Start with * bad * then `code` then * more bad *",
            "Start with *bad* then `code` then *more bad*",
        ),
    ];

    for (original_content, expected_after_fix) in test_cases {
        println!("Testing XXXX regression with: {original_content}");

        let ctx = LintContext::new(original_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let fixed_content = rule.fix(&ctx).unwrap();

        println!("  Original: {original_content}");
        println!("  Fixed:    {fixed_content}");
        println!("  Expected: {expected_after_fix}");

        // Critical check: fixed content should NEVER contain 'X' characters that weren't in the original
        let original_x_count = original_content.chars().filter(|&c| c == 'X').count();
        let fixed_x_count = fixed_content.chars().filter(|&c| c == 'X').count();
        assert_eq!(
            original_x_count, fixed_x_count,
            "CRITICAL REGRESSION: Fixed content contains {fixed_x_count} 'X' characters but original had {original_x_count}. This indicates the XXXX replacement bug has returned!"
        );

        // Check that inline code is preserved exactly
        if original_content.contains('`') {
            // Extract code spans from original and fixed content
            let original_code_spans: Vec<&str> = original_content.split('`').collect();
            let fixed_code_spans: Vec<&str> = fixed_content.split('`').collect();

            assert_eq!(
                original_code_spans.len(),
                fixed_code_spans.len(),
                "Number of backticks changed during fix - inline code structure was corrupted"
            );

            // Check that all code content is preserved
            for i in (1..original_code_spans.len()).step_by(2) {
                assert_eq!(
                    original_code_spans[i], fixed_code_spans[i],
                    "Inline code content was modified during fix: '{}' became '{}'",
                    original_code_spans[i], fixed_code_spans[i]
                );
            }
        }

        // Check that the fix produces the expected result
        assert_eq!(
            fixed_content, expected_after_fix,
            "Fixed content doesn't match expected result"
        );

        // Verify that fixed content passes validation (no warnings)
        let fixed_ctx = LintContext::new(&fixed_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result_after_fix = rule.check(&fixed_ctx).unwrap();
        assert!(
            result_after_fix.is_empty(),
            "Fixed content still has warnings: {result_after_fix:?}"
        );
    }
}
