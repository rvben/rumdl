// Regression prevention test suite for MD051 rule
// These tests ensure that specific bugs like issue 39 never reoccur

use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD051LinkFragments;
use std::collections::HashMap;

/// Critical regression test for issue 39 specific patterns
/// These exact cases MUST work to prevent regression
#[test]
fn regression_test_issue_39_critical_cases() {
    let rule = MD051LinkFragments::new();

    // These are the exact cases reported in issue 39
    // If any of these break, it's a regression
    let critical_cases = vec![
        // Cases that currently work (must continue working)
        // Updated to match official GitHub behavior
        ("Testing & Coverage", "testing--coverage", "MUST_WORK"),
        (
            "API Reference: Methods & Properties",
            "api-reference-methods--properties",
            "MUST_WORK",
        ),
        // Cases that were broken and should be fixed
        // Updated to match official GitHub behavior - now working correctly
        (
            "cbrown --> sbrown: --unsafe-paths",
            "cbrown----sbrown---unsafe-paths",
            "MUST_WORK",
        ),
        ("cbrown -> sbrown", "cbrown---sbrown", "MUST_WORK"),
        // Additional variations that should work
        ("Step 1: Getting Started", "step-1-getting-started", "MUST_WORK"),
        ("FAQ: What's New?", "faq-whats-new", "MUST_WORK"),
    ];

    let mut passed = 0;
    let mut failed = 0;
    let mut critical_failed = 0;

    for (heading, expected_fragment, requirement) in critical_cases {
        let content = format!("# {heading}\n\n[Link](#{expected_fragment})");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        if result.is_empty() {
            println!("✅ PASS: '{heading}' -> '{expected_fragment}'");
            passed += 1;
        } else {
            println!("❌ FAIL: '{heading}' -> '{expected_fragment}'");
            println!(
                "   Warnings: {:?}",
                result.iter().map(|w| &w.message).collect::<Vec<_>>()
            );
            failed += 1;

            if requirement == "MUST_WORK" {
                critical_failed += 1;
            }
        }
    }

    println!("\nRegression Test Summary:");
    println!("  Passed: {passed}");
    println!("  Failed: {failed}");
    println!("  Critical failures: {critical_failed}");

    // Critical cases must never fail
    assert_eq!(
        critical_failed, 0,
        "Critical regression detected! {critical_failed} MUST_WORK cases are failing"
    );

    // At least 80% of all cases should work
    let success_rate = passed as f64 / (passed + failed) as f64;
    assert!(
        success_rate >= 0.8,
        "Success rate too low: {:.1}% (expected >= 80%)",
        success_rate * 100.0
    );
}

/// Test historical bug patterns that have caused issues
#[test]
fn regression_test_historical_patterns() {
    let rule = MD051LinkFragments::new();

    // Historical bugs that should never happen again
    // Updated to match official GitHub behavior
    let historical_patterns = vec![
        // Ampersand issues from previous versions - updated to match GitHub behavior (& -> --)
        ("A&B", "ab"),
        ("A & B", "a--b"),
        ("A & B & C", "a--b--c"),
        // Arrow issues - updated for issue #82 fix (correct GitHub behavior)
        ("A->B", "a-b"),       // Fixed: -> now correctly becomes single hyphen
        ("A -> B", "a---b"),   // Spaces preserved: space+arrow+space = 3 hyphens
        ("A --> B", "a----b"), // Spaces preserved: space+double-arrow+space = 4 hyphens
        // Colon issues
        ("Title:Subtitle", "titlesubtitle"),
        ("Title: Subtitle", "title-subtitle"),
        // Space normalization issues - GitHub preserves each space as a hyphen
        ("Multiple   Spaces", "multiple---spaces"),
        ("Tab\tChar", "tab-char"),
        // Case handling issues
        ("MixedCASE", "mixedcase"),
        ("ALL_CAPS", "all_caps"),
        // Number handling
        ("Step 1", "step-1"),
        ("Version 2.0", "version-20"),
        ("API v3", "api-v3"),
    ];

    for (heading, expected_fragment) in historical_patterns {
        let content = format!("# {heading}\n\n[Link](#{expected_fragment})");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Historical pattern regression for '{}' -> '{}': {:?}",
            heading,
            expected_fragment,
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }
}

/// Test that Liquid template handling continues to work correctly
/// This was part of the issue 39 discussion
#[test]
fn regression_test_liquid_template_handling() {
    let rule = MD051LinkFragments::new();

    let content = r#"# Real Heading

## Another Section

Liquid patterns that MUST be ignored:
[Jekyll post]({% post_url 2023-03-25-post %}#section)
[Variable link]({{ site.url }}/page#anchor)
[Include with fragment]({% include file.html %}#part)
[Complex liquid]({% assign x = "test" %}{{ x }}.md#heading)

Valid internal links:
[Should work](#real-heading)
[Should also work](#another-section)
[Should fail](#nonexistent)
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the nonexistent internal link
    assert_eq!(result.len(), 1, "Liquid template regression detected");
    assert!(
        result[0].message.contains("nonexistent"),
        "Wrong warning message: {}",
        result[0].message
    );

    // Ensure no Liquid patterns are flagged
    for warning in &result {
        assert!(
            !warning.message.contains("post_url")
                && !warning.message.contains("site.url")
                && !warning.message.contains("include"),
            "Liquid template incorrectly flagged: {}",
            warning.message
        );
    }
}

/// Test cross-file link detection edge cases
/// Ensures we don't accidentally flag cross-file links as broken
#[test]
fn regression_test_cross_file_detection() {
    let rule = MD051LinkFragments::new();

    let content = r#"# Main Heading

Cross-file links (MUST be ignored):
[README](README.md#installation)
[Docs](docs/api.md#methods)
[Config](config.yaml#database)
[Script](setup.sh#main)
[Backup](data.tar.gz#files)

Fragment-only links (MUST be validated):
[Valid](#main-heading)
[Invalid](#missing-section)
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the invalid fragment-only link
    assert_eq!(result.len(), 1, "Cross-file detection regression");
    assert!(
        result[0].message.contains("missing-section"),
        "Wrong warning for cross-file test: {}",
        result[0].message
    );
}

/// Test performance regression prevention
#[test]
fn regression_test_performance_bounds() {
    let rule = MD051LinkFragments::new();

    // Create content with many headings and links (real-world scenario)
    let mut content = String::from("# Main Document\n\n");

    // Add 100 headings with various patterns
    for i in 0..100 {
        content.push_str(&format!("## Section {}: Complex & Pattern ({})\n\n", i, i % 10));
        content.push_str("Some content here.\n\n");
    }

    // Add links to many of these headings
    for i in 0..50 {
        let fragment = format!("section-{}-complex--pattern-{}", i, i % 10);
        content.push_str(&format!("[Link {i}](#{fragment})\n"));
    }

    // Measure performance
    let start = std::time::Instant::now();
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let _result = rule.check(&ctx).unwrap();
    let duration = start.elapsed();

    // Performance regression threshold: should handle 100 headings + 50 links in < 70ms
    // Note: Threshold increased from 50ms to accommodate more accurate emoji/symbol handling
    assert!(
        duration.as_millis() < 70,
        "Performance regression: took {}ms for 100 headings + 50 links (threshold: 70ms)",
        duration.as_millis()
    );

    println!(
        "Performance test passed: {}ms for 100 headings + 50 links",
        duration.as_millis()
    );
}

/// Test Unicode handling regression
#[test]
fn regression_test_unicode_handling() {
    let rule = MD051LinkFragments::new();

    // Unicode patterns that must continue to work
    let unicode_cases = vec![
        ("Café Menu", "café-menu"),
        ("Naïve Approach", "naïve-approach"),
        ("数据库设计", "数据库设计"),
        ("Русский Текст", "русский-текст"),
        ("العربية النص", "العربية-النص"),
        ("Mixed English & 中文", "mixed-english--中文"),
    ];

    for (heading, expected_fragment) in unicode_cases {
        let content = format!("# {heading}\n\n[Link](#{expected_fragment})");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Unicode regression for '{}' -> '{}': {:?}",
            heading,
            expected_fragment,
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }
}

/// Test that emoji stripping continues to work
#[test]
fn regression_test_emoji_handling() {
    let rule = MD051LinkFragments::new();

    let emoji_cases = vec![
        // Updated to match official GitHub behavior (emojis -> --)
        ("Emoji 🎉 Party", "emoji--party"),
        ("Multiple 🎊🎈🎁 Emoji", "multiple--emoji"),
        ("Mixed 📚 Content 📝", "mixed--content-"),
        ("Start 🚀 Project", "start--project"),
    ];

    for (heading, expected_fragment) in emoji_cases {
        let content = format!("# {heading}\n\n[Link](#{expected_fragment})");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Emoji handling regression for '{}' -> '{}': {:?}",
            heading,
            expected_fragment,
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }
}

/// Test mode switching continues to work
#[test]
fn regression_test_mode_switching() {
    use rumdl_lib::utils::anchor_styles::AnchorStyle;

    let github_rule = MD051LinkFragments::with_anchor_style(AnchorStyle::GitHub);
    let kramdown_rule = MD051LinkFragments::with_anchor_style(AnchorStyle::Kramdown);

    // Test cases where modes should differ
    let mode_test_cases = vec![
        ("test_method", "test_method", "testmethod"), // Underscores
        ("Café Menu", "café-menu", "caf-menu"),       // Accents - kramdown removes accented chars entirely
    ];

    for (heading, github_expected, kramdown_expected) in mode_test_cases {
        // Test GitHub mode
        let content = format!("# {heading}\n\n[Link](#{github_expected})");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = github_rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            0,
            "GitHub mode regression for '{heading}' -> '{github_expected}'"
        );

        // Test Kramdown mode
        let content = format!("# {heading}\n\n[Link](#{kramdown_expected})");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = kramdown_rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            0,
            "Kramdown mode regression for '{heading}' -> '{kramdown_expected}'"
        );
    }
}

/// Canary test: Simple cases that must always work
#[test]
fn regression_test_basic_canary() {
    let rule = MD051LinkFragments::new();

    // These basic cases should NEVER break
    let canary_cases = vec![
        ("Hello World", "hello-world"),
        ("Simple Test", "simple-test"),
        ("API", "api"),
        ("Step 1", "step-1"),
        ("FAQ", "faq"),
    ];

    for (heading, expected_fragment) in canary_cases {
        let content = format!("# {heading}\n\n[Link](#{expected_fragment})");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "CANARY FAILURE: Basic case '{heading}' -> '{expected_fragment}' is broken! This indicates a fundamental regression."
        );
    }
}

/// Test edge case regressions
#[test]
fn regression_test_edge_cases() {
    let rule = MD051LinkFragments::new();

    // Edge cases that have been problematic in the past
    let edge_cases = vec![
        ("", ""),                                     // Empty heading
        ("   ", ""),                                  // Whitespace only
        ("123", "123"),                               // Numbers only
        ("_", "_"),                                   // Single underscore
        ("A", "a"),                                   // Single character
        ("Multiple---Hyphens", "multiple---hyphens"), // Consecutive hyphens should be preserved
    ];

    for (heading, expected_fragment) in edge_cases {
        if expected_fragment.is_empty() {
            // For empty expected fragments, just ensure no crash
            let content = format!("# {heading}\n\n");
            let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx);
            assert!(result.is_ok(), "Edge case crashed: '{heading}'");
        } else {
            let content = format!("# {heading}\n\n[Link](#{expected_fragment})");
            let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();

            assert_eq!(
                result.len(),
                0,
                "Edge case regression for '{}' -> '{}': {:?}",
                heading,
                expected_fragment,
                result.iter().map(|w| &w.message).collect::<Vec<_>>()
            );
        }
    }
}

/// Comprehensive regression suite runner
#[test]
fn comprehensive_regression_suite() {
    println!("Running comprehensive regression test suite for MD051...");

    let rule = MD051LinkFragments::new();

    // Test a representative set of cases from each category
    // Updated to match official GitHub behavior verified via GitHub Gists
    let test_cases = vec![
        // Issue 39 critical - updated to match GitHub behavior
        ("Testing & Coverage", "testing--coverage", "Issue 39 Critical"),
        // Historical patterns - updated to match GitHub behavior
        ("A & B", "a--b", "Historical Patterns"),
        // Unicode handling
        ("Café Menu", "café-menu", "Unicode Handling"),
        // Emoji handling - updated to match GitHub behavior
        ("Emoji 🎉 Party", "emoji--party", "Emoji Handling"),
        // Basic canary
        ("Hello World", "hello-world", "Basic Canary"),
    ];

    let mut results = HashMap::new();

    for (heading, expected_fragment, category) in test_cases {
        let content = format!("# {heading}\n\n[Link](#{expected_fragment})");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        let success = result.is_empty();
        results.entry(category).or_insert(Vec::new()).push(success);

        if success {
            println!("✅ {category}: '{heading}' -> '{expected_fragment}'");
        } else {
            println!("❌ {category}: '{heading}' -> '{expected_fragment}'");
        }
    }

    // Report summary
    let passed_categories = results.iter().filter(|(_, tests)| tests.iter().all(|&t| t)).count();
    let total_categories = results.len();

    println!("\nRegression Test Summary:");
    println!("  Passed Categories: {passed_categories}/{total_categories}");
    println!(
        "  Success Rate: {:.1}%",
        (passed_categories as f64 / total_categories as f64) * 100.0
    );

    // Require at least 80% of categories to pass completely
    let success_rate = passed_categories as f64 / total_categories as f64;
    assert!(
        success_rate >= 0.8,
        "Regression detected! Category success rate: {:.1}% (expected >= 80%)",
        success_rate * 100.0
    );

    println!("✅ Regression test passed!");
}

// End of MD051 regression prevention tests
