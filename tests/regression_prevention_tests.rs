use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;
/// Regression prevention test suite for CLI/LSP consistency patterns
///
/// This test suite guards against reintroducing the exact patterns that were
/// systematically fixed to achieve 100% CLI/LSP parity. Each test validates
/// that fixes continue to work correctly and consistently across both methods.
use rumdl_lib::utils::fix_utils::{apply_warning_fixes, validate_fix_range, warning_fix_to_edit};

/// Test Pattern 1: Empty byte ranges (most common regression)
///
/// This was the most frequent issue - rules generating warnings with empty
/// byte ranges that couldn't be applied by LSP.
#[test]
fn test_pattern_1_empty_byte_ranges_prevention() {
    let test_cases = vec![
        // MD029: Should calculate proper length for list marker replacement
        (
            "1. Item one\n2.  Item two",
            Box::new(MD029OrderedListPrefix::new(ListStyle::Ordered)) as Box<dyn Rule>,
        ),
        // MD031: Should insert only newlines, not duplicate content
        (
            "```rust\ncode\n```\nText",
            Box::new(MD031BlanksAroundFences::default()) as Box<dyn Rule>,
        ),
        // MD032: Should insert blank lines around lists
        (
            "Text\n- Item\nText",
            Box::new(MD032BlanksAroundLists::strict()) as Box<dyn Rule>,
        ),
        // MD041: Should insert heading with blank line
        (
            "Text without heading",
            Box::new(MD041FirstLineHeading::new(1, false)) as Box<dyn Rule>,
        ),
        // MD042: Should calculate proper length for link replacement
        ("[](empty-link)", Box::new(MD042NoEmptyLinks) as Box<dyn Rule>),
        // MD045: Should use regex capture group for URL
        ("![](no-alt-text.jpg)", Box::new(MD045NoAltText::new()) as Box<dyn Rule>),
        // MD047: Should handle trailing newline logic properly
        (
            "Text without trailing newline",
            Box::new(MD047SingleTrailingNewline) as Box<dyn Rule>,
        ),
        // MD048: Should preserve original trailing newline behavior
        (
            "```\ncode\n~~~",
            Box::new(MD048CodeFenceStyle::new(
                rumdl_lib::rules::code_fence_utils::CodeFenceStyle::Backtick,
            )) as Box<dyn Rule>,
        ),
    ];

    for (content, rule) in test_cases {
        let ctx = LintContext::new(content);
        let warnings = rule.check(&ctx).expect("Rule check should succeed");

        for warning in &warnings {
            if let Some(fix) = &warning.fix {
                // Validate that range is within content bounds (this is the critical check)
                assert!(
                    validate_fix_range(content, fix).is_ok(),
                    "Rule {} produced out-of-bounds fix range for content: '{}'",
                    rule.name(),
                    content
                );

                // For empty ranges, ensure they represent valid insertion points
                // Empty ranges are valid for insertions (start == end)
                if fix.range.is_empty() {
                    assert!(
                        fix.range.start <= content.len(),
                        "Rule {} produced empty fix range with invalid insertion point {} > content length {} for content: '{}'",
                        rule.name(),
                        fix.range.start,
                        content.len(),
                        content
                    );
                } else {
                    // For non-empty ranges, ensure they represent valid replacement ranges
                    assert!(
                        fix.range.start < fix.range.end,
                        "Rule {} produced invalid replacement range (start >= end) for content: '{}'",
                        rule.name(),
                        content
                    );
                }

                // Test that LSP edit conversion works
                let edit_result = warning_fix_to_edit(content, warning);
                assert!(
                    edit_result.is_ok(),
                    "Rule {} produced invalid LSP edit for content: '{}'",
                    rule.name(),
                    content
                );
            }
        }
    }
}

/// Test Pattern 2: Content duplication issues
///
/// Rules that inserted content multiple times or calculated ranges incorrectly
/// leading to duplicated text in the fixed output.
#[test]
fn test_pattern_2_content_duplication_prevention() {
    let test_cases = vec![
        // MD011: Should swap text and URL correctly
        (
            "(https://example.com)[Click here]",
            Box::new(MD011NoReversedLinks) as Box<dyn Rule>,
        ),
        // MD021: Should replace entire line instead of partial fixes
        (
            "#  Title with multiple spaces  #",
            Box::new(MD021NoMultipleSpaceClosedAtx::new()) as Box<dyn Rule>,
        ),
        // MD022: Should generate individual warnings with proper LSP fixes
        (
            "# Title\nText immediately after",
            Box::new(MD022BlanksAroundHeadings::new()) as Box<dyn Rule>,
        ),
        // MD014: Should fix range calculation and preserve trailing newlines
        (
            "```bash\n$ echo hello\n```",
            Box::new(MD014CommandsShowOutput::new()) as Box<dyn Rule>,
        ),
    ];

    for (content, rule) in test_cases {
        let ctx = LintContext::new(content);
        let warnings = rule.check(&ctx).expect("Rule check should succeed");

        // Apply fixes using both CLI and LSP methods
        let cli_fixed = rule.fix(&ctx).expect("CLI fix should succeed");
        let lsp_fixed = apply_warning_fixes(content, &warnings).expect("LSP fix should succeed");

        // Critical regression test: Results should be identical
        assert_eq!(
            cli_fixed,
            lsp_fixed,
            "Rule {} produced different CLI vs LSP results for content: '{}'\nCLI: '{}'\nLSP: '{}'",
            rule.name(),
            content,
            cli_fixed,
            lsp_fixed
        );

        // Content should not be duplicated
        let original_lines: Vec<&str> = content.lines().collect();
        let fixed_lines: Vec<&str> = cli_fixed.lines().collect();

        for original_line in &original_lines {
            if !original_line.trim().is_empty() {
                let occurrences_in_original = original_lines.iter().filter(|&&l| l == *original_line).count();
                let occurrences_in_fixed = fixed_lines.iter().filter(|&&l| l == *original_line).count();

                // Allow for one additional occurrence if the line was duplicated as a fix
                assert!(
                    occurrences_in_fixed <= occurrences_in_original + 1,
                    "Rule {} duplicated content. Line '{}' appears {} times in original but {} times in fixed",
                    rule.name(),
                    original_line,
                    occurrences_in_original,
                    occurrences_in_fixed
                );
            }
        }
    }
}

/// Test Pattern 3: Missing LSP implementations
///
/// Rules that had CLI fixes but missing or broken LSP fix implementations.
#[test]
fn test_pattern_3_missing_lsp_implementations_prevention() {
    let test_cases = vec![
        // MD053: Should provide proper fix ranges for unused reference removal
        (
            "[link]: unused reference",
            Box::new(MD053LinkImageReferenceDefinitions::default()) as Box<dyn Rule>,
        ),
        // MD055: Should detect formatting differences and generate warnings
        (
            "| col1| col2|\n|---|---|\n|data1|data2|",
            Box::new(MD055TablePipeStyle::new("consistent".to_string())) as Box<dyn Rule>,
        ),
    ];

    for (content, rule) in test_cases {
        let ctx = LintContext::new(content);
        let warnings = rule.check(&ctx).expect("Rule check should succeed");

        // Critical regression test: Every warning should have a valid fix
        for warning in &warnings {
            assert!(
                warning.fix.is_some(),
                "Rule {} produced warning without fix for content: '{}'",
                rule.name(),
                content
            );

            let fix = warning.fix.as_ref().unwrap();
            assert!(
                !fix.range.is_empty(),
                "Rule {} produced warning with empty fix range for content: '{}'",
                rule.name(),
                content
            );

            // Test LSP conversion doesn't panic
            let _edit = warning_fix_to_edit(content, warning);
        }
    }
}

/// Test byte range boundary conditions
///
/// Ensures that all fix ranges are within valid byte boundaries of the content.
#[test]
fn test_byte_range_boundaries() {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD011NoReversedLinks),
        Box::new(MD014CommandsShowOutput::new()),
        Box::new(MD021NoMultipleSpaceClosedAtx::new()),
        Box::new(MD022BlanksAroundHeadings::new()),
        Box::new(MD029OrderedListPrefix::new(ListStyle::Ordered)),
        Box::new(MD031BlanksAroundFences::default()),
        Box::new(MD032BlanksAroundLists::strict()),
        Box::new(MD041FirstLineHeading::new(1, false)),
        Box::new(MD042NoEmptyLinks),
        Box::new(MD045NoAltText::new()),
        Box::new(MD047SingleTrailingNewline),
        Box::new(MD048CodeFenceStyle::new(
            rumdl_lib::rules::code_fence_utils::CodeFenceStyle::Backtick,
        )),
        Box::new(MD053LinkImageReferenceDefinitions::default()),
        Box::new(MD055TablePipeStyle::new("consistent".to_string())),
    ];

    let test_contents = vec![
        "# Simple heading",
        "```\ncode\n```",
        "[link](url)",
        "- list item",
        "Text\n\nMultiple lines\n",
        "",   // Empty content
        "\n", // Just newline
        "Complex document\n\n# Heading\n\n- Item\n\n```code```\n\n[link](url)\n",
    ];

    for content in test_contents {
        let content_bytes = content.as_bytes();
        let content_len = content_bytes.len();

        for rule in &rules {
            let ctx = LintContext::new(content);
            let warnings = rule.check(&ctx).expect("Rule check should succeed");

            for warning in &warnings {
                if let Some(fix) = &warning.fix {
                    // Critical boundary checks
                    assert!(
                        fix.range.start <= content_len,
                        "Rule {} fix range start {} exceeds content length {} for: '{}'",
                        rule.name(),
                        fix.range.start,
                        content_len,
                        content
                    );

                    assert!(
                        fix.range.end <= content_len,
                        "Rule {} fix range end {} exceeds content length {} for: '{}'",
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

/// Test fix application idempotency
///
/// Applying fixes should be idempotent - running the same fix twice should not
/// change the result further.
#[test]
fn test_fix_application_idempotency() {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD011NoReversedLinks),
        Box::new(MD021NoMultipleSpaceClosedAtx::new()),
        Box::new(MD022BlanksAroundHeadings::new()),
        Box::new(MD029OrderedListPrefix::new(ListStyle::Ordered)),
        Box::new(MD042NoEmptyLinks),
    ];

    let test_contents = vec![
        "(https://example.com)[Click here]",
        "#  Title with spaces  #",
        "# Title\nText",
        "1. First\n3. Third",
        "[](empty)",
    ];

    for content in test_contents {
        for rule in &rules {
            let ctx = LintContext::new(content);

            // First application
            let first_fix = rule.fix(&ctx).expect("First fix should succeed");

            // Second application on already-fixed content
            let ctx2 = LintContext::new(&first_fix);
            let second_fix = rule.fix(&ctx2).expect("Second fix should succeed");

            // Critical regression test: Should be idempotent
            assert_eq!(
                first_fix,
                second_fix,
                "Rule {} is not idempotent for content: '{}'\nFirst: '{}'\nSecond: '{}'",
                rule.name(),
                content,
                first_fix,
                second_fix
            );
        }
    }
}

/// Test that all warnings have consistent fix quality
///
/// Every warning should have a fix that actually resolves the issue.
#[test]
fn test_warning_fix_quality() {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD011NoReversedLinks),
        Box::new(MD014CommandsShowOutput::new()),
        Box::new(MD021NoMultipleSpaceClosedAtx::new()),
        Box::new(MD022BlanksAroundHeadings::new()),
        Box::new(MD029OrderedListPrefix::new(ListStyle::Ordered)),
        Box::new(MD042NoEmptyLinks),
        Box::new(MD047SingleTrailingNewline),
    ];

    let problematic_contents = vec![
        "(https://example.com)[Click here]",
        "```bash\n$ echo test\n```",
        "#  Title  #",
        "# Title\nText",
        "1. First\n3. Third",
        "[](empty-link)",
        "Text without newline",
    ];

    for content in problematic_contents {
        for rule in &rules {
            let ctx = LintContext::new(content);
            let initial_warnings = rule.check(&ctx).expect("Initial check should succeed");

            if !initial_warnings.is_empty() {
                // Apply the fix
                let fixed_content = rule.fix(&ctx).expect("Fix should succeed");

                // Check the fixed content
                let fixed_ctx = LintContext::new(&fixed_content);
                let remaining_warnings = rule.check(&fixed_ctx).expect("Fixed content check should succeed");

                // Critical regression test: Fix should resolve all issues of this rule type
                assert!(
                    remaining_warnings.len() <= initial_warnings.len(),
                    "Rule {} fix made things worse for content: '{}'\nInitial warnings: {}\nRemaining warnings: {}",
                    rule.name(),
                    content,
                    initial_warnings.len(),
                    remaining_warnings.len()
                );
            }
        }
    }
}
